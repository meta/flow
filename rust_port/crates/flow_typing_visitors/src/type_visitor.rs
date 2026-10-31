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
use flow_typing_context::Context;
use flow_typing_type::type_::ArrType;
use flow_typing_type::type_::ArrayATData;
use flow_typing_type::type_::CallArg;
use flow_typing_type::type_::CallArgInner;
use flow_typing_type::type_::DefT;
use flow_typing_type::type_::Destructor;
use flow_typing_type::type_::DictType;
use flow_typing_type::type_::ExportTypes;
use flow_typing_type::type_::FunType;
use flow_typing_type::type_::GenericTData;
use flow_typing_type::type_::InstType;
use flow_typing_type::type_::InstanceT;
use flow_typing_type::type_::NamespaceType;
use flow_typing_type::type_::ObjType;
use flow_typing_type::type_::Predicate;
use flow_typing_type::type_::Property;
use flow_typing_type::type_::ROArrayATData;
use flow_typing_type::type_::Selector;
use flow_typing_type::type_::Targ;
use flow_typing_type::type_::ThisInstanceTData;
use flow_typing_type::type_::ThisTypeAppTData;
use flow_typing_type::type_::TupleATData;
use flow_typing_type::type_::TupleElement;
use flow_typing_type::type_::Type;
use flow_typing_type::type_::TypeAppTData;
use flow_typing_type::type_::TypeInner;
use flow_typing_type::type_::TypeParam;
use flow_typing_type::type_util;

fn pole_todo() -> Polarity {
    Polarity::Neutral
}

