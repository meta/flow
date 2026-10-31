/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

//! Usually types carry enough information about the "reason" for their
//! existence (e.g., position in code, introduction/elimination rules in
//! the type system), so printing the reason provides a good idea of what the
//! type means to the programmer.

use std::ops::Deref;
use std::rc::Rc;
use std::sync::Arc;

use dupe::Dupe;
use flow_aloc::ALoc;
use flow_aloc::ALocId;
use flow_common::reason::Reason;
use flow_common::reason::ReasonDesc;
use flow_common::reason::VirtualReason;
use flow_common::reason::VirtualReasonDesc;
use flow_common::reason::mk_reason;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_lazy::ContextFamily;

use crate::type_::ArrRestTData;
use crate::type_::ArrayElementCompatibilityData;
use crate::type_::AssertionFunctionConsistencyData;
use crate::type_::AssignVarDefaultData;
use crate::type_::AssignVarInit;
use crate::type_::AssignVarTarget;
use crate::type_::CallTData;
use crate::type_::ClassExtendsCheckData;
use crate::type_::ClassImplementsCheckData;
use crate::type_::ClassMethodDefinitionData;
use crate::type_::ClassOwnProtoCheckData;
use crate::type_::ComponentRestParamCompatibilityData;
use crate::type_::ConditionalTData;
use crate::type_::ConformToCommonInterfaceData;
use crate::type_::ConstrainedAssignmentData;
use crate::type_::ConstructorTData;
use crate::type_::ElemTData;
use crate::type_::EvalMappedTypeData;
use crate::type_::ExtendsUseTData;
use crate::type_::FunCallData;
use crate::type_::FunCallMethodCallee;
use crate::type_::FunCallMethodData;
use crate::type_::FunImplicitReturnData;
use crate::type_::FunMissingArgData;
use crate::type_::FunParamData;
use crate::type_::FunReturnData;
use crate::type_::GenericTData;
use crate::type_::GetElemTData;
use crate::type_::GetEnumTData;
use crate::type_::GetPrivatePropTData;
use crate::type_::GetPropTData;
use crate::type_::HasOwnPropTData;
use crate::type_::ImplicitInstantiationTvarData;
use crate::type_::IndexedTypeAccessData;
use crate::type_::IndexerKeyCompatibilityData;
use crate::type_::InitFieldData;
use crate::type_::JSXCreateElementData;
use crate::type_::MapTypeTData;
use crate::type_::MappedTypeKeyCompatibilityData;
use crate::type_::MergedDeclarationData;
use crate::type_::MethodTData;
use crate::type_::MixedFlavor;
use crate::type_::OpaqueTypeCustomErrorCompatibilityData;
use crate::type_::OptionalIndexedAccessTData;
use crate::type_::PolyTData;
use crate::type_::PolyTKind;
use crate::type_::PositiveTypeGuardConsistencyData;
use crate::type_::PrivateMethodTData;
use crate::type_::PropertyCompatibilityData;
use crate::type_::ROArrayATData;
use crate::type_::ReactCreateElementCallData;
use crate::type_::ReactKitTData;
use crate::type_::RecordCreateData;
use crate::type_::ReposUseTData;
use crate::type_::ResolveSpreadTData;
use crate::type_::RootUseOp;
use crate::type_::SetElemTData;
use crate::type_::SetPrivatePropTData;
use crate::type_::SetPropertyData;
use crate::type_::SpecializeTData;
use crate::type_::StandaloneCallThisData;
use crate::type_::SuperTData;
use crate::type_::SwitchRefinementCheckData;
use crate::type_::TestPropTData;
use crate::type_::ThisInstanceTData;
use crate::type_::ThisTypeAppTData;
use crate::type_::TupleElementCompatibilityData;
use crate::type_::Type;
use crate::type_::TypeAppTData;
use crate::type_::TypeApplicationTarget;
use crate::type_::TypeArgCompatibilityData;
use crate::type_::TypeDestructorT;
use crate::type_::TypeDestructorTInner;
use crate::type_::TypeGuardIncompatibilityData;
use crate::type_::TypeInner;
use crate::type_::TypeStrictnessKind;
use crate::type_::TypeofApplicationData;
use crate::type_::UnionRepresentativeData;
use crate::type_::UseOp;
use crate::type_::UseT;
use crate::type_::UseTInner;
use crate::type_::ValueToTypeReferenceTData;
use crate::type_::VirtualFrameUseOp;
use crate::type_::VirtualRootUseOp;
use crate::type_::VirtualUseOp;
use crate::type_::type_or_type_desc;