/// We walk types in a lot of places for all kinds of things, but often most of
/// the code is boilerplate. The following visitor class for types aims to
/// reduce that boilerplate. It is designed as a fold on the structure of types,
/// parameterized by an accumulator.
///
/// WARNING: This is only a partial implementation, sufficient for current
/// purposes but intended to be completed in a later diff.
pub trait TypeVisitor<Acc> {
    fn type_<'cx>(&mut self, cx: &mut Context<'cx>, pole: Polarity, acc: Acc, t: &Type) -> Acc {
        type_default(self, cx, pole, acc, t)
    }

    fn def_type<'cx>(
        &mut self,
        cx: &mut Context<'cx>,
        pole: Polarity,
        acc: Acc,
        def_t: &DefT,
    ) -> Acc {
        def_type_default(self, cx, pole, acc, def_t)
    }

    fn targ<'cx>(&mut self, cx: &mut Context<'cx>, pole: Polarity, acc: Acc, targ: &Targ) -> Acc {
        targ_default(self, cx, pole, acc, targ)
    }

    fn defer_use_type<'cx>(
        &mut self,
        cx: &mut Context<'cx>,
        acc: Acc,
        defer_use_t: &flow_typing_type::type_::TypeDestructorT,
    ) -> Acc {
        defer_use_type_default(self, cx, acc, defer_use_t)
    }

    fn selector<'cx>(&mut self, cx: &mut Context<'cx>, acc: Acc, selector: &Selector) -> Acc {
        selector_default(self, cx, acc, selector)
    }

    fn predicate<'cx>(&mut self, cx: &mut Context<'cx>, acc: Acc, predicate: &Predicate) -> Acc {
        predicate_default(self, cx, acc, predicate)
    }

    fn call_arg<'cx>(
        &mut self,
        cx: &mut Context<'cx>,
        pole: Polarity,
        acc: Acc,
        arg: &CallArg,
    ) -> Acc {
        call_arg_default(self, cx, pole, acc, arg)
    }

    fn destructor<'cx>(&mut self, cx: &mut Context<'cx>, acc: Acc, destructor: &Destructor) -> Acc {
        destructor_default(self, cx, acc, destructor)
    }

    fn tvar<'cx>(
        &mut self,
        cx: &mut Context<'cx>,
        pole: Polarity,
        acc: Acc,
        r: &Reason,
        id: u32,
    ) -> Acc {
        tvar_default(self, cx, pole, acc, r, id)
    }

    fn dict_type<'cx>(
        &mut self,
        cx: &mut Context<'cx>,
        pole: Polarity,
        acc: Acc,
        dict: &DictType,
    ) -> Acc {
        dict_type_default(self, cx, pole, acc, dict)
    }

    fn props<'cx>(
        &mut self,
        cx: &mut Context<'cx>,
        pole: Polarity,
        acc: Acc,
        props_id: flow_typing_type::type_::properties::Id,
    ) -> Acc {
        props_default(self, cx, pole, acc, props_id)
    }

    fn prop<'cx>(
        &mut self,
        cx: &mut Context<'cx>,
        pole: Polarity,
        acc: Acc,
        prop: &Property,
    ) -> Acc {
        prop_default(self, cx, pole, acc, prop)
    }

    fn call_prop<'cx>(&mut self, cx: &mut Context<'cx>, pole: Polarity, acc: Acc, id: i32) -> Acc {
        call_prop_default(self, cx, pole, acc, id)
    }

    fn exports<'cx>(
        &mut self,
        cx: &mut Context<'cx>,
        pole: Polarity,
        acc: Acc,
        id: flow_typing_type::type_::exports::Id,
    ) -> Acc {
        exports_default(self, cx, pole, acc, id)
    }

    fn eval_id<'cx>(
        &mut self,
        cx: &mut Context<'cx>,
        pole: Polarity,
        acc: Acc,
        id: flow_typing_type::type_::eval::Id,
    ) -> Acc {
        eval_id_default(self, cx, pole, acc, id)
    }

    fn type_param<'cx>(
        &mut self,
        cx: &mut Context<'cx>,
        pole: Polarity,
        acc: Acc,
        tp: &TypeParam,
    ) -> Acc {
        type_param_default(self, cx, pole, acc, tp)
    }

    fn fun_type<'cx>(
        &mut self,
        cx: &mut Context<'cx>,
        pole: Polarity,
        acc: Acc,
        ft: &FunType,
    ) -> Acc {
        fun_type_default(self, cx, pole, acc, ft)
    }

    fn fun_type_guard<'cx>(
        &mut self,
        cx: &mut Context<'cx>,
        pole: Polarity,
        acc: Acc,
        tg: &flow_typing_type::type_::TypeGuard,
    ) -> Acc {
        fun_type_guard_default(self, cx, pole, acc, tg)
    }

    fn obj_flags<'cx>(
        &mut self,
        cx: &mut Context<'cx>,
        pole: Polarity,
        acc: Acc,
        flags: &flow_typing_type::type_::Flags,
    ) -> Acc {
        obj_flags_default(self, cx, pole, acc, flags)
    }

    fn obj_type<'cx>(
        &mut self,
        cx: &mut Context<'cx>,
        pole: Polarity,
        acc: Acc,
        obj: &ObjType,
    ) -> Acc {
        obj_type_default(self, cx, pole, acc, obj)
    }

    fn namespace_type<'cx>(
        &mut self,
        cx: &mut Context<'cx>,
        pole: Polarity,
        acc: Acc,
        ns: &NamespaceType,
    ) -> Acc {
        namespace_type_default(self, cx, pole, acc, ns)
    }

    fn arr_type<'cx>(
        &mut self,
        cx: &mut Context<'cx>,
        pole: Polarity,
        acc: Acc,
        arr: &ArrType,
    ) -> Acc {
        arr_type_default(self, cx, pole, acc, arr)
    }

    fn tuple_element<'cx>(
        &mut self,
        cx: &mut Context<'cx>,
        pole: Polarity,
        acc: Acc,
        element: &TupleElement,
    ) -> Acc {
        tuple_element_default(self, cx, pole, acc, element)
    }

    fn inst_type<'cx>(
        &mut self,
        cx: &mut Context<'cx>,
        pole: Polarity,
        acc: Acc,
        inst: &InstType,
    ) -> Acc {
        inst_type_default(self, cx, pole, acc, inst)
    }

    fn instance_type<'cx>(
        &mut self,
        cx: &mut Context<'cx>,
        pole: Polarity,
        acc: Acc,
        instance: &InstanceT,
    ) -> Acc {
        instance_type_default(self, cx, pole, acc, instance)
    }

    fn export_types<'cx>(
        &mut self,
        cx: &mut Context<'cx>,
        pole: Polarity,
        acc: Acc,
        exports: &ExportTypes,
    ) -> Acc {
        export_types_default(self, cx, pole, acc, exports)
    }

    fn object_kit_spread_operand_slice<'cx>(
        &mut self,
        cx: &mut Context<'cx>,
        acc: Acc,
        slice: &flow_typing_type::type_::object::spread::OperandSlice,
    ) -> Acc {
        object_kit_spread_operand_slice_default(self, cx, acc, slice)
    }

    fn object_kit_spread_operand<'cx>(
        &mut self,
        cx: &mut Context<'cx>,
        acc: Acc,
        operand: &flow_typing_type::type_::object::spread::Operand,
    ) -> Acc {
        object_kit_spread_operand_default(self, cx, acc, operand)
    }
}

// =============================================================================
// Default implementations
// =============================================================================

pub fn type_default<'cx, Acc, V: TypeVisitor<Acc> + ?Sized>(
    visitor: &mut V,
    cx: &mut Context<'cx>,
    pole: Polarity,
    acc: Acc,
    t: &Type,
) -> Acc {
    match &**t {
        TypeInner::OpenT(tvar) => visitor.tvar(cx, pole, acc, tvar.reason(), tvar.id()),
        TypeInner::ImplicitInstantiationTvar(_) => acc,
        TypeInner::DefT(_, def_t) => visitor.def_type(cx, pole, acc, def_t),
        TypeInner::FunProtoT(_) | TypeInner::ObjProtoT(_) | TypeInner::NullProtoT(_) => acc,
        TypeInner::EvalT {
            type_,
            defer_use_t,
            id,
        } => {
            let acc = visitor.type_(cx, Polarity::Positive, acc, type_);
            let acc = visitor.defer_use_type(cx, acc, defer_use_t);
            let eval_pole = if type_util::constraint_node_id(type_).is_some() {
                Polarity::Neutral
            } else {
                Polarity::Positive
            };
            visitor.eval_id(cx, eval_pole, acc, id.dupe())
        }
        TypeInner::GenericT(box GenericTData { bound, .. }) => visitor.type_(cx, pole, acc, bound),
        TypeInner::KeysT(_, t) => visitor.type_(cx, Polarity::Positive, acc, t),
        TypeInner::TemplateLiteralT { types, .. } => types
            .iter()
            .fold(acc, |acc, t| visitor.type_(cx, pole_todo(), acc, t)),
        TypeInner::StringMappingT { arg, .. } => visitor.type_(cx, pole, acc, arg),
        TypeInner::AnnotT(_, t, _) => visitor.type_(cx, Polarity::Positive, acc, t),
        TypeInner::NominalT { nominal_type, .. } => {
            let acc =
                nominal_type
                    .nominal_type_args
                    .iter()
                    .fold(acc, |acc, (_, _, t, pole_prime)| {
                        visitor.type_(cx, Polarity::mult(pole, *pole_prime), acc, t)
                    });
            let acc = match &nominal_type.underlying_t {
                flow_typing_type::type_::nominal::UnderlyingT::OpaqueWithLocal { t }
                | flow_typing_type::type_::nominal::UnderlyingT::CustomError(
                    box flow_typing_type::type_::nominal::CustomErrorData { t, .. },
                ) => visitor.type_(cx, pole, acc, t),
                flow_typing_type::type_::nominal::UnderlyingT::FullyOpaque => acc,
            };
            let acc = if let Some(t) = &nominal_type.lower_t {
                visitor.type_(cx, pole, acc, t)
            } else {
                acc
            };
            if let Some(t) = &nominal_type.upper_t {
                visitor.type_(cx, pole, acc, t)
            } else {
                acc
            }
        }
        TypeInner::NamespaceT(namespace_t) => visitor.namespace_type(cx, pole, acc, namespace_t),
        TypeInner::ThisInstanceT(box ThisInstanceTData { instance, .. }) => {
            visitor.instance_type(cx, pole, acc, instance)
        }
        TypeInner::ThisTypeAppT(box ThisTypeAppTData {
            type_,
            this_t,
            targs,
            ..
        }) => {
            let acc = visitor.type_(cx, Polarity::Positive, acc, type_);
            let acc = visitor.type_(cx, pole, acc, this_t);
            if let Some(targs) = targs {
                targs
                    .iter()
                    // If we knew what `t` resolved to, we could determine the polarities for
                    // `ts`, but in general `t` might be unresolved. Subclasses which have more
                    //  information should override this to be more specific.
                    .fold(acc, |acc, targ| visitor.type_(cx, pole_todo(), acc, targ))
            } else {
                acc
            }
        }
        TypeInner::TypeAppT(box TypeAppTData { type_, targs, .. }) => {
            let acc = visitor.type_(cx, Polarity::Positive, acc, type_);
            targs
                .iter()
                // If we knew what `t` resolved to, we could determine the polarities for
                // `ts`, but in general `t` might be unresolved. Subclasses which have more
                //  information should override this to be more specific.
                .fold(acc, |acc, targ| visitor.type_(cx, pole_todo(), acc, targ))
        }
        TypeInner::AnyT { .. } => acc,
        TypeInner::OptionalT { type_, .. } | TypeInner::MaybeT(_, type_) => {
            visitor.type_(cx, pole, acc, type_)
        }
        TypeInner::IntersectionT(_, members) => members
            .members_iter()
            .fold(acc, |acc, t| visitor.type_(cx, pole, acc, t)),
        TypeInner::UnionT(_, members) => members
            .members_iter()
            .fold(acc, |acc, t| visitor.type_(cx, pole, acc, t)),
    }
}