pub fn reason_of_t(t: &Type) -> &Reason {
    match t.deref() {
        TypeInner::OpenT(tvar) => tvar.reason(),
        TypeInner::AnnotT(reason, _, _) => reason,
        TypeInner::DefT(reason, _) => reason,
        TypeInner::EvalT { defer_use_t, .. } => reason_of_defer_use_t(defer_use_t),
        TypeInner::GenericT(box GenericTData { reason, .. }) => reason,
        TypeInner::ImplicitInstantiationTvar(data) => &data.reason,
        TypeInner::FunProtoT(reason) => reason,
        TypeInner::KeysT(reason, _) => reason,
        TypeInner::TemplateLiteralT { reason, .. } => reason,
        TypeInner::StringMappingT { reason, .. } => reason,
        TypeInner::NamespaceT(namespace) => reason_of_t(&namespace.values_type),
        TypeInner::NullProtoT(reason) => reason,
        TypeInner::ObjProtoT(reason) => reason,
        TypeInner::NominalT { reason, .. } => reason,
        TypeInner::ThisInstanceT(box ThisInstanceTData { reason, .. }) => reason,
        TypeInner::ThisTypeAppT(box ThisTypeAppTData { reason, .. }) => reason,
        TypeInner::TypeAppT(box TypeAppTData { reason, .. }) => reason,
        TypeInner::AnyT(reason, _) => reason,
        TypeInner::UnionT(reason, _) => reason,
        TypeInner::IntersectionT(reason, _) => reason,
        TypeInner::MaybeT(reason, _) => reason,
        TypeInner::OptionalT { reason, .. } => reason,
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ConstraintNodeId {
    OpenT(i32),
    Inference(i32),
}

impl ConstraintNodeId {
    pub fn id(self) -> i32 {
        match self {
            Self::OpenT(id) | Self::Inference(id) => id,
        }
    }

    pub fn is_open_t(self) -> bool {
        matches!(self, Self::OpenT(_))
    }
}

pub fn constraint_node_id(t: &Type) -> Option<ConstraintNodeId> {
    match t.deref() {
        TypeInner::OpenT(tvar) => Some(ConstraintNodeId::OpenT(tvar.id() as i32)),
        TypeInner::ImplicitInstantiationTvar(data) => Some(ConstraintNodeId::Inference(data.id)),
        _ => None,
    }
}

pub fn reason_of_defer_use_t(defer_use_t: &TypeDestructorT) -> &Reason {
    &defer_use_t.1
}

// When the type is a singleton and not from annotation, convert it to literal reason.
pub fn singleton_reason_of_t(t: &Type) -> Reason {
    use crate::type_::DefTInner as D;
    match t.deref() {
        TypeInner::DefT(reason, def_t) => match &**def_t {
            D::SingletonStrT {
                value,
                from_annot: false,
            } => {
                let loc = reason.loc().dupe();
                mk_reason(VirtualReasonDesc::RStringLit(value.dupe()), loc)
            }
            D::SingletonNumT {
                value,
                from_annot: false,
            } => {
                let loc = reason.loc().dupe();
                mk_reason(VirtualReasonDesc::RNumberLit(value.1.dupe()), loc)
            }
            D::SingletonBoolT {
                value,
                from_annot: false,
            } => {
                let loc = reason.loc().dupe();
                mk_reason(VirtualReasonDesc::RBooleanLit(*value), loc)
            }
            D::SingletonBigIntT {
                value,
                from_annot: false,
            } => {
                let loc = reason.loc().dupe();
                mk_reason(VirtualReasonDesc::RBigIntLit(value.1.dupe()), loc)
            }
            _ => reason_of_t(t).dupe(),
        },
        _ => reason_of_t(t).dupe(),
    }
}

pub fn desc_of_t(t: &Type) -> Option<&ReasonDesc> {
    reason_of_t(t).desc(true)
}

pub fn loc_of_t(t: &Type) -> &ALoc {
    reason_of_t(t).loc()
}

pub fn def_loc_of_t(t: &Type) -> &ALoc {
    reason_of_t(t).def_loc()
}

// Where to anchor an error-message reference to `t`. Matches how reasons are rendered
// (`friendly::ref_map`): prefer the annotation location so the reference points at where
// the type was written, falling back to the definition location.
pub fn ref_loc_of_t(t: &Type) -> &ALoc {
    let reason = reason_of_t(t);
    reason.annot_loc().unwrap_or(reason.def_loc())
}

// TODO make a type visitor
pub fn mod_reason_of_t(f: &dyn Fn(Reason) -> Reason, t: &Type) -> Type {
    use crate::type_::NamespaceType;
    match t.deref() {
        TypeInner::OpenT(tvar) => Type::new(TypeInner::OpenT(crate::type_::Tvar::new(
            f(tvar.reason().dupe()),
            tvar.id(),
        ))),
        TypeInner::AnnotT(reason, inner_t, use_desc) => Type::new(TypeInner::AnnotT(
            f(reason.dupe()),
            inner_t.dupe(),
            *use_desc,
        )),
        TypeInner::DefT(reason, def) => Type::new(TypeInner::DefT(f(reason.dupe()), def.dupe())),
        TypeInner::AnyT(reason, src) => Type::new(TypeInner::AnyT(f(reason.dupe()), *src)),
        TypeInner::UnionT(reason, rep) => {
            Type::new(TypeInner::UnionT(f(reason.dupe()), rep.dupe()))
        }
        TypeInner::IntersectionT(reason, rep) => {
            Type::new(TypeInner::IntersectionT(f(reason.dupe()), rep.dupe()))
        }
        TypeInner::MaybeT(reason, inner_t) => {
            Type::new(TypeInner::MaybeT(f(reason.dupe()), inner_t.dupe()))
        }
        TypeInner::OptionalT {
            reason,
            type_,
            use_desc,
        } => Type::new(TypeInner::OptionalT {
            reason: f(reason.dupe()),
            type_: type_.dupe(),
            use_desc: *use_desc,
        }),
        TypeInner::EvalT {
            type_,
            defer_use_t,
            id,
        } => Type::new(TypeInner::EvalT {
            type_: type_.dupe(),
            defer_use_t: mod_reason_of_defer_use_t(&f, defer_use_t),
            id: id.dupe(),
        }),
        TypeInner::GenericT(box GenericTData {
            reason,
            name,
            bound,
            no_infer,
            id,
        }) => Type::new(TypeInner::GenericT(Box::new(GenericTData {
            reason: f(reason.dupe()),
            name: name.dupe(),
            bound: bound.dupe(),
            no_infer: *no_infer,
            id: id.clone(),
        }))),
        TypeInner::ImplicitInstantiationTvar(box ImplicitInstantiationTvarData {
            reason,
            name,
            bound,
            id,
        }) => Type::new(TypeInner::ImplicitInstantiationTvar(Box::new(
            ImplicitInstantiationTvarData {
                reason: f(reason.dupe()),
                name: name.dupe(),
                bound: bound.dupe(),
                id: *id,
            },
        ))),
        TypeInner::FunProtoT(reason) => Type::new(TypeInner::FunProtoT(f(reason.dupe()))),
        TypeInner::KeysT(reason, inner_t) => {
            Type::new(TypeInner::KeysT(f(reason.dupe()), inner_t.dupe()))
        }
        TypeInner::TemplateLiteralT {
            reason,
            from_annot,
            quasis,
            types,
        } => Type::new(TypeInner::TemplateLiteralT {
            reason: f(reason.dupe()),
            from_annot: *from_annot,
            quasis: quasis.to_vec(),
            types: types.to_vec(),
        }),
        TypeInner::StringMappingT { reason, kind, arg } => Type::new(TypeInner::StringMappingT {
            reason: f(reason.dupe()),
            kind: *kind,
            arg: arg.dupe(),
        }),
        TypeInner::NamespaceT(ns) => Type::new(TypeInner::NamespaceT(Rc::new(NamespaceType {
            namespace_symbol: ns.namespace_symbol.dupe(),
            values_type: mod_reason_of_t(&f, &ns.values_type),
            types_tmap: ns.types_tmap.dupe(),
        }))),
        TypeInner::NullProtoT(reason) => Type::new(TypeInner::NullProtoT(f(reason.dupe()))),
        TypeInner::ObjProtoT(reason) => Type::new(TypeInner::ObjProtoT(f(reason.dupe()))),
        TypeInner::NominalT {
            reason,
            nominal_type,
        } => Type::new(TypeInner::NominalT {
            reason: f(reason.dupe()),
            nominal_type: nominal_type.dupe(),
        }),
        TypeInner::ThisInstanceT(box ThisInstanceTData {
            reason,
            instance,
            is_this,
            subst_name,
        }) => Type::new(TypeInner::ThisInstanceT(Box::new(ThisInstanceTData {
            reason: f(reason.dupe()),
            instance: instance.dupe(),
            is_this: *is_this,
            subst_name: subst_name.dupe(),
        }))),
        TypeInner::ThisTypeAppT(box ThisTypeAppTData {
            reason,
            this_t,
            type_,
            targs,
        }) => Type::new(TypeInner::ThisTypeAppT(Box::new(ThisTypeAppTData {
            reason: f(reason.dupe()),
            this_t: this_t.dupe(),
            type_: type_.dupe(),
            targs: targs.dupe(),
        }))),
        TypeInner::TypeAppT(box TypeAppTData {
            reason,
            use_op,
            type_,
            targs,
            from_value,
            use_desc,
        }) => Type::new(TypeInner::TypeAppT(Box::new(TypeAppTData {
            reason: f(reason.dupe()),
            use_op: use_op.dupe(),
            type_: type_.dupe(),
            targs: targs.dupe(),
            from_value: *from_value,
            use_desc: *use_desc,
        }))),
    }
}

/// Clears literal status on a top-level object or array type.
pub fn non_literal_of_t(t: &Type) -> Type {
    use crate::type_::DefT;
    use crate::type_::DefTInner;
    use crate::type_::ObjType;

    match t.deref() {
        TypeInner::DefT(reason, def)
            if let DefTInner::ArrT(arr) = def.deref()
                && (arr.literal() || arr.fresh()) =>
        {
            Type::new(TypeInner::DefT(
                reason.dupe(),
                DefT::new(DefTInner::ArrT(Rc::new(
                    arr.as_ref().clone().with_literal(false, false),
                ))),
            ))
        }
        TypeInner::DefT(reason, def)
            if let DefTInner::ObjT(o) = def.deref()
                && o.literal =>
        {
            Type::new(TypeInner::DefT(
                reason.dupe(),
                DefT::new(DefTInner::ObjT(Rc::new(ObjType {
                    literal: false,
                    ..o.as_ref().clone()
                }))),
            ))
        }
        _ => t.dupe(),
    }
}

pub fn mod_reason_of_defer_use_t(
    f: &dyn Fn(Reason) -> Reason,
    defer_use_t: &TypeDestructorT,
) -> TypeDestructorT {
    TypeDestructorT::new(TypeDestructorTInner(
        defer_use_t.0.dupe(),
        f(defer_use_t.1.dupe()),
        defer_use_t.2.dupe(),
    ))
}

pub fn util_use_op_of_use_t<T, CX: ContextFamily>(
    nope: &dyn Fn(&UseT<CX>) -> T,
    util: &dyn Fn(&UseT<CX>, &UseOp, &dyn Fn(UseOp) -> UseT<CX>) -> T,
    u: &UseT<CX>,
) -> T {
    let call_util = |op: &UseOp, make: &dyn Fn(UseOp) -> UseT<CX>| util(u, op, make);

    match u.deref() {
        UseTInner::UseT(op, t) => {
            let t = t.dupe();
            call_util(op, &move |op| UseT::new(UseTInner::UseT(op, t.dupe())))
        }
        UseTInner::ConditionalT(data) => {
            let reason = data.reason.dupe();
            let distributive_tparam_name = data.distributive_tparam_name.clone();
            let infer_tparams = data.infer_tparams.dupe();
            let extends_t = data.extends_t.clone();
            let true_t = data.true_t.clone();
            let false_t = data.false_t.clone();
            let tout = data.tout.clone();
            call_util(&data.use_op, &move |use_op| {
                UseT::new(UseTInner::ConditionalT(Box::new(ConditionalTData {
                    use_op,
                    reason: reason.dupe(),
                    distributive_tparam_name: distributive_tparam_name.clone(),
                    infer_tparams: infer_tparams.dupe(),
                    extends_t: extends_t.clone(),
                    true_t: true_t.clone(),
                    false_t: false_t.clone(),
                    tout: tout.clone(),
                })))
            })
        }
        UseTInner::CallT(data) => {
            let reason = data.reason.dupe();
            let call_action = data.call_action.clone();
            let return_hint = data.return_hint.clone();
            call_util(&data.use_op, &move |use_op| {
                UseT::new(UseTInner::CallT(Box::new(CallTData {
                    use_op,
                    reason: reason.dupe(),
                    call_action: call_action.clone(),
                    return_hint: return_hint.clone(),
                })))
            })
        }
        UseTInner::MethodT(data) => {
            let r1 = data.reason.dupe();
            let r2 = data.prop_reason.dupe();
            let p = data.propref.clone();
            let f = data.method_action.clone();
            call_util(&data.use_op, &move |op| {
                UseT::new(UseTInner::MethodT(Box::new(MethodTData {
                    use_op: op,
                    reason: r1.dupe(),
                    prop_reason: r2.dupe(),
                    propref: p.clone(),
                    method_action: f.clone(),
                })))
            })
        }
        UseTInner::PrivateMethodT(data) => {
            let r1 = data.reason.dupe();
            let r2 = data.prop_reason.dupe();
            let x = data.name.clone();
            let c = data.class_bindings.clone();
            let s = data.static_;
            let a = data.method_action.clone();
            call_util(&data.use_op, &move |op| {
                UseT::new(UseTInner::PrivateMethodT(Box::new(PrivateMethodTData {
                    use_op: op,
                    reason: r1.dupe(),
                    prop_reason: r2.dupe(),
                    name: x.clone(),
                    class_bindings: c.clone(),
                    static_: s,
                    method_action: a.clone(),
                })))
            })
        }
        UseTInner::SetPropT(op, r, p, m, w, t, tp) => {
            let r = r.dupe();
            let p = p.clone();
            let m = m.clone();
            let w = w.clone();
            let t = t.clone();
            let tp = tp.clone();
            call_util(op, &move |op| {
                UseT::new(UseTInner::SetPropT(
                    op,
                    r.dupe(),
                    p.clone(),
                    m.clone(),
                    w.clone(),
                    t.clone(),
                    tp.clone(),
                ))
            })
        }
        UseTInner::SetPrivatePropT(data) => {
            let r = data.reason.dupe();
            let s = data.name.clone();
            let m = data.set_mode.clone();
            let c = data.class_bindings.clone();
            let b = data.static_;
            let x = data.write_ctx.clone();
            let t = data.tin.clone();
            let tp = data.tout.clone();
            call_util(&data.use_op, &move |op| {
                UseT::new(UseTInner::SetPrivatePropT(Box::new(SetPrivatePropTData {
                    use_op: op,
                    reason: r.dupe(),
                    name: s.clone(),
                    set_mode: m.clone(),
                    class_bindings: c.clone(),
                    static_: b,
                    write_ctx: x.clone(),
                    tin: t.clone(),
                    tout: tp.clone(),
                })))
            })
        }
        UseTInner::GetPropT(data) => {
            let reason = data.reason.dupe();
            let id = data.id.clone();
            let from_annot = data.from_annot;
            let skip_optional = data.skip_optional;
            let propref = data.propref.clone();
            let tout = data.tout.clone();
            let hint = data.hint.clone();
            call_util(&data.use_op, &move |use_op| {
                UseT::new(UseTInner::GetPropT(Box::new(GetPropTData {
                    use_op,
                    reason: reason.dupe(),
                    id: id.clone(),
                    from_annot,
                    skip_optional,
                    propref: propref.clone(),
                    tout: tout.clone(),
                    hint: hint.clone(),
                })))
            })
        }
        UseTInner::TestPropT(data) => {
            let reason = data.reason.dupe();
            let id = data.id;
            let propref = data.propref.clone();
            let tout = data.tout.clone();
            let hint = data.hint.clone();
            call_util(&data.use_op, &move |use_op| {
                UseT::new(UseTInner::TestPropT(Box::new(TestPropTData {
                    use_op,
                    reason: reason.dupe(),
                    id,
                    propref: propref.clone(),
                    tout: tout.clone(),
                    hint: hint.clone(),
                })))
            })
        }
        UseTInner::GetPrivatePropT(data) => {
            let r = data.reason.dupe();
            let s = data.name.clone();
            let c = data.class_bindings.clone();
            let b = data.static_;
            let t = data.tout.clone();
            call_util(&data.use_op, &move |op| {
                UseT::new(UseTInner::GetPrivatePropT(Box::new(GetPrivatePropTData {
                    use_op: op,
                    reason: r.dupe(),
                    name: s.clone(),
                    class_bindings: c.clone(),
                    static_: b,
                    tout: t.clone(),
                })))
            })
        }
        UseTInner::SetElemT(data) => {
            let r = data.reason.dupe();
            let t1 = data.key_t.clone();
            let m = data.set_mode.clone();
            let t2 = data.tin.clone();
            let t3 = data.tout.clone();
            call_util(&data.use_op, &move |op| {
                UseT::new(UseTInner::SetElemT(Box::new(SetElemTData {
                    use_op: op,
                    reason: r.dupe(),
                    key_t: t1.clone(),
                    set_mode: m.clone(),
                    tin: t2.clone(),
                    tout: t3.clone(),
                })))
            })
        }
        UseTInner::GetElemT(data) => {
            let reason = data.reason.dupe();
            let id = data.id.clone();
            let from_annot = data.from_annot;
            let skip_optional = data.skip_optional;
            let access_iterables = data.access_iterables;
            let key_t = data.key_t.clone();
            let tout = data.tout.clone();
            call_util(&data.use_op, &move |use_op| {
                UseT::new(UseTInner::GetElemT(Box::new(GetElemTData {
                    use_op,
                    reason: reason.dupe(),
                    id: id.clone(),
                    from_annot,
                    skip_optional,
                    access_iterables,
                    key_t: key_t.clone(),
                    tout: tout.clone(),
                })))
            })
        }
        UseTInner::OptionalIndexedAccessT(data) => {
            let reason = data.reason.dupe();
            let index = data.index.clone();
            let tout_tvar = data.tout_tvar.clone();
            call_util(&data.use_op, &move |use_op| {
                UseT::new(UseTInner::OptionalIndexedAccessT(Box::new(
                    OptionalIndexedAccessTData {
                        use_op,
                        reason: reason.dupe(),
                        index: index.clone(),
                        tout_tvar: tout_tvar.clone(),
                    },
                )))
            })
        }
        UseTInner::ReposLowerT {
            reason,
            use_desc,
            use_t: inner_use_t,
        } => {
            let reason = reason.dupe();
            let use_desc = *use_desc;

            util_use_op_of_use_t(
                &|_| nope(u),
                &|_, inner_op, inner_make| {
                    util(u, inner_op, &|new_op| {
                        UseT::new(UseTInner::ReposLowerT {
                            reason: reason.dupe(),
                            use_desc,
                            use_t: Box::new(inner_make(new_op)),
                        })
                    })
                },
                inner_use_t.as_ref(),
            )
        }
        UseTInner::ReposUseT(data) => {
            let r = data.reason.dupe();
            let d = data.use_desc;
            let t = data.type_.dupe();
            call_util(&data.use_op, &move |op| {
                UseT::new(UseTInner::ReposUseT(Box::new(ReposUseTData {
                    reason: r.dupe(),
                    use_desc: d,
                    use_op: op,
                    type_: t.dupe(),
                })))
            })
        }
        UseTInner::ConstructorT(data) => {
            let reason = data.reason.dupe();
            let targs = data.targs.clone();
            let args = data.args.clone();
            let tout = data.tout.clone();
            let return_hint = data.return_hint.clone();
            let specialized_ctor = data.specialized_ctor.clone();
            call_util(&data.use_op, &move |use_op| {
                UseT::new(UseTInner::ConstructorT(Box::new(ConstructorTData {
                    use_op,
                    reason: reason.dupe(),
                    targs: targs.clone(),
                    args: args.clone(),
                    tout: tout.clone(),
                    return_hint: return_hint.clone(),
                    specialized_ctor: specialized_ctor.clone(),
                })))
            })
        }
        UseTInner::SuperT(data) => {
            let r = data.reason.dupe();
            let i = data.derived_type.clone();
            call_util(&data.use_op, &move |op| {
                UseT::new(UseTInner::SuperT(Box::new(SuperTData {
                    use_op: op,
                    reason: r.dupe(),
                    derived_type: i.clone(),
                })))
            })
        }
        UseTInner::ImplementsT(op, t) => {
            let t = t.dupe();
            call_util(op, &move |op| {
                UseT::new(UseTInner::ImplementsT(op, t.dupe()))
            })
        }
        UseTInner::ToStringT {
            orig_t,
            reason,
            t_out,
        } => {
            let orig_t = orig_t.dupe();
            let reason = reason.dupe();

            util_use_op_of_use_t(
                &|_| nope(u),
                &|_, inner_op, inner_make| {
                    util(u, inner_op, &|new_op| {
                        UseT::new(UseTInner::ToStringT {
                            orig_t: orig_t.dupe(),
                            reason: reason.dupe(),
                            t_out: Box::new(inner_make(new_op)),
                        })
                    })
                },
                t_out.as_ref(),
            )
        }
        UseTInner::SpecializeT(data) => {
            let r1 = data.reason.dupe();
            let r2 = data.reason2.dupe();
            let ts = data.targs.clone();
            let upper = data.upper.clone();
            call_util(&data.use_op, &move |op| {
                UseT::new(UseTInner::SpecializeT(Box::new(SpecializeTData {
                    use_op: op,
                    reason: r1.dupe(),
                    reason2: r2.dupe(),
                    targs: ts.clone(),
                    upper: upper.clone(),
                })))
            })
        }
        UseTInner::FilterOptionalT(op, t) => {
            let t = t.dupe();
            call_util(op, &move |op| {
                UseT::new(UseTInner::FilterOptionalT(op, t.dupe()))
            })
        }
        UseTInner::FilterMaybeT(op, t) => {
            let t = t.dupe();
            call_util(op, &move |op| {
                UseT::new(UseTInner::FilterMaybeT(op, t.dupe()))
            })
        }
        UseTInner::ConcretizeTypeAppsT(u_inner, box (ts1, b1, op, r1), x2, b2) => {
            let u_inner = u_inner.clone();
            let ts1 = ts1.clone();
            let b1 = *b1;
            let r1 = r1.dupe();
            let x2 = x2.clone();
            let b2 = *b2;
            call_util(op, &move |op| {
                UseT::new(UseTInner::ConcretizeTypeAppsT(
                    u_inner.clone(),
                    Box::new((ts1.clone(), b1, op, r1.dupe())),
                    x2.clone(),
                    b2,
                ))
            })
        }
        UseTInner::ArrRestT(data) => {
            let literal = data.literal;
            let r = data.reason.dupe();
            let i = data.index;
            let t = data.tout.clone();
            call_util(&data.use_op, &move |op| {
                UseT::new(UseTInner::ArrRestT(Box::new(ArrRestTData {
                    literal,
                    use_op: op,
                    reason: r.dupe(),
                    index: i,
                    tout: t.clone(),
                })))
            })
        }
        UseTInner::HasOwnPropT(data) => {
            let r = data.reason.dupe();
            let t = data.type_.clone();
            call_util(&data.use_op, &move |op| {
                UseT::new(UseTInner::HasOwnPropT(Box::new(HasOwnPropTData {
                    use_op: op,
                    reason: r.dupe(),
                    type_: t.clone(),
                })))
            })
        }
        UseTInner::GetKeysT {
            reason: r,
            t_out: inner_use_t,
            include_symbols,
        } => {
            let r = r.dupe();
            let include_symbols = *include_symbols;

            util_use_op_of_use_t(
                &|_| nope(u),
                &|_, inner_op, inner_make| {
                    util(u, inner_op, &|new_op| {
                        UseT::new(UseTInner::GetKeysT {
                            reason: r.dupe(),
                            t_out: Box::new(inner_make(new_op)),
                            include_symbols,
                        })
                    })
                },
                inner_use_t.as_ref(),
            )
        }
        UseTInner::GetKeysDictKeyT {
            reason: r,
            t_out: inner_use_t,
            include_symbols,
        } => {
            let r = r.dupe();
            let include_symbols = *include_symbols;

            util_use_op_of_use_t(
                &|_| nope(u),
                &|_, inner_op, inner_make| {
                    util(u, inner_op, &|new_op| {
                        UseT::new(UseTInner::GetKeysDictKeyT {
                            reason: r.dupe(),
                            t_out: Box::new(inner_make(new_op)),
                            include_symbols,
                        })
                    })
                },
                inner_use_t.as_ref(),
            )
        }
        UseTInner::GetDictValuesT(r, inner_use_t) => {
            let r = r.dupe();

            util_use_op_of_use_t(
                &|_| nope(u),
                &|_, inner_op, inner_make| {
                    util(u, inner_op, &|new_op| {
                        UseT::new(UseTInner::GetDictValuesT(
                            r.dupe(),
                            Box::new(inner_make(new_op)),
                        ))
                    })
                },
                inner_use_t.as_ref(),
            )
        }
        UseTInner::ElemT(data) => {
            let reason = data.reason.dupe();
            let obj = data.obj.dupe();
            let action = data.action.clone();
            call_util(&data.use_op, &move |use_op| {
                UseT::new(UseTInner::ElemT(Box::new(ElemTData {
                    use_op,
                    reason: reason.dupe(),
                    obj: obj.dupe(),
                    action: action.clone(),
                })))
            })
        }
        UseTInner::ObjKitT(op, r, x, y, t) => {
            let r = r.dupe();
            let x = x.clone();
            let y = y.clone();
            let t = t.clone();
            call_util(op, &move |op| {
                UseT::new(UseTInner::ObjKitT(
                    op,
                    r.dupe(),
                    x.clone(),
                    y.clone(),
                    t.clone(),
                ))
            })
        }
        UseTInner::ReactKitT(data) => {
            let r = data.reason.dupe();
            let t = data.tool.clone();
            call_util(&data.use_op, &move |op| {
                UseT::new(UseTInner::ReactKitT(Box::new(ReactKitTData {
                    use_op: op,
                    reason: r.dupe(),
                    tool: t.clone(),
                })))
            })
        }
        UseTInner::ResolveSpreadT(data) => {
            let r = data.reason.dupe();
            let s = data.resolve_spread_type.clone();
            call_util(&data.use_op, &move |op| {
                UseT::new(UseTInner::ResolveSpreadT(Box::new(ResolveSpreadTData {
                    use_op: op,
                    reason: r.dupe(),
                    resolve_spread_type: s.clone(),
                })))
            })
        }
        UseTInner::ExtendsUseT(data) => {
            let r = data.reason.dupe();
            let ts = data.targs.clone();
            let a = data.true_t.dupe();
            let b = data.false_t.dupe();
            call_util(&data.use_op, &move |op| {
                UseT::new(UseTInner::ExtendsUseT(Box::new(ExtendsUseTData {
                    use_op: op,
                    reason: r.dupe(),
                    targs: ts.clone(),
                    true_t: a.dupe(),
                    false_t: b.dupe(),
                })))
            })
        }
        UseTInner::MapTypeT(data) => {
            let r = data.reason.dupe();
            let k = data.type_map.clone();
            let t = data.tout.dupe();
            call_util(&data.use_op, &move |op| {
                UseT::new(UseTInner::MapTypeT(Box::new(MapTypeTData {
                    use_op: op,
                    reason: r.dupe(),
                    type_map: k.clone(),
                    tout: t.dupe(),
                })))
            })
        }
        UseTInner::ValueToTypeReferenceT(data) => {
            let reason = data.reason.dupe();
            let kind = data.kind.clone();
            let t = data.tout.clone();
            call_util(&data.use_op, &move |use_op| {
                UseT::new(UseTInner::ValueToTypeReferenceT(Box::new(
                    ValueToTypeReferenceTData {
                        use_op,
                        reason: reason.dupe(),
                        kind: kind.clone(),
                        tout: t.clone(),
                    },
                )))
            })
        }
        UseTInner::GetEnumT(data) => {
            let loc = data.loc.dupe();
            let orig_t = data.orig_t.dupe();
            let kind = data.kind.clone();
            let collector = data.collector.dupe();
            call_util(&data.use_op, &move |use_op| {
                UseT::new(UseTInner::GetEnumT(Box::new(GetEnumTData {
                    use_op,
                    loc: loc.dupe(),
                    orig_t: orig_t.dupe(),
                    kind: kind.clone(),
                    collector: collector.dupe(),
                })))
            })
        }
        UseTInner::CallElemT(..)
        | UseTInner::GetStaticsT(..)
        | UseTInner::GetProtoT(_, _)
        | UseTInner::MixinT(_, _)
        | UseTInner::ConvertEmptyPropsToMixedT(_, _)
        | UseTInner::DeepReadOnlyT(_, _)
        | UseTInner::HooklikeT(_)
        | UseTInner::ThisSpecializeT(_, _, _)
        | UseTInner::LookupT(..)
        | UseTInner::ObjRestT(_, _, _, _, _)
        | UseTInner::ObjTestProtoT(_, _)
        | UseTInner::GetValuesT(_, _)
        | UseTInner::ConcretizeT(..)
        | UseTInner::CondT(..)
        | UseTInner::ResolveUnionT(..)
        | UseTInner::ExitRendersT { .. }
        | UseTInner::SealGenericT(..)
        | UseTInner::EvalTypeDestructorT(..) => nope(u),
    }
}

pub fn is_in_common_interface_conformance_check(use_op: &UseOp) -> bool {
    use crate::type_::VirtualRootUseOp;
    use crate::type_::root_of_use_op;
    matches!(
        root_of_use_op(use_op),
        VirtualRootUseOp::ConformToCommonInterface(_)
    )
}

pub fn use_op_of_use_t<CX: ContextFamily>(u: &UseT<CX>) -> Option<UseOp> {
    util_use_op_of_use_t(&|_| None, &|_, op, _| Some(op.dupe()), u)
}

pub fn mod_use_op_of_use_t<F, CX: ContextFamily>(f: F, u: &UseT<CX>) -> UseT<CX>
where
    F: Fn(&UseOp) -> UseOp,
{
    util_use_op_of_use_t(
        &|u| u.dupe(),
        &|u, op, make| {
            let op_prime = f(op);
            let same = match (&op_prime, op) {
                (VirtualUseOp::Op(rc1), VirtualUseOp::Op(rc2)) => Arc::ptr_eq(rc1, rc2),
                (VirtualUseOp::Frame(f1, i1), VirtualUseOp::Frame(f2, i2)) => {
                    Arc::ptr_eq(f1, f2) && Arc::ptr_eq(i1, i2)
                }
                _ => false,
            };
            if same { u.dupe() } else { make(op_prime) }
        },
        u,
    )
}

pub fn mod_root_of_use_op<F>(f: F, use_op: &UseOp) -> UseOp
where
    F: Fn(&RootUseOp) -> RootUseOp,
{
    match use_op {
        VirtualUseOp::Op(root) => VirtualUseOp::Op(Arc::new(f(root))),
        VirtualUseOp::Frame(frame, inner) => {
            VirtualUseOp::Frame(frame.dupe(), Arc::new(mod_root_of_use_op(f, inner)))
        }
    }
}

pub fn mod_loc_of_virtual_use_op<L, M, F>(f: &F, use_op: VirtualUseOp<L>) -> VirtualUseOp<M>
where
    F: Fn(L) -> M + Clone,
    L: Clone + Dupe + PartialEq + Eq + PartialOrd + Ord,
    M: Clone + Dupe + PartialEq + Eq + PartialOrd + Ord,
{
    let mod_reason = |r: VirtualReason<L>| -> VirtualReason<M> { r.map_locs(|l: &L| f(l.dupe())) };

    fn mod_loc_of_root_use_op<L, M, F, FR, FU>(
        f: &F,
        mod_reason: &FR,
        mod_loc_of_virtual_use_op_rec: &FU,
        root: VirtualRootUseOp<L>,
    ) -> VirtualRootUseOp<M>
    where
        F: Fn(L) -> M + Clone,
        FR: Fn(VirtualReason<L>) -> VirtualReason<M>,
        FU: Fn(VirtualUseOp<L>) -> VirtualUseOp<M>,
        L: Dupe + PartialOrd + Ord + PartialEq + Eq,
        M: Dupe + PartialOrd + Ord + PartialEq + Eq,
    {
        use VirtualRootUseOp::*;

        match root {
            InitField(box InitFieldData { op, field, body }) => {
                InitField(Box::new(InitFieldData {
                    op: mod_reason(op),
                    field,
                    body: mod_reason(body),
                }))
            }
            ObjectAddComputedProperty { op } => ObjectAddComputedProperty { op: mod_reason(op) },
            ObjectSpread { op } => ObjectSpread { op: mod_reason(op) },
            ObjectRest { op } => ObjectRest { op: mod_reason(op) },
            ObjectChain { op } => ObjectChain { op: mod_reason(op) },
            AssignVar { var, init } => AssignVar {
                var: var.map(|box target| {
                    Box::new(match target {
                        AssignVarTarget::Variable(reason) => {
                            AssignVarTarget::Variable(mod_reason(reason))
                        }
                        AssignVarTarget::Annotation(t) => AssignVarTarget::Annotation(
                            type_or_type_desc::map_loc(|l: &L| f(l.dupe()), t),
                        ),
                        AssignVarTarget::Pattern { reason, annot } => AssignVarTarget::Pattern {
                            reason: mod_reason(reason),
                            annot: type_or_type_desc::map_loc(|l: &L| f(l.dupe()), annot),
                        },
                    })
                }),
                init: match init {
                    AssignVarInit::Reason(reason) => AssignVarInit::Reason(mod_reason(reason)),
                    AssignVarInit::Default(box AssignVarDefaultData { loc, default }) => {
                        AssignVarInit::Default(Box::new(AssignVarDefaultData {
                            loc: f(loc),
                            default: type_or_type_desc::map_loc(|l: &L| f(l.dupe()), default),
                        }))
                    }
                },
            },
            Cast {
                lower,
                upper: box upper,
            } => Cast {
                lower: mod_reason(lower),
                upper: Box::new(type_or_type_desc::map_loc(|l: &L| f(l.dupe()), upper)),
            },
            ClassExtendsCheck(box ClassExtendsCheckData {
                def_loc,
                def,
                extends_loc,
                extends,
            }) => ClassExtendsCheck(Box::new(ClassExtendsCheckData {
                def_loc: f(def_loc),
                def: type_or_type_desc::map_loc(|l: &L| f(l.dupe()), def),
                extends_loc: f(extends_loc),
                extends: type_or_type_desc::map_loc(|l: &L| f(l.dupe()), extends),
            })),
            ClassMethodDefinition(box ClassMethodDefinitionData {
                def_loc,
                method_name,
                name,
            }) => ClassMethodDefinition(Box::new(ClassMethodDefinitionData {
                def_loc: f(def_loc),
                method_name,
                name: mod_reason(name),
            })),
            ClassImplementsCheck(box ClassImplementsCheckData {
                def,
                name,
                implements,
            }) => ClassImplementsCheck(Box::new(ClassImplementsCheckData {
                def: mod_reason(def),
                name: mod_reason(name),
                implements: mod_reason(implements),
            })),
            ClassOwnProtoCheck(box ClassOwnProtoCheckData {
                own_loc,
                proto_loc,
                prop,
            }) => ClassOwnProtoCheck(Box::new(ClassOwnProtoCheckData {
                prop,
                own_loc: own_loc.map(f),
                proto_loc: proto_loc.map(f),
            })),
            Coercion { from, target } => Coercion {
                from: mod_reason(from),
                target: mod_reason(target),
            },
            ConformToCommonInterface(box ConformToCommonInterfaceData {
                self_sig_loc,
                self_module_loc,
                originate_from_import,
            }) => ConformToCommonInterface(Box::new(ConformToCommonInterfaceData {
                self_sig_loc: f(self_sig_loc),
                self_module_loc: f(self_module_loc),
                originate_from_import,
            })),
            MergedDeclaration(box MergedDeclarationData {
                conflict,
                first_decl_loc,
                first_decl,
                current_decl,
            }) => MergedDeclaration(Box::new(MergedDeclarationData {
                conflict: conflict.dupe(),
                first_decl_loc: f(first_decl_loc),
                first_decl: type_or_type_desc::map_loc(|l: &L| f(l.dupe()), first_decl),
                current_decl: mod_reason(current_decl),
            })),
            DeclareComponentRef { op } => DeclareComponentRef { op: mod_reason(op) },
            DeleteProperty { lhs, prop } => DeleteProperty {
                lhs: mod_reason(lhs),
                prop: mod_reason(prop),
            },
            DeleteVar { var } => DeleteVar {
                var: mod_reason(var),
            },
            FunCall(box FunCallData {
                op,
                fn_,
                args,
                implicit_instantiation_reference,
                local,
            }) => FunCall(Box::new(FunCallData {
                local,
                op: mod_reason(op),
                fn_: mod_reason(fn_),
                args: args.iter().map(|a| mod_reason(a.dupe())).collect(),
                implicit_instantiation_reference,
            })),
            FunCallMethod(box FunCallMethodData {
                op,
                fn_,
                args,
                prop,
                implicit_instantiation_reference,
                local,
            }) => FunCallMethod(Box::new(FunCallMethodData {
                local,
                op: mod_reason(op),
                fn_: match fn_ {
                    FunCallMethodCallee::Reason(reason) => {
                        FunCallMethodCallee::Reason(mod_reason(reason))
                    }
                    FunCallMethodCallee::Method(name) => FunCallMethodCallee::Method(name),
                },
                prop: mod_reason(prop),
                args: args.iter().map(|a| mod_reason(a.dupe())).collect(),
                implicit_instantiation_reference,
            })),
            FunReturnStatement { value } => FunReturnStatement {
                value: mod_reason(value),
            },
            FunImplicitReturn(box FunImplicitReturnData {
                fn_,
                upper,
                upper_t,
                type_guard,
            }) => FunImplicitReturn(Box::new(FunImplicitReturnData {
                fn_: mod_reason(fn_),
                upper: mod_reason(upper),
                upper_t: type_or_type_desc::map_loc(|l: &L| f(l.dupe()), upper_t),
                type_guard,
            })),
            GeneratorYield { value } => GeneratorYield {
                value: mod_reason(value),
            },
            GetExport(reason) => GetExport(mod_reason(reason)),
            GetProperty(reason) => GetProperty(mod_reason(reason)),
            IndexedTypeAccess(box IndexedTypeAccessData {
                index_loc,
                object,
                index,
            }) => IndexedTypeAccess(Box::new(IndexedTypeAccessData {
                index_loc: f(index_loc),
                object: type_or_type_desc::map_loc(|l: &L| f(l.dupe()), object),
                index: type_or_type_desc::map_loc(|l: &L| f(l.dupe()), index),
            })),
            InferBoundCompatibilityCheck { bound, infer } => InferBoundCompatibilityCheck {
                bound: mod_reason(bound),
                infer: mod_reason(infer),
            },
            JSXCreateElement(box JSXCreateElementData {
                op,
                component,
                implicit_instantiation_reference,
            }) => JSXCreateElement(Box::new(JSXCreateElementData {
                op: mod_reason(op),
                component: mod_reason(component),
                implicit_instantiation_reference,
            })),
            ReactCreateElementCall(box ReactCreateElementCallData {
                op,
                component,
                children,
                implicit_instantiation_reference,
            }) => ReactCreateElementCall(Box::new(ReactCreateElementCallData {
                op: mod_reason(op),
                component: mod_reason(component),
                children: f(children),
                implicit_instantiation_reference,
            })),
            ReactGetIntrinsic { literal } => ReactGetIntrinsic {
                literal: mod_reason(literal),
            },
            RecordCreate(box RecordCreateData {
                op,
                constructor,
                properties,
            }) => RecordCreate(Box::new(RecordCreateData {
                op: mod_reason(op),
                constructor: mod_reason(constructor),
                properties: f(properties),
            })),
            Speculation(op) => {
                Speculation(Arc::new(mod_loc_of_virtual_use_op_rec(op.as_ref().clone())))
            }
            TypeApplication { type_ } => TypeApplication {
                type_: match type_ {
                    TypeApplicationTarget::Reason(reason) => {
                        TypeApplicationTarget::Reason(mod_reason(reason))
                    }
                    TypeApplicationTarget::Typeof(box TypeofApplicationData { reason, name }) => {
                        TypeApplicationTarget::Typeof(Box::new(TypeofApplicationData {
                            reason: mod_reason(reason),
                            name,
                        }))
                    }
                },
            },
            MakeReadOnly { loc: box loc } => MakeReadOnly {
                loc: Box::new(f(loc)),
            },
            SetProperty(box SetPropertyData { lhs, prop, value }) => {
                SetProperty(Box::new(SetPropertyData {
                    lhs: mod_reason(lhs),
                    prop: mod_reason(prop),
                    value: mod_reason(value),
                }))
            }
            UpdateProperty { lhs, prop } => UpdateProperty {
                lhs: mod_reason(lhs),
                prop: mod_reason(prop),
            },
            RefinementCheck { test, discriminant } => RefinementCheck {
                test: mod_reason(test),
                discriminant: mod_reason(discriminant),
            },
            SwitchRefinementCheck(box SwitchRefinementCheckData { test, discriminant }) => {
                SwitchRefinementCheck(Box::new(SwitchRefinementCheckData {
                    test: f(test),
                    discriminant: f(discriminant),
                }))
            }
            EvalMappedType(box EvalMappedTypeData {
                mapped_type_loc,
                mapped_type,
            }) => EvalMappedType(Box::new(EvalMappedTypeData {
                mapped_type_loc: f(mapped_type_loc),
                mapped_type: type_or_type_desc::map_loc(|l: &L| f(l.dupe()), mapped_type),
            })),
            TypeGuardIncompatibility(box TypeGuardIncompatibilityData {
                guard_type_loc,
                guard_type_reference_loc,
                guard_type,
                param_name,
            }) => TypeGuardIncompatibility(Box::new(TypeGuardIncompatibilityData {
                guard_type_loc: f(guard_type_loc),
                guard_type_reference_loc: f(guard_type_reference_loc),
                guard_type: type_or_type_desc::map_loc(|l: &L| f(l.dupe()), guard_type),
                param_name,
            })),
            RenderTypeInstantiation { render_type } => RenderTypeInstantiation {
                render_type: mod_reason(render_type),
            },
            ComponentRestParamCompatibility(box ComponentRestParamCompatibilityData {
                rest_param_loc,
                rest_param,
            }) => ComponentRestParamCompatibility(Box::new(ComponentRestParamCompatibilityData {
                rest_param_loc: f(rest_param_loc),
                rest_param: type_or_type_desc::map_loc(|l: &L| f(l.dupe()), rest_param),
            })),
            PositiveTypeGuardConsistency(box PositiveTypeGuardConsistencyData {
                reason,
                return_reason,
                param_loc,
                param_name,
                guard_type_reference_loc,
                guard_type,
                is_return_false_statement,
            }) => PositiveTypeGuardConsistency(Box::new(PositiveTypeGuardConsistencyData {
                reason: mod_reason(reason),
                return_reason: mod_reason(return_reason),
                param_loc: f(param_loc),
                param_name,
                guard_type_reference_loc: f(guard_type_reference_loc),
                guard_type: type_or_type_desc::map_loc(|l: &L| f(l.dupe()), guard_type),
                is_return_false_statement,
            })),
            AssertionFunctionConsistency(box AssertionFunctionConsistencyData {
                exit_reason,
                param_loc,
                param_name,
                guard_type_reference_loc,
                guard_type,
                implicit_return,
            }) => AssertionFunctionConsistency(Box::new(AssertionFunctionConsistencyData {
                exit_reason: mod_reason(exit_reason),
                param_loc: f(param_loc),
                param_name,
                guard_type_reference_loc: f(guard_type_reference_loc),
                guard_type: type_or_type_desc::map_loc(|l: &L| f(l.dupe()), guard_type),
                implicit_return,
            })),
            UnknownUse => UnknownUse,
        }
    }

    fn mod_loc_of_frame_use_op<L, M, F, FR>(
        f: &F,
        mod_reason: &FR,
        frame: VirtualFrameUseOp<L>,
    ) -> VirtualFrameUseOp<M>
    where
        F: Fn(L) -> M + Clone,
        FR: Fn(VirtualReason<L>) -> VirtualReason<M>,
        L: Clone + Dupe + PartialEq + Eq + PartialOrd + Ord,
        M: Clone + Dupe + PartialEq + Eq + PartialOrd + Ord,
    {
        use VirtualFrameUseOp::*;

        match frame {
            ConstrainedAssignment(box ConstrainedAssignmentData {
                name,
                declaration,
                providers,
            }) => ConstrainedAssignment(Box::new(ConstrainedAssignmentData {
                name,
                declaration: f(declaration),
                providers: providers.iter().map(|p| f(p.dupe())).collect(),
            })),
            ReactDeepReadOnly(box (loc, l)) => ReactDeepReadOnly(Box::new((f(loc), l))),
            ArrayElementCompatibility(box ArrayElementCompatibilityData {
                lower,
                upper,
                lower_t,
                upper_t,
            }) => ArrayElementCompatibility(Box::new(ArrayElementCompatibilityData {
                lower: mod_reason(lower),
                upper: mod_reason(upper),
                lower_t: type_or_type_desc::map_loc(|l: &L| f(l.dupe()), lower_t),
                upper_t: type_or_type_desc::map_loc(|l: &L| f(l.dupe()), upper_t),
            })),
            FunCompatibility { lower, upper } => FunCompatibility {
                lower: mod_reason(lower),
                upper: mod_reason(upper),
            },
            StandaloneCallThis(box StandaloneCallThisData {
                op,
                fn_,
                receiver_loc,
                receiver: box receiver,
            }) => StandaloneCallThis(Box::new(StandaloneCallThisData {
                op: mod_reason(op),
                fn_: mod_reason(fn_),
                receiver_loc: f(receiver_loc),
                receiver: Box::new(type_or_type_desc::map_loc(|l: &L| f(l.dupe()), receiver)),
            })),
            FunMissingArg(box FunMissingArgData { n, op, def, def_t }) => {
                FunMissingArg(Box::new(FunMissingArgData {
                    n,
                    op: mod_reason(op),
                    def: mod_reason(def),
                    def_t: type_or_type_desc::map_loc(|l: &L| f(l.dupe()), def_t),
                }))
            }
            FunParam(box FunParamData {
                n,
                name,
                lower,
                upper,
                lower_t,
                upper_t,
            }) => FunParam(Box::new(FunParamData {
                n,
                name,
                lower: mod_reason(lower),
                upper: mod_reason(upper),
                lower_t: type_or_type_desc::map_loc(|l: &L| f(l.dupe()), lower_t),
                upper_t: type_or_type_desc::map_loc(|l: &L| f(l.dupe()), upper_t),
            })),
            FunRestParam { lower, upper } => FunRestParam {
                lower: mod_reason(lower),
                upper: mod_reason(upper),
            },
            FunReturn(box FunReturnData {
                lower,
                upper,
                lower_t,
                upper_t,
            }) => FunReturn(Box::new(FunReturnData {
                lower: mod_reason(lower),
                upper: mod_reason(upper),
                lower_t: type_or_type_desc::map_loc(|l: &L| f(l.dupe()), lower_t),
                upper_t: type_or_type_desc::map_loc(|l: &L| f(l.dupe()), upper_t),
            })),
            ImplicitTypeParam => ImplicitTypeParam,
            IndexerKeyCompatibility(box IndexerKeyCompatibilityData {
                lower,
                upper,
                lower_t,
                upper_t,
            }) => IndexerKeyCompatibility(Box::new(IndexerKeyCompatibilityData {
                lower: mod_reason(lower),
                upper: mod_reason(upper),
                lower_t: type_or_type_desc::map_loc(|l: &L| f(l.dupe()), lower_t),
                upper_t: type_or_type_desc::map_loc(|l: &L| f(l.dupe()), upper_t),
            })),
            OpaqueTypeLowerBoundCompatibility { lower, upper } => {
                OpaqueTypeLowerBoundCompatibility {
                    lower: mod_reason(lower),
                    upper: mod_reason(upper),
                }
            }
            OpaqueTypeUpperBoundCompatibility { lower, upper } => {
                OpaqueTypeUpperBoundCompatibility {
                    lower: mod_reason(lower),
                    upper: mod_reason(upper),
                }
            }
            OpaqueTypeCustomErrorCompatibility(box OpaqueTypeCustomErrorCompatibilityData {
                lower,
                upper,
                lower_t,
                upper_t,
                name,
                custom_error_loc,
            }) => OpaqueTypeCustomErrorCompatibility(Box::new(
                OpaqueTypeCustomErrorCompatibilityData {
                    lower: mod_reason(lower),
                    upper: mod_reason(upper),
                    lower_t: type_or_type_desc::map_loc(|l: &L| f(l.dupe()), lower_t),
                    upper_t: type_or_type_desc::map_loc(|l: &L| f(l.dupe()), upper_t),
                    name,
                    custom_error_loc: f(custom_error_loc),
                },
            )),
            MappedTypeKeyCompatibility(box MappedTypeKeyCompatibilityData {
                source_type_loc,
                source_type,
                mapped_type_loc,
                mapped_type,
            }) => MappedTypeKeyCompatibility(Box::new(MappedTypeKeyCompatibilityData {
                source_type_loc: f(source_type_loc),
                source_type: type_or_type_desc::map_loc(|l: &L| f(l.dupe()), source_type),
                mapped_type_loc: f(mapped_type_loc),
                mapped_type: type_or_type_desc::map_loc(|l: &L| f(l.dupe()), mapped_type),
            })),
            PropertyCompatibility(box PropertyCompatibilityData {
                prop,
                lower,
                upper,
                lower_t,
                upper_t,
            }) => PropertyCompatibility(Box::new(PropertyCompatibilityData {
                prop,
                lower: mod_reason(lower),
                upper: mod_reason(upper),
                lower_t: type_or_type_desc::map_loc(|l: &L| f(l.dupe()), lower_t),
                upper_t: type_or_type_desc::map_loc(|l: &L| f(l.dupe()), upper_t),
            })),
            ReactConfigCheck => ReactConfigCheck,
            ReactGetConfig { polarity } => ReactGetConfig { polarity },
            TupleElementCompatibility(box TupleElementCompatibilityData {
                n,
                lower,
                upper,
                lower_t,
                upper_t,
            }) => TupleElementCompatibility(Box::new(TupleElementCompatibilityData {
                n,
                lower: mod_reason(lower),
                upper: mod_reason(upper),
                lower_t: type_or_type_desc::map_loc(|l: &L| f(l.dupe()), lower_t),
                upper_t: type_or_type_desc::map_loc(|l: &L| f(l.dupe()), upper_t),
            })),
            TupleAssignment => TupleAssignment,
            TypeArgCompatibility(box TypeArgCompatibilityData {
                name,
                targ,
                lower,
                upper,
                polarity,
            }) => TypeArgCompatibility(Box::new(TypeArgCompatibilityData {
                name,
                polarity,
                targ: mod_reason(targ),
                lower: mod_reason(lower),
                upper: mod_reason(upper),
            })),
            TypeParamBound { name } => TypeParamBound { name },
            OpaqueTypeLowerBound { opaque_t_reason } => OpaqueTypeLowerBound {
                opaque_t_reason: mod_reason(opaque_t_reason),
            },
            OpaqueTypeUpperBound { opaque_t_reason } => OpaqueTypeUpperBound {
                opaque_t_reason: mod_reason(opaque_t_reason),
            },
            TypeGuardCompatibility => TypeGuardCompatibility,
            RendersCompatibility => RendersCompatibility,
            UnifyFlip => UnifyFlip,
            EnumRepresentationTypeCompatibility { lower, upper } => {
                EnumRepresentationTypeCompatibility {
                    lower: mod_reason(lower),
                    upper: mod_reason(upper),
                }
            }
            UnionRepresentative(box UnionRepresentativeData {
                loc,
                reference_loc,
                union,
            }) => UnionRepresentative(Box::new(UnionRepresentativeData {
                loc: f(loc),
                reference_loc: f(reference_loc),
                union: type_or_type_desc::map_loc(|l: &L| f(l.dupe()), union),
            })),
        }
    }

    let mod_loc_of_virtual_use_op_rec =
        |op: VirtualUseOp<L>| -> VirtualUseOp<M> { mod_loc_of_virtual_use_op(f, op) };

    match use_op {
        VirtualUseOp::Op(root) => VirtualUseOp::Op(Arc::new(mod_loc_of_root_use_op(
            f,
            &mod_reason,
            &mod_loc_of_virtual_use_op_rec,
            root.as_ref().clone(),
        ))),
        VirtualUseOp::Frame(frame, inner_op) => VirtualUseOp::Frame(
            Arc::new(mod_loc_of_frame_use_op(
                f,
                &mod_reason,
                frame.as_ref().clone(),
            )),
            Arc::new(mod_loc_of_virtual_use_op_rec(inner_op.as_ref().clone())),
        ),
    }
}

/// Fast-path comparison for `TypeInner` pairs whose `swap_reason` would
/// only differ from `t2` in the top-level `Reason` field. For such
/// variants, comparing the non-reason fields directly via derived `Ord`
/// gives the same answer without allocating a temporary `Type`.
///
/// Returns `None` for variants whose nested data also carries reasons
/// that some `swap_reason` implementations recursively swap (`MaybeT`,
/// `OptionalT`, `NominalT`); callers handle those with their own
/// `swap_reason`-specific recursion.
///
/// Caller must have already verified that `inner1` and `inner2` have the
/// same discriminant.
///
/// Both `reasonless_compare` (here) and
/// `flow_typing_flow_common::concrete_type_eq::eq_swap_reason` share this
/// fast path so the same set of variants benefits in either call site.
/// Profile on heavy `keyof T`/`T[K]` workloads showed ~25% of CPU spent
/// in `mod_reason_of_t` + `Rc::drop_slow` from the OCaml-style
/// `swap_reason` allocation; the OCaml original tolerates the alloc
/// because of bump-GC, but Rc heap traffic is far more expensive.
pub fn reasonless_cmp_inner_fast(
    inner1: &TypeInner,
    inner2: &TypeInner,
) -> Option<std::cmp::Ordering> {
    use crate::type_::TypeInner::AnnotT;
    use crate::type_::TypeInner::AnyT;
    use crate::type_::TypeInner::DefT;
    use crate::type_::TypeInner::FunProtoT;
    use crate::type_::TypeInner::IntersectionT;
    use crate::type_::TypeInner::KeysT;
    use crate::type_::TypeInner::NullProtoT;
    use crate::type_::TypeInner::ObjProtoT;
    use crate::type_::TypeInner::OpenT;
    use crate::type_::TypeInner::TemplateLiteralT;
    use crate::type_::TypeInner::UnionT;
    match (inner1, inner2) {
        (DefT(_, d1), DefT(_, d2)) => Some(d1.cmp(d2)),
        (UnionT(_, r1), UnionT(_, r2)) => Some(r1.cmp(r2)),
        (IntersectionT(_, r1), IntersectionT(_, r2)) => Some(r1.cmp(r2)),
        (OpenT(tv1), OpenT(tv2)) => Some(tv1.id().cmp(&tv2.id())),
        (AnyT(_, s1), AnyT(_, s2)) => Some(s1.cmp(s2)),
        (KeysT(_, ity1), KeysT(_, ity2)) => Some(ity1.cmp(ity2)),
        (AnnotT(_, ity1, b1), AnnotT(_, ity2, b2)) => Some(ity1.cmp(ity2).then_with(|| b1.cmp(b2))),
        (
            TemplateLiteralT {
                quasis: q1,
                types: t1,
                ..
            },
            TemplateLiteralT {
                quasis: q2,
                types: t2,
                ..
            },
        ) => Some(q1.cmp(q2).then_with(|| t1.cmp(t2))),
        (FunProtoT(_), FunProtoT(_))
        | (NullProtoT(_), NullProtoT(_))
        | (ObjProtoT(_), ObjProtoT(_)) => Some(std::cmp::Ordering::Equal),
        _ => None,
    }
}

// type comparison mod reason
fn reasonless_compare(t1: &Type, t2: &Type) -> std::cmp::Ordering {
    if std::ptr::eq(t1.deref(), t2.deref()) {
        return std::cmp::Ordering::Equal;
    }

    // Different discriminants? Reasons are irrelevant; the discriminant order
    // (per the derived `Ord` on `TypeInner`) determines the result, with no
    // allocation needed.
    let inner1 = t1.deref();
    let inner2 = t2.deref();
    if std::mem::discriminant(inner1) != std::mem::discriminant(inner2) {
        return inner1.cmp(inner2);
    }

    // Same discriminant: shared fast paths via `reasonless_cmp_inner_fast`
    // for variants where ignoring the top-level reason gives the same
    // `Ordering` without allocating a temporary `Type`.
    if let Some(ord) = reasonless_cmp_inner_fast(inner1, inner2) {
        return ord;
    }

    // The local `swap_reason` below recurses through `MaybeT`/`OptionalT`
    // but treats `NominalT` as opaque (top-level reason swap), so derived
    // `Ord` on `NominalType` gives the right answer for that case without
    // allocating either.
    if let (
        TypeInner::NominalT {
            nominal_type: n1, ..
        },
        TypeInner::NominalT {
            nominal_type: n2, ..
        },
    ) = (inner1, inner2)
    {
        return n1.cmp(n2);
    }

    // Fallback (rare): mirrors the OCaml local `swap_reason` for `MaybeT`/
    // `OptionalT` (which need a recursive swap) and for variants whose
    // nested data carries reasons we'd otherwise have to walk by hand.
    fn swap_reason(t2: &Type, t1: &Type) -> Type {
        match (t2.deref(), t1.deref()) {
            // In reposition we also recurse and reposition some nested types. We need
            // to make sure we swap the types for these reasons as well. Otherwise our
            // optimized union ~> union check will not pass.
            (TypeInner::MaybeT(_, inner_t2), TypeInner::MaybeT(r, inner_t1)) => {
                Type::new(TypeInner::MaybeT(r.dupe(), swap_reason(inner_t2, inner_t1)))
            }
            (
                TypeInner::OptionalT {
                    reason: _,
                    type_: inner_t2,
                    use_desc: _,
                },
                TypeInner::OptionalT {
                    reason,
                    type_: inner_t1,
                    use_desc,
                },
            ) => Type::new(TypeInner::OptionalT {
                reason: reason.dupe(),
                type_: swap_reason(inner_t2, inner_t1),
                use_desc: *use_desc,
            }),
            _ => mod_reason_of_t(&|_| reason_of_t(t1).dupe(), t2),
        }
    }

    let swapped = swap_reason(t2, t1);
    t1.cmp(&swapped)
}

fn reasonless_eq(t1: &Type, t2: &Type) -> bool {
    reasonless_compare(t1, t2) == std::cmp::Ordering::Equal
}

// type exemplar set - reasons are not considered in compare
pub mod type_ex_set {
    use std::collections::BTreeSet;

    use super::*;

    #[derive(Clone)]
    pub struct TypeEx(pub Type);

    impl PartialEq for TypeEx {
        fn eq(&self, other: &Self) -> bool {
            reasonless_eq(&self.0, &other.0)
        }
    }

    impl Eq for TypeEx {}

    impl PartialOrd for TypeEx {
        fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
            Some(self.cmp(other))
        }
    }

    impl Ord for TypeEx {
        fn cmp(&self, other: &Self) -> std::cmp::Ordering {
            reasonless_compare(&self.0, &other.0)
        }
    }

    pub type TypeExSet = BTreeSet<TypeEx>;

    pub fn empty() -> TypeExSet {
        BTreeSet::new()
    }

    pub fn singleton(t: Type) -> TypeExSet {
        let mut set = BTreeSet::new();
        set.insert(TypeEx(t));
        set
    }

    pub fn mem(t: &Type, set: &TypeExSet) -> bool {
        set.contains(&TypeEx(t.dupe()))
    }

    pub fn add(t: Type, set: &mut TypeExSet) {
        set.insert(TypeEx(t));
    }
}

pub fn nominal_id_have_same_logical_module(
    file_options: &flow_common::files::FileOptions,
    _projects_options: &flow_common::flow_projects::ProjectsOptions,
    (a_id, a_name): (&ALocId, Option<&str>),
    (b_id, b_name): (&ALocId, Option<&str>),
) -> bool {
    use flow_common::files;
    use flow_parser::file_key::FileKey;
    use flow_parser::file_key::FileKeyInner;

    let haste_name_opt = |file: &FileKey| files::haste_name_opt(file_options, file);

    let matching_platform_specific_impl_and_interface_file_key =
        |a_src: &FileKey, b_src: &FileKey| -> bool {
            // A.js.flow, A.ios.js in the same directory
            files::has_declaration_ext(a_src)
                && files::chop_declaration_ext(a_src)
                    == files::chop_platform_suffix_for_file(file_options, b_src)
                || {
                    // Regardless of which namespace the Haste module has or what platform they have, if they have
                    // the same name, we assume it's the same logical module. It's impossible to happen in normal
                    // circumstances due to uniqueness guarantee. It's only possible to happen during multiplatform
                    // conformance check, but in this case we already enforced uniqueness guarantee elsewhere.
                    a_src != b_src
                        && match (haste_name_opt(a_src), haste_name_opt(b_src)) {
                            (Some(n1), Some(n2)) => {
                                let a_file =
                                    FileKey::new(FileKeyInner::SourceFile(format!("{}.js", n1)));
                                let b_file =
                                    FileKey::new(FileKeyInner::SourceFile(format!("{}.js", n2)));
                                files::chop_platform_suffix_for_file(file_options, &a_file)
                                    == files::chop_platform_suffix_for_file(file_options, &b_file)
                            }
                            _ => false,
                        }
                }
        };

    match (a_name, b_name, a_id.0.source(), b_id.0.source()) {
        (Some(a_name), Some(b_name), Some(a_src), Some(b_src)) => {
            a_name == b_name
                && (matching_platform_specific_impl_and_interface_file_key(a_src, b_src)
                    || matching_platform_specific_impl_and_interface_file_key(b_src, a_src))
        }
        _ => false,
    }
}

pub fn is_falsy(t: &Type) -> bool {
    use crate::type_::DefTInner as D;
    use crate::type_::EnumInfoInner;
    match t.deref() {
        TypeInner::DefT(_, def_t) => match &**def_t {
            D::NullT | D::VoidT => true,
            D::SingletonBoolT { value: false, .. } => true,
            D::EnumValueT(info) => match info.deref().deref() {
                EnumInfoInner::ConcreteEnum(concrete) => match concrete.representation_t.deref() {
                    TypeInner::DefT(_, inner_def_t) => {
                        matches!(&**inner_def_t, D::SingletonBoolT { value: false, .. })
                    }
                    _ => false,
                },
                EnumInfoInner::AbstractEnum { representation_t } => {
                    match representation_t.deref() {
                        TypeInner::DefT(_, inner_def_t) => {
                            matches!(&**inner_def_t, D::SingletonBoolT { value: false, .. })
                        }
                        _ => false,
                    }
                }
            },
            D::SingletonStrT { value, .. } => value.as_str() == "",
            D::SingletonNumT { value, .. } => value.0 == 0.0,
            _ => false,
        },
        _ => false,
    }
}

// We use this predicate below to defensively prevent some shortcuts taken when
// we might not have enough information about the type in the LHS.
pub fn is_concrete(t: &Type) -> bool {
    match t.deref() {
        TypeInner::OpenT(_)
        | TypeInner::ImplicitInstantiationTvar(_)
        | TypeInner::EvalT { .. }
        | TypeInner::TypeAppT(box TypeAppTData { .. })
        | TypeInner::KeysT(_, _)
        | TypeInner::IntersectionT(_, _)
        | TypeInner::UnionT(_, _)
        | TypeInner::NominalT { .. } => false,
        _ => true,
    }
}

pub fn is_mixed_subtype(l: &Type, mixed_flavor: MixedFlavor) -> bool {
    use MixedFlavor::*;

    use crate::type_::DefTInner as D;

    // Helper to check if DefT matches a pattern
    fn is_def_t_match<F>(t: &Type, f: F) -> bool
    where
        F: FnOnce(&D) -> bool,
    {
        match t.deref() {
            TypeInner::DefT(_, def_t) => f(def_t),
            _ => false,
        }
    }

    match l.deref() {
        TypeInner::DefT(_, def_t) if matches!(&**def_t, D::MixedT(flavor) if *flavor == mixed_flavor) => {
            true
        }
        TypeInner::OptionalT { .. } if matches!(mixed_flavor, MixedNonMaybe | MixedNonVoid) => {
            false
        }
        TypeInner::MaybeT(_, _)
            if matches!(mixed_flavor, MixedNonMaybe | MixedNonVoid | MixedNonNull) =>
        {
            false
        }
        TypeInner::DefT(_, def_t)
            if matches!(&**def_t, D::NullT)
                && matches!(mixed_flavor, MixedNonMaybe | MixedNonNull) =>
        {
            false
        }
        TypeInner::DefT(_, def_t)
            if matches!(&**def_t, D::VoidT)
                && matches!(mixed_flavor, MixedNonMaybe | MixedNonVoid) =>
        {
            false
        }
        _ if matches!(mixed_flavor, MixedNonMaybe | MixedNonNull | MixedNonVoid) => is_concrete(l),
        TypeInner::DefT(_, def_t)
            if matches!(&**def_t, D::FunT(_, _)) && matches!(mixed_flavor, MixedFunction) =>
        {
            true
        }
        TypeInner::DefT(_, def_t)
            if matches!(&**def_t, D::PolyT(_)) && matches!(mixed_flavor, MixedFunction) =>
        {
            if let D::PolyT(poly_t_data) = &**def_t {
                let t_out = &poly_t_data.t_out;
                is_def_t_match(t_out, |inner| matches!(inner, D::FunT(_, _)))
            } else {
                false
            }
        }
        _ if matches!(mixed_flavor, MixedFunction) => false,
        _ if matches!(mixed_flavor, MixedTruthy) => is_concrete(l) && !is_falsy(l),
        _ if matches!(mixed_flavor, MixedEverything) => true,
        _ => false,
    }
}

fn ground_subtype<F>(on_singleton_eq: &mut F, l: &Type, u: &Type) -> bool
where
    F: FnMut(&Type),
{
    use crate::type_::DefTInner as D;
    use crate::type_::Literal;

    match (l.deref(), u.deref()) {
        (TypeInner::OpenT(_) | TypeInner::ImplicitInstantiationTvar(_), _)
        | (_, TypeInner::OpenT(_) | TypeInner::ImplicitInstantiationTvar(_))
        | (TypeInner::UnionT(_, _), _) => false,

        (TypeInner::DefT(_, l_def), TypeInner::DefT(_, u_def))
            if matches!(&**l_def, D::NumGeneralT(_) | D::SingletonNumT { .. })
                && matches!(&**u_def, D::NumGeneralT(_)) =>
        {
            true
        }

        (TypeInner::DefT(_, l_def), TypeInner::DefT(_, u_def))
            if matches!(&**l_def, D::StrGeneralT(_) | D::SingletonStrT { .. })
                && matches!(&**u_def, D::StrGeneralT(_)) =>
        {
            true
        }

        (TypeInner::DefT(_, l_def), TypeInner::DefT(_, u_def))
            if matches!(&**l_def, D::BoolGeneralT | D::SingletonBoolT { .. })
                && matches!(&**u_def, D::BoolGeneralT) =>
        {
            true
        }

        (TypeInner::DefT(_, l_def), TypeInner::DefT(_, u_def))
            if matches!(&**l_def, D::BigIntGeneralT(_) | D::SingletonBigIntT { .. })
                && matches!(&**u_def, D::BigIntGeneralT(_)) =>
        {
            true
        }

        (TypeInner::DefT(_, l_def), TypeInner::DefT(_, u_def))
            if matches!(
                (&**l_def, &**u_def),
                (D::SymbolT | D::UniqueSymbolT(_), D::SymbolT)
            ) =>
        {
            true
        }
        (TypeInner::DefT(_, l_def), TypeInner::DefT(_, u_def))
            if matches!((&**l_def, &**u_def), (D::NullT, D::NullT)) =>
        {
            true
        }
        (TypeInner::DefT(_, l_def), TypeInner::DefT(_, u_def))
            if matches!((&**l_def, &**u_def), (D::VoidT, D::VoidT)) =>
        {
            true
        }

        (TypeInner::DefT(_, l_def), TypeInner::DefT(_, u_def)) => match (&**l_def, &**u_def) {
            (
                D::SingletonStrT { value: actual, .. },
                D::SingletonStrT {
                    value: expected, ..
                },
            ) => {
                let result = expected == actual;
                if result {
                    on_singleton_eq(l);
                }
                result
            }
            (
                D::SingletonNumT { value: actual, .. },
                D::SingletonNumT {
                    value: expected, ..
                },
            ) => {
                let result = expected.0 == actual.0;
                if result {
                    on_singleton_eq(l);
                }
                result
            }
            (
                D::SingletonBoolT { value: actual, .. },
                D::SingletonBoolT {
                    value: expected, ..
                },
            ) => {
                let result = expected == actual;
                if result {
                    on_singleton_eq(l);
                }
                result
            }
            (
                D::SingletonBigIntT { value: actual, .. },
                D::SingletonBigIntT {
                    value: expected, ..
                },
            ) => {
                let result = expected.0 == actual.0;
                if result {
                    on_singleton_eq(l);
                }
                result
            }
            (D::NumericStrKeyT(_), D::NumGeneralT(_) | D::StrGeneralT(_)) => true,
            (
                D::NumericStrKeyT(actual),
                D::SingletonNumT {
                    value: expected, ..
                },
            ) => actual.0 == expected.0,
            (
                D::NumericStrKeyT(actual),
                D::SingletonStrT {
                    value: expected, ..
                },
            ) => actual.1.as_str() == expected.as_str(),
            (D::UniqueSymbolT(data1), D::UniqueSymbolT(data2)) => data1.symbol == data2.symbol,
            (_, D::MixedT(mixed_flavor)) => is_mixed_subtype(l, mixed_flavor.clone()),
            _ => false,
        },

        (TypeInner::TemplateLiteralT { quasis, .. }, TypeInner::DefT(_, u_def))
            if matches!(&**u_def, D::StrGeneralT(Literal::Truthy))
                && quasis.iter().any(|q| !q.is_empty()) =>
        {
            true
        }
        (TypeInner::TemplateLiteralT { .. }, TypeInner::DefT(_, u_def))
            if matches!(&**u_def, D::StrGeneralT(Literal::AnyLiteral)) =>
        {
            true
        }

        // SingletonStrT <: `${prefix}${string}`
        (TypeInner::DefT(_, l_def), TypeInner::TemplateLiteralT { quasis, types, .. })
            if matches!(&**l_def, D::SingletonStrT { .. })
                && quasis.len() == 2
                && quasis[1].is_empty()
                && types.len() == 1
                && matches!(types[0].deref(), TypeInner::DefT(_, d) if matches!(&**d, D::StrGeneralT(_)))
                && matches!(&**l_def, D::SingletonStrT { value, .. } if value.as_str().starts_with(quasis[0].as_str())) =>
        {
            on_singleton_eq(l);
            true
        }
        // SingletonStrT <: `${string}${suffix}`
        (TypeInner::DefT(_, l_def), TypeInner::TemplateLiteralT { quasis, types, .. })
            if matches!(&**l_def, D::SingletonStrT { .. })
                && quasis.len() == 2
                && quasis[0].is_empty()
                && types.len() == 1
                && matches!(types[0].deref(), TypeInner::DefT(_, d) if matches!(&**d, D::StrGeneralT(_)))
                && matches!(&**l_def, D::SingletonStrT { value, .. } if value.as_str().ends_with(quasis[1].as_str())) =>
        {
            on_singleton_eq(l);
            true
        }
        // Prefix-extends-prefix: `${p1}${string}` <: `${p2}${string}` when p1 starts with p2
        (
            TypeInner::TemplateLiteralT {
                quasis: l_quasis, ..
            },
            TypeInner::TemplateLiteralT {
                quasis: u_quasis,
                types: u_types,
                ..
            },
        ) if l_quasis.len() == 2
            && l_quasis[1].is_empty()
            && u_quasis.len() == 2
            && u_quasis[1].is_empty()
            && u_types.len() == 1
            && matches!(u_types[0].deref(), TypeInner::DefT(_, d) if matches!(&**d, D::StrGeneralT(_)))
            && l_quasis[0].as_str().starts_with(u_quasis[0].as_str()) =>
        {
            on_singleton_eq(l);
            true
        }
        // Suffix-extends-suffix mirror
        (
            TypeInner::TemplateLiteralT {
                quasis: l_quasis, ..
            },
            TypeInner::TemplateLiteralT {
                quasis: u_quasis,
                types: u_types,
                ..
            },
        ) if l_quasis.len() == 2
            && l_quasis[0].is_empty()
            && u_quasis.len() == 2
            && u_quasis[0].is_empty()
            && u_types.len() == 1
            && matches!(u_types[0].deref(), TypeInner::DefT(_, d) if matches!(&**d, D::StrGeneralT(_)))
            && l_quasis[1].as_str().ends_with(u_quasis[1].as_str()) =>
        {
            on_singleton_eq(l);
            true
        }

        (_, TypeInner::DefT(_, u_def)) if matches!(&**u_def, D::MixedT(..)) => {
            if let D::MixedT(mixed_flavor) = &**u_def {
                is_mixed_subtype(l, mixed_flavor.clone())
            } else {
                false
            }
        }

        (TypeInner::AnyT(_, _), _) | (_, TypeInner::AnyT(_, _)) => false,
        (TypeInner::ObjProtoT(_), TypeInner::ObjProtoT(_))
        | (TypeInner::FunProtoT(_), TypeInner::FunProtoT(_))
        | (TypeInner::FunProtoT(_), TypeInner::ObjProtoT(_)) => true,
        (TypeInner::DefT(_, l_def), TypeInner::ObjProtoT(_)) => {
            if let D::ObjT(obj) = l_def.deref() {
                matches!(
                    obj.proto_t.deref(),
                    TypeInner::ObjProtoT(_) | TypeInner::FunProtoT(_)
                )
            } else {
                false
            }
        }
        (TypeInner::DefT(_, l_def), TypeInner::FunProtoT(_)) => {
            if let D::ObjT(obj) = l_def.deref() {
                matches!(obj.proto_t.deref(), TypeInner::FunProtoT(_))
            } else {
                false
            }
        }
        _ => false,
    }
}

pub fn ground_subtype_use_t<F, CX: ContextFamily>(
    on_singleton_eq: &mut F,
    l: &Type,
    u: &UseT<CX>,
) -> bool
where
    F: FnMut(&Type),
{
    match u.deref() {
        UseTInner::UseT(_, t) => ground_subtype(on_singleton_eq, l, t),
        _ => false,
    }
}

pub fn quick_subtype<F>(on_singleton_eq: Option<&mut F>, t1: &Type, t2: &Type) -> bool
where
    F: FnMut(&Type),
{
    use crate::type_::DefTInner as D;

    let check_ground = match on_singleton_eq {
        Some(f) => ground_subtype(f, t1, t2),
        None => ground_subtype(&mut |_: &Type| {}, t1, t2),
    };

    if check_ground {
        return true;
    }
    match (t1.deref(), t2.deref()) {
        (TypeInner::DefT(_, def1), _) if matches!(&**def1, D::EmptyT) => true,
        (TypeInner::DefT(_, def1), TypeInner::DefT(_, def2))
            if matches!(&**def1, D::StrGeneralT(_))
                && matches!(&**def2, D::SingletonStrT { .. }) =>
        {
            false
        }
        (TypeInner::DefT(_, def1), TypeInner::DefT(_, def2))
            if matches!(&**def1, D::NumGeneralT(_))
                && matches!(&**def2, D::SingletonNumT { .. }) =>
        {
            false
        }
        (TypeInner::DefT(_, def1), TypeInner::DefT(_, def2))
            if matches!(&**def1, D::BigIntGeneralT(_))
                && matches!(&**def2, D::SingletonBigIntT { .. }) =>
        {
            false
        }
        (TypeInner::DefT(_, def1), TypeInner::DefT(_, def2))
            if matches!(&**def1, D::BoolGeneralT)
                && matches!(&**def2, D::SingletonBoolT { .. }) =>
        {
            false
        }
        _ => reasonless_eq(t1, t2),
    }
}

pub fn reason_of_propref(propref: &crate::type_::PropRef) -> &Reason {
    use crate::type_::PropRef;
    match propref {
        PropRef::Named { reason, .. } => reason,
        PropRef::Computed(t) => reason_of_t(t),
    }
}

pub fn mk_named_prop(
    reason: Reason,
    from_indexed_access: bool,
    name: flow_common::reason::Name,
) -> crate::type_::PropRef {
    crate::type_::PropRef::Named {
        reason,
        name,
        from_indexed_access,
    }
}

pub fn optional(t: Type, annot_loc: Option<ALoc>, use_desc: bool) -> Type {
    let reason = reason_of_t(&t).dupe();
    let reason = match annot_loc {
        Some(loc) => reason.reposition_and_annotate(loc),
        None => reason,
    };
    Type::new(TypeInner::OptionalT {
        reason,
        type_: t,
        use_desc,
    })
}

pub fn maybe(t: Type) -> Type {
    let reason = reason_of_t(&t).dupe();
    Type::new(TypeInner::MaybeT(reason, t))
}

pub fn make_exact_object(
    reason_obj: Reason,
    obj: Rc<crate::type_::ObjType>,
    reason_op: &Reason,
) -> Type {
    use flow_common::reason::VirtualReasonDesc::RTypeAlias;

    use crate::type_::DefT;
    use crate::type_::DefTInner;
    use crate::type_::Flags;
    use crate::type_::ObjKind;
    use crate::type_::ObjType;

    let obj_kind = match obj.flags.obj_kind {
        ObjKind::Inexact => ObjKind::Exact,
        ref k => k.clone(),
    };

    // This case analysis aims at recovering a potential type alias associated
    // with an $Exact<> constructor.
    let reason_obj = match reason_op.desc(false) {
        Some(RTypeAlias(box (n, loc, _))) => {
            let n = n.dupe();
            let loc = loc.dupe();
            reason_obj.update_desc(|desc| match &desc {
                Some(RTypeAlias(box (_, _, inner_desc))) => {
                    Some(flow_common::reason::VirtualReasonDesc::RTypeAlias(
                        Box::new((n.dupe(), loc.dupe(), inner_desc.clone())),
                    ))
                }
                _ => Some(flow_common::reason::VirtualReasonDesc::RTypeAlias(
                    Box::new((n.dupe(), loc.dupe(), desc.map(Arc::new))),
                )),
            })
        }
        _ => {
            // If r is an RTypeAlias, then this alias is no longer valid.
            reason_obj.update_desc(|desc| {
                desc.map(flow_common::reason::VirtualReasonDesc::invalidate_rtype_alias)
            })
        }
    };

    Type::new(TypeInner::DefT(
        reason_obj,
        DefT::new(DefTInner::ObjT(Rc::new(ObjType {
            flags: Flags {
                obj_kind,
                react_dro: obj.flags.react_dro.clone(),
            },
            props_tmap: obj.props_tmap.dupe(),
            proto_t: obj.proto_t.dupe(),
            call_t: obj.call_t,
            reachable_targs: obj.reachable_targs.dupe(),
            strictness_kind: obj.strictness_kind,
            literal: obj.literal,
        }))),
    ))
}

pub fn statics(t: &Type) -> Option<&Type> {
    use crate::type_::DefTInner;

    match t.deref() {
        TypeInner::DefT(_, def) => match def.deref() {
            DefTInner::FunT(statics, _) => Some(statics),
            DefTInner::ClassT(t) => statics(t),
            DefTInner::PolyT(poly) => statics(&poly.t_out),
            DefTInner::InstanceT(instance) => Some(&instance.static_),
            _ => None,
        },
        TypeInner::ThisInstanceT(instance) => Some(&instance.instance.static_),
        _ => None,
    }
}

pub fn class_type(t: Type, annot_loc: Option<ALoc>) -> Type {
    use crate::type_::DefT;
    use crate::type_::DefTInner;

    let reason = reason_of_t(&t).dupe();
    let reason = match annot_loc {
        Some(loc) => reason.reposition_and_annotate(loc),
        None => reason,
    };
    Type::new(TypeInner::DefT(reason, DefT::new(DefTInner::ClassT(t))))
}

pub fn extends_use_type<CX: ContextFamily>(use_op: UseOp, l: Type, u: Type) -> UseT<CX> {
    let reason = reason_of_t(&u).dupe().update_desc_new(|desc| desc);
    UseT::new(UseTInner::ExtendsUseT(Box::new(ExtendsUseTData {
        use_op,
        reason,
        targs: Vec::new().into(),
        true_t: l,
        false_t: u,
    })))
}

pub fn poly_type(
    id: crate::type_::poly::Id,
    tparams_loc: ALoc,
    tparams: vec1::Vec1<crate::type_::TypeParam>,
    t: Type,
    strictness_kind: crate::type_::TypeStrictnessKind,
    poly_kind: PolyTKind,
) -> Type {
    use crate::type_::DefT;
    use crate::type_::DefTInner;

    let reason = reason_of_t(&t).dupe();
    let annot_loc = reason.loc().dupe();
    let reason = reason.annotate(annot_loc);
    Type::new(TypeInner::DefT(
        reason,
        DefT::new(DefTInner::PolyT(Box::new(PolyTData {
            kind: poly_kind,
            tparams_loc,
            tparams: tparams.into_vec().into(),
            t_out: t,
            id,
            strictness_kind,
        }))),
    ))
}

pub fn poly_type_of_tparam_list(
    id: crate::type_::poly::Id,
    tparams_loc: ALoc,
    tparams: Rc<[crate::type_::TypeParam]>,
    t: Type,
    strictness_kind: crate::type_::TypeStrictnessKind,
    poly_kind: PolyTKind,
) -> Type {
    match vec1::Vec1::try_from_vec(tparams.to_vec()) {
        Err(_) => t,
        Ok(tparams_nel) => poly_type(id, tparams_loc, tparams_nel, t, strictness_kind, poly_kind),
    }
}

pub fn poly_type_of_tparams(
    id: crate::type_::poly::Id,
    tparams: crate::type_::TypeParams,
    t: Type,
    strictness_kind: crate::type_::TypeStrictnessKind,
    poly_kind: PolyTKind,
) -> Type {
    match tparams {
        None => t,
        Some((tparams_loc, tparams_nel)) => {
            poly_type(id, tparams_loc, tparams_nel, t, strictness_kind, poly_kind)
        }
    }
}

pub fn typeapp_with_use_op(
    from_value: bool,
    use_desc: bool,
    reason: Reason,
    use_op: UseOp,
    t: Type,
    targs: Vec<Type>,
) -> Type {
    let reason = reason.replace_desc(desc_of_t(&t).cloned());
    Type::new(TypeInner::TypeAppT(Box::new(TypeAppTData {
        reason,
        use_op,
        type_: t,
        targs: targs.into(),
        from_value,
        use_desc,
    })))
}

pub fn typeapp(
    from_value: bool,
    use_desc: bool,
    reason: Reason,
    t: Type,
    targs: Vec<Type>,
) -> Type {
    let use_op = UseOp::Op(Arc::new(RootUseOp::TypeApplication {
        type_: TypeApplicationTarget::Reason(reason.dupe()),
    }));
    typeapp_with_use_op(from_value, use_desc, reason, use_op, t, targs)
}

pub fn typeapp_annot(
    from_value: bool,
    use_desc: bool,
    loc: ALoc,
    t: Type,
    targs: Vec<Type>,
) -> Type {
    use flow_common::reason::mk_annot_reason;

    let desc = desc_of_t(&t).cloned();
    let reason = mk_annot_reason(desc, loc);
    let use_op = UseOp::Op(Arc::new(RootUseOp::TypeApplication {
        type_: TypeApplicationTarget::Reason(reason.dupe()),
    }));
    Type::new(TypeInner::TypeAppT(Box::new(TypeAppTData {
        reason,
        use_op,
        type_: t,
        targs: targs.into(),
        from_value,
        use_desc,
    })))
}

pub fn implicit_typeapp(t: Type, targs: Vec<Type>, annot_loc: Option<ALoc>) -> Type {
    let reason = reason_of_t(&t).dupe();
    let reason = match annot_loc {
        Some(loc) => reason.reposition_and_annotate(loc),
        None => reason,
    };
    let use_op = UseOp::Op(Arc::new(RootUseOp::TypeApplication {
        type_: TypeApplicationTarget::Reason(reason.dupe()),
    }));
    Type::new(TypeInner::TypeAppT(Box::new(TypeAppTData {
        reason,
        use_op,
        type_: t,
        targs: targs.into(),
        from_value: false,
        use_desc: false,
    })))
}

pub fn this_typeapp(
    t: Type,
    this: Type,
    targs: Option<Vec<Type>>,
    annot_loc: Option<ALoc>,
) -> Type {
    let reason = reason_of_t(&t).dupe();
    let reason = match annot_loc {
        Some(loc) => reason.reposition_and_annotate(loc),
        None => reason,
    };
    Type::new(TypeInner::ThisTypeAppT(Box::new(ThisTypeAppTData {
        reason,
        this_t: this,
        type_: t,
        targs: targs.map(|v| v.into()),
    })))
}

pub fn typeof_annotation(reason: Reason, t: Type, targs: Option<(FlowSmolStr, Vec<Type>)>) -> Type {
    use flow_common::reason::mk_annot_reason;

    let annot_loc = reason.loc().dupe();
    let desc_for_tapp = reason.desc(true).cloned();
    let t_annot = Type::new(TypeInner::AnnotT(
        reason.dupe().opt_annotate(Some(annot_loc.dupe())),
        t,
        false,
    ));
    match targs {
        None => t_annot,
        Some((name, targs)) => {
            let reason_tapp = mk_annot_reason(desc_for_tapp, annot_loc.dupe());
            let use_op = UseOp::Op(Arc::new(RootUseOp::TypeApplication {
                type_: TypeApplicationTarget::Typeof(Box::new(TypeofApplicationData {
                    reason: reason_tapp,
                    name,
                })),
            }));
            typeapp_with_use_op(true, false, reason, use_op, t_annot, targs)
        }
    }
}

pub fn push_type_alias_reason(r: &Reason, t: Type) -> Type {
    use flow_common::reason::VirtualReasonDesc::RTypeAlias;

    match r.desc(false) {
        Some(RTypeAlias(box (n, _, _))) => {
            let n = n.dupe();
            mod_reason_of_t(
                &|reason: Reason| {
                    reason.update_desc(|desc| {
                        Some(flow_common::reason::VirtualReasonDesc::RTypeAlias(
                            Box::new((n.dupe(), None, desc.map(Arc::new))),
                        ))
                    })
                },
                &t,
            )
        }
        _ => t,
    }
}

pub fn type_t_of_annotated_or_inferred(x: &crate::type_::AnnotatedOrInferred) -> &Type {
    use crate::type_::AnnotatedOrInferred::*;
    match x {
        Inferred(t) | Annotated(t) => t,
    }
}

pub fn map_annotated_or_inferred<F>(
    f: F,
    x: crate::type_::AnnotatedOrInferred,
) -> crate::type_::AnnotatedOrInferred
where
    F: FnOnce(Type) -> Type,
{
    use crate::type_::AnnotatedOrInferred::*;
    match x {
        Inferred(t) => Inferred(f(t)),
        Annotated(t) => Annotated(f(t)),
    }
}

pub fn union_of_ts(
    reason: Reason,
    ts: Vec<Type>,
    kind: Option<crate::type_::union_rep::UnionKind>,
) -> Type {
    use crate::type_::DefT;
    use crate::type_::DefTInner;
    use crate::type_::union_rep;

    let kind = kind.unwrap_or(union_rep::UnionKind::UnknownKind);
    match ts.len() {
        0 => Type::new(TypeInner::DefT(reason, DefT::new(DefTInner::EmptyT))),
        1 => ts.into_iter().next().unwrap(),
        _ => {
            let mut iter = ts.into_iter();
            let t0 = iter.next().unwrap();
            let t1 = iter.next().unwrap();
            let rest: Vec<Type> = iter.collect();
            let rep = union_rep::make(None, kind, t0, t1, rest.into());
            Type::new(TypeInner::UnionT(reason, rep))
        }
    }
}

pub fn union_of_ts_opt(
    reason: Reason,
    ts: Vec<Type>,
    kind: Option<crate::type_::union_rep::UnionKind>,
) -> Option<Type> {
    use crate::type_::union_rep;

    let kind = kind.unwrap_or(union_rep::UnionKind::UnknownKind);
    match ts.len() {
        0 => None,
        1 => ts.into_iter().next(),
        _ => {
            let mut iter = ts.into_iter();
            let t0 = iter.next().unwrap();
            let t1 = iter.next().unwrap();
            let rest: Vec<Type> = iter.collect();
            let rep = union_rep::make(None, kind, t0, t1, rest.into());
            Some(Type::new(TypeInner::UnionT(reason, rep)))
        }
    }
}

pub fn annotated_or_inferred_of_option(
    default: Type,
    t: Option<Type>,
) -> crate::type_::AnnotatedOrInferred {
    use crate::type_::AnnotatedOrInferred::*;
    match t {
        Some(t) => Annotated(t),
        None => Inferred(default),
    }
}

pub fn subtype_this_of_function(funtype: &crate::type_::FunType) -> Type {
    use crate::type_::ThisStatus;
    use crate::type_::mixed_t;

    let (this, subtyping) = &funtype.this_t;
    match subtyping {
        ThisStatus::ThisFunction | ThisStatus::ThisMethodWithExplicitThis => this.dupe(),
        ThisStatus::ThisMethod => mixed_t::make(reason_of_t(this).dupe()),
    }
}

pub fn all_explicit_targs(targs: Option<&[crate::type_::Targ]>) -> Option<Vec<crate::type_::Targ>> {
    use crate::type_::Targ;

    let targs = targs?;
    let mut result = Vec::new();
    for targ in targs {
        match targ {
            Targ::ExplicitArg(_) => result.push(targ.clone()),
            Targ::ImplicitArg(_) => return None,
        }
    }
    Some(result)
}

pub fn all_explicit_targ_ts(targs: Option<&[crate::type_::Targ]>) -> Option<Vec<Type>> {
    use crate::type_::Targ;

    let targs = targs?;
    let mut result = Vec::new();
    for targ in targs {
        match targ {
            Targ::ExplicitArg(t) => result.push(t.dupe()),
            Targ::ImplicitArg(_) => return None,
        }
    }
    Some(result)
}

pub fn tuple_length(reason: Reason, inexact: bool, num_req: i32, num_total: i32) -> Type {
    use crate::type_::DefT;
    use crate::type_::DefTInner as D;
    use crate::type_::Literal;
    use crate::type_::NumberLiteral;
    use crate::type_::TypeInner;

    if inexact {
        let r = reason.replace_desc(VirtualReasonDesc::RNumber);
        Type::new(TypeInner::DefT(
            r,
            DefT::new(D::NumGeneralT(Literal::AnyLiteral)),
        ))
    } else {
        let ts: Vec<Type> = (num_req..=num_total)
            .map(|n: i32| {
                let r = reason.dupe().replace_desc(None);
                let float = n as f64;
                let string = n.to_string().into();
                let t = DefT::new(D::SingletonNumT {
                    from_annot: false,
                    value: NumberLiteral(float, string),
                });
                Type::new(TypeInner::DefT(r, t))
            })
            .collect();
        union_of_ts(reason, ts, None)
    }
}

pub fn tuple_ts_of_elements(elements: &[crate::type_::TupleElement]) -> Vec<Type> {
    elements.iter().map(|elem| elem.t.dupe()).collect()
}

pub fn mk_tuple_element(
    reason: Reason,
    t: Type,
    name: Option<FlowSmolStr>,
    optional: bool,
    polarity: flow_common::polarity::Polarity,
) -> crate::type_::TupleElement {
    crate::type_::TupleElement {
        reason,
        name,
        t,
        polarity,
        optional,
    }
}

pub fn reason_of_resolved_param(param: &crate::type_::ResolvedParam) -> &Reason {
    use crate::type_::ResolvedParam::*;

    match param {
        ResolvedSpreadArg(box crate::type_::ResolvedSpreadArgData(reason, _, _)) => reason,
        ResolvedAnySpreadArg(reason, _) => reason,
        ResolvedArg(box crate::type_::ResolvedArgData(tuple_elem, _)) => &tuple_elem.reason,
    }
}

/// Extract the property name from a key type that is a SingletonStrT,
/// possibly wrapped in a GenericT. Returns None for non-string-literal keys.  
pub fn string_of_singleton_string_type(t: &Type) -> Option<&FlowSmolStr> {
    use crate::type_::DefTInner;

    match t.deref() {
        TypeInner::DefT(_, def_t) => match def_t.deref() {
            DefTInner::SingletonStrT { value: name, .. } => Some(name),
            _ => None,
        },
        TypeInner::GenericT(box GenericTData { bound, .. }) => match bound.deref() {
            TypeInner::DefT(_, def_t) => match def_t.deref() {
                DefTInner::SingletonStrT { value: name, .. } => Some(name),
                _ => None,
            },
            _ => None,
        },
        _ => None,
    }
}

pub fn dro_of_type(t: &Type) -> Option<&crate::type_::ReactDro> {
    use crate::type_::ArrType;
    use crate::type_::ArrayATData;
    use crate::type_::DefTInner;
    use crate::type_::TupleATData;

    match t.deref() {
        TypeInner::DefT(_, def_t) => match &**def_t {
            DefTInner::ObjT(obj) => obj.flags.react_dro.as_ref(),
            DefTInner::InstanceT(inst) => inst.inst.inst_react_dro.as_ref(),
            DefTInner::ArrT(arr) => match arr.as_ref() {
                ArrType::ArrayAT(box ArrayATData { react_dro, .. }) => react_dro.as_ref(),
                ArrType::TupleAT(box TupleATData { react_dro, .. }) => react_dro.as_ref(),
                ArrType::ROArrayAT(box ROArrayATData { react_dro, .. }) => react_dro.as_ref(),
            },
            _ => None,
        },
        _ => None,
    }
}

pub fn normalize_jsx_children_prop(
    loc_children: flow_aloc::ALoc,
    jsx_children: Vec<Type>,
) -> Option<Type> {
    use flow_common::reason::mk_reason;

    use crate::type_::ArrType;
    use crate::type_::ArrayATData;
    use crate::type_::DefT;
    use crate::type_::DefTInner as D;
    use crate::type_::TupleElement;
    use crate::type_::TupleView;
    use crate::type_::TupleViewKind;
    use crate::type_::TypeInner;

    match jsx_children.len() {
        // If we have no children then React will not pass in any value for children.
        0 => None,
        // If we know that we have exactly one argument then React will pass in that single value.
        // Notable we do not wrap the type in an array as React returns the single value.
        1 => jsx_children.into_iter().next(),
        // If we have two or more known arguments then we want to create a tuple array type for our children.
        _ => {
            // Create a reason where the location is between our first and last known argument.
            let r = mk_reason(VirtualReasonDesc::RReactChildren, loc_children);
            let arity = jsx_children.len() as i32;
            let elements: Vec<TupleElement> = jsx_children
                .iter()
                .map(|t| {
                    mk_tuple_element(
                        reason_of_t(t).dupe(),
                        t.dupe(),
                        None,
                        false,
                        flow_common::polarity::Polarity::Neutral,
                    )
                })
                .collect();
            Some(Type::new(TypeInner::DefT(
                r.dupe(),
                DefT::new(D::ArrT(std::rc::Rc::new(ArrType::ArrayAT(Box::new(
                    ArrayATData {
                        elem_t: union_of_ts(r, jsx_children, None),
                        tuple_view: Some(TupleView {
                            kind: TupleViewKind::ArrayLiteral,
                            elements: elements.into(),
                            arity: (arity, arity),
                            inexact: false,
                            rest: None,
                        }),
                        react_dro: None,
                        strictness_kind: TypeStrictnessKind::Flow,
                        literal: true,
                        fresh: true,
                    },
                ))))),
            )))
        }
    }
}

pub fn map_property<F>(f: F, prop: crate::type_::PropertyType) -> crate::type_::PropertyType
where
    F: Fn(Type) -> Type,
{
    use crate::type_::PropertyType;

    match prop {
        PropertyType::OrdinaryField { type_, polarity } => PropertyType::OrdinaryField {
            type_: f(type_),
            polarity,
        },
        PropertyType::SyntheticField { get_type, set_type } => PropertyType::SyntheticField {
            get_type: get_type.map(&f),
            set_type: set_type.map(&f),
        },
    }
}

pub fn mk_possibly_generic_render_type(
    variant: flow_parser::ast::types::RendersVariant,
    reason: Reason,
    t: Type,
) -> Option<Type> {
    use std::rc::Rc;

    use flow_parser::ast::types::RendersVariant;

    use crate::type_::CanonicalRendersForm;
    use crate::type_::DefT;
    use crate::type_::DefTInner as D;
    use crate::type_::RendersVariant as TypeRendersVariant;
    use crate::type_::TypeInner;

    fn singleton_or_union_of_generic_t(t: &Type) -> Option<(Type, Vec<Type>)> {
        fn loop_inner(acc: Option<Vec<Type>>, t: &Type) -> Option<Vec<Type>> {
            match (acc, &**t) {
                (None, _) => None,
                (Some(mut l), TypeInner::GenericT(..)) => {
                    l.push(t.dupe());
                    Some(l)
                }
                (acc, TypeInner::UnionT(_, rep)) => rep.members_iter().fold(acc, loop_inner),
                _ => None,
            }
        }
        match loop_inner(Some(Vec::new()), t) {
            None => None,
            Some(v) if v.is_empty() => None,
            Some(mut v) => {
                let hd = v.remove(0);
                Some((hd, v))
            }
        }
    }

    if !matches!(variant, RendersVariant::Star) {
        if let Some((hd, tl)) = singleton_or_union_of_generic_t(&t) {
            let result_t = match variant {
                RendersVariant::Normal => t,
                RendersVariant::Maybe => {
                    let mut types: Vec<Type> = std::iter::once(hd).chain(tl).collect();
                    types.push(Type::new(TypeInner::DefT(
                        reason.dupe(),
                        DefT::new(D::NullT),
                    )));
                    types.push(Type::new(TypeInner::DefT(
                        reason.dupe(),
                        DefT::new(D::VoidT),
                    )));
                    types.push(Type::new(TypeInner::DefT(
                        reason.dupe(),
                        DefT::new(D::SingletonBoolT {
                            value: false,
                            from_annot: true,
                        }),
                    )));
                    union_of_ts(reason.dupe(), types, None)
                }
                RendersVariant::Star => unreachable!("Already banned above"),
            };
            Some(Type::new(TypeInner::DefT(
                reason,
                DefT::new(D::RendersT(Rc::new(
                    CanonicalRendersForm::StructuralRenders {
                        renders_variant: TypeRendersVariant::RendersNormal,
                        renders_structural_type: result_t,
                    },
                ))),
            )))
        } else {
            None
        }
    } else {
        None
    }
}

// Construct signatures don't have a meaningful `this`, and the class
// `constructor` we sometimes use as a synthetic construct signature has the
// class-instance type as its return. This helper walks the funtype leaves of
// [t] (plain [FunT], a [PolyT] whose [t_out] is a [FunT], or [IntersectionT] of either)
// and rewrites each leaf's [this_t] to an unconstrained any, optionally also
// overriding [return_t]. Construct signatures do not have a meaningful
// receiver, but keeping [ThisMethod] preserves the lenient method-to-method
// subtyping behavior.
//
// Used by the construct-signature plumbing in [type_annotation.rs] and [flow_js/].
pub fn normalize_construct_sig(override_return_t: Option<Type>, t: Type) -> Type {
    use crate::type_::AnySource;
    use crate::type_::DefT;
    use crate::type_::DefTInner;
    use crate::type_::PolyTData;
    use crate::type_::ThisStatus;
    use crate::type_::any_t;

    fn rewrite_fun(
        ft: &crate::type_::FunType,
        override_return_t: &Option<Type>,
    ) -> crate::type_::FunType {
        let (this_t, _): &(Type, ThisStatus) = &ft.this_t;
        let any_this_t = any_t::make(AnySource::AnyError(None), reason_of_t(this_t).dupe());
        let return_t = override_return_t
            .as_ref()
            .map(|t| t.dupe())
            .unwrap_or_else(|| ft.return_t.dupe());
        crate::type_::FunType {
            this_t: (any_this_t, ThisStatus::ThisMethod),
            return_t,
            ..ft.clone()
        }
    }
    fn go(t: Type, override_return_t: &Option<Type>) -> Type {
        match &*t {
            TypeInner::DefT(r, def_t) => match &**def_t {
                DefTInner::FunT(statics, ft) => Type::new(TypeInner::DefT(
                    r.dupe(),
                    DefT::new(DefTInner::FunT(
                        statics.dupe(),
                        Rc::new(rewrite_fun(ft, override_return_t)),
                    )),
                )),
                // Reuse the original Poly id. Every consumer normalizes on read, so the
                // instantiation cache only ever holds entries computed from the
                // normalized body; reusing the id lets repeated reads share those
                // entries. Regenerating it would be correct but defeat the cache.
                DefTInner::PolyT(box PolyTData {
                    kind: poly_kind,
                    tparams_loc,
                    tparams,
                    t_out,
                    id,
                    strictness_kind,
                }) => Type::new(TypeInner::DefT(
                    r.dupe(),
                    DefT::new(DefTInner::PolyT(Box::new(PolyTData {
                        kind: *poly_kind,
                        tparams_loc: tparams_loc.dupe(),
                        tparams: tparams.dupe(),
                        t_out: go(t_out.dupe(), override_return_t),
                        id: id.dupe(),
                        strictness_kind: *strictness_kind,
                    }))),
                )),
                _ => t.dupe(),
            },
            TypeInner::IntersectionT(r, rep) => Type::new(TypeInner::IntersectionT(
                r.dupe(),
                rep.map(|t| go(t.dupe(), override_return_t)),
            )),
            _ => t.dupe(),
        }
    }
    go(t, &override_return_t)
}