pub fn def_type_default<'cx, Acc, V: TypeVisitor<Acc> + ?Sized>(
    visitor: &mut V,
    cx: &mut Context<'cx>,
    pole: Polarity,
    acc: Acc,
    def_t: &DefT,
) -> Acc {
    use flow_typing_type::type_::DefTInner;
    use flow_typing_type::type_::PolyTData;
    use flow_typing_type::type_::ReactAbstractComponentTData;
    match &**def_t {
        DefTInner::NumGeneralT(_)
        | DefTInner::StrGeneralT(_)
        | DefTInner::BoolGeneralT
        | DefTInner::BigIntGeneralT(_)
        | DefTInner::EmptyT
        | DefTInner::MixedT(_)
        | DefTInner::SymbolT
        | DefTInner::UniqueSymbolT(_)
        | DefTInner::NullT
        | DefTInner::VoidT => acc,

        DefTInner::EnumValueT(enum_info) => {
            use flow_typing_type::type_::EnumInfoInner;
            let representation_t = match enum_info.deref().deref() {
                EnumInfoInner::ConcreteEnum(concrete) => &concrete.representation_t,
                EnumInfoInner::AbstractEnum { representation_t } => representation_t,
            };
            visitor.type_(cx, pole, acc, representation_t)
        }

        DefTInner::EnumObjectT {
            enum_value_t,
            enum_info,
        } => {
            use flow_typing_type::type_::EnumInfoInner;
            let acc = visitor.type_(cx, pole, acc, enum_value_t);
            let representation_t = match enum_info.deref().deref() {
                EnumInfoInner::ConcreteEnum(concrete) => &concrete.representation_t,
                EnumInfoInner::AbstractEnum { representation_t } => representation_t,
            };
            visitor.type_(cx, pole, acc, representation_t)
        }

        DefTInner::FunT(static_t, funtype) => {
            let acc = visitor.type_(cx, pole, acc, static_t);
            visitor.fun_type(cx, pole, acc, funtype)
        }

        DefTInner::ObjT(objtype) => visitor.obj_type(cx, pole, acc, objtype),
        DefTInner::ArrT(arrtype) => visitor.arr_type(cx, pole, acc, arrtype),
        DefTInner::ClassT(t) => visitor.type_(cx, pole, acc, t),
        DefTInner::InstanceT(t) => visitor.instance_type(cx, pole, acc, t),

        DefTInner::NumericStrKeyT(_)
        | DefTInner::SingletonStrT { .. }
        | DefTInner::SingletonNumT { .. }
        | DefTInner::SingletonBoolT { .. }
        | DefTInner::SingletonBigIntT { .. } => acc,

        DefTInner::TypeT(_, t) => visitor.type_(cx, pole, acc, t),

        DefTInner::PolyT(box PolyTData { tparams, t_out, .. }) => {
            let acc = tparams
                .iter()
                .fold(acc, |acc, tp| visitor.type_param(cx, pole, acc, tp));
            visitor.type_(cx, pole, acc, t_out)
        }

        DefTInner::ReactAbstractComponentT(box ReactAbstractComponentTData {
            config,
            renders,
            component_kind,
        }) => {
            let acc = visitor.type_(cx, pole.inv(), acc, config);
            let acc = visitor.type_(cx, pole, acc, renders);
            match component_kind {
                flow_typing_type::type_::ComponentKind::Structural => acc,
                flow_typing_type::type_::ComponentKind::Nominal(_, _, ts_opt) => {
                    if let Some(ts) = ts_opt {
                        ts.iter()
                            .fold(acc, |acc, t| visitor.type_(cx, pole, acc, t))
                    } else {
                        acc
                    }
                }
            }
        }

        DefTInner::RendersT(renders_t) => {
            use flow_typing_type::type_::CanonicalRendersForm;
            match renders_t.as_ref() {
                CanonicalRendersForm::NominalRenders { renders_super, .. } => {
                    visitor.type_(cx, pole, acc, renders_super)
                }
                CanonicalRendersForm::StructuralRenders {
                    renders_structural_type,
                    ..
                } => visitor.type_(cx, pole, acc, renders_structural_type),
                CanonicalRendersForm::IntrinsicRenders(_)
                | CanonicalRendersForm::DefaultRenders => acc,
            }
        }
    }
}

pub fn targ_default<'cx, Acc, V: TypeVisitor<Acc> + ?Sized>(
    visitor: &mut V,
    cx: &mut Context<'cx>,
    pole: Polarity,
    acc: Acc,
    targ: &Targ,
) -> Acc {
    match targ {
        Targ::ImplicitArg(_) => acc,
        Targ::ExplicitArg(t) => visitor.type_(cx, pole, acc, t),
    }
}

pub fn defer_use_type_default<'cx, Acc, V: TypeVisitor<Acc> + ?Sized>(
    visitor: &mut V,
    cx: &mut Context<'cx>,
    acc: Acc,
    defer_use_t: &flow_typing_type::type_::TypeDestructorT,
) -> Acc {
    let destructor = &defer_use_t.2;
    visitor.destructor(cx, acc, destructor)
}

pub fn selector_default<'cx, Acc, V: TypeVisitor<Acc> + ?Sized>(
    visitor: &mut V,
    cx: &mut Context<'cx>,
    acc: Acc,
    selector: &Selector,
) -> Acc {
    match selector {
        Selector::Prop(..) => acc,
        Selector::Elem(key) => visitor.type_(cx, pole_todo(), acc, key),
        Selector::ObjRest(_) => acc,
        Selector::ArrRest(_) => acc,
        Selector::Default => acc,
    }
}

pub fn predicate_default<'cx, Acc, V: TypeVisitor<Acc> + ?Sized>(
    visitor: &mut V,
    cx: &mut Context<'cx>,
    acc: Acc,
    predicate: &Predicate,
) -> Acc {
    use flow_typing_type::type_::PredicateInner::*;
    match &**predicate {
        AndP(p1, p2) | OrP(p1, p2) => {
            let acc = visitor.predicate(cx, acc, p1);
            visitor.predicate(cx, acc, p2)
        }
        NotP(p) => visitor.predicate(cx, acc, p),
        BinaryP(_, t) => visitor.type_(cx, Polarity::Positive, acc, t),
        TruthyP
        | NullP
        | MaybeP
        | SingletonBoolP(_)
        | SingletonStrP(_)
        | SingletonNumP(_)
        | SingletonBigIntP(_)
        | BoolP(_)
        | FunP
        | NumP(_)
        | BigIntP(_)
        | ObjP
        | StrP(_)
        | SymbolP(_)
        | VoidP
        | ArrP
        | ArrLenP { .. }
        | PropTruthyP(_, _)
        | PropExistsP { .. }
        | PropNonVoidP(_, _)
        | PropIsExactlyNullP(_, _)
        | PropNonMaybeP(_, _)
        | ImpossibleP => acc,
        LatentP(pred_info, _) | LatentThisP(pred_info) => {
            let flow_typing_type::type_::PredFuncallInfo(_, _, t, targs, argts) =
                pred_info.as_ref();
            let acc = visitor.type_(cx, Polarity::Positive, acc, t);
            let acc = if let Some(targs) = targs {
                targs
                    .iter()
                    .fold(acc, |acc, targ| visitor.targ(cx, pole_todo(), acc, targ))
            } else {
                acc
            };
            argts
                .iter()
                .fold(acc, |acc, arg| visitor.call_arg(cx, pole_todo(), acc, arg))
        }
    }
}

pub fn call_arg_default<'cx, Acc, V: TypeVisitor<Acc> + ?Sized>(
    visitor: &mut V,
    cx: &mut Context<'cx>,
    pole: Polarity,
    acc: Acc,
    arg: &CallArg,
) -> Acc {
    match &**arg {
        CallArgInner::Arg(t) | CallArgInner::SpreadArg(t) => visitor.type_(cx, pole, acc, t),
    }
}

pub fn destructor_default<'cx, Acc, V: TypeVisitor<Acc> + ?Sized>(
    visitor: &mut V,
    cx: &mut Context<'cx>,
    acc: Acc,
    destructor: &Destructor,
) -> Acc {
    use flow_typing_type::type_::Destructor::*;
    match destructor {
        NormalizeIntersection { .. }
        | NonMaybeType
        | ReactDRO(_)
        | OptionalIndexedAccessResultType { .. }
        | PropertyType { .. }
        | ValuesType
        | ExactType
        | ReadOnlyType
        | PartialType
        | RequiredType
        | EnumType
        | ReactElementConfigType => acc,

        OptionalIndexedAccessNonMaybeType { index } => {
            use flow_typing_type::type_::OptionalIndexedAccessIndex::*;
            match index {
                OptionalIndexedAccessStrLitIndex(_) => acc,
                OptionalIndexedAccessTypeIndex(index_type) => {
                    visitor.type_(cx, pole_todo(), acc, index_type)
                }
            }
        }
        ReactCheckComponentConfig { props, .. } => props
            .values()
            .fold(acc, |acc, prop| visitor.prop(cx, pole_todo(), acc, prop)),
        ElementType { index_type, .. } => visitor.type_(cx, pole_todo(), acc, index_type),
        SpreadType(box flow_typing_type::type_::DestructorSpreadTypeData(_, ts, head_slice)) => {
            let acc = ts.iter().fold(acc, |acc, operand| {
                visitor.object_kit_spread_operand(cx, acc, operand)
            });
            if let Some(slice) = head_slice {
                visitor.object_kit_spread_operand_slice(cx, acc, slice)
            } else {
                acc
            }
        }
        SpreadTupleType(box flow_typing_type::type_::DestructorSpreadTupleTypeData {
            resolved,
            unresolved,
            ..
        }) => {
            let acc = resolved.iter().fold(acc, |acc, resolved_el| {
                use flow_typing_type::type_::ResolvedParam::*;
                match resolved_el {
                    ResolvedArg(box flow_typing_type::type_::ResolvedArgData(element, _)) => {
                        visitor.tuple_element(cx, pole_todo(), acc, element)
                    }
                    ResolvedSpreadArg(box flow_typing_type::type_::ResolvedSpreadArgData(
                        _,
                        arr,
                        _,
                    )) => visitor.arr_type(cx, pole_todo(), acc, arr),
                    ResolvedAnySpreadArg(..) => acc,
                }
            });
            unresolved.iter().fold(acc, |acc, unresolved_el| {
                use flow_typing_type::type_::UnresolvedParam::*;
                match unresolved_el {
                    UnresolvedArg(box flow_typing_type::type_::UnresolvedArgData(element, _)) => {
                        visitor.tuple_element(cx, pole_todo(), acc, element)
                    }
                    UnresolvedSpreadArg(t) => visitor.type_(cx, pole_todo(), acc, t),
                }
            })
        }
        RestType(_, t) => visitor.type_(cx, pole_todo(), acc, t),
        ConditionalType(box flow_typing_type::type_::DestructorConditionalTypeData {
            infer_tparams,
            extends_t,
            true_t,
            false_t,
            ..
        }) => {
            let acc = infer_tparams
                .iter()
                .fold(acc, |acc, tp| visitor.type_param(cx, pole_todo(), acc, tp));
            let acc = visitor.type_(cx, pole_todo(), acc, extends_t);
            let acc = visitor.type_(cx, pole_todo(), acc, true_t);
            visitor.type_(cx, pole_todo(), acc, false_t)
        }
        TypeMap(flow_typing_type::type_::TypeMap::ObjectKeyMirror) => acc,
        MappedType(box flow_typing_type::type_::DestructorMappedTypeData {
            property_type,
            name_type,
            homomorphic,
            ..
        }) => {
            let acc = visitor.type_(cx, pole_todo(), acc, property_type);
            let acc = match name_type {
                Some(t) => visitor.type_(cx, pole_todo(), acc, t),
                None => acc,
            };
            match homomorphic {
                flow_typing_type::type_::MappedTypeHomomorphicFlag::SemiHomomorphic(t) => {
                    visitor.type_(cx, pole_todo(), acc, t)
                }
                _ => acc,
            }
        }
    }
}

pub fn tvar_default<'cx, Acc, V: TypeVisitor<Acc> + ?Sized>(
    _visitor: &mut V,
    _cx: &mut Context<'cx>,
    _pole: Polarity,
    acc: Acc,
    _r: &Reason,
    _id: u32,
) -> Acc {
    // The default behavior here could be fleshed out a bit, to look up the graph,
    // handle Resolved and Unresolved cases, etc.
    acc
}

pub fn dict_type_default<'cx, Acc, V: TypeVisitor<Acc> + ?Sized>(
    visitor: &mut V,
    cx: &mut Context<'cx>,
    pole: Polarity,
    acc: Acc,
    dict: &DictType,
) -> Acc {
    let acc = visitor.type_(cx, pole_todo(), acc, &dict.key);
    visitor.type_(
        cx,
        Polarity::mult(pole, dict.dict_polarity),
        acc,
        &dict.value,
    )
}

pub fn props_default<'cx, Acc, V: TypeVisitor<Acc> + ?Sized>(
    visitor: &mut V,
    cx: &mut Context<'cx>,
    pole: Polarity,
    acc: Acc,
    props_id: flow_typing_type::type_::properties::Id,
) -> Acc {
    let props = cx.find_props(props_id);
    props
        .values()
        .fold(acc, |acc, prop| visitor.prop(cx, pole, acc, prop))
}

pub fn prop_default<'cx, Acc, V: TypeVisitor<Acc> + ?Sized>(
    visitor: &mut V,
    cx: &mut Context<'cx>,
    pole: Polarity,
    acc: Acc,
    prop: &Property,
) -> Acc {
    use flow_typing_type::type_::PropertyInner;
    match prop.deref() {
        PropertyInner::Field(fd) => {
            visitor.type_(cx, Polarity::mult(pole, fd.polarity), acc, &fd.type_)
        }
        PropertyInner::Method { type_, .. } => visitor.type_(cx, pole, acc, type_),
        PropertyInner::Get { type_, .. } => visitor.type_(cx, pole, acc, type_),
        PropertyInner::Set { type_, .. } => visitor.type_(cx, pole.inv(), acc, type_),
        PropertyInner::GetSet(gs) => {
            let acc = visitor.type_(cx, pole, acc, &gs.get_type);
            visitor.type_(cx, pole.inv(), acc, &gs.set_type)
        }
    }
}

pub fn call_prop_default<'cx, Acc, V: TypeVisitor<Acc> + ?Sized>(
    visitor: &mut V,
    cx: &mut Context<'cx>,
    pole: Polarity,
    acc: Acc,
    id: i32,
) -> Acc {
    let t = cx.find_call(id);
    visitor.type_(cx, pole, acc, &t)
}

pub fn exports_default<'cx, Acc, V: TypeVisitor<Acc> + ?Sized>(
    visitor: &mut V,
    cx: &mut Context<'cx>,
    pole: Polarity,
    acc: Acc,
    id: flow_typing_type::type_::exports::Id,
) -> Acc {
    let exports = cx.find_exports(id);
    exports.values().fold(acc, |acc, named_symbol| {
        let acc = visitor.type_(cx, pole, acc, &named_symbol.type_);
        match &named_symbol.type_for_extends {
            None => acc,
            Some(t) => visitor.type_(cx, pole, acc, t),
        }
    })
}

pub fn eval_id_default<'cx, Acc, V: TypeVisitor<Acc> + ?Sized>(
    visitor: &mut V,
    cx: &mut Context<'cx>,
    pole: Polarity,
    acc: Acc,
    id: flow_typing_type::type_::eval::Id,
) -> Acc {
    if let Some(t) = cx.evaluated().get(&id) {
        visitor.type_(cx, pole, acc, t)
    } else {
        acc
    }
}

pub fn type_param_default<'cx, Acc, V: TypeVisitor<Acc> + ?Sized>(
    visitor: &mut V,
    cx: &mut Context<'cx>,
    pole: Polarity,
    acc: Acc,
    tp: &TypeParam,
) -> Acc {
    let pole = Polarity::mult(pole, tp.polarity);
    let acc = visitor.type_(cx, pole, acc, &tp.bound);
    if let Some(ref default) = tp.default {
        visitor.type_(cx, pole, acc, default)
    } else {
        acc
    }
}

pub fn fun_type_default<'cx, Acc, V: TypeVisitor<Acc> + ?Sized>(
    visitor: &mut V,
    cx: &mut Context<'cx>,
    pole: Polarity,
    acc: Acc,
    ft: &FunType,
) -> Acc {
    let acc = visitor.type_(cx, pole, acc, &ft.this_t.0);
    let acc = ft.params.iter().fold(acc, |acc, param| {
        visitor.type_(cx, pole.inv(), acc, &param.1)
    });
    let acc = if let Some(rest_param) = &ft.rest_param {
        visitor.type_(cx, pole.inv(), acc, &rest_param.2)
    } else {
        acc
    };
    let acc = visitor.type_(cx, pole, acc, &ft.return_t);
    if let Some(ref tg) = ft.type_guard {
        visitor.fun_type_guard(cx, pole, acc, tg)
    } else {
        acc
    }
}

pub fn fun_type_guard_default<'cx, Acc, V: TypeVisitor<Acc> + ?Sized>(
    visitor: &mut V,
    cx: &mut Context<'cx>,
    pole: Polarity,
    acc: Acc,
    tg: &flow_typing_type::type_::TypeGuard,
) -> Acc {
    let flow_typing_type::type_::TypeGuardInner {
        inferred: _,
        reason: _,
        param_name: _,
        type_guard,
        kind: _,
    } = &**tg;
    match type_guard {
        Some(type_guard) => visitor.type_(cx, pole, acc, type_guard),
        None => acc,
    }
}

pub fn obj_flags_default<'cx, Acc, V: TypeVisitor<Acc> + ?Sized>(
    visitor: &mut V,
    cx: &mut Context<'cx>,
    pole: Polarity,
    acc: Acc,
    flags: &flow_typing_type::type_::Flags,
) -> Acc {
    match &flags.obj_kind {
        flow_typing_type::type_::ObjKind::Indexed(dict) => visitor.dict_type(cx, pole, acc, dict),
        flow_typing_type::type_::ObjKind::Exact | flow_typing_type::type_::ObjKind::Inexact => acc,
    }
}

pub fn obj_type_default<'cx, Acc, V: TypeVisitor<Acc> + ?Sized>(
    visitor: &mut V,
    cx: &mut Context<'cx>,
    pole: Polarity,
    acc: Acc,
    obj: &ObjType,
) -> Acc {
    let ObjType {
        flags,
        props_tmap,
        proto_t,
        call_t,
        // We intentionally do not visit reachable_targs. By definition, they are already reachable
        // by traversing the other fields. Until substitution keeps track of polarity, visitng the
        // other fields will be more accurate
        reachable_targs: _,
        strictness_kind: _,
        literal: _,
    } = obj;
    let acc = visitor.obj_flags(cx, pole, acc, flags);
    let acc = visitor.props(cx, pole, acc, props_tmap.dupe());
    let acc = visitor.type_(cx, pole, acc, proto_t);
    if let Some(call_t) = call_t {
        visitor.call_prop(cx, pole, acc, *call_t)
    } else {
        acc
    }
}

pub fn namespace_type_default<'cx, Acc, V: TypeVisitor<Acc> + ?Sized>(
    visitor: &mut V,
    cx: &mut Context<'cx>,
    pole: Polarity,
    acc: Acc,
    ns: &NamespaceType,
) -> Acc {
    let acc = visitor.type_(cx, pole, acc, &ns.values_type);
    visitor.props(cx, pole, acc, ns.types_tmap.dupe())
}

pub fn arr_type_default<'cx, Acc, V: TypeVisitor<Acc> + ?Sized>(
    visitor: &mut V,
    cx: &mut Context<'cx>,
    pole: Polarity,
    acc: Acc,
    arr: &ArrType,
) -> Acc {
    match arr {
        ArrType::ArrayAT(box ArrayATData {
            elem_t,
            tuple_view: None,
            ..
        }) => visitor.type_(cx, Polarity::Neutral, acc, elem_t),
        ArrType::ArrayAT(box ArrayATData {
            elem_t,
            tuple_view: Some(tuple_view),
            ..
        }) => {
            let acc = visitor.type_(cx, Polarity::Neutral, acc, elem_t);
            tuple_view
                .elements
                .iter()
                .chain(&tuple_view.rest)
                .fold(acc, |acc, el| visitor.tuple_element(cx, pole, acc, el))
        }
        ArrType::TupleAT(box TupleATData {
            elem_t,
            elements,
            rest,
            ..
        }) => {
            let acc = visitor.type_(cx, Polarity::Neutral, acc, elem_t);
            elements
                .iter()
                .chain(rest)
                .fold(acc, |acc, el| visitor.tuple_element(cx, pole, acc, el))
        }
        ArrType::ROArrayAT(box ROArrayATData { elem_t: t, .. }) => visitor.type_(cx, pole, acc, t),
    }
}

pub fn tuple_element_default<'cx, Acc, V: TypeVisitor<Acc> + ?Sized>(
    visitor: &mut V,
    cx: &mut Context<'cx>,
    pole: Polarity,
    acc: Acc,
    element: &TupleElement,
) -> Acc {
    let TupleElement {
        reason: _,
        name: _,
        t,
        polarity: _,
        optional: _,
    } = element;
    visitor.type_(cx, Polarity::mult(pole, element.polarity), acc, t)
}

pub fn inst_type_default<'cx, Acc, V: TypeVisitor<Acc> + ?Sized>(
    visitor: &mut V,
    cx: &mut Context<'cx>,
    pole: Polarity,
    acc: Acc,
    inst: &InstType,
) -> Acc {
    let acc = inst
        .type_args
        .iter()
        .fold(acc, |acc, (_, _, t, pole_prime)| {
            visitor.type_(cx, Polarity::mult(pole, *pole_prime), acc, t)
        });
    let acc = visitor.props(cx, pole, acc, inst.own_props.dupe());
    let acc = visitor.props(cx, pole, acc, inst.proto_props.dupe());
    let acc = if let Some(call_t) = inst.inst_call_t {
        visitor.call_prop(cx, pole, acc, call_t)
    } else {
        acc
    };
    let acc = if let Some(construct_t) = inst.inst_construct_t {
        visitor.call_prop(cx, pole, acc, construct_t)
    } else {
        acc
    };
    let acc = if let Some(ref dict) = inst.inst_dict {
        visitor.dict_type(cx, pole_todo(), acc, dict)
    } else {
        acc
    };
    let acc = visitor.props(cx, pole_todo(), acc, inst.class_private_fields.dupe());
    let acc = visitor.props(
        cx,
        pole_todo(),
        acc,
        inst.class_private_static_fields.dupe(),
    );
    let acc = visitor.props(cx, pole_todo(), acc, inst.class_private_methods.dupe());
    visitor.props(
        cx,
        pole_todo(),
        acc,
        inst.class_private_static_methods.dupe(),
    )
}

pub fn instance_type_default<'cx, Acc, V: TypeVisitor<Acc> + ?Sized>(
    visitor: &mut V,
    cx: &mut Context<'cx>,
    pole: Polarity,
    acc: Acc,
    instance: &InstanceT,
) -> Acc {
    let acc = visitor.type_(cx, pole, acc, &instance.static_);
    let acc = visitor.type_(cx, pole, acc, &instance.super_);
    let acc = instance
        .implements
        .iter()
        .fold(acc, |acc, t| visitor.type_(cx, pole_todo(), acc, t));
    visitor.inst_type(cx, pole, acc, &instance.inst)
}

pub fn export_types_default<'cx, Acc, V: TypeVisitor<Acc> + ?Sized>(
    visitor: &mut V,
    cx: &mut Context<'cx>,
    pole: Polarity,
    acc: Acc,
    exports: &ExportTypes,
) -> Acc {
    let acc = visitor.exports(cx, pole, acc, exports.value_exports_tmap);
    let acc = visitor.exports(cx, pole, acc, exports.type_exports_tmap);
    if let Some((_, t)) = &exports.cjs_export {
        visitor.type_(cx, pole, acc, t)
    } else {
        acc
    }
}

pub fn object_kit_spread_operand_slice_default<'cx, Acc, V: TypeVisitor<Acc> + ?Sized>(
    visitor: &mut V,
    cx: &mut Context<'cx>,
    acc: Acc,
    slice: &flow_typing_type::type_::object::spread::OperandSlice,
) -> Acc {
    let flow_typing_type::type_::object::spread::OperandSliceInner {
        reason: _,
        prop_map,
        generics: _,
        dict,
        // See obj_type for why we don't visit reachable targs
        reachable_targs: _,
    } = &**slice;
    let acc = prop_map.values().fold(acc, |acc, prop| {
        use flow_typing_type::type_::PropertyInner;
        match prop.deref() {
            PropertyInner::Field(fd) => visitor.type_(cx, pole_todo(), acc, &fd.type_),
            PropertyInner::Method { type_, .. }
            | PropertyInner::Get { type_, .. }
            | PropertyInner::Set { type_, .. } => visitor.type_(cx, pole_todo(), acc, type_),
            PropertyInner::GetSet(gs) => {
                let acc = visitor.type_(cx, pole_todo(), acc, &gs.get_type);
                visitor.type_(cx, pole_todo(), acc, &gs.set_type)
            }
        }
    });
    if let Some(dict) = dict {
        visitor.dict_type(cx, pole_todo(), acc, dict)
    } else {
        acc
    }
}

pub fn object_kit_spread_operand_default<'cx, Acc, V: TypeVisitor<Acc> + ?Sized>(
    visitor: &mut V,
    cx: &mut Context<'cx>,
    acc: Acc,
    operand: &flow_typing_type::type_::object::spread::Operand,
) -> Acc {
    use flow_typing_type::type_::object::spread::Operand;
    match operand {
        Operand::Slice(slice) => visitor.object_kit_spread_operand_slice(cx, acc, slice),
        Operand::Type(t) => visitor.type_(cx, pole_todo(), acc, t),
    }
}
