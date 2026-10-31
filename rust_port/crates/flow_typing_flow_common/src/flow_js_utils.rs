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
use std::rc::Rc;
use std::sync::Arc;
use std::sync::LazyLock;

use dupe::Dupe;
use dupe::IterDupedExt;
use flow_aloc::ALoc;
use flow_aloc::ALocId;
use flow_common::reason::Name;
use flow_common::reason::Reason;
use flow_common::reason::VirtualReasonDesc;
use flow_common::reason::code_desc_of_expression;
use flow_common_utils::utils_js;
use flow_data_structure_wrapper::ord_map::FlowOrdMap;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_lazy::ContextFamily;
use flow_parser::ast;
use flow_parser::loc_sig::LocSig;
use flow_parser_utils::signature_utils;
use flow_typing_context::Context;
use flow_typing_context::ContextId;
use flow_typing_context::Cx;
use flow_typing_errors::error_message::EArithmeticOperandData;
use flow_typing_errors::error_message::EBuiltinModuleLookupFailedData;
use flow_typing_errors::error_message::EBuiltinNameLookupFailedData;
use flow_typing_errors::error_message::EIncompatibleTypeData;
use flow_typing_errors::error_message::EIncompatibleTypesWithUseOpData;
use flow_typing_errors::error_message::EInvalidBinaryArithData;
use flow_typing_errors::error_message::EPropNotFoundInSubtypingData;
use flow_typing_errors::error_message::EPropNotReadableData;
use flow_typing_errors::error_message::ETupleElementNotReadableData;
use flow_typing_errors::error_message::ETupleElementNotWritableData;
use flow_typing_errors::error_message::ETupleNonIntegerIndexData;
use flow_typing_errors::error_message::ETupleOutOfBoundsData;
use flow_typing_errors::error_message::ETupleRequiredAfterOptionalData;
use flow_typing_errors::error_message::EUmdGlobalValueInModuleData;
use flow_typing_errors::error_message::ErrorMessage;
use flow_typing_errors::error_message::ErrorTypeReferenceData;
use flow_typing_errors::error_message::ErrorTypeReferenceWithLocData;
use flow_typing_errors::error_message::IncompatibleUpperData;
use flow_typing_errors::intermediate_error_types::Explanation;
use flow_typing_errors::intermediate_error_types::ExpressionFunctionKind;
use flow_typing_errors::intermediate_error_types::ExpressionReferenceData;
use flow_typing_errors::intermediate_error_types::ExpressionReferenceKind;
use flow_typing_errors::intermediate_error_types::TupleElementReferenceData;
use flow_typing_errors::intermediate_error_types::TypeGuardReferenceData;
use flow_typing_errors::intermediate_error_types::TypeGuardReferenceKind;
use flow_typing_errors::intermediate_error_types::ValueAsTypeReference;
use flow_typing_flow_js_env::FlowJsEnv;
use flow_typing_type::type_::AnySource;
use flow_typing_type::type_::CallElemTData;
use flow_typing_type::type_::ClassExtendsCheckData;
use flow_typing_type::type_::ComponentRestParamCompatibilityData;
use flow_typing_type::type_::DefTInner;
use flow_typing_type::type_::DepthTrace;
use flow_typing_type::type_::ElemTData;
use flow_typing_type::type_::EvalMappedTypeData;
use flow_typing_type::type_::ExportKind;
use flow_typing_type::type_::GenericTData;
use flow_typing_type::type_::GetElemTData;
use flow_typing_type::type_::HasOwnPropTData;
use flow_typing_type::type_::InstType;
use flow_typing_type::type_::InstanceKind;
use flow_typing_type::type_::InstanceT;
use flow_typing_type::type_::MapTypeTData;
use flow_typing_type::type_::MergedDeclarationData;
use flow_typing_type::type_::MethodTData;
use flow_typing_type::type_::MixedFlavor;
use flow_typing_type::type_::NamedSymbol;
use flow_typing_type::type_::PolyTData;
use flow_typing_type::type_::ROArrayATData;
use flow_typing_type::type_::ResolveUnionTData;
use flow_typing_type::type_::SetElemTData;
use flow_typing_type::type_::SpecializeTData;
use flow_typing_type::type_::ThisInstanceTData;
use flow_typing_type::type_::Type;
use flow_typing_type::type_::TypeAppTData;
use flow_typing_type::type_::TypeInner;
use flow_typing_type::type_::TypeParam;
use flow_typing_type::type_::TypeStrictnessKind;
use flow_typing_type::type_::UnaryArithKind;
use flow_typing_type::type_::UniqueSymbolTData;
use flow_typing_type::type_::UseOp;
use flow_typing_type::type_::UseT;
use flow_typing_type::type_::UseTInner;
use flow_typing_type::type_::arith_kind::ArithKind;
use flow_typing_type::type_::constraint::Constraints;
use flow_typing_type::type_::inter_rep;
use flow_typing_type::type_::properties;
use flow_typing_type::type_::type_or_type_desc::ErrorTypeDesc;
use flow_typing_type::type_::type_or_type_desc::TypeOrTypeDescT;
use flow_typing_type::type_::union_rep;
use flow_typing_type::type_::union_rep::UnionKind;
use flow_typing_type::type_::unknown_use;
use flow_typing_type::type_util;
use flow_typing_type::type_util::reason_of_t;
use flow_utils_concurrency::job_error::JobError;
use vec1::Vec1;

// For any constraints, return a list of def types that form either the lower
// bounds of the solution, or a singleton containing the solution itself.
pub fn types_of<'cx>(cx: &mut Context<'cx>, constraints: &Constraints<Cx>) -> Vec<Type> {
    match constraints {
        Constraints::Unresolved(bounds) => bounds.borrow().lower.keys().duped().collect(),
        Constraints::Resolved(t) => vec![t.dupe()],
        Constraints::FullyResolved(s) => vec![cx.force_fully_resolved_tvar(s)],
    }
}

pub fn uses_of<'cx>(cx: &mut Context<'cx>, constraints: &Constraints<Cx>) -> Vec<UseT<Cx>> {
    match constraints {
        Constraints::Unresolved(bounds) => bounds
            .borrow()
            .upper
            .keys()
            .map(|key| key.use_t.dupe())
            .collect(),
        Constraints::Resolved(t) => vec![UseT::new(UseTInner::UseT(unknown_use(), t.dupe()))],
        Constraints::FullyResolved(s) => {
            vec![UseT::new(UseTInner::UseT(
                unknown_use(),
                cx.force_fully_resolved_tvar(s),
            ))]
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolymorphicClassKind {
    Poly,
    Mono,
    Other,
}

pub fn polymorphic_class_kind<'cx>(cx: &mut Context<'cx>, t: &Type) -> PolymorphicClassKind {
    fn resolve<'cx>(cx: &mut Context<'cx>, t: &Type) -> Type {
        match t.deref() {
            _ if let Some(node) = type_util::constraint_node_id(t) => {
                match cx.find_constraints(node.id()).1 {
                    Constraints::Resolved(t) => resolve(cx, &t),
                    Constraints::FullyResolved(s) => {
                        let forced = cx.force_fully_resolved_tvar(&s);
                        resolve(cx, &forced)
                    }
                    Constraints::Unresolved(_) => t.dupe(),
                }
            }
            TypeInner::AnnotT(_, inner_t, _) => resolve(cx, inner_t),
            _ => t.dupe(),
        }
    }

    match resolve(cx, t).deref() {
        TypeInner::DefT(_, def_t) => match def_t.deref() {
            DefTInner::PolyT(box PolyTData { t_out, .. }) => match t_out.deref() {
                TypeInner::DefT(_, inner_def_t) => match inner_def_t.deref() {
                    DefTInner::ClassT(inst)
                        if matches!(inst.deref(), TypeInner::ThisInstanceT(_)) =>
                    {
                        PolymorphicClassKind::Poly
                    }
                    _ => PolymorphicClassKind::Other,
                },
                _ => PolymorphicClassKind::Other,
            },
            DefTInner::ClassT(inst) if matches!(inst.deref(), TypeInner::ThisInstanceT(_)) => {
                PolymorphicClassKind::Mono
            }
            _ => PolymorphicClassKind::Other,
        },
        _ => PolymorphicClassKind::Other,
    }
}

// These possible_* functions would ideally be in constraint.rs, but since they use
// Context and Context depends on Constraint we need to extract these functions
// to a separate module in order to avoid a circular dependency
//
// Def types that describe the solution of a type variable.
pub fn possible_types<'cx>(cx: &mut Context<'cx>, id: i32) -> Vec<Type> {
    let (_, constraints) = cx.find_constraints(id);
    types_of(cx, &constraints)
}

pub fn possible_types_of_type<'cx>(cx: &mut Context<'cx>, t: &Type) -> Vec<Type> {
    match type_util::constraint_node_id(t) {
        Some(node) => possible_types(cx, node.id()),
        None => Vec::new(),
    }
}

pub fn possible_uses<'cx>(cx: &mut Context<'cx>, id: i32) -> Vec<UseT<Cx>> {
    let (_, constraints) = cx.find_constraints(id);
    uses_of(cx, &constraints)
}

// Does [t] carry an [abstract] bit? Any-abstract over both connectives: a
// union may be the abstract branch at runtime (the iteration over
// [concretize]'s split), and an intersection IS abstract if any branch is.
// [concretize] handles the wrappers ([AnnotT], [TypeAppT], unions, resolved
// tvars, etc.), so only the structural forms remain here.
pub fn is_class_abstract(
    cx: &mut Context<'_>,
    concretize: &dyn Fn(&mut Context<'_>, &Type) -> Result<Vec<Type>, FlowJsException>,
    t: &Type,
) -> Result<bool, FlowJsException> {
    for t in concretize(cx, t)? {
        let is_abstract = match t.deref() {
            TypeInner::DefT(_, def_t) if let DefTInner::InstanceT(inst_t) = def_t.deref() => {
                inst_t.inst.inst_abstract
            }
            TypeInner::ThisInstanceT(box ThisInstanceTData { instance, .. }) => {
                instance.inst.inst_abstract
            }
            TypeInner::DefT(_, def_t) if let DefTInner::ClassT(inner) = def_t.deref() => {
                is_class_abstract(cx, concretize, inner)?
            }
            TypeInner::DefT(_, def_t)
                if let DefTInner::PolyT(box PolyTData { t_out, .. }) = def_t.deref() =>
            {
                is_class_abstract(cx, concretize, t_out)?
            }
            TypeInner::GenericT(box GenericTData { bound, .. }) => {
                is_class_abstract(cx, concretize, bound)?
            }
            TypeInner::IntersectionT(_, rep) => {
                let mut any = false;
                for m in rep.members_iter() {
                    if is_class_abstract(cx, concretize, m)? {
                        any = true;
                        break;
                    }
                }
                any
            }
            _ => false,
        };
        if is_abstract {
            return Ok(true);
        }
    }
    Ok(false)
}

// Both writer pipelines ([type_annotation.rs] and [type_sig_merge.rs]) store
// the raw funtype produced by their respective signature lowering. Subtyping
// requires the construct-sig form (unbound [this_t], optional [return_t]
// override), so consumers normalize on read. Centralizing the normalize here
// keeps the "always normalize on read" invariant in one place rather than
// open-coded at each call site.
// (Lives here rather than [type_util.rs] because [Context] sits above
// [TypeUtil] in the build graph.)
pub fn read_construct_t<'cx>(cx: &mut Context<'cx>, id: i32) -> Type {
    flow_typing_type::type_util::normalize_construct_sig(None, cx.find_call(id))
}

// Walk a type, collecting all [inst_construct_t]s reachable through interface
// inheritance ([super]) and the standard set of wrappers. Returns sigs in
// source order: own sigs first, then sigs inherited via [extends]
// (derived-first). Order is preserved so callers can rely on a stable shape
// when overload resolution is order-sensitive (e.g. [infer]'s overload pick).
// Only [InterfaceKind] walks [super]; class [inst_kind]s use the
// [proto_props.constructor] mechanism handled by [extract_class_ctor_t].
pub fn collect_construct_ts<'cx>(
    concretize: &dyn Fn(&mut Context<'_>, &Type) -> Result<Vec<Type>, FlowJsException>,
    cx: &mut Context<'cx>,
    t: &Type,
) -> Result<Vec<Type>, FlowJsException> {
    fn go<'cx>(
        concretize: &dyn Fn(&mut Context<'_>, &Type) -> Result<Vec<Type>, FlowJsException>,
        cx: &mut Context<'cx>,
        mut acc: Vec<Type>,
        t: &Type,
    ) -> Result<Vec<Type>, FlowJsException> {
        match t.deref() {
            TypeInner::ObjProtoT(_) | TypeInner::FunProtoT(_) | TypeInner::NullProtoT(_) => {
                return Ok(acc);
            }
            TypeInner::DefT(_, def_t) if matches!(def_t.deref(), DefTInner::NullT) => {
                return Ok(acc);
            }
            _ => {}
        }
        for t in concretize(cx, t)? {
            acc = match t.deref() {
                TypeInner::DefT(_, def_t) if let DefTInner::ClassT(inner) = def_t.deref() => {
                    go(concretize, cx, acc, inner)?
                }
                TypeInner::DefT(_, def_t) if let DefTInner::InstanceT(inst_t) = def_t.deref() => {
                    if let Some(id) = inst_t.inst.inst_construct_t {
                        acc.push(read_construct_t(cx, id));
                    }
                    match &inst_t.inst.inst_kind {
                        InstanceKind::InterfaceKind { .. } => {
                            go(concretize, cx, acc, &inst_t.super_)?
                        }
                        _ => acc,
                    }
                }
                TypeInner::ThisInstanceT(box ThisInstanceTData { instance, .. }) => {
                    if let Some(id) = instance.inst.inst_construct_t {
                        acc.push(read_construct_t(cx, id));
                    }
                    match &instance.inst.inst_kind {
                        InstanceKind::InterfaceKind { .. } => {
                            go(concretize, cx, acc, &instance.super_)?
                        }
                        _ => acc,
                    }
                }
                TypeInner::GenericT(box GenericTData { bound, .. }) => {
                    go(concretize, cx, acc, bound)?
                }
                TypeInner::IntersectionT(_, rep) => {
                    for member in rep.members_iter() {
                        acc = go(concretize, cx, acc, member)?;
                    }
                    acc
                }
                _ => acc,
            };
        }
        Ok(acc)
    }
    go(concretize, cx, Vec::new(), t)
}

// The type of the `prototype` property an interface-typed constructor value
// declares, searched through the same wrappers and [extends] as
// [collect_construct_ts]. It stands in for the instance where a value has no
// construct signature: TypeScript's `SymbolConstructor` has a `prototype` but no
// `new`, yet `x instanceof Symbol` narrows and `declare class C extends Symbol`
// has an instance to inherit.
pub fn collect_prototype_t<'cx>(
    concretize: &dyn Fn(&mut Context<'_>, &Type) -> Result<Vec<Type>, FlowJsException>,
    cx: &mut Context<'cx>,
    t: &Type,
) -> Result<Option<Type>, FlowJsException> {
    fn go<'cx>(
        concretize: &dyn Fn(&mut Context<'_>, &Type) -> Result<Vec<Type>, FlowJsException>,
        cx: &mut Context<'cx>,
        name: &Name,
        t: &Type,
    ) -> Result<Option<Type>, FlowJsException> {
        use flow_typing_type::type_::property;

        match t.deref() {
            TypeInner::ObjProtoT(_) | TypeInner::FunProtoT(_) | TypeInner::NullProtoT(_) => {
                return Ok(None);
            }
            TypeInner::DefT(_, def_t) if matches!(def_t.deref(), DefTInner::NullT) => {
                return Ok(None);
            }
            _ => {}
        }
        for t in concretize(cx, t)? {
            let (inst, super_) = match t.deref() {
                TypeInner::DefT(_, def_t) if let DefTInner::InstanceT(inst_t) = def_t.deref() => {
                    (&inst_t.inst, &inst_t.super_)
                }
                TypeInner::ThisInstanceT(box ThisInstanceTData { instance, .. }) => {
                    (&instance.inst, &instance.super_)
                }
                TypeInner::GenericT(box GenericTData { bound, .. }) => {
                    if let Some(prototype) = go(concretize, cx, name, bound)? {
                        return Ok(Some(prototype));
                    }
                    continue;
                }
                TypeInner::IntersectionT(_, rep) => {
                    for member in rep.members_iter() {
                        if let Some(prototype) = go(concretize, cx, name, member)? {
                            return Ok(Some(prototype));
                        }
                    }
                    continue;
                }
                _ => continue,
            };
            if !matches!(inst.inst_kind, InstanceKind::InterfaceKind { .. }) {
                continue;
            }
            for props in [inst.own_props.dupe(), inst.proto_props.dupe()] {
                if let Some(prop) = cx.get_prop(props, name)
                    && let Some(prototype) = property::read_t(&prop)
                {
                    // As in TypeScript, a `prototype` of type `any` says nothing
                    // about the instance.
                    if concretize(cx, &prototype)?
                        .iter()
                        .all(|t| matches!(t.deref(), TypeInner::AnyT(..)))
                    {
                        return Ok(None);
                    }
                    return Ok(Some(prototype));
                }
            }
            if let Some(prototype) = go(concretize, cx, name, super_)? {
                return Ok(Some(prototype));
            }
        }
        Ok(None)
    }
    go(
        concretize,
        cx,
        &Name::new(FlowSmolStr::new_inline("prototype")),
        t,
    )
}

// Whether a value can be extended: it has a construct signature, or failing
// that, a `prototype` ([collect_prototype_t]).
pub fn is_inheritable<'cx>(
    concretize: &dyn Fn(&mut Context<'_>, &Type) -> Result<Vec<Type>, FlowJsException>,
    cx: &mut Context<'cx>,
    t: &Type,
) -> Result<bool, FlowJsException> {
    Ok(!collect_construct_ts(concretize, cx, t)?.is_empty()
        || collect_prototype_t(concretize, cx, t)?.is_some())
}

// What each construct signature returns, overloads flattened out, in the source
// order [collect_construct_ts] preserves.
fn construct_return_ts(construct_ts: &[Type]) -> Vec<Type> {
    construct_ts
        .iter()
        .flat_map(|construct_t| match construct_t.deref() {
            TypeInner::IntersectionT(_, rep) => rep.members_iter().duped().collect::<Vec<_>>(),
            _ => vec![construct_t.dupe()],
        })
        .filter_map(|construct_t| match construct_t.deref() {
            TypeInner::DefT(_, def_t) if let DefTInner::FunT(_, funtype) = def_t.deref() => {
                Some(funtype.return_t.dupe())
            }
            _ => None,
        })
        .collect()
}

/// Returns the instance types produced by all construct signatures reachable from `t`.
///
/// `concretize_sig` runs over each signature before its return is read. A
/// polymorphic signature is stored as a [PolyT] wrapping the [FunT], which
/// [construct_return_ts] does not match; concretizing it the way an `instanceof`
/// RHS is concretized instantiates the parameters first, so `new <T>(v: T):
/// Box<T>` contributes `Box<any>` instead of being dropped.
pub fn collect_construct_return_ts<'cx>(
    concretize: &dyn Fn(&mut Context<'_>, &Type) -> Result<Vec<Type>, FlowJsException>,
    concretize_sig: &dyn Fn(&mut Context<'_>, &Type) -> Result<Vec<Type>, FlowJsException>,
    cx: &mut Context<'cx>,
    t: &Type,
) -> Result<Vec<Type>, FlowJsException> {
    let construct_ts = collect_construct_ts(concretize, cx, t)?
        .iter()
        .map(|t| concretize_sig(cx, t))
        .collect::<Result<Vec<_>, _>>()?
        .concat();
    Ok(construct_return_ts(&construct_ts))
}

// A construct signature as a `constructor` method: same parameters, returning
// void. The instance `new` produces comes from the `ClassT ~> ConstructorT`
// rule, which hands back the class being constructed only when the constructor
// itself returns a non-object — exactly how a class's own
// `constructor(): void` behaves. Leaving the signature's return in place would
// make `new Derived()` the base's instance instead of the derived one.
fn void_returning(construct: &Type) -> Type {
    use flow_typing_type::type_::DefT;

    let rebuild = |t: &Type| match t.deref() {
        TypeInner::DefT(r, def_t) if let DefTInner::FunT(static_, ft) = def_t.deref() => {
            let mut void_ft = (**ft).clone();
            void_ft.return_t = Type::new(TypeInner::DefT(r.dupe(), DefT::new(DefTInner::VoidT)));
            Type::new(TypeInner::DefT(
                r.dupe(),
                DefT::new(DefTInner::FunT(static_.dupe(), Rc::new(void_ft))),
            ))
        }
        _ => t.dupe(),
    };
    match construct.deref() {
        TypeInner::IntersectionT(r, rep) => {
            let mut voided = rep.members_iter().map(rebuild);
            match (voided.next(), voided.next()) {
                (Some(t0), Some(t1)) => Type::new(TypeInner::IntersectionT(
                    r.dupe(),
                    inter_rep::make(t0, t1, voided.collect::<Vec<_>>().into()),
                )),
                _ => rebuild(construct),
            }
        }
        _ => rebuild(construct),
    }
}

// The instance a derived class inherits from when its base is a value with a
// construct signature: what the first signature returns, but re-pointed at the
// constructor object for the two things that live on the object rather than on
// the instance it returns — the statics and the construct signature itself.
//
// A class's statics are reached as `Class<instance>` -> `instance.static_`, so
// the derived class only inherits `Base.of` / `new Base()` if the instance it
// extends carries them. `node.js.flow`'s `declare class Buffer extends
// Uint8Array` needs both: `Buffer.from` resolves through `static_`, and
// `new Buffer()` through [inst_construct_t].
//
// A value without a construct signature contributes its `prototype` instead
// ([collect_prototype_t]): `node.js.flow`'s `declare class $SymbolReplModeMagic
// extends Symbol` extends TypeScript's `SymbolConstructor`, which has no `new`.
pub fn construct_base_instance<'cx>(
    concretize: &dyn Fn(
        &mut Context<'_>,
        &Type,
        Option<&Type>,
    ) -> Result<Vec<Type>, FlowJsException>,
    cx: &mut Context<'cx>,
    t: &Type,
    this: &Type,
) -> Result<Option<Type>, FlowJsException> {
    use flow_typing_type::type_::DefT;
    use flow_typing_type::type_::InstTypeInner;
    use flow_typing_type::type_::InstanceTInner;
    use flow_typing_type::type_::Property;
    use flow_typing_type::type_::PropertyInner;

    let concretize_fully = |cx: &mut Context<'_>, t: &Type| concretize(cx, t, None);
    let construct_ts = collect_construct_ts(&concretize_fully, cx, t)?
        .iter()
        .map(|t| concretize_fully(cx, t))
        .collect::<Result<Vec<_>, _>>()?
        .concat();
    let base = if construct_ts.is_empty() {
        let Some(prototype) = collect_prototype_t(&concretize_fully, cx, t)? else {
            return Ok(None);
        };
        prototype
    } else {
        let Some(base) = construct_return_ts(&construct_ts).into_iter().next() else {
            return Ok(None);
        };
        base
    };
    // The return type arrives still wrapped in whatever the signature wrote —
    // an [AnnotT] over the interface, typically — and only the instance
    // underneath has the slots to re-point. A return that concretizes to
    // anything else, or to more than one thing, stands as it is.
    //
    // A `this`-typed member of that instance (`subarray(): this` on a typed
    // array) has to refer to the derived class, as it does when extending a
    // class. Concretize the return with the derived `this` so that any type
    // application reached through annotations, tvars, or evaluations is
    // this-specialized before it fixes its own `this`.
    let concrete = concretize(cx, &base, Some(this))?;
    let [concrete] = concrete.as_slice() else {
        return Ok(Some(base.dupe()));
    };
    let TypeInner::DefT(base_reason, def_t) = concrete.deref() else {
        return Ok(Some(base.dupe()));
    };
    let DefTInner::InstanceT(instance) = def_t.deref() else {
        return Ok(Some(base.dupe()));
    };
    let construct = combine_construct_ts(construct_ts);
    let inst = InstType::new(InstTypeInner {
        proto_props: match &construct {
            // `new Derived()` resolves `constructor` as a prototype method —
            // the slot a class writes its own constructor into — so the
            // signature has to be reachable there. [inst_construct_t] alone is
            // not enough: it is only consulted for structural subtyping.
            Some(construct) => {
                let mut props = cx.find_props(instance.inst.proto_props.dupe());
                props.insert(
                    Name::new(FlowSmolStr::new_inline("constructor")),
                    Property::new(PropertyInner::Method {
                        key_loc: None,
                        type_: void_returning(construct),
                    }),
                );
                cx.generate_property_map(props)
            }
            None => instance.inst.proto_props.dupe(),
        },
        inst_construct_t: construct.map(|c| cx.make_call_prop(c)),
        ..(*instance.inst).clone()
    });
    Ok(Some(Type::new(TypeInner::DefT(
        base_reason.dupe(),
        DefT::new(DefTInner::InstanceT(Rc::new(InstanceT::new(
            InstanceTInner {
                inst,
                static_: t.dupe(),
                super_: instance.super_.dupe(),
                implements: instance.implements.dupe(),
            },
        )))),
    ))))
}

// The overloads among construct sigs that `n` type arguments can specialize:
// the ones whose type parameters accept that many, the way TypeScript picks the
// base constructors of `extends Base<...>`.
pub fn construct_ts_for_targs(ts: &[Type], n: usize) -> Vec<Type> {
    ts.iter()
        .flat_map(|t| match t.deref() {
            TypeInner::IntersectionT(_, rep) => rep.members_iter().duped().collect::<Vec<_>>(),
            _ => vec![t.dupe()],
        })
        .filter(|t| match t.deref() {
            TypeInner::DefT(_, def_t) => match def_t.deref() {
                DefTInner::PolyT(poly) => {
                    let required = poly
                        .tparams
                        .iter()
                        .filter(|tp| tp.default.is_none())
                        .count();
                    (required..=poly.tparams.len()).contains(&n)
                }
                DefTInner::FunT(..) => n == 0,
                _ => false,
            },
            _ => false,
        })
        .collect()
}

// Combine a list of construct sigs into the canonical overload form: a single
// funtype if there's only one, an [IntersectionT] of funtypes for two or more.
// This is the same shape [class_sig.rs] and [type_sig_merge.rs] produce for
// direct overloads, so [CallT] dispatch and structural subtyping get standard
// overload resolution for free.
//
// Members that are themselves [IntersectionT] are flattened: an interface's
// own slot already stores its overloads as an [IntersectionT], and an
// inherited slot does the same, so without flattening a child + inherited
// pair becomes [IntersectionT(IntersectionT(o0, o1), inh)] — a nesting
// [ty_normalizer]'s one-level [InterRep.members] unwrap doesn't handle.
pub fn combine_construct_ts(ts: Vec<Type>) -> Option<Type> {
    let flat: Vec<Type> = ts
        .into_iter()
        .flat_map(|t| match t.deref() {
            TypeInner::IntersectionT(_, rep) => rep.members_iter().duped().collect::<Vec<_>>(),
            _ => vec![t.dupe()],
        })
        .collect();
    let mut iter = flat.into_iter();
    let t0 = iter.next()?;
    match iter.next() {
        None => Some(t0),
        Some(t1) => {
            let rest: Vec<Type> = iter.collect();
            let reason = flow_typing_type::type_util::reason_of_t(&t0).dupe();
            Some(Type::new(TypeInner::IntersectionT(
                reason,
                inter_rep::make(t0, t1, rest.into()),
            )))
        }
    }
}

// Extract a construct sig from a class [this] (the instance type wrapped by
// [ClassT]). Walks the class's super chain looking for "constructor" in
// [proto_props], then normalizes: unbinds `this` (a construct sig has no
// meaningful receiver — `new` produces a fresh instance) and sets [return_t]
// to the instance. We walk manually rather than dispatching [MethodT] because
// [MethodT] with [NoMethodAction] returns an unresolved [OpenT] that
// [normalize_construct_sig] can't rewrite — by the time it resolves, subtyping
// has already seen the raw method-bound funtype.
pub fn extract_class_ctor_t<'cx>(
    concretize: &dyn Fn(&mut Context<'_>, &Type) -> Result<Vec<Type>, FlowJsException>,
    cx: &mut Context<'cx>,
    this: &Type,
) -> Result<Option<Type>, FlowJsException> {
    use flow_typing_type::type_::DefT;
    use flow_typing_type::type_::DefTInner;
    use flow_typing_type::type_::PolyTData;
    use flow_typing_type::type_::poly;
    use flow_typing_type::type_::properties;
    fn lookup_ctor<'cx>(
        cx: &mut Context<'cx>,
        proto_props: properties::Id,
        super_: &Type,
        recur: impl Fn(&mut Context<'cx>, &Type) -> Result<Option<Type>, FlowJsException>,
    ) -> Result<Option<Type>, FlowJsException> {
        let props = cx.find_props(proto_props);
        match props.get(&Name::new(FlowSmolStr::new_inline("constructor"))) {
            // A setter-only "constructor" is reachable: the parser emits
            // [ConstructorCannotBeAccessor] but still produces a [Method.Set] AST
            // node which lands in [s.setters] under "constructor", and the
            // default-ctor synthesizer's [Method] then loses to the setter in
            // [Class_sig.elements]'s left-biased [SMap.union]. Bail with [None]
            // rather than walking [super]: the user explicitly wrote
            // [set constructor], so silently inheriting a parent ctor would mask
            // the parse error and produce a misleading construct sig at
            // [new C(...)] sites.
            Some(p) => Ok(flow_typing_type::type_::property::read_t(p)),
            None => recur(cx, super_),
        }
    }
    fn find_ctor<'cx>(
        concretize: &dyn Fn(&mut Context<'_>, &Type) -> Result<Vec<Type>, FlowJsException>,
        cx: &mut Context<'cx>,
        t: &Type,
    ) -> Result<Option<Type>, FlowJsException> {
        for t in concretize(cx, t)? {
            let result = match t.deref() {
                TypeInner::DefT(_, def_t) if let DefTInner::InstanceT(inst_t) = def_t.deref() => {
                    lookup_ctor(
                        cx,
                        inst_t.inst.proto_props.dupe(),
                        &inst_t.super_,
                        |cx, t| find_ctor(concretize, cx, t),
                    )?
                }
                TypeInner::ThisInstanceT(box ThisInstanceTData { instance, .. }) => lookup_ctor(
                    cx,
                    instance.inst.proto_props.dupe(),
                    &instance.super_,
                    |cx, t| find_ctor(concretize, cx, t),
                )?,
                TypeInner::DefT(_, def_t) if let DefTInner::ClassT(inner) = def_t.deref() => {
                    find_ctor(concretize, cx, inner)?
                }
                // Preserve [PolyT] so generic classes carry their tparams through to
                // call sites. Without this, downstream sees unsubstituted [BoundT]
                // in argument/return positions. Fresh generated id — the wrapped
                // polytype is a different value from the class's [PolyT], so
                // memoization on the original id would be wrong.
                TypeInner::DefT(r, def_t)
                    if let DefTInner::PolyT(box PolyTData {
                        kind: poly_kind,
                        tparams_loc,
                        tparams,
                        t_out,
                        id: _,
                        strictness_kind,
                    }) = def_t.deref() =>
                {
                    find_ctor(concretize, cx, t_out)?.map(|ctor| {
                        Type::new(TypeInner::DefT(
                            r.dupe(),
                            DefT::new(DefTInner::PolyT(Box::new(PolyTData {
                                kind: *poly_kind,
                                tparams_loc: tparams_loc.dupe(),
                                tparams: tparams.dupe(),
                                t_out: ctor,
                                id: poly::Id::generate_id(),
                                strictness_kind: *strictness_kind,
                            }))),
                        ))
                    })
                }
                TypeInner::GenericT(box GenericTData { bound, .. }) => {
                    find_ctor(concretize, cx, bound)?
                }
                _ => None,
            };
            if let Some(ctor) = result {
                return Ok(Some(ctor));
            }
        }
        Ok(None)
    }
    Ok(find_ctor(concretize, cx, this)?
        .map(|ctor| flow_typing_type::type_util::normalize_construct_sig(Some(this.dupe()), ctor)))
}

pub fn collect_lowers<'cx>(
    filter_empty: bool,
    cx: &mut Context<'cx>,
    seen: &mut BTreeSet<i32>,
    acc: &mut Vec<Type>,
    ts: Vec<Type>,
) {
    let mut work_list = VecDeque::from(ts);
    while let Some(t) = work_list.pop_front() {
        match type_util::constraint_node_id(&t) {
            Some(node) => {
                let id = node.id();
                if seen.contains(&id) {
                    // already unwrapped, continue
                    continue;
                } else {
                    seen.insert(id);
                    let new_types = possible_types(cx, id);
                    for t in new_types.into_iter().rev() {
                        work_list.push_front(t);
                    }
                }
            }
            None if let TypeInner::DefT(_, def_t) = t.deref()
                && filter_empty =>
            {
                if matches!(def_t.deref(), DefTInner::EmptyT) {
                    continue;
                } else {
                    acc.push(t);
                }
            }
            _ => {
                acc.push(t);
            }
        }
    }
}

pub fn merge_tvar_opt<'cx>(
    cx: &mut Context<'cx>,
    filter_empty: bool,
    union_kind: UnionKind,
    reason: &Reason,
    id: i32,
) -> Option<Type> {
    let mut seen = BTreeSet::new();
    seen.insert(id);
    let mut lowers = Vec::new();
    let types = possible_types(cx, id);
    collect_lowers(filter_empty, cx, &mut seen, &mut lowers, types);
    match lowers.len() {
        1 => Some(lowers.into_iter().next().unwrap()),
        n if n >= 2 => {
            let mut iter = lowers.into_iter();
            let t0 = iter.next().unwrap();
            let t1 = iter.next().unwrap();
            let ts: Vec<Type> = iter.collect();
            Some(Type::new(TypeInner::UnionT(
                reason.dupe(),
                union_rep::make(None, union_kind, t0, t1, ts.into()),
            )))
        }
        _ => None,
    }
}

pub fn merge_tvar<'cx, F>(
    cx: &mut Context<'cx>,
    filter_empty: bool,
    no_lowers: F,
    reason: &Reason,
    id: i32,
) -> Type
where
    F: FnOnce(&mut Context, &Reason) -> Type,
{
    match merge_tvar_opt(cx, filter_empty, UnionKind::ResolvedKind, reason, id) {
        Some(t) => t,
        None => no_lowers(cx, reason),
    }
}

pub mod invalid_cyclic_type_validation {
    use std::collections::BTreeSet;
    use std::ops::Deref;

    use dupe::Dupe;
    use flow_common::polarity::Polarity;
    use flow_common::reason::Reason;
    use flow_typing_context::Context;
    use flow_typing_context::Cx;
    use flow_typing_type::type_::DefTInner;
    use flow_typing_type::type_::Type;
    use flow_typing_type::type_::TypeInner;
    use flow_typing_type::type_::constraint::Constraints;
    use flow_typing_type::type_::constraint::forcing_state::ForcingState;
    use flow_typing_visitors::type_visitor::TypeVisitor;
    use flow_typing_visitors::type_visitor::type_default;

    type Acc = (BTreeSet<i32>, bool);

    enum TvarVisitingVariant {
        ValidateLocalType,
        ValidateTypeSigType,
    }

    struct TvarForcingVisitor {
        variant: TvarVisitingVariant,
    }

    impl TypeVisitor<Acc> for TvarForcingVisitor {
        fn call_prop<'cx>(
            &mut self,
            _cx: &mut Context<'cx>,
            _pole: Polarity,
            acc: Acc,
            _id: i32,
        ) -> Acc {
            acc
        }

        fn props<'cx>(
            &mut self,
            _cx: &mut Context<'cx>,
            _pole: Polarity,
            acc: Acc,
            _props_id: flow_typing_type::type_::properties::Id,
        ) -> Acc {
            acc
        }

        fn exports<'cx>(
            &mut self,
            _cx: &mut Context<'cx>,
            _pole: Polarity,
            acc: Acc,
            _id: flow_typing_type::type_::exports::Id,
        ) -> Acc {
            acc
        }

        fn type_<'cx>(&mut self, cx: &mut Context<'cx>, pole: Polarity, acc: Acc, t: &Type) -> Acc {
            match t.deref() {
                TypeInner::TypeAppT(..) | TypeInner::ThisTypeAppT(..) => acc,
                TypeInner::ThisInstanceT(..) => acc,
                TypeInner::EvalT { id, .. } => {
                    let evaluated = cx.evaluated();
                    match evaluated.get(id) {
                        None => acc,
                        Some(eval_t) => self.type_(cx, pole, acc, eval_t),
                    }
                }
                TypeInner::DefT(_, def_t) => {
                    match def_t.deref() {
                        DefTInner::FunT(_, _)
                        | DefTInner::ArrT(_)
                        | DefTInner::InstanceT(_)
                        | DefTInner::ReactAbstractComponentT(_)
                        | DefTInner::RendersT(_)
                        | DefTInner::EnumValueT(_)
                        | DefTInner::EnumObjectT { .. } => acc,
                        DefTInner::ObjT(obj) => type_default(self, cx, pole, acc, &obj.proto_t),
                        // Base types
                        DefTInner::NumGeneralT(_)
                        | DefTInner::StrGeneralT(_)
                        | DefTInner::BoolGeneralT
                        | DefTInner::BigIntGeneralT(_)
                        | DefTInner::EmptyT
                        | DefTInner::MixedT(_)
                        | DefTInner::NullT
                        | DefTInner::VoidT
                        | DefTInner::SymbolT
                        | DefTInner::UniqueSymbolT(_)
                        | DefTInner::SingletonStrT { .. }
                        | DefTInner::NumericStrKeyT(_)
                        | DefTInner::SingletonNumT { .. }
                        | DefTInner::SingletonBoolT { .. }
                        | DefTInner::SingletonBigIntT { .. }
                        | DefTInner::ClassT(_)
                        | DefTInner::TypeT(_, _)
                        | DefTInner::PolyT(_) => type_default(self, cx, pole, acc, t),
                    }
                }
                TypeInner::GenericT(..)
                | TypeInner::AnyT(_, _)
                | TypeInner::FunProtoT(_)
                | TypeInner::ObjProtoT(_)
                | TypeInner::NullProtoT(_)
                | TypeInner::TemplateLiteralT { .. }
                | TypeInner::StringMappingT { .. }
                | TypeInner::OpenT(_)
                | TypeInner::ImplicitInstantiationTvar(_)
                | TypeInner::IntersectionT(_, _)
                | TypeInner::UnionT(_, _)
                | TypeInner::MaybeT(_, _)
                | TypeInner::OptionalT { .. }
                | TypeInner::KeysT(_, _)
                | TypeInner::AnnotT(_, _, _)
                | TypeInner::NominalT { .. }
                | TypeInner::NamespaceT(_) => type_default(self, cx, pole, acc, t),
            }
        }

        fn tvar<'cx>(
            &mut self,
            cx: &mut Context<'cx>,
            pole: Polarity,
            acc: Acc,
            r: &Reason,
            id: u32,
        ) -> Acc {
            match self.variant {
                TvarVisitingVariant::ValidateLocalType => {
                    let (mut seen, in_lazy_tvar) = acc;
                    let (root_id, constraints) = cx.find_constraints(id as i32);
                    let s = get_fully_resolved_state(cx, r, id as i32, constraints);
                    if !seen.contains(&root_id)
                        && (in_lazy_tvar || cx.delayed_forcing_tvars().contains(&root_id))
                    {
                        seen.insert(root_id);
                        let forced_t = cx.force_fully_resolved_tvar(&s);
                        let (seen, _) = self.type_(cx, pole, (seen, true), &forced_t);
                        (seen, in_lazy_tvar)
                    } else {
                        (seen, in_lazy_tvar)
                    }
                }
                TvarVisitingVariant::ValidateTypeSigType => {
                    let (mut seen, in_lazy_tvar) = acc;
                    let (root_id, constraints) = cx.find_constraints(id as i32);
                    let s = get_fully_resolved_state(cx, r, id as i32, constraints);
                    if !seen.contains(&root_id) {
                        seen.insert(root_id);
                        let forced_t = s.force(cx, |cx, reason| {
                            let msg = flow_typing_errors::error_message::ErrorMessage::ETrivialRecursiveDefinition(reason.loc().dupe());
                            super::add_annot_inference_error(cx, msg);
                            flow_typing_type::type_::any_t::error(reason.dupe())
                        });
                        self.type_(cx, pole, (seen, in_lazy_tvar), &forced_t)
                    } else {
                        (seen, in_lazy_tvar)
                    }
                }
            }
        }
    }

    fn get_fully_resolved_state<'cx>(
        cx: &mut Context<'cx>,
        r: &Reason,
        id: i32,
        constraints: Constraints<Cx>,
    ) -> ForcingState<Cx> {
        match constraints {
            Constraints::FullyResolved(s) => s,
            Constraints::Resolved(t) => {
                panic!(
                    "tvar ({}, {}) = {} is resolved but not fully resolved",
                    flow_typing_debug::dump_reason(cx, r),
                    id,
                    flow_typing_debug::dump_t(Some(3), cx, &t),
                )
            }
            Constraints::Unresolved(_) => {
                panic!(
                    "tvar ({}, {}) is unresolved",
                    flow_typing_debug::dump_reason(cx, r),
                    id,
                )
            }
        }
    }

    pub fn validate_local_type<'cx>(cx: &mut Context<'cx>, t: &Type) {
        let mut visitor = TvarForcingVisitor {
            variant: TvarVisitingVariant::ValidateLocalType,
        };
        visitor.type_(cx, Polarity::Positive, (BTreeSet::new(), false), t);
    }

    /// Validates the type `t` of `cx`, reporting errors to `dst_cx`.
    pub fn validate_type_sig_type<'cx>(cx: &mut Context<'cx>, t: &Type) {
        let mut visitor = TvarForcingVisitor {
            variant: TvarVisitingVariant::ValidateTypeSigType,
        };
        visitor.type_(cx, Polarity::Positive, (BTreeSet::new(), false), t);
    }
}

pub fn unwrap_fully_resolved_open_t<'cx>(cx: &mut Context<'cx>, t: &Type) -> Type {
    let unwrapped = match type_util::constraint_node_id(t) {
        Some(node) => {
            let r = type_util::reason_of_t(t);
            let id = node.id();
            match cx.find_constraints(id).1 {
                Constraints::FullyResolved(s) => cx.force_fully_resolved_tvar(&s),
                Constraints::Resolved(resolved_t) => {
                    panic!(
                        "tvar ({:?}, {}) = {:?} is resolved but not fully resolved",
                        r, id, resolved_t
                    );
                }
                Constraints::Unresolved(_) => {
                    panic!("tvar ({:?}, {}) is unresolved", r, id);
                }
            }
        }
        None => t.dupe(),
    };
    invalid_cyclic_type_validation::validate_local_type(cx, &unwrapped);
    unwrapped
}

/// The tvar is forced later, as a unit of checking of its own rather than a
/// continuation of whatever solve happens to force it, so `f` runs at
/// [`FlowJsEnv::entry`].
pub fn map_on_resolved_type<'cx, F>(cx: &mut Context<'cx>, reason_op: Reason, l: Type, f: F) -> Type
where
    F: for<'a> FnOnce(&mut Context<'a>, Type) -> Result<Type, JobError> + 'static,
{
    flow_typing_tvar::mk_fully_resolved_lazy(cx, reason_op, true, move |cx: &mut Context<'_>| {
        cx.run_in_signature_tvar_env(|cx| {
            let result = f(cx, l)?;
            Ok(unwrap_fully_resolved_open_t(cx, &result))
        })
    })
}

// Type predicates

// some types need to be resolved before proceeding further
pub fn needs_resolution(t: &Type) -> bool {
    if type_util::constraint_node_id(t).is_some() {
        return true;
    }

    match t.deref() {
        TypeInner::UnionT(_, _) => true,
        TypeInner::OptionalT { .. } => true,
        TypeInner::MaybeT(_, _) => true,
        TypeInner::AnnotT(_, _, _) => true,
        TypeInner::NominalT { nominal_type, .. } => {
            matches!(
                nominal_type.underlying_t,
                flow_typing_type::type_::nominal::UnderlyingT::CustomError(
                    box flow_typing_type::type_::nominal::CustomErrorData { .. },
                )
            )
        }
        _ => false,
    }
}

pub fn is_generic(t: &Type) -> bool {
    matches!(t.deref(), TypeInner::GenericT(..))
}

pub fn is_object_prototype_method(name: &Name) -> bool {
    matches!(
        name.as_str_opt(),
        Some(
            "isPrototypeOf"
                | "hasOwnProperty"
                | "propertyIsEnumerable"
                | "toLocaleString"
                | "toString"
                | "valueOf"
        )
    )
}

pub fn is_function_prototype(name: &Name) -> bool {
    matches!(
        name.as_str_opt(),
        Some("apply" | "bind" | "call" | "arguments" | "caller" | "length" | "name")
    ) || is_object_prototype_method(name)
}

// neither object prototype methods nor callable signatures should be
// implied by an object indexer type
pub fn is_dictionary_exempt(name: &Name) -> bool {
    is_object_prototype_method(name)
}

pub fn update_lit_type_from_annot<'cx>(cx: &mut Context<'cx>, env: &FlowJsEnv, t: &Type) {
    match t.deref() {
        TypeInner::DefT(r, def_t) => match def_t.deref() {
            DefTInner::SingletonStrT {
                from_annot: false, ..
            } if env.in_implicit_instantiation() => {
                cx.record_primitive_literal_check(r.loc().dupe());
            }
            DefTInner::SingletonNumT {
                from_annot: false, ..
            }
            | DefTInner::SingletonBoolT {
                from_annot: false, ..
            }
            | DefTInner::SingletonBigIntT {
                from_annot: false, ..
            } if env.in_implicit_instantiation() => {
                cx.record_primitive_literal_check(r.loc().dupe());
            }
            _ => {}
        },
        TypeInner::TemplateLiteralT {
            reason,
            from_annot: false,
            ..
        } if env.in_implicit_instantiation() => {
            cx.record_primitive_literal_check(reason.loc().dupe());
        }
        _ => {}
    }
}

pub fn is_date(t: &Type) -> bool {
    match t.deref() {
        TypeInner::DefT(reason, def_t) => {
            if let DefTInner::InstanceT(_) = def_t.deref() {
                flow_typing_type::type_::desc_format::name_of_instance_reason(reason) == "Date"
            } else {
                false
            }
        }
        _ => false,
    }
}

pub fn function_like(t: &Type) -> bool {
    match t.deref() {
        TypeInner::DefT(_, def_t) => {
            matches!(def_t.deref(), DefTInner::ClassT(_) | DefTInner::FunT(_, _))
        }
        _ => false,
    }
}

pub fn object_like(t: &Type) -> bool {
    match t.deref() {
        TypeInner::DefT(_, def_t) => {
            matches!(def_t.deref(), DefTInner::ObjT(_) | DefTInner::InstanceT(_))
                || function_like(t)
        }
        TypeInner::ThisInstanceT(..) => true,
        TypeInner::ObjProtoT(_) => true,
        TypeInner::FunProtoT(_) => true,
        TypeInner::AnyT(_, _) => true,
        _ => function_like(t),
    }
}

pub fn object_like_op<CX: ContextFamily>(u: &UseT<CX>) -> bool {
    match u.deref() {
        UseTInner::SetPropT(..)
        | UseTInner::GetPropT(..)
        | UseTInner::TestPropT(..)
        | UseTInner::MethodT(..)
        | UseTInner::LookupT(..)
        | UseTInner::GetProtoT(_, _)
        | UseTInner::SuperT(..)
        | UseTInner::GetKeysT { .. }
        | UseTInner::HasOwnPropT(..)
        | UseTInner::GetValuesT(_, _)
        | UseTInner::GetDictValuesT(_, _)
        | UseTInner::ObjRestT(..)
        | UseTInner::SetElemT(..)
        | UseTInner::GetElemT(..) => true,
        UseTInner::UseT(_, t) => matches!(t.deref(), TypeInner::AnyT(_, _)),
        _ => false,
    }
}

pub fn function_like_op(u: &UseT) -> bool {
    match u.deref() {
        UseTInner::CallT(..) => true,
        UseTInner::ConstructorT(..) => true,
        UseTInner::UseT(_, t) => matches!(t.deref(), TypeInner::AnyT(_, _)),
        _ => object_like_op(u),
    }
}

pub fn is_union_resolvable(t: &Type) -> bool {
    matches!(t.deref(), TypeInner::EvalT { .. } | TypeInner::KeysT(_, _))
}

pub mod tvar_visitors {

    use std::collections::BTreeSet;
    use std::ops::Deref;

    use flow_common::polarity::Polarity;
    use flow_typing_context::Context;
    use flow_typing_type::type_::AnySource;
    use flow_typing_type::type_::Destructor;
    use flow_typing_type::type_::Type;
    use flow_typing_type::type_::TypeInner;
    use flow_typing_type::type_::constraint::Constraints;
    use flow_typing_type::type_util;
    use flow_typing_visitors::type_visitor::TypeVisitor;
    use flow_typing_visitors::type_visitor::type_default;

    enum TvarVisitorResult {
        EncounteredPlaceholderType,
        EncounteredUnresolvedTvar,
    }

    type Seen = BTreeSet<i32>;

    struct HasPlaceholdersVisitor;

    impl HasPlaceholdersVisitor {
        fn constraint_node<'cx>(
            &mut self,
            cx: &mut Context<'cx>,
            pole: Polarity,
            acc: Result<Seen, TvarVisitorResult>,
            id: i32,
        ) -> Result<Seen, TvarVisitorResult> {
            let mut seen = acc?;
            let (root_id, constraints) = cx.find_constraints(id);
            if seen.contains(&root_id) {
                return Ok(seen);
            }

            seen.insert(root_id);
            match constraints {
                Constraints::FullyResolved(_) => Ok(seen),
                Constraints::Resolved(t) => self.type_(cx, pole, Ok(seen), &t),
                Constraints::Unresolved(bounds) => {
                    let lower: Vec<_> = bounds.borrow().lower.keys().cloned().collect();
                    lower
                        .iter()
                        .try_fold(seen, |seen, t| self.type_(cx, pole, Ok(seen), t))
                }
            }
        }
    }

    impl TypeVisitor<Result<Seen, TvarVisitorResult>> for HasPlaceholdersVisitor {
        fn type_<'cx>(
            &mut self,
            cx: &mut Context<'cx>,
            pole: Polarity,
            acc: Result<Seen, TvarVisitorResult>,
            t: &Type,
        ) -> Result<Seen, TvarVisitorResult> {
            let seen = acc?;
            match t.deref() {
                TypeInner::AnyT(_, AnySource::Placeholder) => {
                    Err(TvarVisitorResult::EncounteredPlaceholderType)
                }
                _ if let Some(node) = type_util::constraint_node_id(t) => {
                    self.constraint_node(cx, pole, Ok(seen), node.id())
                }
                _ => type_default(self, cx, pole, Ok(seen), t),
            }
        }

        fn tvar<'cx>(
            &mut self,
            cx: &mut Context<'cx>,
            pole: Polarity,
            acc: Result<Seen, TvarVisitorResult>,
            _r: &flow_common::reason::Reason,
            id: u32,
        ) -> Result<Seen, TvarVisitorResult> {
            self.constraint_node(cx, pole, acc, id as i32)
        }
    }

    pub fn has_placeholders(cx: &mut Context<'_>, t: &Type) -> bool {
        let mut visitor = HasPlaceholdersVisitor;
        match visitor.type_(cx, Polarity::Positive, Ok(BTreeSet::new()), t) {
            Err(TvarVisitorResult::EncounteredPlaceholderType) => true,
            _ => false,
        }
    }

    struct HasUnresolvedTvarsVisitor;

    impl HasUnresolvedTvarsVisitor {
        fn constraint_node<'cx>(
            &mut self,
            cx: &mut Context<'cx>,
            pole: Polarity,
            acc: Result<Seen, TvarVisitorResult>,
            id: i32,
        ) -> Result<Seen, TvarVisitorResult> {
            let mut seen = acc?;
            let (root_id, constraints) = cx.find_constraints(id);
            if seen.contains(&root_id) {
                Ok(seen)
            } else {
                seen.insert(root_id);
                match constraints {
                    Constraints::FullyResolved(_) => Ok(seen),
                    Constraints::Resolved(t) => self.type_(cx, pole, Ok(seen), &t),
                    Constraints::Unresolved(_) => Err(TvarVisitorResult::EncounteredUnresolvedTvar),
                }
            }
        }
    }

    impl TypeVisitor<Result<Seen, TvarVisitorResult>> for HasUnresolvedTvarsVisitor {
        fn type_<'cx>(
            &mut self,
            cx: &mut Context<'cx>,
            pole: Polarity,
            acc: Result<Seen, TvarVisitorResult>,
            t: &Type,
        ) -> Result<Seen, TvarVisitorResult> {
            match type_util::constraint_node_id(t) {
                Some(node) => self.constraint_node(cx, pole, acc, node.id()),
                None => type_default(self, cx, pole, acc, t),
            }
        }

        fn tvar<'cx>(
            &mut self,
            cx: &mut Context<'cx>,
            pole: Polarity,
            acc: Result<Seen, TvarVisitorResult>,
            _r: &flow_common::reason::Reason,
            id: u32,
        ) -> Result<Seen, TvarVisitorResult> {
            self.constraint_node(cx, pole, acc, id as i32)
        }
    }

    struct HasUnresolvedTvarsOrPlaceholdersVisitor;

    impl HasUnresolvedTvarsOrPlaceholdersVisitor {
        fn constraint_node<'cx>(
            &mut self,
            cx: &mut Context<'cx>,
            pole: Polarity,
            acc: Result<Seen, TvarVisitorResult>,
            id: i32,
        ) -> Result<Seen, TvarVisitorResult> {
            let mut seen = acc?;
            let (root_id, constraints) = cx.find_constraints(id);
            if seen.contains(&root_id) {
                Ok(seen)
            } else {
                seen.insert(root_id);
                match constraints {
                    Constraints::FullyResolved(_) => Ok(seen),
                    Constraints::Resolved(t) => self.type_(cx, pole, Ok(seen), &t),
                    Constraints::Unresolved(_) => Err(TvarVisitorResult::EncounteredUnresolvedTvar),
                }
            }
        }
    }

    impl TypeVisitor<Result<Seen, TvarVisitorResult>> for HasUnresolvedTvarsOrPlaceholdersVisitor {
        fn type_<'cx>(
            &mut self,
            cx: &mut Context<'cx>,
            pole: Polarity,
            acc: Result<Seen, TvarVisitorResult>,
            t: &Type,
        ) -> Result<Seen, TvarVisitorResult> {
            let seen = acc?;
            match t.deref() {
                TypeInner::AnyT(_, AnySource::Placeholder) => {
                    Err(TvarVisitorResult::EncounteredPlaceholderType)
                }
                _ if let Some(node) = type_util::constraint_node_id(t) => {
                    self.constraint_node(cx, pole, Ok(seen), node.id())
                }
                _ => type_default(self, cx, pole, Ok(seen), t),
            }
        }

        fn tvar<'cx>(
            &mut self,
            cx: &mut Context<'cx>,
            pole: Polarity,
            acc: Result<Seen, TvarVisitorResult>,
            _r: &flow_common::reason::Reason,
            id: u32,
        ) -> Result<Seen, TvarVisitorResult> {
            self.constraint_node(cx, pole, acc, id as i32)
        }
    }

    pub fn has_unresolved_tvars_or_placeholders<'cx>(cx: &mut Context<'cx>, t: &Type) -> bool {
        let mut visitor = HasUnresolvedTvarsOrPlaceholdersVisitor;
        match visitor.type_(cx, Polarity::Positive, Ok(BTreeSet::new()), t) {
            Err(TvarVisitorResult::EncounteredUnresolvedTvar) => true,
            Err(TvarVisitorResult::EncounteredPlaceholderType) => true,
            Ok(_) => false,
        }
    }

    pub fn has_unresolved_tvars<'cx>(cx: &mut Context<'cx>, t: &Type) -> bool {
        let mut visitor = HasUnresolvedTvarsVisitor;
        match visitor.type_(cx, Polarity::Positive, Ok(BTreeSet::new()), t) {
            Err(TvarVisitorResult::EncounteredUnresolvedTvar) => true,
            _ => false,
        }
    }

    pub fn has_unresolved_tvars_in_destructors<'cx>(cx: &mut Context<'cx>, d: &Destructor) -> bool {
        let mut visitor = HasUnresolvedTvarsVisitor;
        match visitor.destructor(cx, Ok(BTreeSet::new()), d) {
            Err(TvarVisitorResult::EncounteredUnresolvedTvar) => true,
            _ => false,
        }
    }
}

// Errors

pub fn error_message_kind_of_lower(
    t: &Type,
) -> Option<flow_typing_errors::error_message::LowerKind> {
    use flow_typing_errors::error_message::LowerKind;
    match t.deref() {
        TypeInner::DefT(_, def_t) => match def_t.deref() {
            DefTInner::NullT => Some(LowerKind::PossiblyNull),
            DefTInner::VoidT => Some(LowerKind::PossiblyVoid),
            _ => None,
        },
        TypeInner::MaybeT(_, _) => Some(LowerKind::PossiblyNullOrVoid),
        _ => None,
    }
}

pub fn error_message_kind_of_upper<CX: ContextFamily>(
    u: &UseT<CX>,
) -> flow_typing_errors::error_message::UpperKind<ALoc> {
    use flow_typing_errors::error_message::UpperKind;
    use flow_typing_type::type_::PropRef;
    use flow_typing_type::type_::string_of_use_ctor;
    use flow_typing_type::type_util::loc_of_t;

    match u.deref() {
        UseTInner::GetPropT(data) => match data.propref.as_ref() {
            PropRef::Named { reason, name, .. } => {
                UpperKind::IncompatibleGetPropT(reason.loc().dupe(), Some(name.dupe()))
            }
            PropRef::Computed(t) => UpperKind::IncompatibleGetPropT(loc_of_t(t).dupe(), None),
        },
        UseTInner::GetPrivatePropT(..) => UpperKind::IncompatibleGetPrivatePropT,
        UseTInner::SetPropT(_, _, propref, _, _, _, _) => match propref.as_ref() {
            PropRef::Named { reason, name, .. } => {
                UpperKind::IncompatibleSetPropT(reason.loc().dupe(), Some(name.dupe()))
            }
            PropRef::Computed(t) => UpperKind::IncompatibleSetPropT(loc_of_t(t).dupe(), None),
        },
        UseTInner::SetPrivatePropT(..) => UpperKind::IncompatibleSetPrivatePropT,
        UseTInner::MethodT(box MethodTData { propref, .. }) => match propref.as_ref() {
            PropRef::Named { reason, name, .. } => {
                UpperKind::IncompatibleMethodT(reason.loc().dupe(), Some(name.dupe()))
            }
            PropRef::Computed(t) => UpperKind::IncompatibleMethodT(loc_of_t(t).dupe(), None),
        },
        UseTInner::CallT(..) => UpperKind::IncompatibleCallT,
        UseTInner::GetElemT(box GetElemTData { key_t, .. }) => {
            UpperKind::IncompatibleGetElemT(loc_of_t(key_t).dupe())
        }
        UseTInner::SetElemT(box SetElemTData { key_t: t, .. }) => {
            UpperKind::IncompatibleSetElemT(loc_of_t(t).dupe())
        }
        UseTInner::CallElemT(box CallElemTData { key_t: t, .. }) => {
            UpperKind::IncompatibleCallElemT(loc_of_t(t).dupe())
        }
        UseTInner::ElemT(box ElemTData { obj, .. }) => {
            if matches!(obj.deref(), TypeInner::DefT(_, def_t) if matches!(def_t.deref(), DefTInner::ArrT(_)))
            {
                UpperKind::IncompatibleElemTOfArrT
            } else {
                UpperKind::IncompatibleUnclassified(string_of_use_ctor(u).into())
            }
        }
        UseTInner::ObjRestT(..) => UpperKind::IncompatibleObjRestT,
        UseTInner::ArrRestT(..) => UpperKind::IncompatibleArrRestT,
        UseTInner::SuperT(..) => UpperKind::IncompatibleSuperT,
        UseTInner::MixinT(..) => UpperKind::IncompatibleMixinT,
        UseTInner::SpecializeT(box SpecializeTData { use_op, .. }) => {
            if matches!(
                use_op,
                flow_typing_type::type_::VirtualUseOp::Op(inner)
                    if matches!(inner.as_ref(), flow_typing_type::type_::VirtualRootUseOp::ClassExtendsCheck(..))
            ) {
                UpperKind::IncompatibleSuperT
            } else {
                UpperKind::IncompatibleSpecializeT
            }
        }
        UseTInner::ConcretizeTypeAppsT(..) => UpperKind::IncompatibleSpecializeT,
        UseTInner::ThisSpecializeT(..) => UpperKind::IncompatibleThisSpecializeT,
        UseTInner::GetKeysT { .. } => UpperKind::IncompatibleGetKeysT,
        UseTInner::HasOwnPropT(box HasOwnPropTData {
            reason: r,
            type_: t,
            ..
        }) => {
            let name = match t.deref() {
                TypeInner::DefT(_, def_t) => {
                    if let DefTInner::SingletonStrT { value, .. } = def_t.deref() {
                        Some(Name::new(value.dupe()))
                    } else {
                        None
                    }
                }
                TypeInner::GenericT(box GenericTData { bound, .. }) => match bound.deref() {
                    TypeInner::DefT(_, def_t) => {
                        if let DefTInner::SingletonStrT { value, .. } = def_t.deref() {
                            Some(Name::new(value.dupe()))
                        } else {
                            None
                        }
                    }
                    _ => None,
                },
                _ => None,
            };
            UpperKind::IncompatibleHasOwnPropT(r.loc().dupe(), name)
        }
        UseTInner::GetValuesT(..) => UpperKind::IncompatibleGetValuesT,
        UseTInner::GetDictValuesT(..) => UpperKind::IncompatibleGetValuesT,
        UseTInner::MapTypeT(box MapTypeTData { type_map: kind, .. }) => {
            if matches!(kind, flow_typing_type::type_::TypeMap::ObjectKeyMirror) {
                UpperKind::IncompatibleMapTypeTObject
            } else {
                UpperKind::IncompatibleUnclassified(string_of_use_ctor(u).into())
            }
        }
        UseTInner::GetStaticsT(..) => UpperKind::IncompatibleGetStaticsT,
        _ => UpperKind::IncompatibleUnclassified(string_of_use_ctor(u).into()),
    }
}

pub fn error_message_loc_of_upper<CX: ContextFamily>(u: &UseT<CX>) -> ALoc {
    use flow_typing_type::type_util::loc_of_t;

    match u.deref() {
        UseTInner::UseT(_, t) => loc_of_t(t).dupe(),
        UseTInner::CallT(data) => data.reason.loc().dupe(),
        UseTInner::ConditionalT(data) => data.reason.loc().dupe(),
        UseTInner::MethodT(data) => data.reason.loc().dupe(),
        UseTInner::PrivateMethodT(data) => data.reason.loc().dupe(),
        UseTInner::SetPropT(_, reason, ..) => reason.loc().dupe(),
        UseTInner::SetPrivatePropT(data) => data.reason.loc().dupe(),
        UseTInner::GetPropT(data) => data.reason.loc().dupe(),
        UseTInner::GetPrivatePropT(data) => data.reason.loc().dupe(),
        UseTInner::TestPropT(data) => data.reason.loc().dupe(),
        UseTInner::SetElemT(data) => data.reason.loc().dupe(),
        UseTInner::GetElemT(data) => data.reason.loc().dupe(),
        UseTInner::CallElemT(data) => data.reason.loc().dupe(),
        UseTInner::GetStaticsT(reason, _) => reason.loc().dupe(),
        UseTInner::GetProtoT(reason, _) => reason.loc().dupe(),
        UseTInner::ReposLowerT { reason, .. } => reason.loc().dupe(),
        UseTInner::ReposUseT(data) => data.reason.loc().dupe(),
        UseTInner::ConstructorT(data) => data.reason.loc().dupe(),
        UseTInner::SuperT(data) => data.reason.loc().dupe(),
        UseTInner::ImplementsT(_, t) => loc_of_t(t).dupe(),
        UseTInner::MixinT(reason, _) => reason.loc().dupe(),
        UseTInner::ToStringT { reason, .. } => reason.loc().dupe(),
        UseTInner::SpecializeT(data) => data.reason2.loc().dupe(),
        UseTInner::ThisSpecializeT(reason, _, _) => reason.loc().dupe(),
        UseTInner::ValueToTypeReferenceT(data) => data.reason.loc().dupe(),
        UseTInner::ConcretizeTypeAppsT(_, box (_, _, _, reason), _, _) => reason.loc().dupe(),
        UseTInner::LookupT(data) => data.reason.loc().dupe(),
        UseTInner::ObjRestT(reason, ..) => reason.loc().dupe(),
        UseTInner::ObjTestProtoT(reason, _) => reason.loc().dupe(),
        UseTInner::ArrRestT(data) => data.reason.loc().dupe(),
        UseTInner::GetKeysT { reason, .. } => reason.loc().dupe(),
        UseTInner::GetKeysDictKeyT { reason, .. } => reason.loc().dupe(),
        UseTInner::HasOwnPropT(data) => data.reason.loc().dupe(),
        UseTInner::GetValuesT(reason, _) => reason.loc().dupe(),
        UseTInner::GetDictValuesT(reason, _) => reason.loc().dupe(),
        UseTInner::ElemT(data) => data.reason.loc().dupe(),
        UseTInner::MapTypeT(data) => data.reason.loc().dupe(),
        UseTInner::ObjKitT(_, reason, _, _, _) => reason.loc().dupe(),
        UseTInner::ReactKitT(data) => data.reason.loc().dupe(),
        UseTInner::ConcretizeT(data) => data.reason.loc().dupe(),
        UseTInner::ResolveSpreadT(data) => data.reason.loc().dupe(),
        UseTInner::CondT(data) => data.loc.dupe(),
        UseTInner::ExtendsUseT(data) => data.reason.loc().dupe(),
        UseTInner::ResolveUnionT(data) => data.reason.loc().dupe(),
        UseTInner::GetEnumT(data) => data.loc.dupe(),
        UseTInner::FilterOptionalT(_, t) | UseTInner::FilterMaybeT(_, t) => loc_of_t(t).dupe(),
        UseTInner::DeepReadOnlyT(tvar, _) | UseTInner::HooklikeT(tvar) => {
            tvar.reason().loc().dupe()
        }
        UseTInner::SealGenericT(data) => data.loc.dupe(),
        UseTInner::OptionalIndexedAccessT(data) => data.reason.loc().dupe(),
        UseTInner::ConvertEmptyPropsToMixedT(loc, _) => loc.dupe(),
        UseTInner::ExitRendersT { renders_reason, .. } => renders_reason.loc().dupe(),
        UseTInner::EvalTypeDestructorT(data) => data.reason.loc().dupe(),
    }
}

pub fn primary_reason_of_use_op(use_op: &UseOp) -> Option<&Reason> {
    use flow_typing_type::type_::AssignVarDefaultData;
    use flow_typing_type::type_::AssignVarInit;
    use flow_typing_type::type_::AssignVarTarget;
    use flow_typing_type::type_::InitFieldData;
    use flow_typing_type::type_::JSXCreateElementData;
    use flow_typing_type::type_::RootUseOp;
    use flow_typing_type::type_::TypeApplicationTarget;
    use flow_typing_type::type_::TypeofApplicationData;
    use flow_typing_type::type_::root_of_use_op;

    match root_of_use_op(use_op) {
        RootUseOp::ObjectAddComputedProperty { op }
        | RootUseOp::ObjectSpread { op }
        | RootUseOp::ObjectRest { op }
        | RootUseOp::ObjectChain { op }
        | RootUseOp::DeclareComponentRef { op }
        | RootUseOp::FunReturnStatement { value: op }
        | RootUseOp::GeneratorYield { value: op }
        | RootUseOp::GetExport(op)
        | RootUseOp::GetProperty(op)
        | RootUseOp::InitField(box InitFieldData { op, .. })
        | RootUseOp::ReactGetIntrinsic { literal: op }
        | RootUseOp::TypeApplication {
            type_:
                TypeApplicationTarget::Reason(op)
                | TypeApplicationTarget::Typeof(box TypeofApplicationData { reason: op, .. }),
        }
        | RootUseOp::RenderTypeInstantiation { render_type: op } => Some(op),
        RootUseOp::JSXCreateElement(box JSXCreateElementData { op, .. }) => Some(op),
        RootUseOp::AssignVar { var, init } => match var.as_deref() {
            Some(AssignVarTarget::Variable(reason) | AssignVarTarget::Pattern { reason, .. }) => {
                Some(reason)
            }
            Some(AssignVarTarget::Annotation(TypeOrTypeDescT::Type(t))) => Some(reason_of_t(t)),
            Some(AssignVarTarget::Annotation(TypeOrTypeDescT::TypeDesc(_))) => None,
            None => match init {
                AssignVarInit::Reason(reason) => Some(reason),
                AssignVarInit::Default(box AssignVarDefaultData {
                    default: TypeOrTypeDescT::Type(t),
                    ..
                }) => Some(reason_of_t(t)),
                AssignVarInit::Default(box AssignVarDefaultData {
                    default: TypeOrTypeDescT::TypeDesc(_),
                    ..
                }) => None,
            },
        },
        RootUseOp::Cast {
            upper: box upper, ..
        }
        | RootUseOp::ComponentRestParamCompatibility(box ComponentRestParamCompatibilityData {
            rest_param: upper,
            ..
        })
        | RootUseOp::ClassExtendsCheck(box ClassExtendsCheckData { extends: upper, .. })
        | RootUseOp::EvalMappedType(box EvalMappedTypeData {
            mapped_type: upper, ..
        }) => match upper {
            TypeOrTypeDescT::Type(t) => Some(reason_of_t(t)),
            TypeOrTypeDescT::TypeDesc(_) => None,
        },
        RootUseOp::ClassImplementsCheck(data) => Some(&data.implements),
        RootUseOp::ClassOwnProtoCheck(_) => None,
        RootUseOp::MakeReadOnly { .. } => None,
        RootUseOp::ClassMethodDefinition(data) => Some(&data.name),
        RootUseOp::Coercion { target, .. } => Some(target),
        RootUseOp::ConformToCommonInterface(_) => None,
        RootUseOp::MergedDeclaration(box MergedDeclarationData { current_decl, .. }) => {
            Some(current_decl)
        }
        RootUseOp::DeleteProperty { prop, .. } => Some(prop),
        RootUseOp::DeleteVar { var } => Some(var),
        RootUseOp::FunCall(data) => Some(&data.op),
        RootUseOp::FunCallMethod(data) => Some(&data.op),
        RootUseOp::FunImplicitReturn(data) => Some(&data.upper),
        RootUseOp::IndexedTypeAccess(data) => match &data.index {
            TypeOrTypeDescT::Type(t) => Some(reason_of_t(t)),
            TypeOrTypeDescT::TypeDesc(_) => None,
        },
        RootUseOp::TypeGuardIncompatibility(data) => match &data.guard_type {
            TypeOrTypeDescT::Type(t) => Some(reason_of_t(t)),
            TypeOrTypeDescT::TypeDesc(_) => None,
        },
        RootUseOp::InferBoundCompatibilityCheck { infer, .. } => Some(infer),
        RootUseOp::ReactCreateElementCall(data) => Some(&data.op),
        RootUseOp::RecordCreate(data) => Some(&data.op),
        RootUseOp::Speculation(use_op) => primary_reason_of_use_op(use_op),
        RootUseOp::SetProperty(data) => Some(&data.prop),
        RootUseOp::UpdateProperty { prop, .. } => Some(prop),
        RootUseOp::RefinementCheck { test, .. } => Some(test),
        RootUseOp::SwitchRefinementCheck(_) => None,
        RootUseOp::PositiveTypeGuardConsistency(data) => Some(&data.reason),
        RootUseOp::AssertionFunctionConsistency(data) => Some(&data.exit_reason),
        RootUseOp::UnknownUse => None,
    }
}

pub fn use_op_of_lookup_action(action: &flow_typing_type::type_::LookupAction) -> UseOp {
    use flow_typing_type::type_::LookupAction;
    use flow_typing_type::type_::LookupActionMatchPropData;
    use flow_typing_type::type_::LookupPropsForSubtypingData;
    use flow_typing_type::type_::ReadPropData;
    use flow_typing_type::type_::WritePropData;
    match action {
        LookupAction::ReadProp(box ReadPropData { use_op, .. })
        | LookupAction::WriteProp(box WritePropData { use_op, .. })
        | LookupAction::LookupPropsForSubtyping(box LookupPropsForSubtypingData {
            use_op, ..
        })
        | LookupAction::SuperProp(box (use_op, _, _, _))
        | LookupAction::MatchProp(box LookupActionMatchPropData { use_op, .. }) => use_op.dupe(),
        LookupAction::LookupPropForTvarPopulation { .. } => unknown_use(),
    }
}

// Errors

#[derive(Debug, Clone)]
pub struct SpeculativeError(pub Box<ErrorMessage<ALoc>>);

impl From<SpeculativeError> for FlowJsException {
    fn from(e: SpeculativeError) -> Self {
        FlowJsException::Speculative(e)
    }
}

impl From<flow_utils_concurrency::worker_cancel::WorkerCanceled> for FlowJsException {
    fn from(e: flow_utils_concurrency::worker_cancel::WorkerCanceled) -> Self {
        FlowJsException::WorkerCanceled(e)
    }
}

impl From<flow_utils_concurrency::job_error::CheckTimeout> for FlowJsException {
    fn from(e: flow_utils_concurrency::job_error::CheckTimeout) -> Self {
        FlowJsException::TimedOut(e)
    }
}

impl From<JobError> for FlowJsException {
    fn from(e: JobError) -> Self {
        match e {
            JobError::Canceled(c) => FlowJsException::WorkerCanceled(c),
            JobError::TimedOut(t) => FlowJsException::TimedOut(t),
            JobError::DebugThrow { loc } => FlowJsException::DebugThrow { loc },
        }
    }
}

#[derive(Debug, Clone)]
pub enum FlowJsException {
    Speculative(SpeculativeError),
    SpeculationSingletonError,
    LimitExceeded,
    WorkerCanceled(flow_utils_concurrency::worker_cancel::WorkerCanceled),
    TimedOut(flow_utils_concurrency::job_error::CheckTimeout),
    DebugThrow { loc: ALoc },
}

pub fn flow_js_result_to_job_error<T>(result: Result<T, FlowJsException>) -> Result<T, JobError> {
    match result {
        Ok(value) => Ok(value),
        Err(FlowJsException::WorkerCanceled(c)) => Err(JobError::Canceled(c)),
        Err(FlowJsException::TimedOut(t)) => Err(JobError::TimedOut(t)),
        Err(FlowJsException::DebugThrow { loc }) => Err(JobError::DebugThrow { loc }),
        Err(err) => panic!("Non speculating: {:?}", err),
    }
}

// When emitting a single error for a representative union member, wrap the
// [use_op] with a [UnionRepresentative] frame so the error says "at least one
// member of …".  Skip the frame when [representative] itself resolves to a
// union, because the recursive flow will add its own frame.
pub fn union_representative_use_op(
    cx: &mut Context,
    l: &Type,
    representative: &Type,
    use_op: UseOp,
) -> UseOp {
    use flow_typing_type::type_::UnionRepresentativeData;
    use flow_typing_type::type_::VirtualFrameUseOp;
    use flow_typing_type::type_::VirtualUseOp;
    use flow_typing_type::type_util;

    match cx.find_resolved(representative) {
        Some(resolved) if matches!(&*resolved, TypeInner::UnionT(_, _)) => use_op,
        _ => VirtualUseOp::Frame(
            Arc::new(VirtualFrameUseOp::UnionRepresentative(Box::new(
                UnionRepresentativeData {
                    loc: type_util::loc_of_t(l).dupe(),
                    reference_loc: type_util::ref_loc_of_t(l).dupe(),
                    union: type_or_type_desc_for_error(l),
                },
            ))),
            Arc::new(use_op),
        ),
    }
}

/// Keeps a type for post-inference normalization unless its reason carries
/// explanatory provenance that the normalized type cannot express.
pub fn type_or_type_desc_for_error(t: &Type) -> TypeOrTypeDescT<ALoc> {
    use flow_typing_type::type_util;

    if let Some(desc) = type_util::reason_of_t(t).desc(true)
        && (desc.is_explanatory()
            || matches!(
                t.deref(),
                TypeInner::DefT(_, def_t)
                    if matches!(
                        def_t.deref(),
                        DefTInner::MixedT(MixedFlavor::MixedFunction)
                            | DefTInner::UniqueSymbolT(_)
                    )
            ))
    {
        TypeOrTypeDescT::TypeDesc(ErrorTypeDesc::ReasonDesc(desc.clone()))
    } else {
        TypeOrTypeDescT::Type(t.dupe())
    }
}

/// Retains a type for error normalization alongside its stable rendered reference.
pub fn type_reference_for_error(t: &Type) -> ErrorTypeReferenceData<ALoc> {
    use flow_typing_type::type_util;

    let reason = type_util::reason_of_t(t);
    ErrorTypeReferenceData {
        loc: reason.loc().dupe(),
        reference_loc: type_util::ref_loc_of_t(t).dupe(),
        type_desc: type_or_type_desc_for_error(t),
    }
}

/// Keeps a type for post-inference normalization at a caller-selected primary location.
pub fn type_reference_at_loc_for_error(t: &Type, loc: ALoc) -> ErrorTypeReferenceData<ALoc> {
    use flow_typing_type::type_util;

    ErrorTypeReferenceData {
        loc,
        reference_loc: type_util::ref_loc_of_t(t).dupe(),
        type_desc: type_or_type_desc_for_error(t),
    }
}

/// Extracts a user-written name for error messages without retaining its reason.
pub fn description_name_for_error(reason: &Reason) -> Option<FlowSmolStr> {
    match reason.desc(false) {
        Some(
            VirtualReasonDesc::RIdentifier(name)
            | VirtualReasonDesc::RType(name)
            | VirtualReasonDesc::REnumMember {
                member_name: name, ..
            },
        ) => Some(name.dupe()),
        _ => None,
    }
}

/// Captures the source-level construct used as a type without retaining its reason.
pub fn value_as_type_reference_for_error(reason: &Reason) -> Option<ValueAsTypeReference> {
    match reason.desc(false) {
        Some(
            VirtualReasonDesc::RIdentifier(name)
            | VirtualReasonDesc::RType(name)
            | VirtualReasonDesc::REnumMember {
                member_name: name, ..
            },
        ) => Some(ValueAsTypeReference::Name(name.dupe())),
        Some(VirtualReasonDesc::RModule(module)) => {
            Some(ValueAsTypeReference::ImportType(module.dupe()))
        }
        _ => None,
    }
}

/// Captures the semantic description and location of a type guard without retaining its reason.
pub fn type_guard_reference_for_error(reason: &Reason) -> TypeGuardReferenceData<ALoc> {
    let kind = match reason.desc(false) {
        Some(VirtualReasonDesc::RParameter(name)) => TypeGuardReferenceKind::Parameter(name.dupe()),
        Some(VirtualReasonDesc::RIdentifier(name)) => {
            TypeGuardReferenceKind::Parameter(Some(name.dupe()))
        }
        Some(VirtualReasonDesc::RThis) => TypeGuardReferenceKind::This,
        _ => TypeGuardReferenceKind::TypeGuard,
    };
    TypeGuardReferenceData {
        loc: reason.loc().dupe(),
        kind,
    }
}

pub fn expression_reference_kind_for_error<M: Dupe, T: Dupe>(
    expression: &ast::expression::Expression<M, T>,
) -> ExpressionReferenceKind {
    use ast::expression::ExpressionInner;

    match expression.deref() {
        ExpressionInner::TypeCast { inner, .. } => {
            expression_reference_kind_for_error(&inner.expression)
        }
        ExpressionInner::Object { .. } => ExpressionReferenceKind::ObjectLiteral,
        ExpressionInner::Array { .. } => ExpressionReferenceKind::ArrayLiteral,
        ExpressionInner::ArrowFunction { inner, .. } => {
            let kind = if inner.async_ {
                ExpressionFunctionKind::Async
            } else {
                ExpressionFunctionKind::Normal
            };
            ExpressionReferenceKind::Function(kind)
        }
        ExpressionInner::Function { inner, .. } => {
            let kind = match (inner.async_, inner.generator) {
                (false, false) => ExpressionFunctionKind::Normal,
                (true, false) => ExpressionFunctionKind::Async,
                (false, true) => ExpressionFunctionKind::Generator,
                (true, true) => ExpressionFunctionKind::AsyncGenerator,
            };
            ExpressionReferenceKind::Function(kind)
        }
        ExpressionInner::StringLiteral { inner, .. } if inner.value.is_empty() => {
            ExpressionReferenceKind::EmptyString
        }
        ExpressionInner::TaggedTemplate { .. } | ExpressionInner::TemplateLiteral { .. } => {
            ExpressionReferenceKind::TemplateString
        }
        _ => ExpressionReferenceKind::Code(code_desc_of_expression(false, expression).into()),
    }
}

pub fn expression_reference_for_error<M: Dupe, T: Dupe>(
    expression: &ast::expression::Expression<M, T>,
) -> ExpressionReferenceData<T> {
    ExpressionReferenceData {
        loc: expression.loc().dupe(),
        kind: expression_reference_kind_for_error(expression),
    }
}

/// Extracts a user-written type name for error messages without retaining its reason.
pub fn type_name_for_error(reason: &Reason) -> Option<FlowSmolStr> {
    match reason.desc(false) {
        Some(
            VirtualReasonDesc::RType(name)
            | VirtualReasonDesc::ROpaqueType(name)
            | VirtualReasonDesc::RTypeAlias(box (name, _, _)),
        ) => Some(name.dupe()),
        _ => None,
    }
}

/// Retains a type for error normalization while preserving a selected reason as error identity.
pub fn type_reference_with_reason_for_error(
    t: &Type,
    reason: Reason,
) -> ErrorTypeReferenceWithLocData<ALoc> {
    use flow_typing_type::type_util;

    ErrorTypeReferenceWithLocData::new(
        reason,
        type_util::ref_loc_of_t(t).dupe(),
        type_or_type_desc_for_error(t),
    )
}

/// Builds an `instanceof` RHS error while preserving descriptions that carry
/// provenance not expressible by the normalized type.
pub fn instanceof_rhs_error(t: &Type) -> ErrorMessage<ALoc> {
    use flow_typing_errors::error_message::EInstanceofRHSData;
    use flow_typing_type::type_util;

    let reason = type_util::reason_of_t(t);
    let rhs_desc = if let Some(desc) = reason.desc(false)
        && matches!(
            t.deref(),
            TypeInner::DefT(_, def_t)
                if matches!(def_t.deref(), DefTInner::NullT | DefTInner::VoidT)
        ) {
        TypeOrTypeDescT::TypeDesc(ErrorTypeDesc::ReasonDesc(desc.clone()))
    } else {
        type_or_type_desc_for_error(t)
    };
    ErrorMessage::EInstanceofRHS(Box::new(EInstanceofRHSData {
        loc: reason.loc().dupe(),
        rhs_loc: type_util::ref_loc_of_t(t).dupe(),
        rhs_desc,
    }))
}

pub fn invalid_constructor_error(t: &Type) -> ErrorMessage<ALoc> {
    use flow_typing_errors::error_message::EInvalidConstructorData;
    use flow_typing_type::type_util;

    ErrorMessage::EInvalidConstructor(Box::new(EInvalidConstructorData {
        loc: type_util::reason_of_t(t).loc().dupe(),
        value_loc: type_util::ref_loc_of_t(t).dupe(),
        value_desc: type_or_type_desc_for_error(t),
    }))
}

/// Builds an invalid-prototype error that retains the type for normalization.
pub fn invalid_prototype_error(loc: ALoc, t: &Type) -> ErrorMessage<ALoc> {
    use flow_typing_errors::error_message::EInvalidPrototypeData;
    use flow_typing_type::type_util;

    ErrorMessage::EInvalidPrototype(Box::new(EInvalidPrototypeData {
        loc,
        prototype_loc: type_util::ref_loc_of_t(t).dupe(),
        prototype_desc: type_or_type_desc_for_error(t),
    }))
}

/// Builds an invalid React create-element error referencing the identifier
/// `react_name`, whose value is `react_t`.
pub fn invalid_react_create_element_error(
    create_element_loc: ALoc,
    react_name: FlowSmolStr,
    react_t: &Type,
) -> ErrorMessage<ALoc> {
    use flow_typing_errors::error_message::EInvalidReactCreateElementData;
    use flow_typing_errors::intermediate_error_types::NamedReferenceData;
    use flow_typing_type::type_util;

    ErrorMessage::EInvalidReactCreateElement(Box::new(EInvalidReactCreateElementData {
        create_element_loc,
        react: NamedReferenceData {
            loc: type_util::ref_loc_of_t(react_t).dupe(),
            name: react_name,
        },
    }))
}

pub fn incompatible_type_error(
    lower: &Type,
    upper: IncompatibleUpperData<ALoc>,
    use_op: Option<UseOp>,
) -> ErrorMessage<ALoc> {
    incompatible_type_error_with_lower_kind(
        lower,
        error_message_kind_of_lower(lower),
        upper,
        use_op,
    )
}

pub fn incompatible_type_error_with_lower_kind(
    lower: &Type,
    lower_kind: Option<flow_typing_errors::error_message::LowerKind>,
    upper: IncompatibleUpperData<ALoc>,
    use_op: Option<UseOp>,
) -> ErrorMessage<ALoc> {
    use flow_typing_type::type_util;

    ErrorMessage::EIncompatibleType(Box::new(EIncompatibleTypeData {
        lower_kind,
        lower_loc: type_util::loc_of_t(lower).dupe(),
        lower_def_loc: type_util::ref_loc_of_t(lower).dupe(),
        lower_desc: type_or_type_desc_for_error(lower),
        upper,
        use_op,
    }))
}

pub fn incompatible_types_error(
    lower: &Type,
    upper: &Type,
    use_op: UseOp,
    explanation: Option<Explanation<ALoc>>,
) -> ErrorMessage<ALoc> {
    incompatible_types_error_with_branches(lower, upper, use_op, explanation, vec![])
}

pub fn incompatible_types_error_with_branches(
    lower: &Type,
    upper: &Type,
    use_op: UseOp,
    explanation: Option<Explanation<ALoc>>,
    branches: Vec<ErrorMessage<ALoc>>,
) -> ErrorMessage<ALoc> {
    ErrorMessage::EIncompatibleTypesWithUseOp(Box::new(incompatible_types_error_data(
        lower,
        upper,
        use_op,
        explanation,
        branches,
    )))
}

/// Builds the shared payload used by typed incompatibility error variants.
pub fn incompatible_types_error_data(
    lower: &Type,
    upper: &Type,
    use_op: UseOp,
    explanation: Option<Explanation<ALoc>>,
    branches: Vec<ErrorMessage<ALoc>>,
) -> EIncompatibleTypesWithUseOpData<ALoc> {
    use flow_typing_type::type_util;

    EIncompatibleTypesWithUseOpData {
        lower_loc: type_util::loc_of_t(lower).dupe(),
        lower_def_loc: type_util::ref_loc_of_t(lower).dupe(),
        upper_loc: type_util::loc_of_t(upper).dupe(),
        upper_def_loc: type_util::ref_loc_of_t(upper).dupe(),
        lower_desc: type_or_type_desc_for_error(lower),
        upper_desc: type_or_type_desc_for_error(upper),
        use_op,
        explanation,
        example: None,
        branches,
    }
}

// [src_cx] is the context in which the error is created, and [dst_cx] the context
// in which it is recorded.
/// Adds the error `msg` raised in `src_cx` to the context `dst_cx`.
pub fn add_output_generic<'cx>(
    src_cx: &mut Context<'cx>,
    dst_cx: ContextId,
    env: &FlowJsEnv,
    msg: ErrorMessage<ALoc>,
) -> Result<(), SpeculativeError> {
    use flow_typing_errors::flow_error::error_of_msg;

    if env.speculating() {
        if msg.defered_in_speculation() {
            env.defer_error(msg);
            Ok(())
        } else {
            if src_cx.is_verbose() {
                eprintln!(
                    "\nspeculative_error: {:?}. (score: {})",
                    msg,
                    flow_typing_errors::intermediate_error::score_of_msg(&msg)
                );
            }
            Err(SpeculativeError(Box::new(msg)))
        }
    } else {
        if src_cx.is_verbose() {
            eprintln!("\nadd_output: {:?}", msg);
        }

        let error = error_of_msg(src_cx.file().dupe(), msg);
        // catch no-loc errors early, before they get into error map
        if error
            .loc_of_error()
            .map(|loc| loc.source().is_none())
            .unwrap_or(false)
        {
            panic!("add_output: no source for error: {:?}", error);
        }

        src_cx.switch_to(dst_cx).add_error(error);
        Ok(())
    }
}

/// Decide reason order based on UB's flavor and blamability, asking the checker which files are
/// global libdefs.
pub fn ordered_reasons(cx: &Context<'_>, reasons: (Reason, Reason)) -> (Reason, Reason) {
    flow_typing_errors::flow_error::ordered_reasons(reasons, |file| cx.is_global_libdef(file))
}

/// Orders types using the same blamability rule as [`ordered_reasons`].
pub fn ordered_types<'a>(cx: &Context<'_>, types: (&'a Type, &'a Type)) -> (&'a Type, &'a Type) {
    let is_blamable = |t: &Type| {
        let reason = reason_of_t(t);
        *reason.loc() != ALoc::none()
            && !reason
                .loc()
                .source()
                .is_some_and(|file| cx.is_global_libdef(file))
    };
    let (lower, upper) = types;
    if is_blamable(upper) && !is_blamable(lower) {
        (upper, lower)
    } else {
        (lower, upper)
    }
}

pub fn add_output_with_env<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    msg: ErrorMessage<ALoc>,
) -> Result<(), FlowJsException> {
    let dst_cx = cx.id();
    add_output_generic(cx, dst_cx, env, msg).map_err(FlowJsException::Speculative)
}

pub fn add_output_non_speculating<'cx>(cx: &mut Context<'cx>, msg: ErrorMessage<ALoc>) {
    if let Err(err) = add_output_with_env(cx, &FlowJsEnv::entry(), msg) {
        panic!("Non speculating: {:?}", err);
    }
}

// In annotation inference, errors are created in the exporting side (cx), and
// are recorded on the file being checked (cx.annot_error_dst()).
pub fn add_annot_inference_error(cx: &mut Context, msg: ErrorMessage<ALoc>) {
    let dst_cx = cx.annot_error_dst();
    add_output_generic(cx, dst_cx, &FlowJsEnv::entry(), msg).unwrap()
}

pub fn exact_obj_error<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    obj_kind: &flow_typing_type::type_::ObjKind,
    use_op: UseOp,
    exact: &Type,
    l: &Type,
) -> Result<(), FlowJsException> {
    use flow_typing_errors::error_message::ErrorMessage;
    use flow_typing_errors::intermediate_error_types::ExactnessErrorKind;
    use flow_typing_type::type_::ObjKind;
    use flow_typing_type::type_util::reason_of_t;

    let error_kind = match obj_kind {
        ObjKind::Indexed { .. } => ExactnessErrorKind::UnexpectedIndexer,
        _ => ExactnessErrorKind::UnexpectedInexact,
    };
    let (lower, upper) = ordered_types(cx, (l, exact));
    add_output_with_env(
        cx,
        env,
        ErrorMessage::EIncompatibleWithExact(
            (
                type_reference_with_reason_for_error(lower, reason_of_t(lower).dupe()),
                type_reference_with_reason_for_error(upper, reason_of_t(upper).dupe()),
            ),
            use_op,
            error_kind,
        ),
    )
}

// Unions

#[derive(Debug, Clone)]
pub enum UnionOptimizationGuardResult {
    True,
    Maybe,
    False {
        diff: flow_typing_type::type_::UnionEnumSet,
    },
}

pub fn union_optimization_guard<'cx, F>(
    cx: &mut Context<'cx>,
    comparator: F,
    l: &Type,
    u: &Type,
) -> UnionOptimizationGuardResult
where
    F: Fn(&Type, &Type) -> bool + Copy,
{
    use flow_typing_type::type_::union_rep::UnionRep;
    use flow_typing_type::type_util::type_ex_set;
    use flow_typing_visitors::type_mapper;

    type TypeSet = BTreeSet<Type>;

    fn unwrap_type<'cx>(cx: &mut Context<'cx>, t: &Type) -> Type {
        cx.find_resolved(t).unwrap_or_else(|| t.dupe())
    }

    fn union_compare<'cx, F2>(
        cx: &mut Context<'cx>,
        comparator: F2,
        lts: &[Type],
        uts: &[Type],
    ) -> bool
    where
        F2: Fn(&Type, &Type) -> bool,
    {
        if cx.is_verbose() {
            eprintln!("union_compare slow");
        }
        let ts2 = type_mapper::union_flatten(cx, uts.iter().duped());
        let ts1 = type_mapper::union_flatten(cx, lts.iter().duped());
        let mut rhs_set = type_ex_set::empty();
        let mut has_str_general = false;
        let mut has_num_general = false;
        let mut has_bool_general = false;
        let mut has_bigint_general = false;
        for t in ts2.iter().duped() {
            match t.deref() {
                TypeInner::DefT(_, def) => match &**def {
                    DefTInner::StrGeneralT(_) => has_str_general = true,
                    DefTInner::NumGeneralT(_) => has_num_general = true,
                    DefTInner::BoolGeneralT => has_bool_general = true,
                    DefTInner::BigIntGeneralT(_) => has_bigint_general = true,
                    _ => {}
                },
                _ => {}
            }
            type_ex_set::add(t, &mut rhs_set);
        }
        ts1.iter().all(|t1| {
            let fast_ground = match t1.deref() {
                TypeInner::DefT(_, def) => match &**def {
                    DefTInner::StrGeneralT(_) | DefTInner::SingletonStrT { .. } => has_str_general,
                    DefTInner::NumGeneralT(_) | DefTInner::SingletonNumT { .. } => has_num_general,
                    DefTInner::BoolGeneralT | DefTInner::SingletonBoolT { .. } => has_bool_general,
                    DefTInner::BigIntGeneralT(_) | DefTInner::SingletonBigIntT { .. } => {
                        has_bigint_general
                    }
                    _ => false,
                },
                _ => false,
            };
            fast_ground || type_ex_set::mem(t1, &rhs_set) || ts2.iter().any(|t2| comparator(t1, t2))
        })
    }

    fn union_optimization_guard_impl<'cx, F2>(
        seen: &mut TypeSet,
        cx: &mut Context<'cx>,
        comparator: F2,
        l: &Type,
        u: &Type,
    ) -> UnionOptimizationGuardResult
    where
        F2: Fn(&Type, &Type) -> bool + Copy,
    {
        match (l.deref(), u.deref()) {
            (TypeInner::UnionT(_, rep1), TypeInner::UnionT(_, rep2)) => {
                if UnionRep::same_source(rep1, rep2) {
                    return UnionOptimizationGuardResult::True;
                }
                if UnionRep::same_structure(rep1, rep2) {
                    return UnionOptimizationGuardResult::True;
                }
                // Try O(n) check, then O(n log n) check, then O(n^2) check

                // Only optimize for enums, since this is the only fast path examined below.
                // Note that optimizing both reps with [UnionRep.optimize] can potentially
                // cause a `RecursionCheck.LimitExceeded` exception. (`tests/typeapp_termination`
                // is a sanity check against that.)
                if !rep1.is_optimized_finally() {
                    rep1.optimize_enum_only(cx, |cx, iter| {
                        type_mapper::union_flatten(cx, iter.duped())
                    });
                }
                if !rep2.is_optimized_finally() {
                    rep2.optimize_enum_only(cx, |cx, iter| {
                        type_mapper::union_flatten(cx, iter.duped())
                    });
                }

                match (rep1.check_enum_with_tag(), rep2.check_enum_with_tag()) {
                    (Some((enums1, tag1)), Some((enums2, tag2))) => {
                        if enums1.is_subset(&enums2) {
                            UnionOptimizationGuardResult::True
                        } else if tag1.is_some() && tag1 == tag2 {
                            UnionOptimizationGuardResult::False {
                                diff: enums1.difference(&enums2).cloned().collect(),
                            }
                        } else {
                            UnionOptimizationGuardResult::Maybe
                        }
                    }
                    _ => {
                        let lts: Vec<Type> =
                            rep1.members_iter().map(|t| unwrap_type(cx, t)).collect();
                        let uts: Vec<Type> =
                            rep2.members_iter().map(|t| unwrap_type(cx, t)).collect();

                        if lts.len() == uts.len() && lts.iter().zip(uts.iter()).all(|(l, u)| l == u)
                        {
                            return UnionOptimizationGuardResult::True;
                        }

                        // Check if u contains l after unwrapping annots, tvars and repos types.
                        // This is faster than the n^2 case below because it avoids flattening both
                        // unions
                        let guard = |u_elem: &Type| -> bool {
                            if seen.contains(u_elem) {
                                return false;
                            }
                            seen.insert(u_elem.dupe());
                            let result = matches!(
                                union_optimization_guard_impl(seen, cx, comparator, l, u_elem),
                                UnionOptimizationGuardResult::True
                            );
                            seen.remove(u_elem);
                            result
                        };
                        if uts.iter().any(guard) {
                            return UnionOptimizationGuardResult::True;
                        }

                        // `union_compare` is potentially very expensive as it flattens the
                        // input unions. Do not perform this check on synthetic unions.
                        // Instead, allow the unfolding of the LHS union which might enable
                        // other optimized path.
                        if !rep1.is_synthetic() && union_compare(cx, comparator, &lts, &uts) {
                            return UnionOptimizationGuardResult::True;
                        }

                        UnionOptimizationGuardResult::Maybe
                    }
                }
            }
            _ => UnionOptimizationGuardResult::Maybe,
        }
    }

    union_optimization_guard_impl(&mut TypeSet::new(), cx, comparator, l, u)
}

// Optimization where an union is a subset of another. Equality modulo
// reasons is important for this optimization to be effective, since types
// are repositioned everywhere.
pub fn remove_predicate_from_union<'cx, F>(
    reason: Reason,
    cx: &mut Context<'cx>,
    predicate: F,
    rep: &flow_typing_type::type_::union_rep::UnionRep,
) -> Type
where
    F: Fn(&Type) -> bool,
{
    use flow_typing_type::type_util::union_of_ts;
    use flow_typing_visitors::type_mapper::union_flatten;

    let flattened = union_flatten(cx, rep.members_iter().duped());
    let filtered: Vec<Type> = flattened.into_iter().filter(|t| !predicate(t)).collect();
    union_of_ts(reason, filtered, None)
}

pub fn iter_union<'cx, T, E, F, J, CX: ContextFamily>(
    f: F,
    init: T,
    join: J,
    cx: &mut Context<'cx>,
    trace: &DepthTrace,
    rep: &flow_typing_type::type_::union_rep::UnionRep,
    u: &UseT<CX>,
) -> Result<T, E>
where
    F: Fn(&mut Context<'cx>, &DepthTrace, (&Type, &UseT<CX>)) -> Result<T, E>,
    J: Fn(T, T) -> T,
{
    let mut acc = init;
    for b in rep.members_iter() {
        let r = f(cx, trace, utils_js::mk_tuple_swapped(u, b))?;
        acc = join(acc, r);
    }
    Ok(acc)
}

pub fn map_union<'cx, F>(
    f: F,
    cx: &mut Context<'cx>,
    trace: &DepthTrace,
    rep: &flow_typing_type::type_::union_rep::UnionRep,
    reason: Reason,
) -> Result<Type, FlowJsException>
where
    F: Fn(&mut Context<'cx>, &DepthTrace, &Type, Type) -> Result<(), FlowJsException>,
{
    use flow_typing_tvar::mk_where;
    use flow_typing_type::type_util::reason_of_t;
    use flow_typing_type::type_util::union_of_ts;

    let ts: Result<Vec<_>, _> = rep
        .members_iter()
        .map(|t| {
            mk_where(cx, reason_of_t(t).dupe(), |cx, tout| {
                f(cx, trace, t, tout.dupe())
            })
        })
        .collect();
    Ok(union_of_ts(reason, ts?, None))
}

pub fn map_inter<'cx, F>(
    f: F,
    cx: &mut Context<'cx>,
    trace: &DepthTrace,
    rep: &inter_rep::InterRep,
    reason: Reason,
) -> Result<Type, FlowJsException>
where
    F: Fn(&mut Context<'cx>, &DepthTrace, &Type, Type) -> Result<(), FlowJsException>,
{
    use flow_typing_tvar::mk_where;
    use flow_typing_type::type_::DefT;
    use flow_typing_type::type_util::reason_of_t;

    let ts: Result<Vec<_>, _> = rep
        .members_iter()
        .map(|t| {
            mk_where(cx, reason_of_t(t).dupe(), |cx, tout| {
                f(cx, trace, t, tout.dupe())
            })
        })
        .collect();
    let ts = ts?;

    Ok(match ts.len() {
        // If we have no types then this is an error.
        0 => Type::new(TypeInner::DefT(reason, DefT::new(DefTInner::EmptyT))),
        // If we only have one type then only that should be used.
        1 => ts.into_iter().next().unwrap(),
        // If we have more than one type then we make a union type.
        _ => {
            let mut iter = ts.into_iter();
            let t0 = iter.next().unwrap();
            let t1 = iter.next().unwrap();
            let rest: Vec<Type> = iter.collect();
            Type::new(TypeInner::IntersectionT(
                reason,
                inter_rep::make(t0, t1, rest.into()),
            ))
        }
    })
}

pub fn iter_resolve_union<'cx, F, CX: ContextFamily>(
    f: F,
    cx: &mut Context<'cx>,
    trace: &DepthTrace,
    reason: Reason,
    rep: &flow_typing_type::type_::union_rep::UnionRep,
    upper: UseT<CX>,
) -> Result<(), FlowJsException>
where
    F: FnOnce(&mut Context<'cx>, &DepthTrace, (Type, UseT<CX>)) -> Result<(), FlowJsException>,
{
    use flow_common::reason::mk_id;
    use flow_typing_type::type_::UseTInner;

    // We can't guarantee that tvars or typeapps get resolved, even though we'd like
    // to believe they will. Instead, we separate out all the resolvable types from
    // the union, resolve them (f), and then rejoin them with the other types once
    // they have been resolved.
    let (evals, resolved): (Vec<Type>, Vec<Type>) =
        rep.members_iter().duped().partition(is_union_resolvable);

    let mut evals_iter = evals.into_iter();
    if let Some(first) = evals_iter.next() {
        let resolve_union_t = UseT::new(UseTInner::ResolveUnionT(Box::new(ResolveUnionTData {
            reason: reason.dupe(),
            resolved: resolved.into_iter().collect(),
            unresolved: evals_iter.collect(),
            upper: Box::new(upper),
            id: mk_id() as i32,
        })));
        f(cx, trace, (first, resolve_union_t))?;
    }
    Ok(())
}

// Generics

// New generics mode: generate a GenericT from a generic
pub fn generic_of_tparam<'cx, F>(cx: &mut Context<'cx>, f: F, tparam: &TypeParam) -> Type
where
    F: FnOnce(&mut Context<'cx>, &Type) -> Type,
{
    use flow_typing_type::type_util::mod_reason_of_t;

    let param_loc = tparam.reason.loc().dupe();
    let bound = f(cx, &tparam.bound);
    let id = cx.make_generic_id(tparam.name.dupe(), &param_loc);

    let bound = mod_reason_of_t(
        &|bound_reason: Reason| {
            let annot_loc = bound_reason.annot_loc().map(|l| l.dupe());
            let desc = bound_reason.desc(false).cloned();
            let base_reason = Reason::new(desc, param_loc.dupe());
            base_reason.opt_annotate(annot_loc)
        },
        &bound,
    );

    Type::new(TypeInner::GenericT(Box::new(GenericTData {
        reason: tparam.reason.dupe(),
        name: tparam.name.dupe(),
        id,
        bound,
        no_infer: false,
    })))
}

pub fn generic_bound<'cx>(
    cx: &mut Context<'cx>,
    prev_map: FlowOrdMap<flow_common::subst_name::SubstName, Type>,
    tparam: &TypeParam,
) -> (Type, FlowOrdMap<flow_common::subst_name::SubstName, Type>) {
    use crate::type_subst::Purpose;
    use crate::type_subst::subst;

    let generic = generic_of_tparam(
        cx,
        |cx, bound| {
            subst(
                cx,
                None,
                true,
                false,
                Purpose::Normal,
                &prev_map,
                bound.dupe(),
            )
        },
        tparam,
    );
    let mut new_map = prev_map;
    new_map.insert(tparam.name.dupe(), generic.dupe());
    (generic, new_map)
}

pub fn mk_tparams<'cx>(
    cx: &mut Context<'cx>,
    params: &[TypeParam],
) -> (
    FlowOrdMap<flow_common::subst_name::SubstName, Type>,
    Vec<Type>,
) {
    params.iter().fold(
        (FlowOrdMap::new(), Vec::new()),
        |(prev_map, mut lst), tparam| {
            let (generic, map) = generic_bound(cx, prev_map, tparam);
            lst.push(generic);
            (map, lst)
        },
    )
}

pub fn poly_minimum_arity(tparams: &Vec1<TypeParam>) -> usize {
    tparams
        .iter()
        .filter(|typeparam| typeparam.default.is_none())
        .count()
}

// This typing
pub fn default_this_type<'cx>(
    cx: &mut Context<'cx>,
    needs_this_param: bool,
    func: &flow_parser::ast::function::Function<ALoc, ALoc>,
) -> Result<Type, FlowJsException> {
    use flow_common::reason::ReasonDescFunction;
    use flow_parser_utils::signature_utils::this_finder;
    use flow_typing_errors::error_message::ErrorMessage;
    use flow_typing_type::type_::AnyErrorKind;
    use flow_typing_type::type_::any_t;
    use flow_typing_type::type_::mixed_t;

    let env = &FlowJsEnv::entry();
    let params = &func.params;
    let body = &func.body;
    let reason = Reason::new(
        VirtualReasonDesc::RFunction(ReasonDescFunction::RNormal),
        params.loc.dupe(),
    );
    if this_finder::missing_this_annotation(needs_this_param, body, params) {
        add_output_with_env(
            cx,
            env,
            ErrorMessage::EMissingThisAnnotation(params.loc.dupe()),
        )?;
        Ok(any_t::error_of_kind(
            AnyErrorKind::MissingAnnotation,
            reason,
        ))
    } else {
        Ok(mixed_t::make(reason))
    }
}

// Object Subtyping

/// The key type of a string-valued property key. A symbol key has no string
/// spelling, so callers must route it through `type_of_key_name` instead.
pub fn string_key(value: FlowSmolStr, reason: &Reason) -> Type {
    use flow_typing_type::type_::DefT;

    let key_reason = reason.dupe().replace_desc(None);
    Type::new(TypeInner::DefT(
        key_reason,
        DefT::new(DefTInner::SingletonStrT {
            value,
            from_annot: false,
        }),
    ))
}

// common case checking a function as an object
pub fn quick_error_fun_as_obj<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    use_op: &UseOp,
    lower: &Type,
    statics: &Type,
    upper: &Type,
    props: &flow_typing_type::type_::properties::PropertiesMap,
) -> Result<bool, FlowJsException> {
    use flow_typing_errors::error_message::ErrorMessage;
    use flow_typing_type::type_::PropertyInner;
    use flow_typing_type::type_::properties::PropertiesMap;

    let statics_own_props = match statics.deref() {
        TypeInner::DefT(_, def) => match def.deref() {
            DefTInner::ObjT(obj) => Some(cx.find_props(obj.props_tmap.dupe())),
            DefTInner::MixedT(_) => Some(PropertiesMap::new()),
            _ => None,
        },
        TypeInner::AnyT(_, _) => Some(PropertiesMap::new()),
        _ => None,
    };

    match statics_own_props {
        Some(statics_own_props) => {
            let mut props_not_found = BTreeMap::new();
            for (x, p) in props.iter() {
                let optional = match p.deref() {
                    PropertyInner::Field(fd) => {
                        matches!(fd.type_.deref(), TypeInner::OptionalT { .. })
                    }
                    _ => false,
                };
                if !(optional || is_function_prototype(x) || statics_own_props.contains_key(x)) {
                    props_not_found.insert(x.dupe(), p.dupe());
                }
            }

            if props_not_found.is_empty() {
                Ok(false)
            } else if statics_own_props.is_empty() && !props.is_empty() {
                let error_message = incompatible_types_error(
                    lower,
                    upper,
                    use_op.dupe(),
                    Some(Explanation::ExplanationFunctionsWithStaticsToObject),
                );
                add_output_with_env(cx, env, error_message)?;
                Ok(true)
            } else {
                let reason = flow_typing_type::type_util::reason_of_t(lower);
                let reason_o = flow_typing_type::type_util::reason_of_t(upper);
                for x in props_not_found.keys() {
                    let err = ErrorMessage::EPropNotFoundInSubtyping(Box::new(
                        EPropNotFoundInSubtypingData {
                            prop_name: Some(x.dupe()),
                            lower: type_reference_with_reason_for_error(lower, reason.dupe()),
                            upper: type_reference_with_reason_for_error(upper, reason_o.dupe()),
                            use_op: use_op.dupe(),
                            suggestion: None,
                        },
                    ));
                    add_output_with_env(cx, env, err)?;
                }
                Ok(true)
            }
        }
        None => Ok(false),
    }
}

// Instantiation

// instantiate each param of a polymorphic type with its upper bound
pub fn instantiate_poly_param_upper_bounds<'cx>(
    cx: &mut Context<'cx>,
    typeparams: &Vec1<TypeParam>,
) -> Vec<Type> {
    use flow_data_structure_wrapper::ord_map::FlowOrdMap;

    use crate::type_subst::Purpose;
    use crate::type_subst::subst;

    let (_, list) = typeparams.iter().fold(
        (FlowOrdMap::new(), Vec::new()),
        |(mut map, mut list), tparam| {
            let t = subst(
                cx,
                None,
                true,
                false,
                Purpose::Normal,
                &map,
                tparam.bound.dupe(),
            );
            map.insert(tparam.name.dupe(), t.dupe());
            list.push(t);
            (map, list)
        },
    );
    list
}

// Builtins

pub fn emit_cacheable_env_error<'cx>(
    cx: &mut Context<'cx>,
    loc: ALoc,
    err: flow_env_builder::env_api::CacheableEnvError<ALoc>,
) {
    emit_cacheable_env_error_with_env(cx, &FlowJsEnv::entry(), loc, err)
}

fn emit_cacheable_env_error_with_env<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    loc: ALoc,
    err: flow_env_builder::env_api::CacheableEnvError<ALoc>,
) {
    use flow_env_builder::env_api::CacheableEnvError;
    use flow_typing_errors::error_message::BindingError;
    use flow_typing_errors::error_message::ErrorMessage;

    let msg = match err {
        CacheableEnvError::ReferencedBeforeDeclaration { name, def_loc } => {
            ErrorMessage::EBindingError(Box::new((
                BindingError::EReferencedBeforeDeclaration,
                loc,
                Name::new(name),
                def_loc,
            )))
        }
        CacheableEnvError::BuiltinNameLookupFailed(name) => {
            ErrorMessage::EBuiltinNameLookupFailed(Box::new(EBuiltinNameLookupFailedData {
                loc,
                name,
            }))
        }
        CacheableEnvError::UmdGlobalValueInModule(name) => {
            ErrorMessage::EUmdGlobalValueInModule(Box::new(EUmdGlobalValueInModuleData {
                loc,
                name,
            }))
        }
    };
    add_output_with_env(cx, env, msg).unwrap()
}

pub fn lookup_builtin_module_error<'cx>(
    cx: &mut Context<'cx>,
    module_name: &flow_data_structure_wrapper::smol_str::FlowSmolStr,
    loc: ALoc,
) -> Result<Type, FlowJsException> {
    use flow_typing_errors::error_message::ErrorMessage;
    use flow_typing_type::type_::AnySource;

    let env = &FlowJsEnv::entry();
    let module_name_str = module_name.dupe();
    let potential_generator = cx
        .missing_module_generators()
        .iter()
        .find(|(pattern, _)| pattern.is_match(module_name_str.as_str()))
        .map(|(_, generator)| flow_data_structure_wrapper::smol_str::FlowSmolStr::new(generator));

    add_output_with_env(
        cx,
        env,
        ErrorMessage::EBuiltinModuleLookupFailed(Box::new(EBuiltinModuleLookupFailedData {
            loc: loc.dupe(),
            potential_generator,
            name: module_name_str,
        })),
    )?;

    let reason = Reason::new(None, loc);
    Ok(Type::new(TypeInner::AnyT(
        reason,
        AnySource::AnyError(Some(flow_typing_type::type_::AnyErrorKind::UnresolvedName)),
    )))
}

/// The specifier resolved to a global libdef (a `[libs]` file), which is not a
/// module. Report a dedicated, actionable error at the import site instead of
/// the misleading cannot-resolve-module.
pub fn cannot_import_global_libdef_error<'cx>(
    cx: &mut Context<'cx>,
    module_name: &flow_data_structure_wrapper::smol_str::FlowSmolStr,
    libdef_name: &flow_data_structure_wrapper::smol_str::FlowSmolStr,
    loc: ALoc,
) -> Result<Type, FlowJsException> {
    use flow_typing_errors::error_message::ECannotImportGlobalLibdefData;
    use flow_typing_errors::error_message::ErrorMessage;
    use flow_typing_type::type_::AnySource;

    add_output_with_env(
        cx,
        &FlowJsEnv::entry(),
        ErrorMessage::ECannotImportGlobalLibdef(Box::new(ECannotImportGlobalLibdefData {
            loc: loc.dupe(),
            module_name: module_name.dupe(),
            libdef_name: libdef_name.dupe(),
        })),
    )?;

    let reason = Reason::new(None, loc);
    Ok(Type::new(TypeInner::AnyT(
        reason,
        AnySource::AnyError(Some(flow_typing_type::type_::AnyErrorKind::UnresolvedName)),
    )))
}

fn lookup_builtin_name_error(
    name: &str,
    loc: ALoc,
) -> Result<
    Type,
    (
        Type,
        Vec<flow_env_builder::env_api::CacheableEnvError<ALoc>>,
    ),
> {
    use flow_env_builder::env_api::CacheableEnvError;
    use flow_typing_type::type_::AnySource;

    let reason = Reason::new(None, loc);
    let any_t = Type::new(TypeInner::AnyT(
        reason,
        AnySource::AnyError(Some(flow_typing_type::type_::AnyErrorKind::UnresolvedName)),
    ));
    Err((
        any_t,
        vec![CacheableEnvError::BuiltinNameLookupFailed(
            flow_data_structure_wrapper::smol_str::FlowSmolStr::new(name),
        )],
    ))
}

pub fn lookup_builtin_value_result<'cx>(
    cx: &mut Context<'cx>,
    x: &str,
    reason: Reason,
) -> Result<
    Type,
    (
        Type,
        Vec<flow_env_builder::env_api::CacheableEnvError<ALoc>>,
    ),
> {
    use flow_env_builder::env_api::CacheableEnvError;
    use flow_typing_type::type_util::mod_reason_of_t;

    if let Some((_, t)) = cx.builtin_value_opt(x) {
        return Ok(mod_reason_of_t(&|_| reason.dupe(), &t));
    }
    // Every non-library file is a module, and TypeScript does not let modules use UMD globals as
    // values.
    match cx.builtin_umd_value_opt(x) {
        Some((_, t)) => Err((
            mod_reason_of_t(&|_| reason.dupe(), &t),
            vec![CacheableEnvError::UmdGlobalValueInModule(FlowSmolStr::new(
                x,
            ))],
        )),
        None => lookup_builtin_name_error(x, reason.loc().dupe()),
    }
}

pub fn lookup_builtin_type_result<'cx>(
    cx: &mut Context<'cx>,
    x: &str,
    reason: Reason,
) -> Result<
    Type,
    (
        Type,
        Vec<flow_env_builder::env_api::CacheableEnvError<ALoc>>,
    ),
> {
    use flow_typing_type::type_util::mod_reason_of_t;

    match cx.builtin_type_opt(x) {
        Some((_, t)) => Ok(mod_reason_of_t(&|_| reason.dupe(), &t)),
        None => lookup_builtin_name_error(x, reason.loc().dupe()),
    }
}

fn apply_errors<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    reason: Reason,
    result: Result<
        Type,
        (
            Type,
            Vec<flow_env_builder::env_api::CacheableEnvError<ALoc>>,
        ),
    >,
) -> Type {
    match result {
        Ok(t) => t,
        Err((t, errs)) => {
            for err in errs {
                emit_cacheable_env_error_with_env(cx, env, reason.loc().dupe(), err);
            }
            t
        }
    }
}

pub fn lookup_builtin_value<'cx>(cx: &mut Context<'cx>, x: &str, reason: Reason) -> Type {
    let env = &FlowJsEnv::entry();
    let result = lookup_builtin_value_result(cx, x, reason.dupe());
    apply_errors(cx, env, reason, result)
}

pub fn lookup_builtin_type<'cx>(cx: &mut Context<'cx>, x: &str, reason: Reason) -> Type {
    lookup_builtin_type_with_env(cx, &FlowJsEnv::entry(), x, reason)
}

pub fn lookup_builtin_type_with_env<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    x: &str,
    reason: Reason,
) -> Type {
    let result = lookup_builtin_type_result(cx, x, reason.dupe());
    apply_errors(cx, env, reason, result)
}

pub fn lookup_builtin_typeapp<'cx>(
    cx: &mut Context<'cx>,
    reason: Reason,
    x: &str,
    targs: Vec<Type>,
) -> Type {
    lookup_builtin_typeapp_with_env(cx, &FlowJsEnv::entry(), reason, x, targs)
}

fn lookup_builtin_typeapp_with_env<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    reason: Reason,
    x: &str,
    targs: Vec<Type>,
) -> Type {
    use flow_typing_type::type_util::typeapp;

    let t = lookup_builtin_type_with_env(cx, env, x, reason.dupe());
    typeapp(false, false, reason, t, targs)
}

pub fn is_builtin_class_id<'cx>(
    class_ref: &str,
    class_id: flow_aloc::ALocId,
    cx: &mut Context<'cx>,
) -> bool {
    use flow_typing_type::type_::DefTInner;
    use flow_typing_type::type_::PolyTData;

    match cx.builtin_type_opt(class_ref) {
        Some((_, t)) => {
            if let TypeInner::OpenT(tvar) = t.deref() {
                let id = tvar.id();
                let (_, constraints) = cx.find_constraints(id as i32);
                if let Constraints::FullyResolved(s) = &constraints {
                    let forced = cx.force_fully_resolved_tvar(s);
                    if let TypeInner::DefT(_, def_t) = forced.deref() {
                        match def_t.deref() {
                            DefTInner::PolyT(box PolyTData { t_out, .. }) => {
                                if let TypeInner::DefT(_, inner_def_t) = t_out.deref() {
                                    if let DefTInner::ClassT(class_inner) = inner_def_t.deref() {
                                        let ref_class_id = match class_inner.deref() {
                                            TypeInner::DefT(_, def_t) => {
                                                if let DefTInner::InstanceT(inst_type) =
                                                    def_t.deref()
                                                {
                                                    Some(inst_type.inst.class_id.dupe())
                                                } else {
                                                    None
                                                }
                                            }
                                            TypeInner::ThisInstanceT(box ThisInstanceTData {
                                                instance,
                                                ..
                                            }) => Some(instance.inst.class_id.dupe()),
                                            _ => None,
                                        };
                                        if let Some(ref_class_id) = ref_class_id {
                                            return class_id == ref_class_id;
                                        }
                                    }
                                }
                            }
                            DefTInner::ClassT(class_inner) => {
                                let ref_class_id = match class_inner.deref() {
                                    TypeInner::DefT(_, def_t) => {
                                        if let DefTInner::InstanceT(inst_type) = def_t.deref() {
                                            Some(inst_type.inst.class_id.dupe())
                                        } else {
                                            None
                                        }
                                    }
                                    TypeInner::ThisInstanceT(box ThisInstanceTData {
                                        instance,
                                        ..
                                    }) => Some(instance.inst.class_id.dupe()),
                                    _ => None,
                                };
                                if let Some(ref_class_id) = ref_class_id {
                                    return class_id == ref_class_id;
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
            false
        }
        _ => false,
    }
}

pub fn is_builtin_iterable_class_id<'cx>(
    class_id: flow_aloc::ALocId,
    cx: &mut Context<'cx>,
) -> bool {
    is_builtin_class_id("Iterable", class_id, cx)
}

pub fn builtin_react_element_nominal_id<'cx>(
    cx: &mut Context<'cx>,
) -> Option<flow_typing_type::type_::nominal::Id> {
    use flow_typing_type::type_::DefTInner;
    use flow_typing_type::type_::PolyTData;
    use flow_typing_type::type_::TypeTKind;

    match cx.builtin_type_opt("ExactReactElement_DEPRECATED") {
        Some((_, t)) => {
            if let TypeInner::OpenT(tvar) = t.deref() {
                let id = tvar.id();
                let (_, constraints) = cx.find_constraints(id as i32);
                if let Constraints::FullyResolved(s) = &constraints {
                    let forced = cx.force_fully_resolved_tvar(s);
                    if let TypeInner::DefT(_, def_t) = forced.deref() {
                        if let DefTInner::PolyT(box PolyTData { t_out, .. }) = def_t.deref() {
                            if let TypeInner::DefT(_, inner_def_t) = t_out.deref() {
                                if let DefTInner::TypeT(TypeTKind::OpaqueKind, inner_type) =
                                    inner_def_t.deref()
                                {
                                    if let TypeInner::NominalT { nominal_type, .. } =
                                        inner_type.deref()
                                    {
                                        return Some(nominal_type.nominal_id.clone());
                                    }
                                }
                            }
                        }
                    }
                }
            }
            None
        }
        _ => None,
    }
}

pub fn builtin_react_renders_exactly_nominal_id<'cx>(
    cx: &mut Context<'cx>,
) -> Option<flow_typing_type::type_::nominal::Id> {
    use flow_typing_type::type_::DefTInner;
    use flow_typing_type::type_::PolyTData;
    use flow_typing_type::type_::TypeTKind;

    match cx.builtin_type_opt("React$RendersExactly") {
        Some((_, t)) => {
            if let TypeInner::OpenT(tvar) = t.deref() {
                let id = tvar.id();
                let (_, constraints) = cx.find_constraints(id as i32);
                if let Constraints::FullyResolved(s) = &constraints {
                    let forced = cx.force_fully_resolved_tvar(s);
                    if let TypeInner::DefT(_, def_t) = forced.deref() {
                        if let DefTInner::PolyT(box PolyTData { t_out, .. }) = def_t.deref() {
                            if let TypeInner::DefT(_, inner_def_t) = t_out.deref() {
                                if let DefTInner::TypeT(TypeTKind::OpaqueKind, inner_type) =
                                    inner_def_t.deref()
                                {
                                    if let TypeInner::NominalT { nominal_type, .. } =
                                        inner_type.deref()
                                    {
                                        return Some(nominal_type.nominal_id.clone());
                                    }
                                }
                            }
                        }
                    }
                }
            }
            None
        }
        _ => None,
    }
}

pub fn enum_proto<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    reason: Reason,
    enum_object_t: Type,
    enum_value_t: Type,
    representation_t: Type,
) -> Type {
    lookup_builtin_typeapp_with_env(
        cx,
        env,
        reason,
        "$EnumProto",
        vec![enum_object_t, enum_value_t, representation_t],
    )
}

// Determines whether a property name should be considered "munged"/private when
// the `munge_underscores` config option is set.
pub fn is_munged_prop_name_with_munge(name: &Name, should_munge_underscores: bool) -> bool {
    signature_utils::is_munged_property_name(name) && should_munge_underscores
}

pub fn is_munged_prop_name<'cx>(cx: &Context<'cx>, name: &Name) -> bool {
    is_munged_prop_name_with_munge(name, cx.should_munge_underscores())
}

pub fn obj_key_mirror<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    o: &flow_typing_type::type_::ObjType,
    reason_op: &Reason,
) -> Type {
    use flow_typing_type::type_::DefT;
    use flow_typing_type::type_::FieldData;
    use flow_typing_type::type_::Flags;
    use flow_typing_type::type_::ObjKind;
    use flow_typing_type::type_::ObjType;
    use flow_typing_type::type_::Property;
    use flow_typing_type::type_::PropertyInner;
    use flow_typing_type::type_util::optional;

    let map_t = |key: Type, t: &Type| -> Type {
        if matches!(t.deref(), TypeInner::OptionalT { .. }) {
            optional(key, None, false)
        } else {
            key
        }
    };

    let map_field = |key: &Name, t: &Type| -> Type {
        match key {
            // A symbol-typed key mirrors to its `unique symbol` type, not a string literal.
            Name::Symbol(_) => {
                let r = reason_op
                    .dupe()
                    .update_desc(|_| Some(VirtualReasonDesc::RUniqueSymbol));
                map_t(type_of_key_name_with_env(env, key.dupe(), &r), t)
            }
            Name::Str(key_str) => {
                let key_str = key_str.dupe();
                let reason = reason_op
                    .dupe()
                    .update_desc(|_| Some(VirtualReasonDesc::RStringLit(key_str.dupe())));
                let singleton = Type::new(TypeInner::DefT(
                    reason,
                    DefT::new(DefTInner::SingletonStrT {
                        from_annot: true,
                        value: key_str,
                    }),
                ));
                map_t(singleton, t)
            }
        }
    };

    let props = cx.find_props(o.props_tmap.dupe());
    let mut new_props = BTreeMap::new();
    for (name, prop) in props.iter() {
        match prop.deref() {
            PropertyInner::Field(fd) => {
                let new_type = map_field(name, &fd.type_);
                new_props.insert(
                    name.dupe(),
                    Property::new(PropertyInner::Field(Box::new(FieldData {
                        preferred_def_locs: fd.preferred_def_locs.clone(),
                        key_loc: fd.key_loc.dupe(),
                        type_: new_type,
                        polarity: fd.polarity,
                    }))),
                );
            }
            _ => {
                new_props.insert(name.dupe(), prop.dupe());
            }
        }
    }
    let props_tmap = cx.generate_property_map(new_props.into());
    let obj_kind = match &o.flags.obj_kind {
        ObjKind::Indexed(dict) => {
            let value = map_t(dict.key.dupe(), &dict.value);
            ObjKind::Indexed(flow_typing_type::type_::DictType {
                dict_name: dict.dict_name.dupe(),
                key: dict.key.dupe(),
                value,
                dict_polarity: dict.dict_polarity,
            })
        }
        other => other.clone(),
    };
    let flags = Flags {
        obj_kind,
        react_dro: o.flags.react_dro.clone(),
    };
    let reason = reason_op
        .dupe()
        .update_desc(|_| Some(VirtualReasonDesc::RObjectType));
    Type::new(TypeInner::DefT(
        reason,
        DefT::new(DefTInner::ObjT(
            ObjType {
                flags,
                props_tmap,
                proto_t: o.proto_t.dupe(),
                call_t: o.call_t,
                reachable_targs: o.reachable_targs.dupe(),
                strictness_kind: o.strictness_kind,
                literal: false,
            }
            .into(),
        )),
    ))
}

pub fn namespace_type_with_values_type<'cx>(
    cx: &mut Context<'cx>,
    namespace_symbol: flow_common::flow_symbol::Symbol,
    values_type: Type,
    types: &std::collections::BTreeMap<Name, flow_typing_type::type_::NamedSymbol>,
) -> Type {
    use flow_common::polarity::Polarity;
    use flow_typing_type::type_::FieldData;
    use flow_typing_type::type_::NamespaceType;
    use flow_typing_type::type_::Property;
    use flow_typing_type::type_::PropertyInner;
    use flow_typing_type::type_::properties::PropertiesMap;

    let mk_prop = |symbol: &flow_typing_type::type_::NamedSymbol| -> Property {
        Property::new(PropertyInner::Field(Box::new(FieldData {
            preferred_def_locs: symbol.preferred_def_locs.clone(),
            key_loc: symbol.name_loc.dupe(),
            type_: symbol.type_.dupe(),
            polarity: Polarity::Positive,
        })))
    };

    let types_props: PropertiesMap = types
        .iter()
        .map(|(name, symbol)| (name.dupe(), mk_prop(symbol)))
        .collect();
    let types_tmap = cx.generate_property_map(types_props);
    Type::new(TypeInner::NamespaceT(std::rc::Rc::new(NamespaceType {
        namespace_symbol,
        values_type,
        types_tmap,
    })))
}

pub fn namespace_type<'cx>(
    cx: &mut Context<'cx>,
    reason: Reason,
    namespace_symbol: flow_common::flow_symbol::Symbol,
    values: &std::collections::BTreeMap<Name, flow_typing_type::type_::NamedSymbol>,
    types: &std::collections::BTreeMap<Name, flow_typing_type::type_::NamedSymbol>,
) -> Type {
    use flow_common::polarity::Polarity;
    use flow_typing_type::type_::FieldData;
    use flow_typing_type::type_::ObjKind;
    use flow_typing_type::type_::Property;
    use flow_typing_type::type_::PropertyInner;
    use flow_typing_type::type_::properties::PropertiesMap;

    let mk_prop = |symbol: &flow_typing_type::type_::NamedSymbol| -> Property {
        Property::new(PropertyInner::Field(Box::new(FieldData {
            preferred_def_locs: symbol.preferred_def_locs.clone(),
            key_loc: symbol.name_loc.dupe(),
            type_: symbol.type_.dupe(),
            polarity: Polarity::Positive,
        })))
    };

    let props: PropertiesMap = values
        .iter()
        .map(|(name, symbol)| (name.dupe(), mk_prop(symbol)))
        .collect();

    let proto = Type::new(TypeInner::ObjProtoT(reason.dupe()));
    let values_type = crate::obj_type::mk_with_proto(
        cx,
        reason.dupe(),
        ObjKind::Exact,
        None,
        None,
        Some(props),
        None,
        proto,
        false,
    );
    namespace_type_with_values_type(cx, namespace_symbol, values_type, types)
}

// TS enums (.ts/.d.ts) are represented as a NamespaceT whose symbol kind is
// [SymbolEnum]: its values_type is an exact object of literal-typed members and
// its types_tmap maps each member to a TypeT-wrapped singleton. These helpers
// are the single source of truth for (a) recognizing that representation and
// (b) computing its meaning as a *bare* type — the union of the member literals.
// The producers that build the representation live in ts_enum.rs
// (ts_enum.rs's mk_ts_enum_namespace for local checking and
// type_sig_merge.rs's merge_ts_enum_namespace for signatures); the consumers (the
// value->type transforms, inspection-concretization, and the import value/type
// classifier) call these.
pub fn is_ts_enum_symbol(namespace_symbol: &flow_common::flow_symbol::Symbol) -> bool {
    namespace_symbol.kind() == flow_common::flow_symbol::SymbolKind::SymbolEnum
}

// The member literal types of a TS enum, read out of its [types_tmap] and
// unwrapped from the [TypeT] wrappers the representation stores them in. Shared
// by [ts_enum_member_union] and the normalizer.rs rendering of a TS enum as a
// type. Entries that are not TypeT-wrapped are dropped: the representation never
// produces them, and a malformed one should not leak into the result.
pub fn ts_enum_member_types<'cx>(
    cx: &mut Context<'cx>,
    types_tmap: flow_typing_type::type_::properties::Id,
) -> Vec<Type> {
    use flow_typing_type::type_::DefTInner;
    use flow_typing_type::type_::TypeInner;
    use flow_typing_type::type_::property;

    // [NameUtils.Map.values] (OCaml [WrappedMap.values]) folds with a prepend, so
    // it yields values in descending key order; mirror that with [rev] so the
    // member union renders in the same order as the OCaml [.exp].
    cx.find_props(types_tmap)
        .iter()
        .rev()
        .filter_map(|(_, p)| match property::read_t(p) {
            Some(t) => match t.deref() {
                TypeInner::DefT(_, def_t) => match def_t.deref() {
                    DefTInner::TypeT(_, inner) => Some(inner.dupe()),
                    _ => None,
                },
                _ => None,
            },
            _ => None,
        })
        .collect()
}

pub fn ts_enum_member_union<'cx>(
    cx: &mut Context<'cx>,
    reason: Reason,
    types_tmap: flow_typing_type::type_::properties::Id,
) -> Type {
    use flow_typing_type::type_util::union_of_ts;

    union_of_ts(reason, ts_enum_member_types(cx, types_tmap), None)
}

pub fn obj_is_readonlyish(obj: &flow_typing_type::type_::ObjType) -> bool {
    obj.flags.react_dro.is_some()
}

pub fn is_exception_to_react_dro(propref: &flow_typing_type::type_::PropRef) -> bool {
    use flow_typing_type::type_::PropRef;
    match propref {
        PropRef::Named { name, .. } => name.matches_str("current"),
        _ => false,
    }
}

// Fix a this-abstracted instance type by tying a "knot": assume that the
// fixpoint is some `this`, substitute it as This in the instance type, and
// finally return the result as `this`.
pub fn fix_this_instance<'cx>(
    cx: &mut Context<'cx>,
    reason: Reason,
    reason_i: Reason,
    i: &flow_typing_type::type_::InstanceT,
    is_this: bool,
    this_name: flow_common::subst_name::SubstName,
) -> Type {
    use flow_data_structure_wrapper::ord_map::FlowOrdMap;
    use flow_typing_type::type_::DefT;
    use flow_typing_type::type_::DefTInner;

    use crate::flow_cache;
    use crate::type_subst;

    let cache_key = Type::new(TypeInner::DefT(
        reason_i.dupe(),
        DefT::new(DefTInner::InstanceT(std::rc::Rc::new(i.dupe()))),
    ));
    if let Some(cached) = flow_cache::fix::find(cx, is_this, &cache_key) {
        return cached;
    }

    // Knot-tying: result_cell will hold the computed result once available.
    // The `this` tvar's forcing closure reads from result_cell.
    let result_cell: std::rc::Rc<std::cell::RefCell<Option<Type>>> =
        std::rc::Rc::new(std::cell::RefCell::new(None));
    let result_cell_c = result_cell.dupe();

    let this = flow_typing_tvar::mk_fully_resolved_lazy(
        cx,
        reason_i.dupe(),
        true,
        move |_cx: &mut Context<'_>| Ok(result_cell_c.borrow().as_ref().unwrap().dupe()),
    );
    let this_generic = if is_this {
        Type::new(TypeInner::GenericT(Box::new(GenericTData {
            id: cx.make_generic_id(this_name.dupe(), reason_i.def_loc()),
            reason,
            name: this_name.dupe(),
            bound: this,
            no_infer: false,
        })))
    } else {
        this
    };
    let mut map = FlowOrdMap::new();
    map.insert(this_name, this_generic);
    let subst_i = type_subst::subst_instance_type(
        cx,
        None,
        true,
        false,
        type_subst::Purpose::Normal,
        &map,
        i,
    );
    let result = Type::new(TypeInner::DefT(
        reason_i,
        DefT::new(DefTInner::InstanceT(std::rc::Rc::new(subst_i))),
    ));
    flow_cache::fix::add(cx, is_this, cache_key, result.dupe());
    *result_cell.borrow_mut() = Some(result.dupe());
    result
}

pub trait InstantiationHelper {
    fn reposition<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        trace: Option<DepthTrace>,
        loc: ALoc,
        t: Type,
    ) -> Result<Type, FlowJsException>;

    fn is_subtype<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        trace: DepthTrace,
        use_op: UseOp,
        t1: Type,
        t2: Type,
    ) -> Result<(), FlowJsException>;

    fn unify<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        trace: DepthTrace,
        use_op: UseOp,
        t1: Type,
        t2: Type,
    ) -> Result<(), FlowJsException>;

    fn mk_targ<'cx>(
        cx: &mut Context<'cx>,
        typeparam: &TypeParam,
        reason_op: &Reason,
        reason_tapp: &Reason,
    ) -> Type;
}

pub mod instantiation_kit {
    use std::collections::VecDeque;
    use std::rc::Rc;
    use std::sync::Arc;

    use dupe::Dupe;
    use flow_aloc::ALoc;
    use flow_common::reason::Reason;
    use flow_common::subst_name::SubstName;
    use flow_data_structure_wrapper::ord_map::FlowOrdMap;
    use flow_typing_context::Context;
    use flow_typing_errors::error_message::ETooFewTypeArgsData;
    use flow_typing_errors::error_message::ETooManyTypeArgsData;
    use flow_typing_flow_js_env::FlowJsEnv;
    use flow_typing_type::type_::DepthTrace;
    use flow_typing_type::type_::Type;
    use flow_typing_type::type_::TypeInner;
    use flow_typing_type::type_::TypeParam;
    use flow_typing_type::type_::UseOp;
    use vec1::Vec1;

    use super::FlowJsException;
    use super::InstantiationHelper;
    use super::add_output_with_env;
    use super::poly_minimum_arity;
    use crate::type_subst::Purpose;
    use crate::type_subst::subst;

    // Instantiate a polymorphic definition given type arguments.
    pub fn instantiate_poly_with_targs<'cx, H: InstantiationHelper>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        trace: DepthTrace,
        use_op: UseOp,
        reason_op: &Reason,
        reason_tapp: &Reason,
        errs_ref: &mut Option<Vec<flow_typing_context::SubstCacheErr>>,
        unify_bounds: bool,
        tparams_loc: ALoc,
        xs: &Vec1<TypeParam>,
        t: Type,
        ts: Vec<Type>,
    ) -> Result<(Type, Vec<(Type, SubstName)>), FlowJsException> {
        use flow_typing_context::SubstCacheErr;
        use flow_typing_errors::error_message::ErrorMessage;
        use flow_typing_type::type_::AnySource;
        use flow_typing_type::type_::VirtualFrameUseOp;

        let minimum_arity = poly_minimum_arity(xs) as i32;
        let maximum_arity = xs.len() as i32;
        let arity_loc = tparams_loc;

        if (ts.len() as i32) > maximum_arity {
            add_output_with_env(
                cx,
                env,
                ErrorMessage::ETooManyTypeArgs(Box::new(ETooManyTypeArgsData {
                    reason_tapp: reason_tapp.to_error_reference(),
                    arity_loc: arity_loc.dupe(),
                    maximum_arity,
                })),
            )?;
            if let Some(errs) = errs_ref.as_mut() {
                errs.push(SubstCacheErr::ETooManyTypeArgs(
                    arity_loc.dupe(),
                    maximum_arity,
                ));
            }
        }

        let mut map: FlowOrdMap<SubstName, Type> = FlowOrdMap::default();
        let mut remaining_ts = VecDeque::from(ts);
        let mut all_ts: Vec<(Type, SubstName)> = Vec::new();

        for typeparam in xs.iter() {
            let (t_val, new_all_ts_entry) =
                match (typeparam.default.as_ref(), remaining_ts.pop_front()) {
                    (Some(default), None) => {
                        // fewer arguments than params and we have a default
                        let substituted = subst(
                            cx,
                            Some(use_op.dupe()),
                            true,
                            false,
                            Purpose::Normal,
                            &map,
                            default.dupe(),
                        );
                        (substituted, Some((default.dupe(), typeparam.name.dupe())))
                    }
                    (None, None) => {
                        // fewer arguments than params but no default
                        add_output_with_env(
                            cx,
                            env,
                            ErrorMessage::ETooFewTypeArgs(Box::new(ETooFewTypeArgsData {
                                reason_tapp: reason_tapp.to_error_reference(),
                                arity_loc: arity_loc.dupe(),
                                minimum_arity,
                            })),
                        )?;
                        if let Some(errs) = errs_ref.as_mut() {
                            errs.push(SubstCacheErr::ETooFewTypeArgs(
                                arity_loc.dupe(),
                                minimum_arity,
                            ));
                        }
                        let any_t =
                            Type::new(TypeInner::AnyT(reason_op.dupe(), AnySource::AnyError(None)));
                        (any_t, None)
                    }
                    (_, Some(t_arg)) => (t_arg.dupe(), Some((t_arg, typeparam.name.dupe()))),
                };

            if let Some(entry) = new_all_ts_entry {
                all_ts.push(entry);
            }

            map.insert(typeparam.name.dupe(), t_val);
        }

        if !env.in_implicit_instantiation() {
            for typeparam in xs.iter() {
                let t_val = map
                    .get(&typeparam.name)
                    .expect("every type parameter should have a type argument")
                    .dupe();
                let frame = UseOp::Frame(
                    Arc::new(VirtualFrameUseOp::TypeParamBound {
                        name: typeparam.name.dupe(),
                    }),
                    Arc::new(use_op.dupe()),
                );
                let bound_subst = subst(
                    cx,
                    Some(use_op.dupe()),
                    true,
                    false,
                    Purpose::Normal,
                    &map,
                    typeparam.bound.dupe(),
                );

                if unify_bounds {
                    H::unify(cx, env, trace, frame, t_val, bound_subst)?;
                } else {
                    H::is_subtype(cx, env, trace, frame, t_val, bound_subst)?;
                }
            }
        }

        let substituted_t = subst(
            cx,
            Some(use_op),
            true,
            env.in_implicit_instantiation(),
            Purpose::Normal,
            &map,
            t,
        );
        let repositioned = H::reposition(
            cx,
            env,
            Some(trace),
            reason_tapp.loc().dupe(),
            substituted_t,
        )?;
        Ok((repositioned, all_ts))
    }

    pub fn mk_typeapp_of_poly<'cx, H: InstantiationHelper>(
        cx: &mut Context<'cx>,
        trace: DepthTrace,
        use_op: UseOp,
        reason_op: &Reason,
        reason_tapp: &Reason,
        id: flow_typing_type::type_::poly::Id,
        tparams_loc: ALoc,
        xs: &Vec1<TypeParam>,
        t: Type,
        ts: Rc<[Type]>,
    ) -> Result<Type, FlowJsException> {
        mk_typeapp_of_poly_with_env::<H>(
            cx,
            &FlowJsEnv::entry(),
            trace,
            use_op,
            reason_op,
            reason_tapp,
            id,
            tparams_loc,
            xs,
            t,
            ts,
        )
    }

    pub fn mk_typeapp_of_poly_with_env<'cx, H: InstantiationHelper>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        trace: DepthTrace,
        use_op: UseOp,
        reason_op: &Reason,
        reason_tapp: &Reason,
        id: flow_typing_type::type_::poly::Id,
        tparams_loc: ALoc,
        xs: &Vec1<TypeParam>,
        t: Type,
        ts: Rc<[Type]>,
    ) -> Result<Type, FlowJsException> {
        use flow_typing_context::SubstCacheErr;
        use flow_typing_errors::error_message::ErrorMessage;

        let key = (id.dupe(), ts.dupe());
        if let Some((errs, cached_t)) = cx.subst_cache().get(&key).cloned() {
            for err in errs.iter() {
                match err {
                    SubstCacheErr::ETooManyTypeArgs(arity_loc, maximum_arity) => {
                        add_output_with_env(
                            cx,
                            env,
                            ErrorMessage::ETooManyTypeArgs(Box::new(ETooManyTypeArgsData {
                                reason_tapp: reason_tapp.to_error_reference(),
                                arity_loc: arity_loc.dupe(),
                                maximum_arity: *maximum_arity,
                            })),
                        )?;
                    }
                    SubstCacheErr::ETooFewTypeArgs(arity_loc, minimum_arity) => {
                        add_output_with_env(
                            cx,
                            env,
                            ErrorMessage::ETooFewTypeArgs(Box::new(ETooFewTypeArgsData {
                                reason_tapp: reason_tapp.to_error_reference(),
                                arity_loc: arity_loc.dupe(),
                                minimum_arity: *minimum_arity,
                            })),
                        )?;
                    }
                }
            }
            return Ok(cached_t);
        }

        let mut errs_ref = Some(Vec::new());
        let (result_t, _) = instantiate_poly_with_targs::<H>(
            cx,
            env,
            trace,
            use_op,
            reason_op,
            reason_tapp,
            &mut errs_ref,
            false,
            tparams_loc.dupe(),
            xs,
            t,
            ts.to_vec(),
        )?;

        cx.subst_cache_mut()
            .insert(key, (errs_ref.unwrap(), result_t.dupe()));

        Ok(result_t)
    }

    pub fn instantiate_poly<'cx, H: InstantiationHelper>(
        cx: &mut Context<'cx>,
        trace: DepthTrace,
        use_op: UseOp,
        reason_op: &Reason,
        reason_tapp: &Reason,
        unify_bounds: bool,
        tparams_loc: ALoc,
        xs: &Vec1<TypeParam>,
        t: Type,
    ) -> Result<(Type, Vec<(Type, SubstName)>), FlowJsException> {
        instantiate_poly_with_env::<H>(
            cx,
            &FlowJsEnv::entry(),
            trace,
            use_op,
            reason_op,
            reason_tapp,
            unify_bounds,
            tparams_loc,
            xs,
            t,
        )
    }

    // Instantiate a polymorphic definition by creating fresh type arguments.
    pub fn instantiate_poly_with_env<'cx, H: InstantiationHelper>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        trace: DepthTrace,
        use_op: UseOp,
        reason_op: &Reason,
        reason_tapp: &Reason,
        unify_bounds: bool,
        tparams_loc: ALoc,
        xs: &Vec1<TypeParam>,
        t: Type,
    ) -> Result<(Type, Vec<(Type, SubstName)>), FlowJsException> {
        let ts: Vec<Type> = xs
            .iter()
            .map(|typeparam| H::mk_targ(cx, typeparam, reason_op, reason_tapp))
            .collect();

        instantiate_poly_with_targs::<H>(
            cx,
            env,
            trace,
            use_op,
            reason_op,
            reason_tapp,
            &mut None,
            unify_bounds,
            tparams_loc,
            xs,
            t,
            ts,
        )
    }
}

pub fn mk_distributive_tparam_subst_fn<'cx>(
    cx: &mut Context<'cx>,
    use_op: Option<UseOp>,
    name: flow_common::subst_name::SubstName,
    distributed_t: Type,
) -> impl Fn(&mut Context<'_>, Type) -> Type + use<> {
    use flow_data_structure_wrapper::ord_map::FlowOrdMap;
    use flow_typing_type::type_util::reason_of_t;
    use tvar_visitors::has_unresolved_tvars;

    use crate::type_subst::Purpose;
    use crate::type_subst::free_var_finder;
    use crate::type_subst::subst;

    let distributed_t = match distributed_t.deref() {
        _ if type_util::constraint_node_id(&distributed_t).is_some() => distributed_t.dupe(),
        _ if !free_var_finder(cx, None, &distributed_t).is_empty() => distributed_t.dupe(),
        _ => {
            if has_unresolved_tvars(cx, &distributed_t) {
                flow_typing_tvar::mk_resolved(
                    cx,
                    reason_of_t(&distributed_t).dupe(),
                    distributed_t.dupe(),
                )
            } else {
                flow_typing_tvar::mk_fully_resolved(
                    cx,
                    reason_of_t(&distributed_t).dupe(),
                    distributed_t.dupe(),
                )
            }
        }
    };

    let subst_map: FlowOrdMap<flow_common::subst_name::SubstName, Type> =
        std::iter::once((name, distributed_t)).collect();

    move |cx: &mut Context<'_>, t: Type| -> Type {
        subst(
            cx,
            use_op.dupe(),
            true,
            false,
            Purpose::Normal,
            &subst_map,
            t,
        )
    }
}

pub fn substitute_mapped_type_distributive_tparams<'cx>(
    cx: &mut Context<'cx>,
    use_op: Option<UseOp>,
    distributive_tparam_name: Option<flow_common::subst_name::SubstName>,
    property_type: Type,
    name_type: Option<Type>,
    homomorphic: flow_typing_type::type_::MappedTypeHomomorphicFlag,
    source: Type,
) -> (
    Type,
    Option<Type>,
    flow_typing_type::type_::MappedTypeHomomorphicFlag,
) {
    use flow_typing_type::type_::MappedTypeHomomorphicFlag;

    match distributive_tparam_name {
        None => (property_type, name_type, homomorphic),
        Some(name) => {
            let subst_fn = mk_distributive_tparam_subst_fn(cx, use_op, name, source);

            let homomorphic_prime = match &homomorphic {
                MappedTypeHomomorphicFlag::SemiHomomorphic(t) => {
                    MappedTypeHomomorphicFlag::SemiHomomorphic(subst_fn(cx, t.dupe()))
                }
                MappedTypeHomomorphicFlag::Homomorphic
                | MappedTypeHomomorphicFlag::Unspecialized => homomorphic.dupe(),
            };

            (
                subst_fn(cx, property_type),
                name_type.map(|t| subst_fn(cx, t)),
                homomorphic_prime,
            )
        }
    }
}

pub mod value_to_type_reference_transform {
    use std::ops::Deref;

    use dupe::Dupe;
    use flow_common::reason::Reason;
    use flow_typing_context::Context;
    use flow_typing_errors::error_message::EMissingTypeArgsData;
    use flow_typing_errors::error_message::EnumMemberUsedAsTypeData;
    use flow_typing_errors::error_message::EnumReferenceData;
    use flow_typing_flow_js_env::FlowJsEnv;
    use flow_typing_type::type_::AnyErrorKind;
    use flow_typing_type::type_::DefTInner;
    use flow_typing_type::type_::PolyTData;
    use flow_typing_type::type_::ThisInstanceTData;
    use flow_typing_type::type_::Type;
    use flow_typing_type::type_::TypeInner;
    use flow_typing_type::type_::TypeTKind;
    use flow_typing_type::type_::UseOp;
    use flow_typing_type::type_::any_t;
    use flow_typing_type::type_util;
    use flow_typing_type::type_util::reason_of_t;

    use super::FlowJsException;
    use super::add_output_with_env;
    use super::description_name_for_error;
    use super::fix_this_instance;
    use super::lookup_builtin_type_with_env;
    use super::type_reference_at_loc_for_error;
    use super::type_reference_with_reason_for_error;
    use super::value_as_type_reference_for_error;
    use crate::type_subst::Purpose;
    use crate::type_subst::subst;

    // a component syntax value annotation becomes React$RendersExactly of that component
    fn run_on_abstract_component<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        reason_component: &Reason,
        reason_op: &Reason,
        l: Type,
    ) -> Type {
        use flow_common::reason::react_element_desc_of_component_reason;
        use flow_typing_type::type_util;

        let desc = react_element_desc_of_component_reason(reason_component);
        let annot_loc = reason_op.loc().dupe();
        let elem_reason = reason_op.dupe().replace_desc(desc).annotate(annot_loc);

        let builtin_t =
            lookup_builtin_type_with_env(cx, env, "React$RendersExactly", elem_reason.dupe());
        let t = flow_typing_tvar::mk_fully_resolved(cx, elem_reason.dupe(), builtin_t);

        type_util::typeapp(false, true, elem_reason, t, vec![l])
    }

    pub fn run_on_concrete_type<'cx>(
        cx: &mut Context<'cx>,
        use_op: UseOp,
        reason_op: &Reason,
        kind: TypeTKind,
        t: Type,
    ) -> Result<Type, FlowJsException> {
        run_on_concrete_type_with_env(cx, &FlowJsEnv::entry(), use_op, reason_op, kind, t)
    }

    pub fn run_on_concrete_type_with_env<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        use_op: UseOp,
        reason_op: &Reason,
        kind: TypeTKind,
        t: Type,
    ) -> Result<Type, FlowJsException> {
        use flow_data_structure_wrapper::ord_map::FlowOrdMap;
        use flow_typing_errors::error_message::EnumErrorKind;
        use flow_typing_errors::error_message::ErrorMessage;
        use flow_typing_type::type_::AnySource;

        match t.deref() {
            TypeInner::DefT(reason, def_t) => match def_t.deref() {
                DefTInner::PolyT(box PolyTData { tparams, t_out, .. })
                    if kind == TypeTKind::RenderTypeKind
                        && matches!(
                            t_out.deref(),
                            TypeInner::DefT(_, inner)
                                if matches!(inner.deref(), DefTInner::ReactAbstractComponentT(_))
                        ) =>
                {
                    let mut subst_map = FlowOrdMap::default();
                    for tparam in tparams.iter() {
                        let any_untyped =
                            Type::new(TypeInner::AnyT(reason_op.dupe(), AnySource::Untyped));
                        subst_map.insert(tparam.name.dupe(), any_untyped);
                    }

                    let substituted = subst(
                        cx,
                        Some(use_op),
                        true,
                        false,
                        Purpose::Normal,
                        &subst_map,
                        t_out.dupe(),
                    );

                    let reason_component = match t_out.deref() {
                        TypeInner::DefT(r, _) => r,
                        _ => unreachable!(),
                    };
                    Ok(run_on_abstract_component(
                        cx,
                        env,
                        reason_component,
                        reason_op,
                        substituted,
                    ))
                }

                DefTInner::PolyT(box PolyTData {
                    kind: poly_kind,
                    tparams_loc,
                    tparams: ids,
                    t_out,
                    id,
                    strictness_kind,
                }) if kind == TypeTKind::RenderTypeKind
                    && let TypeInner::DefT(_, inner) = t_out.deref()
                    && let DefTInner::TypeT(TypeTKind::ImportTypeofKind, inner_t) =
                        inner.deref()
                    && let TypeInner::AnnotT(_, t_out_mono, _) = inner_t.deref() =>
                {
                    // `renders` over `import typeof` of a generic component must
                    // behave like `renders` over the component itself. Unwrap the
                    // alias to a polymorphic component and recurse so the direct
                    // renders arm above applies.
                    let poly = type_util::poly_type_of_tparam_list(
                        id.dupe(),
                        tparams_loc.dupe(),
                        ids.dupe(),
                        t_out_mono.dupe(),
                        *strictness_kind,
                        *poly_kind,
                    );
                    run_on_concrete_type_with_env(cx, env, use_op, reason_op, kind, poly)
                }

                DefTInner::PolyT(box PolyTData {
                    kind: poly_kind,
                    tparams_loc,
                    tparams: ids,
                    t_out,
                    id,
                    strictness_kind,
                }) if let TypeInner::DefT(_, inner) = t_out.deref()
                    && let DefTInner::TypeT(TypeTKind::ImportTypeofKind, inner_t) =
                        inner.deref()
                    && let TypeInner::AnnotT(_, t_out_mono, _) = inner_t.deref() =>
                {
                    // `import typeof` of a generic function, class, or component is a
                    // parameterized alias over the typeof of the body. Using it bare
                    // must behave like `typeof v` (keep the polymorphism) rather than
                    // reporting missing-type-arg, so rebuild the polymorphic typeof
                    // from the alias pieces.
                    let poly = type_util::poly_type_of_tparam_list(
                        id.dupe(),
                        tparams_loc.dupe(),
                        ids.dupe(),
                        t_out_mono.dupe(),
                        *strictness_kind,
                        *poly_kind,
                    );
                    Ok(type_util::typeof_annotation(reason_op.dupe(), poly, None))
                }

                DefTInner::PolyT(box PolyTData {
                    tparams_loc,
                    tparams: ids,
                    ..
                }) => {
                    let tparams_loc = tparams_loc.dupe();

                    let min_arity = ids.iter().filter(|tp| tp.default.is_none()).count() as i32;
                    add_output_with_env(
                        cx,
                        env,
                        ErrorMessage::EMissingTypeArgs(Box::new(EMissingTypeArgsData {
                            loc: reason_op.loc().dupe(),
                            type_: type_reference_with_reason_for_error(&t, reason.dupe()),
                            arity_loc: tparams_loc,
                            min_arity,
                            max_arity: ids.len() as i32,
                        })),
                    )?;

                    Ok(any_t::error(reason.dupe()))
                }

                DefTInner::ClassT(it) => {
                    // a class value annotation becomes the instance type
                    match it.deref() {
                        TypeInner::ThisInstanceT(box ThisInstanceTData {
                            reason: inst_r,
                            instance: i,
                            is_this,
                            subst_name: this_name,
                        }) => Ok(fix_this_instance(
                            cx,
                            reason_op.dupe(),
                            inst_r.dupe(),
                            i,
                            *is_this,
                            this_name.dupe(),
                        )),
                        _ => Ok(it.dupe()),
                    }
                }

                DefTInner::TypeT(_, inner_t) => Ok(inner_t.dupe()),
                // A `unique symbol` value used in a type position resolves to its
                // own `unique symbol` type, so a value-level symbol binding can
                // key a type-body member (`interface I { [s]: T }`). Distinct
                // symbols stay distinct.
                DefTInner::UniqueSymbolT(_) => Ok(t.dupe()),
                // an enum object value annotation becomes the enum type
                DefTInner::EnumObjectT { enum_value_t, .. } => Ok(enum_value_t.dupe()),
                DefTInner::EnumValueT(enum_info) => {
                    add_output_with_env(
                        cx,
                        env,
                        ErrorMessage::EEnumError(EnumErrorKind::EnumMemberUsedAsType(Box::new(
                            EnumMemberUsedAsTypeData {
                                loc: reason_op.loc().dupe(),
                                description_name: description_name_for_error(reason_op),
                                enum_: EnumReferenceData {
                                    loc: type_util::ref_loc_of_t(&t).dupe(),
                                    name: enum_info.enum_name().map(Dupe::dupe),
                                },
                            },
                        ))),
                    )?;
                    Ok(any_t::error(reason_op.dupe()))
                }

                DefTInner::ReactAbstractComponentT(_) => Ok(run_on_abstract_component(
                    cx,
                    env,
                    reason,
                    reason_op,
                    t.dupe(),
                )),

                DefTInner::EmptyT => {
                    add_output_with_env(
                        cx,
                        env,
                        ErrorMessage::EValueUsedAsType {
                            reference: value_as_type_reference_for_error(reason_op),
                            value: type_reference_at_loc_for_error(&t, reason_op.loc().dupe()),
                        },
                    )?;
                    Ok(any_t::error(reason.dupe()))
                }

                _ => {
                    add_output_with_env(
                        cx,
                        env,
                        ErrorMessage::EValueUsedAsType {
                            reference: value_as_type_reference_for_error(reason_op),
                            value: type_reference_at_loc_for_error(&t, reason_op.loc().dupe()),
                        },
                    )?;
                    Ok(any_t::error(reason_of_t(&t).dupe()))
                }
            },

            // A TS enum used as a bare type is the union of its member literal types. Any
            // other namespace value used as a type is an error and falls through to the
            // catch-all below.
            TypeInner::NamespaceT(ns) if super::is_ts_enum_symbol(&ns.namespace_symbol) => Ok(
                super::ts_enum_member_union(cx, reason_op.dupe(), ns.types_tmap.dupe()),
            ),

            TypeInner::AnyT(r, AnySource::AnyError(Some(AnyErrorKind::MissingAnnotation))) => {
                add_output_with_env(
                    cx,
                    env,
                    ErrorMessage::EValueUsedAsType {
                        reference: value_as_type_reference_for_error(reason_op),
                        value: type_reference_at_loc_for_error(&t, reason_op.loc().dupe()),
                    },
                )?;
                Ok(any_t::error(r.dupe()))
            }

            // Short-circut as we already error on the unresolved name.
            TypeInner::AnyT(_, AnySource::AnyError(_)) => Ok(t),
            TypeInner::AnyT(r, _) => {
                add_output_with_env(
                    cx,
                    env,
                    ErrorMessage::EAnyValueUsedAsType {
                        reference: value_as_type_reference_for_error(reason_op),
                        value: type_reference_at_loc_for_error(&t, reason_op.loc().dupe()),
                    },
                )?;
                Ok(any_t::error(r.dupe()))
            }

            _ => {
                add_output_with_env(
                    cx,
                    env,
                    ErrorMessage::EValueUsedAsType {
                        reference: value_as_type_reference_for_error(reason_op),
                        value: type_reference_at_loc_for_error(&t, reason_op.loc().dupe()),
                    },
                )?;
                Ok(any_t::error(reason_of_t(&t).dupe()))
            }
        }
    }
}

// Imports

pub fn check_nonstrict_import<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    is_strict: bool,
    imported_is_strict: bool,
    reason: &Reason,
) -> Result<(), FlowJsException> {
    if is_strict && !imported_is_strict {
        let loc = reason.loc().dupe();
        add_output_with_env(cx, env, ErrorMessage::ENonstrictImport(loc))?;
    }
    Ok(())
}

// **************************************************************************
// * Module imports                                                         *
// *                                                                        *
// * The process of importing from a module consists of reading from the    *
// * foreign ModuleT type and generating a user-visible construct from it.  *
// *                                                                        *
// * For CommonJS imports (AKA 'require()'), if the foreign module is an ES *
// * module we generate an object whose properties correspond to each of    *
// * the named exports of the foreign module. If the foreign module is also *
// * a CommonJS module, use the type of the foreign CommonJS exports value  *
// * directly.                                                              *
// *                                                                        *
// * For ES imports (AKA `import` statements), simply generate a model of   *
// * an ES ModuleNamespace object from the individual named exports of the  *
// * foreign module. This object can then be passed up to "userland"        *
// * directly (via `import * as`) or it can be used to extract individual   *
// * exports from the foreign module (via `import {}` and `import X from`). *
// **************************************************************************

pub trait ImportExportHelperSig {
    type R;

    fn reposition<'cx>(cx: &mut Context<'cx>, loc: ALoc, t: Type) -> Type;

    fn export_named<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        info: (
            Reason,
            FlowOrdMap<Name, NamedSymbol>,
            FlowOrdMap<Name, NamedSymbol>,
            ExportKind,
        ),
        module_: Type,
    ) -> Self::R;

    fn return_<'cx>(cx: &mut Context<'cx>, t: Type) -> Self::R;
}

// *********************************************************************
// * `import type` creates a properly-parameterized type alias for the *
// * remote type -- but only for particular, valid remote types.       *
// *********************************************************************

/*
  TODO: This rule allows interpreting an object as a type!

  It is currently used to work with modules that export named types,
  e.g. 'react' or 'immutable'. For example, one can do

  `import type React from 'react'`

  followed by uses of `React` as a container of types in (say) type
  definitions like

  `type C = React.Component<any,any,any>`

  Fortunately, in that case `React` is stored as a type binding in the
  environment, so it cannot be used as a value.

  However, removing this special case causes no loss of expressibility
  (while making the model simpler). For example, in the above example we
  can write

  `import type { Component } from 'react'`

  followed by (say)

  `type C = Component<any,any,any>`

  Overall, we should be able to (at least conceptually) desugar `import
  type` to `import` followed by `type`.
*/
pub mod import_type_t_kit {
    use std::ops::Deref;

    use dupe::Dupe;
    use flow_common::reason::Reason;
    use flow_data_structure_wrapper::smol_str::FlowSmolStr;
    use flow_typing_context::Context;
    use flow_typing_errors::error_message::ErrorMessage;
    use flow_typing_type::type_::AnySource;
    use flow_typing_type::type_::DefT;
    use flow_typing_type::type_::DefTInner;
    use flow_typing_type::type_::InstanceKind;
    use flow_typing_type::type_::PolyTData;
    use flow_typing_type::type_::ThisInstanceTData;
    use flow_typing_type::type_::Type;
    use flow_typing_type::type_::TypeInner;
    use flow_typing_type::type_::TypeTKind;
    use flow_typing_type::type_util;

    use super::add_output_non_speculating;
    use super::fix_this_instance;
    use super::mk_tparams;

    pub fn canonicalize_imported_type<'cx>(
        cx: &mut Context<'cx>,
        reason: Reason,
        t: &Type,
    ) -> Option<Type> {
        match t.deref() {
            TypeInner::DefT(_, def_t) => match def_t.deref() {
                // fix this-abstracted class when used as a type
                DefTInner::ClassT(inst) => match inst.deref() {
                    TypeInner::ThisInstanceT(box ThisInstanceTData {
                        reason: r,
                        instance: i,
                        is_this,
                        subst_name: this_name,
                    }) => {
                        let fixed = fix_this_instance(
                            cx,
                            reason.dupe(),
                            r.dupe(),
                            i,
                            *is_this,
                            this_name.dupe(),
                        );
                        let inner = match &i.inst.inst_kind {
                            InstanceKind::InterfaceKind { .. } => {
                                DefTInner::TypeT(TypeTKind::ImportClassKind, fixed)
                            }
                            InstanceKind::ClassKind | InstanceKind::RecordKind { .. } => {
                                DefTInner::ClassT(fixed)
                            }
                        };
                        Some(Type::new(TypeInner::DefT(reason, DefT::new(inner))))
                    }
                    _ => Some(Type::new(TypeInner::DefT(
                        reason,
                        DefT::new(DefTInner::TypeT(TypeTKind::ImportClassKind, inst.dupe())),
                    ))),
                },

                // delay fixing a polymorphic this-abstracted class until it is specialized,
                // by transforming the instance type to a type application
                DefTInner::PolyT(box PolyTData {
                    kind: poly_kind,
                    tparams_loc,
                    tparams: typeparams,
                    t_out,
                    id,
                    strictness_kind,
                }) => match t_out.deref() {
                    TypeInner::DefT(_, inner_def) => match inner_def.deref() {
                        DefTInner::ClassT(inst) => match inst.deref() {
                            TypeInner::ThisInstanceT(box ThisInstanceTData {
                                instance: i, ..
                            }) => {
                                let (_, targs) = mk_tparams(cx, typeparams);
                                let tapp = type_util::implicit_typeapp(t.dupe(), targs, None);
                                let new_id = flow_typing_type::type_::poly::Id::generate_id();
                                let inner = match &i.inst.inst_kind {
                                    InstanceKind::InterfaceKind { .. } => {
                                        Type::new(TypeInner::DefT(
                                            reason,
                                            DefT::new(DefTInner::TypeT(
                                                TypeTKind::ImportClassKind,
                                                tapp,
                                            )),
                                        ))
                                    }
                                    InstanceKind::ClassKind | InstanceKind::RecordKind { .. } => {
                                        type_util::class_type(tapp, None)
                                    }
                                };
                                Some(type_util::poly_type_of_tparam_list(
                                    new_id,
                                    tparams_loc.dupe(),
                                    typeparams.dupe(),
                                    inner,
                                    *strictness_kind,
                                    *poly_kind,
                                ))
                            }
                            _ => {
                                let type_t = Type::new(TypeInner::DefT(
                                    reason,
                                    DefT::new(DefTInner::TypeT(
                                        TypeTKind::ImportClassKind,
                                        inst.dupe(),
                                    )),
                                ));
                                Some(type_util::poly_type_of_tparam_list(
                                    id.dupe(),
                                    tparams_loc.dupe(),
                                    typeparams.dupe(),
                                    type_t,
                                    *strictness_kind,
                                    *poly_kind,
                                ))
                            }
                        },
                        DefTInner::TypeT(_, _) => Some(t.dupe()),
                        DefTInner::ReactAbstractComponentT(_) => Some(t.dupe()),
                        _ => None,
                    },
                    _ => None,
                },

                DefTInner::EnumObjectT { enum_value_t, .. } => Some(Type::new(TypeInner::DefT(
                    reason,
                    DefT::new(DefTInner::TypeT(
                        TypeTKind::ImportEnumKind,
                        enum_value_t.dupe(),
                    )),
                ))),
                DefTInner::ReactAbstractComponentT(_) => Some(t.dupe()),
                DefTInner::TypeT(_, _) => Some(t.dupe()),
                _ => None,
            },

            TypeInner::NamespaceT(_) => Some(t.dupe()),
            TypeInner::AnyT(_, _) => Some(t.dupe()),
            _ => None,
        }
    }

    pub fn on_concrete_type<'cx>(
        cx: &mut Context<'cx>,
        reason: Reason,
        export_name: &str,
        exported_type: Type,
    ) -> Type {
        if export_name == "default"
            && let TypeInner::DefT(_, def_t) = exported_type.deref()
            && matches!(def_t.deref(), DefTInner::ObjT(_))
        {
            return exported_type;
        }

        match canonicalize_imported_type(cx, reason.dupe(), &exported_type) {
            Some(imported_t) => imported_t,
            None => {
                add_output_non_speculating(
                    cx,
                    ErrorMessage::EImportValueAsType(Box::new((
                        reason.loc().dupe(),
                        FlowSmolStr::new(export_name),
                    ))),
                );
                Type::new(TypeInner::AnyT(reason, AnySource::AnyError(None)))
            }
        }
    }
}

// ************************************************************************
// * `import typeof` creates a properly-parameterized type alias for the  *
// * "typeof" the remote export.                                          *
// ************************************************************************

pub mod import_typeof_t_kit {
    use std::ops::Deref;

    use dupe::Dupe;
    use flow_common::reason::Reason;
    use flow_data_structure_wrapper::smol_str::FlowSmolStr;
    use flow_typing_context::Context;
    use flow_typing_errors::error_message::ErrorMessage;
    use flow_typing_type::type_::AnySource;
    use flow_typing_type::type_::DefT;
    use flow_typing_type::type_::DefTInner;
    use flow_typing_type::type_::PolyTData;
    use flow_typing_type::type_::Type;
    use flow_typing_type::type_::TypeInner;
    use flow_typing_type::type_::TypeTKind;
    use flow_typing_type::type_util;

    use super::add_output_non_speculating;

    pub fn on_concrete_type<'cx>(
        cx: &mut Context<'cx>,
        reason: Reason,
        export_name: &str,
        l: &Type,
    ) -> Type {
        match l.deref() {
            TypeInner::DefT(_, def_t) => match def_t.deref() {
                DefTInner::PolyT(box PolyTData {
                    tparams_loc,
                    kind: poly_kind,
                    tparams: typeparams,
                    t_out,
                    strictness_kind,
                    ..
                }) => match t_out.deref() {
                    TypeInner::DefT(_, inner_def) => match inner_def.deref() {
                        DefTInner::ClassT(inst)
                            if matches!(inst.deref(), TypeInner::ThisInstanceT(..)) =>
                        {
                            let typeof_t =
                                type_util::typeof_annotation(reason.dupe(), l.dupe(), None);
                            Type::new(TypeInner::DefT(
                                reason,
                                DefT::new(DefTInner::TypeT(TypeTKind::ImportTypeofKind, typeof_t)),
                            ))
                        }
                        DefTInner::ClassT(_)
                        | DefTInner::FunT(_, _)
                        | DefTInner::ReactAbstractComponentT(_) => {
                            let typeof_t =
                                type_util::typeof_annotation(reason.dupe(), t_out.dupe(), None);
                            let new_id = flow_typing_type::type_::poly::Id::generate_id();
                            type_util::poly_type_of_tparam_list(
                                new_id,
                                tparams_loc.dupe(),
                                typeparams.dupe(),
                                Type::new(TypeInner::DefT(
                                    reason,
                                    DefT::new(DefTInner::TypeT(
                                        TypeTKind::ImportTypeofKind,
                                        typeof_t,
                                    )),
                                )),
                                *strictness_kind,
                                *poly_kind,
                            )
                        }
                        DefTInner::TypeT(_, _) => {
                            add_output_non_speculating(
                                cx,
                                ErrorMessage::EImportTypeAsTypeof(Box::new((
                                    reason.loc().dupe(),
                                    FlowSmolStr::new(export_name),
                                ))),
                            );
                            Type::new(TypeInner::AnyT(reason, AnySource::AnyError(None)))
                        }
                        _ => {
                            let typeof_t =
                                type_util::typeof_annotation(reason.dupe(), l.dupe(), None);
                            Type::new(TypeInner::DefT(
                                reason,
                                DefT::new(DefTInner::TypeT(TypeTKind::ImportTypeofKind, typeof_t)),
                            ))
                        }
                    },
                    _ => {
                        let typeof_t = type_util::typeof_annotation(reason.dupe(), l.dupe(), None);
                        Type::new(TypeInner::DefT(
                            reason,
                            DefT::new(DefTInner::TypeT(TypeTKind::ImportTypeofKind, typeof_t)),
                        ))
                    }
                },

                DefTInner::TypeT(_, _) => {
                    add_output_non_speculating(
                        cx,
                        ErrorMessage::EImportTypeAsTypeof(Box::new((
                            reason.loc().dupe(),
                            FlowSmolStr::new(export_name),
                        ))),
                    );
                    Type::new(TypeInner::AnyT(reason, AnySource::AnyError(None)))
                }

                _ => {
                    let typeof_t = type_util::typeof_annotation(reason.dupe(), l.dupe(), None);
                    Type::new(TypeInner::DefT(
                        reason,
                        DefT::new(DefTInner::TypeT(TypeTKind::ImportTypeofKind, typeof_t)),
                    ))
                }
            },

            _ => {
                let typeof_t = type_util::typeof_annotation(reason.dupe(), l.dupe(), None);
                Type::new(TypeInner::DefT(
                    reason,
                    DefT::new(DefTInner::TypeT(TypeTKind::ImportTypeofKind, typeof_t)),
                ))
            }
        }
    }
}

pub mod cjs_require_t_kit {
    use std::rc::Rc;

    use dupe::Dupe;
    use flow_aloc::ALoc;
    use flow_common::flow_symbol::Symbol;
    use flow_common::polarity::Polarity;
    use flow_common::reason::Name;
    use flow_common::reason::Reason;
    use flow_typing_context::Context;
    use flow_typing_flow_js_env::FlowJsEnv;
    use flow_typing_type::type_::FieldData;
    use flow_typing_type::type_::ModuleType;
    use flow_typing_type::type_::ModuleTypeInner;
    use flow_typing_type::type_::NamedSymbol;
    use flow_typing_type::type_::NamespaceType;
    use flow_typing_type::type_::ObjKind;
    use flow_typing_type::type_::Property;
    use flow_typing_type::type_::PropertyInner;
    use flow_typing_type::type_::Type;
    use flow_typing_type::type_::TypeInner;
    use flow_typing_type::type_::properties::PropertiesMap;
    use flow_typing_type::type_util;
    use flow_utils_concurrency::job_error::JobError;

    use super::FlowJsException;
    use super::check_nonstrict_import;
    use super::lookup_builtin_typeapp_with_env;

    // require('SomeModule')
    pub fn on_module_t<'cx, R>(
        cx: &mut Context<'cx>,
        reposition: R,
        reason: Reason,
        module_symbol: Symbol,
        is_strict: bool,
        standard_cjs_esm_interop: bool,
        module_: &ModuleType,
    ) -> Result<(Type, ALoc), FlowJsException>
    where
        R: Fn(&mut Context<'cx>, ALoc, Type) -> Result<Type, JobError>,
    {
        let env = &FlowJsEnv::entry();
        let ModuleTypeInner {
            module_reason,
            module_export_types: exports,
            module_is_strict: imported_is_strict,
            module_available_platforms: _ignored_todo,
        } = &**module_;
        check_nonstrict_import(cx, env, is_strict, *imported_is_strict, &reason)?;
        Ok(match &exports.cjs_export {
            Some((def_loc_opt, t)) => {
                // reposition the export to point at the require(), like the object
                // we create below for non-CommonJS exports
                let def_loc = match def_loc_opt {
                    None => type_util::def_loc_of_t(t).dupe(),
                    Some(l) => l.dupe(),
                };
                (reposition(cx, reason.loc().dupe(), t.dupe())?, def_loc)
            }
            None => {
                let value_exports_tmap = cx.find_exports(exports.value_exports_tmap);
                let type_exports_tmap = cx.find_exports(exports.type_exports_tmap);

                // Convert ES module's named exports to an object
                let mk_exports_namespace = |cx: &mut Context<'cx>| {
                    let proto = Type::new(TypeInner::ObjProtoT(reason.dupe()));

                    let named_symbol_to_field = |ns: &NamedSymbol| -> PropertyInner {
                        PropertyInner::Field(Box::new(FieldData {
                            preferred_def_locs: ns.preferred_def_locs.clone(),
                            key_loc: ns.name_loc.dupe(),
                            type_: ns.type_.dupe(),
                            polarity: Polarity::Positive,
                        }))
                    };

                    let value_props: PropertiesMap = value_exports_tmap
                        .iter()
                        .map(|(k, v)| (k.dupe(), Property::new(named_symbol_to_field(v))))
                        .collect();
                    let type_props: PropertiesMap = type_exports_tmap
                        .iter()
                        .map(|(k, v)| (k.dupe(), Property::new(named_symbol_to_field(v))))
                        .collect();
                    let values_type = crate::obj_type::mk_with_proto(
                        cx,
                        reason.dupe(),
                        ObjKind::Exact,
                        None,
                        None,
                        Some(value_props),
                        None,
                        proto,
                        false,
                    );
                    let types_tmap = cx.generate_property_map(type_props);
                    Type::new(TypeInner::NamespaceT(Rc::new(NamespaceType {
                        namespace_symbol: module_symbol.dupe(),
                        values_type,
                        types_tmap,
                    })))
                };

                let t = if standard_cjs_esm_interop {
                    let exports_namespace = mk_exports_namespace(cx);
                    lookup_builtin_typeapp_with_env(
                        cx,
                        env,
                        reason.dupe(),
                        "$Flow$EsmModuleMarkerWrapperInModuleRef",
                        vec![exports_namespace],
                    )
                } else {
                    // Use default export if option is enabled and module is not lib
                    let automatic_require_default =
                        cx.automatic_require_default() && !cx.is_lib_reason_def(module_reason);
                    if automatic_require_default {
                        match value_exports_tmap.get(&Name::new("default")) {
                            Some(ns) => ns.type_.dupe(),
                            _ => mk_exports_namespace(cx),
                        }
                    } else {
                        mk_exports_namespace(cx)
                    }
                };

                let def_loc_of_export = |ns: &NamedSymbol| -> ALoc {
                    match &ns.preferred_def_locs {
                        Some(l) => l.first().dupe(),
                        None => match &ns.name_loc {
                            Some(l) => l.dupe(),
                            None => type_util::def_loc_of_t(&ns.type_).dupe(),
                        },
                    }
                };
                let def_loc = match value_exports_tmap.get(&Name::new("default")) {
                    Some(e) => def_loc_of_export(e),
                    None => {
                        match value_exports_tmap
                            .iter()
                            .fold(None, |acc, (_, e)| match acc {
                                None => Some(def_loc_of_export(e)),
                                Some(acc) => {
                                    let def_loc = def_loc_of_export(e);
                                    if acc.cmp(&def_loc) == std::cmp::Ordering::Less {
                                        Some(acc)
                                    } else {
                                        Some(def_loc)
                                    }
                                }
                            }) {
                            Some(l) => l,
                            None => module_reason.def_loc().dupe(),
                        }
                    }
                };
                (t, def_loc)
            }
        })
    }
}

// import * as X from 'SomeModule';
pub mod import_module_ns_t_kit {
    use dupe::Dupe;
    use flow_common::polarity::Polarity;
    use flow_common::reason::Name;
    use flow_common::reason::Reason;
    use flow_typing_context::Context;
    use flow_typing_flow_js_env::FlowJsEnv;
    use flow_typing_type::type_::AnySource;
    use flow_typing_type::type_::DictType;
    use flow_typing_type::type_::FieldData;
    use flow_typing_type::type_::ModuleType;
    use flow_typing_type::type_::ModuleTypeInner;
    use flow_typing_type::type_::NamedSymbol;
    use flow_typing_type::type_::ObjKind;
    use flow_typing_type::type_::Property;
    use flow_typing_type::type_::PropertyInner;
    use flow_typing_type::type_::Type;
    use flow_typing_type::type_::TypeInner;
    use flow_typing_type::type_::properties;
    use flow_typing_type::type_::str_module_t;
    use flow_typing_type::type_util;

    use super::FlowJsException;
    use super::check_nonstrict_import;
    use crate::obj_type;

    pub fn on_module_t<'cx>(
        cx: &mut Context<'cx>,
        is_common_interface_module: bool,
        reason_op: Reason,
        is_strict: bool,
        module_: &ModuleType,
    ) -> Result<(Type, properties::Id), FlowJsException> {
        let env = &FlowJsEnv::entry();
        let ModuleTypeInner {
            module_reason,
            module_export_types: exports,
            module_is_strict: imported_is_strict,
            module_available_platforms: _ignored_todo,
        } = &**module_;

        if !is_common_interface_module {
            check_nonstrict_import(cx, env, is_strict, *imported_is_strict, &reason_op)?;
        }

        let reason = module_reason
            .dupe()
            .reposition(reason_op.loc().dupe())
            .replace_desc(reason_op.desc(true).cloned());

        let value_exports_tmap = cx.find_exports(exports.value_exports_tmap);
        let type_exports_tmap = cx.find_exports(exports.type_exports_tmap);

        // Prefer [type_for_extends] (the raw, un-canonicalized exporter-side
        // type) when present, so polymorphic [this] survives the trip through
        // a namespace synthesized for [import * as NS from ...]. Downstream
        // [extends NS.X] / [implements NS.X] consumers go through
        // [Annot_GetTypeFromNamespaceT] which reads [types_tmap] first and
        // falls back to [GetPropT] on the values object — both paths need the
        // raw form to detect the (optionally PolyT-wrapped)
        // [ClassT(ThisInstanceT(..))] shape and rebind [this]. When
        // [type_for_extends] is [None] (the common case)
        // we keep [type_] unchanged, matching the prior behavior.
        let named_symbol_to_field = |ns: &NamedSymbol| -> Property {
            let type_ = ns
                .type_for_extends
                .clone()
                .unwrap_or_else(|| ns.type_.dupe());
            Property::new(PropertyInner::Field(Box::new(FieldData {
                preferred_def_locs: ns.preferred_def_locs.clone(),
                key_loc: ns.name_loc.dupe(),
                type_,
                polarity: Polarity::Positive,
            })))
        };
        let default_entry = if !cx.facebook_module_interop() {
            exports.cjs_export.as_ref().map(|(def_loc_opt, type_)| {
                let key_loc = Some(match def_loc_opt {
                    None => type_util::def_loc_of_t(type_).dupe(),
                    Some(l) => l.dupe(),
                });
                let p = Property::new(PropertyInner::Field(Box::new(FieldData {
                    preferred_def_locs: None,
                    key_loc,
                    type_: type_.dupe(),
                    polarity: Polarity::Positive,
                })));
                (Name::new("default"), p)
            })
        } else {
            None
        };
        let value_props: properties::PropertiesMap = value_exports_tmap
            .iter()
            .map(|(name, ns)| (name.dupe(), named_symbol_to_field(ns)))
            .chain(default_entry)
            .collect();
        let type_props: properties::PropertiesMap = type_exports_tmap
            .iter()
            .map(|(name, ns)| (name.dupe(), named_symbol_to_field(ns)))
            .collect();
        let obj_kind = if exports.has_every_named_export {
            ObjKind::Indexed(DictType {
                key: str_module_t::why(reason.dupe()),
                value: Type::new(TypeInner::AnyT(reason.dupe(), AnySource::Untyped)),
                dict_name: None,
                dict_polarity: Polarity::Neutral,
            })
        } else if is_common_interface_module {
            ObjKind::Inexact
        } else {
            ObjKind::Exact
        };
        let proto = Type::new(TypeInner::ObjProtoT(reason.dupe()));
        let values_type = obj_type::mk_with_proto(
            cx,
            reason.dupe(),
            obj_kind,
            None,
            None,
            Some(value_props),
            None,
            proto,
            false,
        );
        let types_tmap = cx.generate_property_map(type_props);
        Ok((values_type, types_tmap))
    }
}

pub mod import_default_t_kit {
    use std::rc::Rc;

    use dupe::Dupe;
    use flow_aloc::ALoc;
    use flow_common::flow_import_specifier::Userland;
    use flow_common::reason::Name;
    use flow_common::reason::Reason;
    use flow_common_utils::utils_js::typo_suggestion;
    use flow_data_structure_wrapper::smol_str::FlowSmolStr;
    use flow_typing_context::Context;
    use flow_typing_context::Cx;
    use flow_typing_errors::error_message::ErrorMessage;
    use flow_typing_flow_js_env::FlowJsEnv;
    use flow_typing_type::type_::AnySource;
    use flow_typing_type::type_::ImportKind;
    use flow_typing_type::type_::ModuleType;
    use flow_typing_type::type_::ModuleTypeInner;
    use flow_typing_type::type_::Type;
    use flow_typing_type::type_::TypeInner;
    use flow_typing_type::type_::aconstraint::AnnotConcretizeForImportsExportsFn;
    use flow_typing_type::type_util;

    use super::FlowJsException;
    use super::add_output_with_env;
    use super::check_nonstrict_import;
    use super::import_type_t_kit;
    use super::import_typeof_t_kit;

    pub fn on_module_t<'cx>(
        cx: &mut Context<'cx>,
        with_concretized_type: &dyn Fn(
            &mut Context<'cx>,
            Reason,
            AnnotConcretizeForImportsExportsFn<Cx>,
            Type,
        ) -> Type,
        reason: Reason,
        import_kind: ImportKind,
        local_name: &str,
        module_name: Userland,
        is_strict: bool,
        module_: &ModuleType,
    ) -> Result<(Option<ALoc>, Type), FlowJsException> {
        on_module_t_with_env(
            cx,
            &FlowJsEnv::entry(),
            with_concretized_type,
            reason,
            import_kind,
            local_name,
            module_name,
            is_strict,
            module_,
        )
    }

    // import [type] X from 'SomeModule';
    pub fn on_module_t_with_env<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        with_concretized_type: &dyn Fn(
            &mut Context<'cx>,
            Reason,
            AnnotConcretizeForImportsExportsFn<Cx>,
            Type,
        ) -> Type,
        reason: Reason,
        import_kind: ImportKind,
        local_name: &str,
        module_name: Userland,
        is_strict: bool,
        module_: &ModuleType,
    ) -> Result<(Option<ALoc>, Type), FlowJsException> {
        let ModuleTypeInner {
            module_reason,
            module_export_types: exports,
            module_is_strict: imported_is_strict,
            module_available_platforms: _ignored_todo,
        } = &**module_;
        check_nonstrict_import(cx, env, is_strict, *imported_is_strict, &reason)?;

        let (loc_opt, export_t) = match &exports.cjs_export {
            Some((def_loc_opt, t)) => {
                let def_loc = Some(match def_loc_opt {
                    None => type_util::def_loc_of_t(t).dupe(),
                    Some(l) => l.dupe(),
                });
                (def_loc, t.dupe())
            }
            None => {
                let exports_tmap = cx.find_exports(exports.value_exports_tmap);
                match exports_tmap.get(&Name::new("default")) {
                    Some(ns) => (ns.name_loc.dupe(), ns.type_.dupe()),
                    None => {
                        // A common error while using `import` syntax is to forget or
                        // misunderstand the difference between `import foo from ...`
                        // and `import {foo} from ...`. The former means to import the
                        // default export to a local var called "foo", and the latter
                        // means to import a named export called "foo" to a local var
                        // called "foo".
                        //
                        // To help guide users here, if we notice that the module being
                        // imported from has no default export (but it does have a named
                        // export that fuzzy-matches the local name specified), we offer
                        // that up as a possible "did you mean?" suggestion.
                        //
                        // TODO consider filtering these to OrdinaryNames only
                        let known_exports: Vec<&FlowSmolStr> = exports_tmap
                            .keys()
                            .filter_map(|n| n.as_smol_str_opt())
                            .collect();
                        let suggestion = typo_suggestion(&known_exports, local_name);
                        add_output_with_env(
                            cx,
                            env,
                            ErrorMessage::ENoDefaultExport(Box::new((
                                reason.loc().dupe(),
                                module_name.dupe(),
                                suggestion,
                            ))),
                        )?;
                        (
                            None,
                            Type::new(TypeInner::AnyT(
                                module_reason.dupe(),
                                AnySource::AnyError(None),
                            )),
                        )
                    }
                }
            }
        };

        match import_kind {
            ImportKind::ImportType => {
                let cx_id = cx.id();
                let t = with_concretized_type(
                    cx,
                    reason.dupe(),
                    Rc::new(move |cx: &mut Context<'_>, t: Type| {
                        import_type_t_kit::on_concrete_type(
                            &mut cx.switch_to(cx_id),
                            reason.dupe(),
                            "default",
                            t,
                        )
                    }),
                    export_t,
                );
                Ok((loc_opt, t))
            }
            ImportKind::ImportTypeof => {
                let cx_id = cx.id();
                let t = with_concretized_type(
                    cx,
                    reason.dupe(),
                    Rc::new(move |cx: &mut Context<'_>, t: Type| {
                        import_typeof_t_kit::on_concrete_type(
                            &mut cx.switch_to(cx_id),
                            reason.dupe(),
                            "default",
                            &t,
                        )
                    }),
                    export_t,
                );
                Ok((loc_opt, t))
            }
            ImportKind::ImportValue => Ok((loc_opt, export_t)),
        }
    }

    pub fn on_module_t_for_extends<'cx>(
        cx: &mut Context<'cx>,
        reason: Reason,
        _import_kind: ImportKind,
        local_name: &str,
        module_name: Userland,
        is_strict: bool,
        module_: &ModuleType,
    ) -> Result<(Option<ALoc>, Type), FlowJsException> {
        let env = &FlowJsEnv::entry();
        let ModuleTypeInner {
            module_reason,
            module_export_types: exports,
            module_is_strict: imported_is_strict,
            module_available_platforms: _ignored_todo,
        } = &**module_;
        check_nonstrict_import(cx, env, is_strict, *imported_is_strict, &reason)?;

        match &exports.cjs_export {
            Some((def_loc_opt, t)) => {
                let def_loc = Some(match def_loc_opt {
                    None => type_util::def_loc_of_t(t).dupe(),
                    Some(l) => l.dupe(),
                });
                Ok((def_loc, t.dupe()))
            }
            None => {
                let exports_tmap = cx.find_exports(exports.value_exports_tmap);
                match exports_tmap.get(&Name::new("default")) {
                    Some(ns) => {
                        let t = ns
                            .type_for_extends
                            .as_ref()
                            .map(|t| t.dupe())
                            .unwrap_or_else(|| ns.type_.dupe());
                        Ok((ns.name_loc.dupe(), t))
                    }
                    None => {
                        let known_exports: Vec<&FlowSmolStr> = exports_tmap
                            .keys()
                            .filter_map(|n| n.as_smol_str_opt())
                            .collect();
                        let suggestion = typo_suggestion(&known_exports, local_name);
                        add_output_with_env(
                            cx,
                            env,
                            ErrorMessage::ENoDefaultExport(Box::new((
                                reason.loc().dupe(),
                                module_name,
                                suggestion,
                            ))),
                        )?;
                        Ok((
                            None,
                            Type::new(TypeInner::AnyT(
                                module_reason.dupe(),
                                AnySource::AnyError(None),
                            )),
                        ))
                    }
                }
            }
        }
    }
}

pub mod import_named_t_kit {
    use std::rc::Rc;

    use dupe::Dupe;
    use flow_aloc::ALoc;
    use flow_common::flow_import_specifier::Userland;
    use flow_common::reason::Name;
    use flow_common::reason::Reason;
    use flow_common_utils::utils_js::typo_suggestion;
    use flow_data_structure_wrapper::smol_str::FlowSmolStr;
    use flow_typing_context::Context;
    use flow_typing_context::Cx;
    use flow_typing_errors::error_message::ErrorMessage;
    use flow_typing_flow_js_env::FlowJsEnv;
    use flow_typing_type::type_::AnySource;
    use flow_typing_type::type_::ImportKind;
    use flow_typing_type::type_::ModuleType;
    use flow_typing_type::type_::ModuleTypeInner;
    use flow_typing_type::type_::NamedSymbol;
    use flow_typing_type::type_::Type;
    use flow_typing_type::type_::TypeInner;
    use flow_typing_type::type_::aconstraint::AnnotConcretizeForImportsExportsFn;
    use flow_typing_type::type_::exports;
    use flow_typing_type::type_util;

    use super::FlowJsException;
    use super::add_output_with_env;
    use super::check_nonstrict_import;
    use super::import_type_t_kit;
    use super::import_typeof_t_kit;

    pub fn on_module_t<'cx>(
        cx: &mut Context<'cx>,
        with_concretized_type: &dyn Fn(
            &mut Context<'cx>,
            Reason,
            AnnotConcretizeForImportsExportsFn<Cx>,
            Type,
        ) -> Type,
        reason: Reason,
        import_kind: ImportKind,
        export_name: &FlowSmolStr,
        module_name: Userland,
        is_strict: bool,
        module_: &ModuleType,
    ) -> Result<(Option<ALoc>, Type), FlowJsException> {
        on_module_t_with_env(
            cx,
            &FlowJsEnv::entry(),
            with_concretized_type,
            reason,
            import_kind,
            export_name,
            module_name,
            is_strict,
            module_,
        )
    }

    pub fn on_module_t_with_env<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        with_concretized_type: &dyn Fn(
            &mut Context<'cx>,
            Reason,
            AnnotConcretizeForImportsExportsFn<Cx>,
            Type,
        ) -> Type,
        reason: Reason,
        import_kind: ImportKind,
        export_name: &FlowSmolStr,
        module_name: Userland,
        is_strict: bool,
        module_: &ModuleType,
    ) -> Result<(Option<ALoc>, Type), FlowJsException> {
        let ModuleTypeInner {
            module_reason: _,
            module_export_types: exports,
            module_is_strict: imported_is_strict,
            module_available_platforms: _ignored_todo,
        } = &**module_;

        check_nonstrict_import(cx, env, is_strict, *imported_is_strict, &reason)?;

        // When importing from a CommonJS module, we shadow any potential named
        // exports called "default" with a pointer to the raw `module.exports`
        // object
        let mut value_exports_tmap = cx.find_exports(exports.value_exports_tmap);
        if let Some((def_loc_opt, type_)) = &exports.cjs_export {
            let name_loc = Some(match def_loc_opt {
                None => type_util::def_loc_of_t(type_).dupe(),
                Some(l) => l.dupe(),
            });
            value_exports_tmap.insert(
                Name::new("default"),
                NamedSymbol::new(name_loc, None, type_.dupe(), None),
            );
        }
        let type_exports_tmap = cx.find_exports(exports.type_exports_tmap);
        let has_every_named_export = exports.has_every_named_export;

        let export_name_key = Name::new(export_name.dupe());
        let exported_symbol_opt: Option<&NamedSymbol> = match import_kind {
            ImportKind::ImportValue => value_exports_tmap.get(&export_name_key),
            ImportKind::ImportType | ImportKind::ImportTypeof => {
                match type_exports_tmap.get(&export_name_key) {
                    Some(s) => Some(s),
                    None => value_exports_tmap.get(&export_name_key),
                }
            }
        };

        match (&import_kind, exported_symbol_opt) {
            (ImportKind::ImportType, Some(ns)) => {
                let cx_id = cx.id();
                let export_name = export_name.dupe();
                let t = with_concretized_type(
                    cx,
                    reason.dupe(),
                    Rc::new(move |cx: &mut Context<'_>, t: Type| {
                        import_type_t_kit::on_concrete_type(
                            &mut cx.switch_to(cx_id),
                            reason.dupe(),
                            export_name.as_str(),
                            t,
                        )
                    }),
                    ns.type_.dupe(),
                );
                Ok((ns.name_loc.dupe(), t))
            }
            (ImportKind::ImportType, None) if has_every_named_export => {
                let cx_id = cx.id();
                let export_name = export_name.dupe();
                let any_reason = reason.dupe();
                let t = with_concretized_type(
                    cx,
                    reason.dupe(),
                    Rc::new(move |cx: &mut Context<'_>, t: Type| {
                        import_type_t_kit::on_concrete_type(
                            &mut cx.switch_to(cx_id),
                            reason.dupe(),
                            export_name.as_str(),
                            t,
                        )
                    }),
                    Type::new(TypeInner::AnyT(any_reason, AnySource::Untyped)),
                );
                Ok((None, t))
            }
            (ImportKind::ImportTypeof, Some(ns)) => {
                let cx_id = cx.id();
                let export_name = export_name.dupe();
                let t = with_concretized_type(
                    cx,
                    reason.dupe(),
                    Rc::new(move |cx: &mut Context<'_>, t: Type| {
                        import_typeof_t_kit::on_concrete_type(
                            &mut cx.switch_to(cx_id),
                            reason.dupe(),
                            export_name.as_str(),
                            &t,
                        )
                    }),
                    ns.type_.dupe(),
                );
                Ok((ns.name_loc.dupe(), t))
            }
            (ImportKind::ImportTypeof, None) if has_every_named_export => {
                let cx_id = cx.id();
                let export_name = export_name.dupe();
                let any_reason = reason.dupe();
                let t = with_concretized_type(
                    cx,
                    reason.dupe(),
                    Rc::new(move |cx: &mut Context<'_>, t: Type| {
                        import_typeof_t_kit::on_concrete_type(
                            &mut cx.switch_to(cx_id),
                            reason.dupe(),
                            export_name.as_str(),
                            &t,
                        )
                    }),
                    Type::new(TypeInner::AnyT(any_reason, AnySource::Untyped)),
                );
                Ok((None, t))
            }
            (ImportKind::ImportValue, Some(ns)) => Ok((ns.name_loc.dupe(), ns.type_.dupe())),
            (ImportKind::ImportValue, None) if has_every_named_export => {
                let t = Type::new(TypeInner::AnyT(reason, AnySource::Untyped));
                Ok((None, t))
            }
            (ImportKind::ImportValue, None) if type_exports_tmap.contains_key(&export_name_key) => {
                if flow_common::files::has_ts_ext(cx.file()) {
                    let ns = type_exports_tmap
                        .get(&export_name_key)
                        .expect("checked above");
                    let cx_id = cx.id();
                    let export_name = export_name.dupe();
                    let t = with_concretized_type(
                        cx,
                        reason.dupe(),
                        Rc::new(move |cx: &mut Context<'_>, t: Type| {
                            import_type_t_kit::on_concrete_type(
                                &mut cx.switch_to(cx_id),
                                reason.dupe(),
                                export_name.as_str(),
                                t,
                            )
                        }),
                        ns.type_.dupe(),
                    );
                    Ok((ns.name_loc.dupe(), t))
                } else {
                    add_output_with_env(
                        cx,
                        env,
                        ErrorMessage::EImportTypeAsValue(Box::new((
                            reason.loc().dupe(),
                            FlowSmolStr::new(export_name),
                        ))),
                    )?;
                    Ok((
                        None,
                        Type::new(TypeInner::AnyT(reason, AnySource::AnyError(None))),
                    ))
                }
            }
            (_, None) => {
                let combined_exports: exports::T = match import_kind {
                    ImportKind::ImportValue => value_exports_tmap,
                    ImportKind::ImportType | ImportKind::ImportTypeof => {
                        let mut combined = value_exports_tmap;
                        for (k, v) in type_exports_tmap.iter() {
                            combined.entry_or_insert(k.dupe(), v.clone());
                        }
                        combined
                    }
                };
                let num_exports = combined_exports.len();
                let has_default_export = combined_exports.contains_key(&Name::new("default"));
                let msg = if num_exports == 1 && has_default_export {
                    ErrorMessage::EOnlyDefaultExport(Box::new((
                        reason.loc().dupe(),
                        module_name.dupe(),
                        FlowSmolStr::new(export_name),
                    )))
                } else {
                    // TODO consider filtering to OrdinaryNames only
                    let known_exports: Vec<&FlowSmolStr> = combined_exports
                        .keys()
                        .filter_map(|n| n.as_smol_str_opt())
                        .collect();
                    let suggestion = typo_suggestion(&known_exports, export_name);
                    ErrorMessage::ENoNamedExport(Box::new((
                        reason.loc().dupe(),
                        module_name.dupe(),
                        FlowSmolStr::new(export_name),
                        suggestion,
                    )))
                };
                add_output_with_env(cx, env, msg)?;
                Ok((
                    None,
                    Type::new(TypeInner::AnyT(reason, AnySource::AnyError(None))),
                ))
            }
        }
    }

    pub fn on_module_t_for_extends<'cx>(
        cx: &mut Context<'cx>,
        reason: Reason,
        import_kind: ImportKind,
        export_name: &FlowSmolStr,
        _module_name: Userland,
        is_strict: bool,
        module_: &ModuleType,
    ) -> Result<(Option<ALoc>, Type), FlowJsException> {
        let ModuleTypeInner {
            module_reason: _,
            module_export_types: exports,
            module_is_strict: imported_is_strict,
            module_available_platforms: _ignored_todo,
        } = &**module_;

        check_nonstrict_import(
            cx,
            &FlowJsEnv::entry(),
            is_strict,
            *imported_is_strict,
            &reason,
        )?;

        let mut value_exports_tmap = cx.find_exports(exports.value_exports_tmap);
        if let Some((def_loc_opt, type_)) = &exports.cjs_export {
            let name_loc = Some(match def_loc_opt {
                None => type_util::def_loc_of_t(type_).dupe(),
                Some(l) => l.dupe(),
            });
            value_exports_tmap.insert(
                Name::new("default"),
                NamedSymbol::new(name_loc, None, type_.dupe(), None),
            );
        }
        let type_exports_tmap = cx.find_exports(exports.type_exports_tmap);

        let export_name_key = Name::new(export_name.dupe());
        let exported_symbol_opt: Option<&NamedSymbol> = match import_kind {
            ImportKind::ImportValue => value_exports_tmap.get(&export_name_key),
            ImportKind::ImportType | ImportKind::ImportTypeof => {
                match type_exports_tmap.get(&export_name_key) {
                    Some(s) => Some(s),
                    None => value_exports_tmap.get(&export_name_key),
                }
            }
        };

        match exported_symbol_opt {
            Some(ns) => {
                let t = ns
                    .type_for_extends
                    .as_ref()
                    .map(|t| t.dupe())
                    .unwrap_or_else(|| ns.type_.dupe());
                Ok((ns.name_loc.dupe(), t))
            }
            None => match import_kind {
                ImportKind::ImportValue if type_exports_tmap.contains_key(&export_name_key) => {
                    if flow_common::files::has_ts_ext(cx.file()) {
                        let ns = type_exports_tmap
                            .get(&export_name_key)
                            .expect("checked above");
                        let t = ns
                            .type_for_extends
                            .as_ref()
                            .map(|t| t.dupe())
                            .unwrap_or_else(|| ns.type_.dupe());
                        Ok((ns.name_loc.dupe(), t))
                    } else {
                        Ok((
                            None,
                            Type::new(TypeInner::AnyT(reason, AnySource::AnyError(None))),
                        ))
                    }
                }
                _ => Ok((
                    None,
                    Type::new(TypeInner::AnyT(reason, AnySource::AnyError(None))),
                )),
            },
        }
    }
}

// **************************************************************************
// * Module exports                                                         *
// *                                                                        *
// * Flow supports both CommonJS and standard ES modules as well as some    *
// * interoperability semantics for communicating between the two module    *
// * systems in both directions.                                            *
// *                                                                        *
// * In order to support both systems at once, Flow abstracts the notion of *
// * module exports by storing a type map for each of the exports of a      *
// * given module, and for each module there is a ModuleT that maintains    *
// * this type map. The exported types are then considered immutable once   *
// * the module has finished inference.                                     *
// *                                                                        *
// * When a type is set for the CommonJS exports value, we store it         *
// * separately from the normal named exports tmap that ES exports are      *
// * stored within. This allows us to distinguish CommonJS modules from ES  *
// * modules when interpreting an ES import statement -- which is important *
// * because ES ModuleNamespace objects built from CommonJS exports are a   *
// * little bit magic.                                                      *
// *                                                                        *
// * For example: If a CommonJS module exports an object, we will extract   *
// * each of the properties of that object and consider them as "named"     *
// * exports for the purposes of an import statement elsewhere:             *
// *                                                                        *
// *   // CJSModule.js                                                      *
// *   module.exports = {                                                   *
// *     someNumber: 42                                                     *
// *   };                                                                   *
// *                                                                        *
// *   // ESModule.js                                                       *
// *   import {someNumber} from "CJSModule";                                *
// *   var a: number = someNumber;                                          *
// *                                                                        *
// * We also map CommonJS export values to the "default" export for         *
// * purposes of import statements in other modules:                        *
// *                                                                        *
// *   // CJSModule.js                                                      *
// *   module.exports = {                                                   *
// *     someNumber: 42                                                     *
// *   };                                                                   *
// *                                                                        *
// *   // ESModule.js                                                       *
// *   import CJSDefaultExport from "CJSModule";                            *
// *   var a: number = CJSDefaultExport.someNumber;                         *
// *                                                                        *
// * Note that the ModuleT type is not intended to be surfaced to any       *
// * userland-visible constructs. Instead it's meant as an internal         *
// * construct that is only *mapped* to/from userland constructs (such as a *
// * CommonJS exports object or an ES ModuleNamespace object).              *
// **************************************************************************

/* In the following rules, ModuleT appears in two contexts: as imported
modules, and as modules to be exported.

As a module to be exported, ModuleT denotes a "growing" module. In this
form, its contents may change: e.g., its named exports may be
extended. Conversely, the rules that drive this growing phase can expect
to work only on ModuleT. In particular, modules that are not @flow never
hit growing rules: they are modeled as `any`.

On the other hand, as an imported module, ModuleT denotes a "fully
formed" module. The rules hit by such a module don't grow it: they just
take it apart and read it. The same rules could also be hit by modules
that are not @flow, so the rules have to deal with `any`. */

// util that grows a module by adding named exports from a given map
pub mod export_named_t_kit {
    use dupe::Dupe;
    use flow_common::reason::Name;
    use flow_typing_context::Context;
    use flow_typing_type::type_::ExportKind;
    use flow_typing_type::type_::ModuleType;
    use flow_typing_type::type_::NamedSymbol;
    use flow_typing_type::type_::exports;

    pub fn mod_module_t<'cx>(
        cx: &mut Context<'cx>,
        value_tmap: exports::T,
        type_tmap: exports::T,
        export_kind: ExportKind,
        module_: &ModuleType,
    ) {
        let value_exports_tmap = module_.module_export_types.value_exports_tmap;
        let type_exports_tmap = module_.module_export_types.type_exports_tmap;

        let add_export = |name: Name, export: NamedSymbol, mut acc: exports::T| -> exports::T {
            let export_prime = match &export_kind {
                ExportKind::DirectExport => export,
                ExportKind::ReExport => {
                    // Re-exports do not overwrite named exports from the local module.
                    acc.get(&name).cloned().unwrap_or(export)
                }
            };
            acc.insert(name, export_prime);
            acc
        };

        // Optimization: avoid O(N log N) individual inserts when possible
        let merge_exports = |existing: exports::T, new_exports: exports::T| -> exports::T {
            match &export_kind {
                ExportKind::DirectExport if existing.is_empty() => {
                    // Empty target + direct exports: just use the new map
                    new_exports
                }
                ExportKind::ReExport if existing.is_empty() => {
                    // Empty target + re-exports: just use the source map
                    new_exports
                }
                _ => {
                    let mut acc = existing;
                    for (name, export) in new_exports.iter() {
                        acc = add_export(name.dupe(), export.clone(), acc);
                    }
                    acc
                }
            }
        };

        let value_exports = cx.find_exports(value_exports_tmap);
        let value_exports = merge_exports(value_exports, value_tmap);
        cx.add_export_map(value_exports_tmap, value_exports);

        let type_exports = cx.find_exports(type_exports_tmap);
        let type_exports = merge_exports(type_exports, type_tmap);
        cx.add_export_map(type_exports_tmap, type_exports);
    }
}

pub mod assert_export_is_type_t_kit {
    use std::ops::Deref;

    use dupe::Dupe;
    use flow_common::reason::Name;
    use flow_typing_context::Context;
    use flow_typing_errors::error_message::ErrorMessage;
    use flow_typing_flow_js_env::FlowJsEnv;
    use flow_typing_type::type_::AnySource;
    use flow_typing_type::type_::DefTInner;
    use flow_typing_type::type_::PolyTData;
    use flow_typing_type::type_::Type;
    use flow_typing_type::type_::TypeInner;
    use flow_typing_type::type_util::reason_of_t;

    use crate::flow_js_utils::FlowJsException;
    use crate::flow_js_utils::add_output_with_env;

    fn is_type(t: &Type) -> bool {
        match t.deref() {
            TypeInner::DefT(_, def_t) => match def_t.deref() {
                DefTInner::ClassT(_) | DefTInner::EnumObjectT { .. } | DefTInner::TypeT(_, _) => {
                    true
                }
                DefTInner::PolyT(box PolyTData { t_out, .. }) => is_type(t_out),
                _ => false,
            },
            TypeInner::NamespaceT { .. } => true,
            TypeInner::AnyT(_, _) => true,
            _ => false,
        }
    }

    pub fn on_concrete_type<'cx>(
        cx: &mut Context<'cx>,
        name: Name,
        l: Type,
    ) -> Result<Type, FlowJsException> {
        let env = &FlowJsEnv::entry();
        if is_type(&l) {
            Ok(l)
        } else {
            let reason = reason_of_t(&l).dupe();
            add_output_with_env(
                cx,
                env,
                ErrorMessage::EExportValueAsType(Box::new((reason.loc().dupe(), name))),
            )?;
            Ok(Type::new(TypeInner::AnyT(
                reason,
                AnySource::AnyError(None),
            )))
        }
    }
}

// Copy the named exports from a source module into a target module. Used
// to implement `export * from 'SomeModule'`, with the current module as
// the target and the imported module as the source.
pub mod copy_named_exports_t_kit {
    use flow_typing_context::Context;
    use flow_typing_type::type_::ExportKind;
    use flow_typing_type::type_::ModuleType;

    use super::export_named_t_kit;

    pub fn mod_module_t<'cx>(
        cx: &mut Context<'cx>,
        target_module_type: &ModuleType,
        module_: &ModuleType,
    ) {
        let source_exports = &module_.module_export_types;
        let value_exports = cx.find_exports(source_exports.value_exports_tmap);
        let type_exports = cx.find_exports(source_exports.type_exports_tmap);
        // Per the ECMAScript spec, `export * from 'x'` re-exports the named
        // exports of `x` but never its default export (GetExportedNames skips
        // "default"). Historically Flow forwarded the default too, matching a
        // now-fixed Metro runtime bug. When the flag is set, drop "default" so
        // Flow agrees with the spec and the fixed bundler runtime.
        let value_exports = if cx.export_star_excludes_default() {
            value_exports.without_default()
        } else {
            value_exports
        };
        export_named_t_kit::mod_module_t(
            cx,
            value_exports,
            type_exports,
            ExportKind::ReExport,
            target_module_type,
        );
    }
}

// Export a type from a given ModuleT, but only if the type is compatible
// with `import type`/`export type`. When it is not compatible, it is simply
// not added to the exports map.
//
// Note that this is very similar to `ExportNamedT` except that it only
// exports one type at a time and it takes the type to be exported as a
// lower (so that the type can be filtered post-resolution).
pub mod export_type_t_kit {
    use std::ops::Deref;

    use dupe::Dupe;
    use flow_common::reason::Name;
    use flow_common::reason::Reason;
    use flow_typing_context::Context;
    use flow_typing_type::type_::DefTInner;
    use flow_typing_type::type_::ExportKind;
    use flow_typing_type::type_::ModuleType;
    use flow_typing_type::type_::NamedSymbol;
    use flow_typing_type::type_::TypeInner;
    use flow_typing_type::type_::exports;

    use super::export_named_t_kit;
    use super::import_type_t_kit;

    pub fn on_concrete_type<'cx>(
        cx: &mut Context<'cx>,
        reason: Reason,
        named_symbol: &NamedSymbol,
        export_name: Name,
        target_module_type: &ModuleType,
    ) {
        let l = &named_symbol.type_;

        let is_type_export = match l.deref() {
            TypeInner::DefT(_, def_t) => {
                if matches!(def_t.deref(), DefTInner::ObjT(_)) && export_name.matches_str("default")
                {
                    true
                } else {
                    import_type_t_kit::canonicalize_imported_type(cx, reason.dupe(), l).is_some()
                }
            }
            _ => import_type_t_kit::canonicalize_imported_type(cx, reason.dupe(), l).is_some(),
        };

        if is_type_export {
            let mut named = exports::T::new();
            named.insert(export_name, named_symbol.clone());
            export_named_t_kit::mod_module_t(
                cx,
                exports::T::new(),
                named,
                ExportKind::ReExport,
                target_module_type,
            );
        }
    }
}

// Copy only the type exports from a source module into a target module.
// Used to implement `export type * from ...`.
pub mod copy_type_exports_t_kit {
    use dupe::Dupe;
    use flow_common::reason::Reason;
    use flow_typing_context::Context;
    use flow_typing_type::type_::ExportKind;
    use flow_typing_type::type_::ModuleType;
    use flow_typing_type::type_::NamedSymbol;
    use flow_typing_type::type_::Type;
    use flow_typing_type::type_::exports;
    use flow_utils_concurrency::job_error::JobError;

    use super::export_named_t_kit;
    use super::export_type_t_kit;

    pub fn mod_module_t<'cx, F>(
        cx: &mut Context<'cx>,
        concretize_export_type: F,
        reason: Reason,
        target_module_type: &ModuleType,
        module_: &ModuleType,
    ) -> Result<(), JobError>
    where
        F: Fn(&mut Context<'cx>, Reason, Type) -> Result<Type, JobError>,
    {
        let source_exports = &module_.module_export_types;

        let export_all = |cx: &mut Context<'cx>, exports_tmap_id| -> Result<(), JobError> {
            let exports_tmap = cx.find_exports(exports_tmap_id);
            for (export_name, ns) in exports_tmap.iter() {
                let type_ = concretize_export_type(cx, reason.dupe(), ns.type_.dupe())?;
                let concretized_ns = NamedSymbol::new(
                    ns.name_loc.dupe(),
                    ns.preferred_def_locs.clone(),
                    type_,
                    ns.type_for_extends.clone(),
                );
                export_type_t_kit::on_concrete_type(
                    cx,
                    reason.dupe(),
                    &concretized_ns,
                    export_name.dupe(),
                    target_module_type,
                );
            }
            Ok(())
        };

        export_all(cx, source_exports.value_exports_tmap)?;
        let type_exports = cx.find_exports(source_exports.type_exports_tmap);
        export_named_t_kit::mod_module_t(
            cx,
            exports::T::new(),
            type_exports,
            ExportKind::ReExport,
            target_module_type,
        );
        Ok(())
    }
}

pub mod cjs_extract_named_exports_t_kit {
    use std::ops::Deref;

    use dupe::Dupe;
    use flow_common::reason::Reason;
    use flow_typing_context::Context;
    use flow_typing_type::type_::DefTInner;
    use flow_typing_type::type_::ExportKind;
    use flow_typing_type::type_::ModuleType;
    use flow_typing_type::type_::ModuleTypeInner;
    use flow_typing_type::type_::NamedSymbol;
    use flow_typing_type::type_::Type;
    use flow_typing_type::type_::TypeInner;
    use flow_typing_type::type_::exports;
    use flow_typing_type::type_::poly;
    use flow_typing_type::type_::properties;
    use flow_typing_type::type_util;
    use flow_utils_concurrency::job_error::JobError;

    use super::export_named_t_kit;
    use super::is_munged_prop_name;

    pub fn on_type<'cx, F>(
        cx: &mut Context<'cx>,
        concretize: &F,
        reason: Reason,
        local_module: ModuleType,
        t: Type,
    ) -> Result<ModuleType, JobError>
    where
        F: Fn(&mut Context<'_>, Type) -> Result<Type, JobError>,
    {
        let t = concretize(cx, t)?;
        match t.deref() {
            TypeInner::NamespaceT(ns) => {
                //   Copy props from the values part
                let module_type = on_type(
                    cx,
                    concretize,
                    reason.dupe(),
                    local_module,
                    ns.values_type.dupe(),
                )?;
                //   Copy type exports
                let type_props = cx.find_props(ns.types_tmap.dupe());
                let type_exports: exports::T =
                    type_props.extract_named_exports().into_iter().collect();
                export_named_t_kit::mod_module_t(
                    cx,
                    exports::T::new(),
                    type_exports,
                    ExportKind::DirectExport,
                    &module_type,
                );
                Ok(module_type)
            }
            TypeInner::DefT(_, def)
                if let DefTInner::PolyT(poly) = def.deref()
                    && let TypeInner::DefT(_, inner_def) = poly.t_out.deref()
                    && matches!(inner_def.deref(), DefTInner::ClassT(_)) =>
            {
                let Some(statics) = type_util::statics(&poly.t_out) else {
                    return Ok(local_module);
                };
                let mut module_export_types = local_module.module_export_types.clone();
                module_export_types.value_exports_tmap = cx.make_export_map(exports::T::new());
                let extracted = on_type(
                    cx,
                    concretize,
                    reason,
                    ModuleType::new(ModuleTypeInner {
                        module_reason: local_module.module_reason.dupe(),
                        module_export_types,
                        module_is_strict: local_module.module_is_strict,
                        module_available_platforms: local_module.module_available_platforms.clone(),
                    }),
                    statics.dupe(),
                )?;
                let bind = |t: &Type| {
                    type_util::poly_type_of_tparam_list(
                        poly::Id::generate_id(),
                        poly.tparams_loc.dupe(),
                        poly.tparams.dupe(),
                        t.dupe(),
                        poly.strictness_kind,
                        poly.kind,
                    )
                };
                let named = cx
                    .find_exports(extracted.module_export_types.value_exports_tmap)
                    .iter()
                    .map(|(name, symbol)| {
                        (
                            name.dupe(),
                            NamedSymbol::new(
                                symbol.name_loc.dupe(),
                                symbol.preferred_def_locs.clone(),
                                bind(&symbol.type_),
                                symbol.type_for_extends.as_ref().map(bind),
                            ),
                        )
                    })
                    .collect();
                export_named_t_kit::mod_module_t(
                    cx,
                    named,
                    exports::T::new(),
                    ExportKind::DirectExport,
                    &local_module,
                );
                Ok(local_module)
            }
            TypeInner::DefT(_, def)
                if matches!(
                    def.deref(),
                    DefTInner::FunT(_, _) | DefTInner::ClassT(_) | DefTInner::PolyT(_)
                ) =>
            {
                match type_util::statics(&t) {
                    Some(statics) => on_type(cx, concretize, reason, local_module, statics.dupe()),
                    None => Ok(local_module),
                }
            }
            // ObjT CommonJS export values have their properties turned into named exports.
            TypeInner::DefT(_, def_t) if matches!(def_t.deref(), DefTInner::ObjT(_)) => {
                if let DefTInner::ObjT(o) = def_t.deref() {
                    let props_tmap = o.props_tmap.dupe();
                    let proto_t = &o.proto_t;
                    // Copy props from the prototype
                    let module_type =
                        on_type(cx, concretize, reason.dupe(), local_module, proto_t.dupe())?;
                    // Copy own props
                    let own_props = cx.find_props(props_tmap);
                    let own_exports: exports::T =
                        own_props.extract_named_exports().into_iter().collect();
                    export_named_t_kit::mod_module_t(
                        cx,
                        own_exports,
                        exports::T::new(),
                        ExportKind::DirectExport,
                        &module_type,
                    );
                    Ok(module_type)
                } else {
                    unreachable!()
                }
            }
            // InstanceT CommonJS export values have their properties turned into named exports.
            TypeInner::DefT(_, def_t) if matches!(def_t.deref(), DefTInner::InstanceT(_)) => {
                if let DefTInner::InstanceT(inst) = def_t.deref() {
                    let own_props = inst.inst.own_props.dupe();
                    let proto_props = inst.inst.proto_props.dupe();

                    let extract_named_exports =
                        |cx: &Context<'_>, id: properties::Id| -> exports::T {
                            let props = cx.find_props(id);
                            let filtered: properties::PropertiesMap = props
                                .iter()
                                .filter(|(name, _)| !is_munged_prop_name(cx, name))
                                .map(|(name, prop)| (name.dupe(), prop.dupe()))
                                .collect();
                            filtered.extract_named_exports().into_iter().collect()
                        };

                    // Copy own props
                    let own_exports = extract_named_exports(cx, own_props);
                    export_named_t_kit::mod_module_t(
                        cx,
                        own_exports,
                        exports::T::new(),
                        ExportKind::DirectExport,
                        &local_module,
                    );

                    // Copy proto props
                    // TODO: own props should take precedence
                    let proto_exports = extract_named_exports(cx, proto_props);
                    export_named_t_kit::mod_module_t(
                        cx,
                        proto_exports,
                        exports::T::new(),
                        ExportKind::DirectExport,
                        &local_module,
                    );
                    Ok(local_module)
                } else {
                    unreachable!()
                }
            }
            // If the module is exporting any or Object, then we allow any named import.
            TypeInner::AnyT(_, _) => {
                let mut module_export_types = local_module.module_export_types.clone();
                module_export_types.has_every_named_export = true;
                Ok(ModuleType::new(ModuleTypeInner {
                    module_reason: local_module.module_reason.dupe(),
                    module_export_types,
                    module_is_strict: local_module.module_is_strict,
                    module_available_platforms: local_module.module_available_platforms.clone(),
                }))
            }
            // All other CommonJS export value types do not get merged into the named
            // exports tmap in any special way.
            _ => Ok(local_module),
        }
    }
}

pub enum ExportClassification {
    FoundValue(NamedSymbol),
    FoundTypeOnly(NamedSymbol),
    UnknownButAllowedByEveryNamedExport,
    ExportMissing,
}

pub mod import_export_utils {
    use std::collections::BTreeSet;
    use std::rc::Rc;

    use dupe::Dupe;
    use flow_aloc::ALoc;
    use flow_common::flow_import_specifier::FlowImportSpecifier;
    use flow_common::flow_import_specifier::Userland;
    use flow_common::flow_symbol::Symbol;
    use flow_common::platform_set::PlatformSet;
    use flow_common::reason::Name;
    use flow_common::reason::Reason;
    use flow_common::reason::VirtualReasonDesc;
    use flow_common::reason::mk_reason;
    use flow_data_structure_wrapper::smol_str::FlowSmolStr;
    use flow_parser::ast::statement::ImportKind as AstImportKind;
    use flow_typing_context::Context;
    use flow_typing_context::Cx;
    use flow_typing_context::ResolvedRequire;
    use flow_typing_errors::error_message::EExpectedModuleLookupFailedData;
    use flow_typing_errors::error_message::EMissingPlatformSupportWithAvailablePlatformsData;
    use flow_typing_errors::error_message::ErrorMessage;
    use flow_typing_errors::intermediate_error_types::ExpectedModulePurpose;
    use flow_typing_flow_js_env::FlowJsEnv;
    use flow_typing_type::type_::AnySource;
    use flow_typing_type::type_::ImportKind;
    use flow_typing_type::type_::ModuleType;
    use flow_typing_type::type_::NamedSymbol;
    use flow_typing_type::type_::NamespaceType;
    use flow_typing_type::type_::Type;
    use flow_typing_type::type_::TypeInner;
    use flow_typing_type::type_::aconstraint::AnnotConcretizeForImportsExportsFn;
    use flow_typing_type::type_::any_t;
    use flow_typing_type::type_util::def_loc_of_t;
    use flow_utils_concurrency::job_error::JobError;

    use super::ExportClassification;
    use super::FlowJsException;
    use super::add_output_with_env;
    use super::cannot_import_global_libdef_error;
    use super::import_default_t_kit;
    use super::import_module_ns_t_kit;
    use super::import_named_t_kit;
    use super::import_typeof_t_kit;
    use super::lookup_builtin_module_error;

    fn check_platform_availability<'cx>(
        cx: &mut Context<'cx>,
        error_loc: ALoc,
        imported_module_available_platforms: Option<&PlatformSet>,
    ) -> Result<(), FlowJsException> {
        let current_module_available_platforms = cx.available_platforms();
        match (
            current_module_available_platforms,
            imported_module_available_platforms,
        ) {
            (None, None) | (None, Some(_)) | (Some(_), None) => Ok(()),
            (Some(required_platforms), Some(available_platforms)) => {
                let file_options = cx.file_options();
                let required_platforms = required_platforms.to_platform_string_set(&file_options);
                let available_platforms = available_platforms.to_platform_string_set(&file_options);
                let missing_platforms: BTreeSet<_> = required_platforms
                    .difference(&available_platforms)
                    .cloned()
                    .collect();
                if !missing_platforms.is_empty() {
                    let message = ErrorMessage::EMissingPlatformSupportWithAvailablePlatforms(
                        Box::new(EMissingPlatformSupportWithAvailablePlatformsData {
                            loc: error_loc,
                            available_platforms,
                            required_platforms,
                        }),
                    );
                    add_output_with_env(cx, &FlowJsEnv::entry(), message)?;
                }
                Ok(())
            }
        }
    }

    pub fn get_module_type_or_any<'cx>(
        cx: &mut Context<'cx>,
        perform_platform_validation: bool,
        import_kind_for_untyped_import_validation: Option<ImportKind>,
        loc: ALoc,
        mref: Userland,
    ) -> Result<Result<ModuleType, Type>, FlowJsException> {
        let env = &FlowJsEnv::entry();
        if cx.in_declare_module() {
            match cx.builtin_module_opt(&mref) {
                Some((_reason, m)) => Ok(Ok(m.get_forced(cx).dupe())),
                None => {
                    let err_t =
                        lookup_builtin_module_error(cx, &FlowSmolStr::new(mref.as_str()), loc)?;
                    Ok(Err(err_t))
                }
            }
        } else {
            let module_type_or_any =
                match cx.find_require(&FlowImportSpecifier::Userland(mref.dupe())) {
                    ResolvedRequire::TypedModule(f) => f(cx),
                    ResolvedRequire::UncheckedModule(module_def_loc) => {
                        if let Some(import_kind) = &import_kind_for_untyped_import_validation {
                            match import_kind {
                                ImportKind::ImportType | ImportKind::ImportTypeof => {
                                    let message = ErrorMessage::EUntypedTypeImport(Box::new((
                                        loc.dupe(),
                                        mref.dupe(),
                                    )));
                                    add_output_with_env(cx, env, message)?;
                                }
                                ImportKind::ImportValue => {
                                    let message = ErrorMessage::EUntypedImport(Box::new((
                                        loc.dupe(),
                                        mref.dupe(),
                                    )));
                                    add_output_with_env(cx, env, message)?;
                                }
                            }
                        }
                        Err(any_t::why(
                            AnySource::Untyped,
                            mk_reason(VirtualReasonDesc::RModule(mref.dupe()), module_def_loc),
                        ))
                    }
                    ResolvedRequire::MissingModule => Err(lookup_builtin_module_error(
                        cx,
                        &FlowSmolStr::new(mref.as_str()),
                        loc.dupe(),
                    )?),
                    ResolvedRequire::GlobalLibdefModule(libdef) => {
                        Err(cannot_import_global_libdef_error(
                            cx,
                            &FlowSmolStr::new(mref.as_str()),
                            &FlowSmolStr::new(libdef.as_str()),
                            loc.dupe(),
                        )?)
                    }
                };
            let need_platform_validation =
                perform_platform_validation && cx.file_options().multi_platform;
            if need_platform_validation {
                match &module_type_or_any {
                    Ok(m) => {
                        if need_platform_validation {
                            check_platform_availability(
                                cx,
                                loc.dupe(),
                                m.module_available_platforms.as_ref(),
                            )?;
                        }
                    }
                    Err(_) => {}
                }
            }
            Ok(module_type_or_any)
        }
    }

    pub fn get_imported_type<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        singleton_concretize_type_for_imports_exports: &dyn Fn(
            &mut Context<'cx>,
            Reason,
            Type,
        )
            -> Result<Type, FlowJsException>,
        import_reason: Reason,
        module_name: Userland,
        source_module: &Result<ModuleType, Type>,
        import_kind: ImportKind,
        remote_name: &FlowSmolStr,
        local_name: &FlowSmolStr,
    ) -> Result<(Option<ALoc>, Type), FlowJsException> {
        let is_strict = cx.is_strict();
        let mut name_def_loc_ref: Option<ALoc> = None;
        let with_concretized_type = |cx: &mut Context<'cx>,
                                     r: Reason,
                                     f: AnnotConcretizeForImportsExportsFn<Cx>,
                                     t: Type|
         -> Type {
            match singleton_concretize_type_for_imports_exports(cx, r.dupe(), t) {
                Ok(t) => f(cx, t),
                Err(_) => Type::new(TypeInner::AnyT(r, AnySource::AnyError(None))),
            }
        };
        let t = match source_module {
            Ok(m) => {
                if remote_name == "default" {
                    let (name_loc_opt, t) = import_default_t_kit::on_module_t_with_env(
                        cx,
                        env,
                        &with_concretized_type,
                        import_reason.dupe(),
                        import_kind,
                        local_name,
                        module_name,
                        is_strict,
                        m,
                    )?;
                    name_def_loc_ref = name_loc_opt;
                    t
                } else {
                    let (name_loc_opt, t) = import_named_t_kit::on_module_t_with_env(
                        cx,
                        env,
                        &with_concretized_type,
                        import_reason.dupe(),
                        import_kind.clone(),
                        remote_name,
                        module_name,
                        is_strict,
                        m,
                    )?;
                    name_def_loc_ref = name_loc_opt;
                    t
                }
            }
            Err(t) => t.dupe(),
        };
        let name_def_loc = name_def_loc_ref;
        Ok((name_def_loc, t))
    }

    pub fn get_module_namespace_type<'cx>(
        cx: &mut Context<'cx>,
        reason: Reason,
        namespace_symbol: Symbol,
        source_module: &Result<ModuleType, Type>,
    ) -> Result<Type, FlowJsException> {
        let is_strict = cx.is_strict();
        Ok(match source_module {
            Ok(m) => {
                let (values_type, types_tmap) =
                    import_module_ns_t_kit::on_module_t(cx, false, reason, is_strict, m)?;
                Type::new(TypeInner::NamespaceT(Rc::new(NamespaceType {
                    namespace_symbol,
                    values_type,
                    types_tmap,
                })))
            }
            Err(t) => t.dupe(),
        })
    }

    pub fn import_namespace_specifier_type<'cx>(
        cx: &mut Context<'cx>,
        import_reason: Reason,
        import_kind: &AstImportKind,
        module_name: Userland,
        namespace_symbol: Symbol,
        source_module: &Result<ModuleType, Type>,
        local_loc: ALoc,
    ) -> Result<Type, FlowJsException> {
        match import_kind {
            AstImportKind::ImportType => {
                get_module_namespace_type(cx, import_reason, namespace_symbol, source_module)
            }
            AstImportKind::ImportTypeof => {
                let module_ns_t = get_module_namespace_type(
                    cx,
                    import_reason.dupe(),
                    namespace_symbol,
                    source_module,
                )?;
                let bind_reason = import_reason.reposition(local_loc);
                Ok(import_typeof_t_kit::on_concrete_type(
                    cx,
                    bind_reason,
                    "*",
                    &module_ns_t,
                ))
            }
            AstImportKind::ImportValue => {
                let reason = mk_reason(
                    VirtualReasonDesc::RModule(module_name.dupe()),
                    local_loc.dupe(),
                );
                let namespace_symbol =
                    Symbol::mk_module_symbol(FlowSmolStr::new(module_name.as_str()), local_loc);
                get_module_namespace_type(cx, reason, namespace_symbol, source_module)
            }
        }
    }

    pub fn type_kind_of_kind(kind: &AstImportKind) -> ImportKind {
        match kind {
            AstImportKind::ImportType => ImportKind::ImportType,
            AstImportKind::ImportTypeof => ImportKind::ImportTypeof,
            AstImportKind::ImportValue => ImportKind::ImportValue,
        }
    }

    pub fn import_named_specifier_type<'cx>(
        cx: &mut Context<'cx>,
        import_reason: Reason,
        singleton_concretize_type_for_imports_exports: &dyn Fn(
            &mut Context<'cx>,
            Reason,
            Type,
        )
            -> Result<Type, FlowJsException>,
        import_kind: &AstImportKind,
        module_name: Userland,
        source_module: &Result<ModuleType, Type>,
        remote_name: &FlowSmolStr,
        local_name: &FlowSmolStr,
    ) -> Result<(Option<ALoc>, Type), FlowJsException> {
        let env = &FlowJsEnv::entry();
        let import_kind = type_kind_of_kind(import_kind);
        get_imported_type(
            cx,
            env,
            singleton_concretize_type_for_imports_exports,
            import_reason,
            module_name,
            source_module,
            import_kind,
            remote_name,
            local_name,
        )
    }

    pub fn import_default_specifier_type<'cx>(
        cx: &mut Context<'cx>,
        import_reason: Reason,
        singleton_concretize_type_for_imports_exports: &dyn Fn(
            &mut Context<'cx>,
            Reason,
            Type,
        )
            -> Result<Type, FlowJsException>,
        import_kind: &AstImportKind,
        module_name: Userland,
        source_module: &Result<ModuleType, Type>,
        local_name: &FlowSmolStr,
    ) -> Result<(Option<ALoc>, Type), FlowJsException> {
        let env = &FlowJsEnv::entry();
        let import_kind = type_kind_of_kind(import_kind);
        get_imported_type(
            cx,
            env,
            singleton_concretize_type_for_imports_exports,
            import_reason,
            module_name,
            source_module,
            import_kind,
            &FlowSmolStr::new_inline("default"),
            local_name,
        )
    }

    pub fn import_named_specifier_type_for_extends<'cx>(
        cx: &mut Context<'cx>,
        import_reason: Reason,
        import_kind: &AstImportKind,
        module_name: Userland,
        source_module: &Result<ModuleType, Type>,
        remote_name: &FlowSmolStr,
    ) -> Result<Type, FlowJsException> {
        let import_kind = type_kind_of_kind(import_kind);
        let is_strict = cx.is_strict();
        match source_module {
            Ok(m) => {
                let (_name_loc_opt, t) = import_named_t_kit::on_module_t_for_extends(
                    cx,
                    import_reason,
                    import_kind,
                    remote_name,
                    module_name,
                    is_strict,
                    m,
                )?;
                Ok(t)
            }
            Err(t) => Ok(t.dupe()),
        }
    }

    pub fn import_default_specifier_type_for_extends<'cx>(
        cx: &mut Context<'cx>,
        import_reason: Reason,
        import_kind: &AstImportKind,
        module_name: Userland,
        source_module: &Result<ModuleType, Type>,
        local_name: &FlowSmolStr,
    ) -> Result<Type, FlowJsException> {
        let import_kind = type_kind_of_kind(import_kind);
        let is_strict = cx.is_strict();
        match source_module {
            Ok(m) => {
                let (_name_loc_opt, t) = import_default_t_kit::on_module_t_for_extends(
                    cx,
                    import_reason,
                    import_kind,
                    local_name.as_str(),
                    module_name,
                    is_strict,
                    m,
                )?;
                Ok(t)
            }
            Err(t) => Ok(t.dupe()),
        }
    }

    pub fn cjs_require_type<'cx, R>(
        cx: &mut Context<'cx>,
        reason: Reason,
        reposition: R,
        namespace_symbol: Symbol,
        standard_cjs_esm_interop: bool,
        source_module: &Result<ModuleType, Type>,
    ) -> Result<(Option<ALoc>, Type), FlowJsException>
    where
        R: Fn(&mut Context<'cx>, ALoc, Type) -> Result<Type, JobError>,
    {
        let is_strict = cx.is_strict();
        Ok(match source_module {
            Ok(m) => {
                let (t, def_loc) = super::cjs_require_t_kit::on_module_t(
                    cx,
                    reposition,
                    reason,
                    namespace_symbol,
                    is_strict,
                    standard_cjs_esm_interop,
                    m,
                )?;
                (Some(def_loc), t)
            }
            Err(t) => (None, t.dupe()),
        })
    }

    pub fn get_implicitly_imported_react_type<'cx>(
        cx: &mut Context<'cx>,
        loc: ALoc,
        singleton_concretize_type_for_imports_exports: &dyn Fn(
            &mut Context<'cx>,
            Reason,
            Type,
        )
            -> Result<Type, FlowJsException>,
        purpose: ExpectedModulePurpose,
    ) -> Result<Type, FlowJsException> {
        get_implicitly_imported_react_type_with_env(
            cx,
            &FlowJsEnv::entry(),
            loc,
            singleton_concretize_type_for_imports_exports,
            purpose,
        )
    }

    pub fn get_implicitly_imported_react_type_with_env<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        loc: ALoc,
        singleton_concretize_type_for_imports_exports: &dyn Fn(
            &mut Context<'cx>,
            Reason,
            Type,
        )
            -> Result<Type, FlowJsException>,
        purpose: ExpectedModulePurpose,
    ) -> Result<Type, FlowJsException> {
        let react_userland = Userland::from_smol_str(FlowSmolStr::new("react"));
        let source_module = match cx.builtin_module_opt(&react_userland) {
            Some((_reason, module_type)) => Ok(module_type.get_forced(cx).dupe()),
            None => {
                let reason = mk_reason(
                    VirtualReasonDesc::RModule(react_userland.dupe()),
                    loc.dupe(),
                );
                add_output_with_env(
                    cx,
                    env,
                    ErrorMessage::EExpectedModuleLookupFailed(Box::new(
                        EExpectedModuleLookupFailedData {
                            loc: loc.dupe(),
                            name: FlowSmolStr::new("react"),
                            expected_module_purpose: purpose.clone(),
                        },
                    )),
                )?;
                Err(any_t::error(reason))
            }
        };
        let (name, reason, import_kind) = match &purpose {
            ExpectedModulePurpose::ReactModuleForJSXFragment => (
                "Fragment",
                mk_reason(
                    VirtualReasonDesc::RIdentifier("Fragment".into()),
                    loc.dupe(),
                ),
                ImportKind::ImportValue,
            ),
            ExpectedModulePurpose::ReactModuleForReactClassComponent => (
                "Component",
                mk_reason(
                    VirtualReasonDesc::RIdentifier("Component".into()),
                    loc.dupe(),
                ),
                ImportKind::ImportValue,
            ),
            ExpectedModulePurpose::ReactModuleForReactMixedElementType => (
                "MixedElement",
                mk_reason(
                    VirtualReasonDesc::RType("React.MixedElement".into()),
                    loc.dupe(),
                ),
                ImportKind::ImportType,
            ),
            ExpectedModulePurpose::ReactModuleForReactNodeType => (
                "Node",
                mk_reason(VirtualReasonDesc::RType("React.Node".into()), loc.dupe()),
                ImportKind::ImportType,
            ),
            ExpectedModulePurpose::ReactModuleForReactRefSetterType => (
                "RefSetter",
                mk_reason(
                    VirtualReasonDesc::RType("React.RefSetter".into()),
                    loc.dupe(),
                ),
                ImportKind::ImportType,
            ),
            ExpectedModulePurpose::ReactModuleForReactElementRefType => (
                "ElementRef",
                mk_reason(
                    VirtualReasonDesc::RType("React.ElementRef".into()),
                    loc.dupe(),
                ),
                ImportKind::ImportType,
            ),
            ExpectedModulePurpose::ReactModuleForReactManagedAttributesType => (
                "ReactManagedAttributes",
                mk_reason(
                    VirtualReasonDesc::RType("React.ReactManagedAttributes".into()),
                    loc.dupe(),
                ),
                ImportKind::ImportType,
            ),
        };
        let name = FlowSmolStr::new(name);
        let (_name_def_loc, t) = get_imported_type(
            cx,
            env,
            singleton_concretize_type_for_imports_exports,
            reason,
            react_userland,
            &source_module,
            import_kind,
            &name,
            &name,
        )?;
        Ok(t)
    }

    pub fn classify_named_export<'cx>(
        cx: &mut Context<'cx>,
        module_: &ModuleType,
        export_name: &FlowSmolStr,
    ) -> ExportClassification {
        let exports = &module_.module_export_types;
        let mut value_exports_tmap = cx.find_exports(exports.value_exports_tmap);
        match &exports.cjs_export {
            Some((def_loc_opt, type_)) => {
                let name_loc = Some(match def_loc_opt {
                    None => def_loc_of_t(type_).dupe(),
                    Some(l) => l.dupe(),
                });
                value_exports_tmap.insert(
                    Name::new(FlowSmolStr::new("default")),
                    NamedSymbol::new(name_loc, None, type_.dupe(), None),
                );
            }
            None => {}
        }
        let type_exports_tmap = cx.find_exports(exports.type_exports_tmap);
        let key = Name::new(export_name.dupe());
        match value_exports_tmap.get(&key) {
            Some(sym) => ExportClassification::FoundValue(sym.dupe()),
            None => match type_exports_tmap.get(&key) {
                Some(sym) => ExportClassification::FoundTypeOnly(sym.dupe()),
                None => {
                    if exports.has_every_named_export {
                        ExportClassification::UnknownButAllowedByEveryNamedExport
                    } else {
                        ExportClassification::ExportMissing
                    }
                }
            },
        }
    }

    pub fn is_ts_import_type_only<'cx>(cx: &mut Context<'cx>, def_loc: &ALoc) -> bool {
        match cx.find_ts_import_provenance(def_loc) {
            Some((source, remote_name)) => {
                let module_result = if cx.in_declare_module() {
                    match &source {
                        FlowImportSpecifier::Userland(mref) => cx
                            .builtin_module_opt(mref)
                            .map(|(_reason, m)| m.get_forced(cx).dupe()),
                    }
                } else {
                    match cx.find_require(&source) {
                        ResolvedRequire::TypedModule(f) => f(cx).ok(),
                        _ => None,
                    }
                };
                match module_result {
                    Some(m) => match classify_named_export(cx, &m, &remote_name) {
                        ExportClassification::FoundTypeOnly(_) => true,
                        _ => false,
                    },
                    None => false,
                }
            }
            None => false,
        }
    }
}

// *******************
// * GetPropT helper *
// *******************

pub fn method_property_for_read(
    method_accessible: bool,
    p: flow_typing_type::type_::Property,
) -> flow_typing_type::type_::Property {
    use std::ops::Deref;

    use flow_typing_type::type_::PropertyInner;
    use flow_typing_type::type_::properties;

    match p.deref() {
        PropertyInner::Method { key_loc, type_: t } => {
            flow_typing_type::type_::Property::new(PropertyInner::Method {
                key_loc: key_loc.dupe(),
                type_: properties::method_to_function(t, method_accessible),
            })
        }
        _ => p,
    }
}

static INT_REGEX: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"^-?(0|[1-9][0-9]*)$").unwrap());

pub fn is_str_intlike(s: &str) -> bool {
    INT_REGEX.is_match(s)
}

/// Returns the value of an int-like string key (for example `"0"` or `"42"`)
/// when it is a JavaScript safe integer, or `None` otherwise. Pairs the
/// `is_str_intlike` shape check with the safe-integer bound the numeric-key call
/// sites need before treating the string as a number.
fn intlike_str_safe_int(s: &str) -> Option<f64> {
    if !is_str_intlike(s) {
        return None;
    }
    match s.parse::<f64>() {
        Ok(value) if flow_common::js_number::is_float_safe_integer(value) => Some(value),
        Ok(_) | Err(_) => None,
    }
}

/// Whether an element-access key is an array index: a number, or an int-like
/// string whose value is a safe integer, since `arr['0']` indexes the array just
/// like `arr[0]`. An int-like string outside the safe-integer range is not an
/// index, so it is left to named member access, the same as `arr['map']`.
pub fn is_array_index_key(def_t: &DefTInner) -> bool {
    match def_t {
        DefTInner::NumGeneralT(_) | DefTInner::SingletonNumT { .. } => true,
        DefTInner::SingletonStrT { value, .. } => intlike_str_safe_int(value).is_some(),
        _ => false,
    }
}

/// If `l` is an int-like string singleton whose value is a safe integer (for
/// example the `'0'` in `arr['0']`), returns the equivalent numeric singleton so
/// the array-index path resolves it as an index. Returns `None` for any other
/// key.
pub fn intlike_str_key_as_num(l: &Type) -> Option<Type> {
    use std::ops::Deref;

    use dupe::Dupe;
    use flow_typing_type::type_::DefT;
    use flow_typing_type::type_::NumberLiteral;
    use flow_typing_type::type_util::reason_of_t;

    let TypeInner::DefT(_, def_t) = l.deref() else {
        return None;
    };
    let DefTInner::SingletonStrT { value, from_annot } = def_t.deref() else {
        return None;
    };
    let float = intlike_str_safe_int(value)?;
    Some(Type::new(TypeInner::DefT(
        reason_of_t(l).dupe(),
        DefT::new(DefTInner::SingletonNumT {
            from_annot: *from_annot,
            value: NumberLiteral(float, value.dupe()),
        }),
    )))
}

pub fn type_of_key_name(name: Name, reason: &Reason) -> Type {
    type_of_key_name_with_env(&FlowJsEnv::entry(), name, reason)
}

pub fn type_of_key_name_with_env(env: &FlowJsEnv, name: Name, reason: &Reason) -> Type {
    use flow_typing_type::type_::DefT;

    let str = match &name {
        Name::Symbol(sym) => {
            let key_reason = reason
                .dupe()
                .update_desc(|_| Some(VirtualReasonDesc::RUniqueSymbol));
            return Type::new(TypeInner::DefT(
                key_reason,
                DefT::new(DefTInner::UniqueSymbolT(UniqueSymbolTData {
                    symbol: sym.dupe(),
                    from_annot: true,
                })),
            ));
        }
        Name::Str(s) => s,
    };

    let str_key = || {
        let key_reason = reason.dupe().replace_desc(None);
        Type::new(TypeInner::DefT(
            key_reason,
            DefT::new(DefTInner::SingletonStrT {
                value: str.dupe(),
                from_annot: true,
            }),
        ))
    };

    // We don't want the `NumericStrKeyT` type to leak out of the obj-to-obj
    // subtyping check context. Other than `Object.key` and `$Keys`, which
    // already we ensure always return a string, another way this type could
    // leak out is during implicit instantiation, e.g. for a function like
    // ```
    // // declare function f<K>({+[K]: mixed}): K;
    // f({1: true});
    // ```
    // So, if we are in implicit instantiation, we always treat this as a string.
    if !env.in_implicit_instantiation()
        && let Some(value) = intlike_str_safe_int(str)
    {
        let key_reason = reason
            .dupe()
            .update_desc(|_| Some(VirtualReasonDesc::RProperty(Some(name.dupe()))));
        Type::new(TypeInner::DefT(
            key_reason,
            DefT::new(DefTInner::NumericStrKeyT(
                flow_typing_type::type_::NumberLiteral(
                    value,
                    flow_data_structure_wrapper::smol_str::FlowSmolStr::new(str),
                ),
            )),
        ))
    } else {
        str_key()
    }
}

/// Everything needed to continue a property lookup along the prototype chain, handed to
/// [`GetPropHelper::cg_lookup`].
pub struct CgLookupArgs {
    pub reason_op: Reason,
    pub lookup_kind: flow_typing_type::type_::LookupKind,
    pub indexer_fallback: Option<Box<flow_typing_type::type_::IndexerFallbackData>>,
    pub propref: flow_typing_type::type_::PropRef,
    pub use_op: UseOp,
    pub ids: flow_typing_type::type_::properties::Set,
}

pub trait GetPropHelper {
    type R;

    fn dict_read_check<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        trace: DepthTrace,
        use_op: &UseOp,
        pair: (&Type, &Type),
    ) -> Result<(), FlowJsException>;

    fn cg_lookup<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        trace: DepthTrace,
        obj_t: Type,
        method_accessible: bool,
        super_t: Type,
        args: CgLookupArgs,
    ) -> Result<Self::R, FlowJsException>;

    fn reposition<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        trace: Option<DepthTrace>,
        loc: ALoc,
        t: Type,
    ) -> Result<Type, FlowJsException>;

    fn mk_react_dro<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        use_op: UseOp,
        dro: &flow_typing_type::type_::ReactDro,
        t: Type,
    ) -> Type;

    fn return_<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        use_op: UseOp,
        trace: DepthTrace,
        t: Type,
    ) -> Result<Self::R, FlowJsException>;

    fn error_type<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        trace: DepthTrace,
        reason: Reason,
    ) -> Result<Self::R, FlowJsException>;

    fn cg_get_prop<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        trace: DepthTrace,
        t: Type,
        args: (
            UseOp,
            Reason,
            Option<i32>,
            (Reason, flow_common::reason::Name),
        ),
    ) -> Result<Self::R, FlowJsException>;

    // When looking up a prop on an object with an indexer we can do a subtyping check to see if
    // the prop exists instead of assuming the indexer catches all non-enumerated props. This
    // allows us to check if e.g. the indexer on a props object may contain the key/ref props.
    // It's not possible to do this in all contexts that GetPropTKit is used, like annotation
    // inference, so we allow this behavior to be disabled by passing None. Note that this will
    // likely introduce inconsistent semantics and is undesirable, but at the time of writing this
    // comment we had no alternative for annotation inference.
    fn key_overlaps_with_indexer()
    -> Option<for<'b> fn(&mut Context<'b>, &FlowJsEnv, &Type, &Type) -> Result<bool, JobError>>;
}

pub mod get_prop_t_kit {
    use std::ops::Deref;

    use dupe::Dupe;
    use dupe::OptionDupedExt;
    use flow_aloc::ALoc;
    use flow_common::reason::Name;
    use flow_common::reason::Reason;
    use flow_common_utils::utils_js::typo_suggestion;
    use flow_data_structure_wrapper::ord_set::FlowOrdSet;
    use flow_data_structure_wrapper::smol_str::FlowSmolStr;
    use flow_typing_context::Context;
    use flow_typing_context::Cx;
    use flow_typing_errors::error_message::EObjectComputedPropertyAccessData;
    use flow_typing_errors::error_message::EPropNotReadableData;
    use flow_typing_errors::error_message::EnumInvalidMemberAccessData;
    use flow_typing_errors::error_message::EnumMemberAccess;
    use flow_typing_errors::error_message::EnumReferenceData;
    use flow_typing_flow_js_env::FlowJsEnv;
    use flow_typing_type::type_;
    use flow_typing_type::type_::DefT;
    use flow_typing_type::type_::DefTInner;
    use flow_typing_type::type_::DepthTrace;
    use flow_typing_type::type_::DictType;
    use flow_typing_type::type_::FieldData;
    use flow_typing_type::type_::GenericTData;
    use flow_typing_type::type_::IndexerFallbackData;
    use flow_typing_type::type_::InstType;
    use flow_typing_type::type_::LookupDefaultResult;
    use flow_typing_type::type_::LookupKind;
    use flow_typing_type::type_::NonstrictReturningData;
    use flow_typing_type::type_::NumberLiteral;
    use flow_typing_type::type_::ObjType;
    use flow_typing_type::type_::PropRef;
    use flow_typing_type::type_::Property;
    use flow_typing_type::type_::PropertyInner;
    use flow_typing_type::type_::PropertySource;
    use flow_typing_type::type_::PropertyType;
    use flow_typing_type::type_::ReactDro;
    use flow_typing_type::type_::Type;
    use flow_typing_type::type_::TypeInner;
    use flow_typing_type::type_::UseOp;
    use flow_typing_type::type_::any_t;
    use flow_typing_type::type_::name_of_propref;
    use flow_typing_type::type_::property;
    use flow_typing_type::type_::union_rep;
    use flow_typing_type::type_::unknown_use;
    use flow_typing_type::type_::void;
    use flow_typing_type::type_util;
    use flow_typing_type::type_util::reason_of_t;

    use super::CgLookupArgs;
    use super::FlowJsException;
    use super::GetPropHelper;
    use super::add_output_with_env;
    use super::enum_proto;
    use super::is_dictionary_exempt;
    use super::is_exception_to_react_dro;
    use super::is_munged_prop_name;
    use super::method_property_for_read;
    use super::tvar_visitors;
    use super::type_of_key_name_with_env;
    use crate::flow_js_utils::type_reference_with_reason_for_error;
    use crate::obj_type;

    pub fn perform_read_prop_action<'cx, F: GetPropHelper>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        trace: &DepthTrace,
        use_op: UseOp,
        propref: &PropRef,
        p: PropertyType,
        ureason: &Reason,
        react_dro: &Option<ReactDro>,
    ) -> Result<F::R, FlowJsException> {
        match property::read_t_of_property_type(&p) {
            Some(t) => {
                let loc = ureason.loc().dupe();
                let t = match react_dro {
                    Some(dro) if !is_exception_to_react_dro(propref) => {
                        F::mk_react_dro(cx, env, use_op.dupe(), dro, t)
                    }
                    _ => t,
                };
                let t = F::reposition(cx, env, Some(*trace), loc, t)?;
                F::return_(cx, env, unknown_use(), *trace, t)
            }
            None => {
                let (reason_prop, prop_name) = match propref {
                    PropRef::Named {
                        reason,
                        name,
                        from_indexed_access: false,
                    } => (reason.dupe(), Some(name.dupe())),
                    PropRef::Named {
                        reason,
                        name: _,
                        from_indexed_access: true,
                    } => (reason.dupe(), None),
                    PropRef::Computed(t) => (reason_of_t(t).dupe(), None),
                };
                let msg = flow_typing_errors::error_message::ErrorMessage::EPropNotReadable(
                    Box::new(EPropNotReadableData {
                        prop_loc: reason_prop.loc().dupe(),
                        prop_name,
                        use_op,
                    }),
                );
                add_output_with_env(cx, env, msg)?;
                F::error_type(cx, env, *trace, ureason.dupe())
            }
        }
    }

    /// Whether a named property access can be served by an indexer whose key type is
    /// `key`. When the engine can run the subtyping test and the key is already resolved,
    /// ask it; otherwise fall back to flowing the key name into the indexer key, which
    /// accepts the match and reports any mismatch as an incompatibility instead.
    fn named_prop_matches_indexer<'cx, F: GetPropHelper>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        trace: &DepthTrace,
        use_op: &UseOp,
        name: &Name,
        reason_op: &Reason,
        key: &Type,
    ) -> Result<bool, FlowJsException> {
        let name_t = type_of_key_name_with_env(env, name.dupe(), reason_op);
        match F::key_overlaps_with_indexer() {
            Some(key_overlaps_with_indexer) if !tvar_visitors::has_unresolved_tvars(cx, key) => {
                Ok(key_overlaps_with_indexer(cx, env, &name_t, key)?)
            }
            _ => {
                F::dict_read_check(cx, env, *trace, use_op, (&name_t, key))?;
                Ok(true)
            }
        }
    }

    fn computed_prop_matches_indexer<'cx, F: GetPropHelper>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        trace: &DepthTrace,
        use_op: &UseOp,
        prop: &Type,
        key: &Type,
    ) -> Result<bool, FlowJsException> {
        match F::key_overlaps_with_indexer() {
            Some(key_overlaps_with_indexer)
                if !tvar_visitors::has_unresolved_tvars(cx, prop)
                    && !tvar_visitors::has_unresolved_tvars(cx, key) =>
            {
                Ok(key_overlaps_with_indexer(cx, env, prop, key)?)
            }
            _ => {
                F::dict_read_check(cx, env, *trace, use_op, (prop, key))?;
                Ok(true)
            }
        }
    }

    pub fn get_instance_prop<'cx, F: GetPropHelper>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        trace: &DepthTrace,
        use_op: &UseOp,
        ignore_dicts: bool,
        inst: &InstType,
        propref: &PropRef,
        reason_op: &Reason,
    ) -> Result<Option<(Property, PropertySource)>, FlowJsException> {
        let dict = &inst.inst_dict;
        let named_prop = match propref {
            PropRef::Named { name, .. } => {
                let own_props = cx.find_props(inst.own_props.dupe());
                let proto_props = cx.find_props(inst.proto_props.dupe());
                own_props
                    .get(name)
                    .duped()
                    .or_else(|| proto_props.get(name).duped())
            }
            PropRef::Computed(_) => None,
        };
        match (&named_prop, propref, dict) {
            (Some(prop), _, _) => Ok(Some((prop.dupe(), PropertySource::PropertyMapProperty))),
            (
                None,
                PropRef::Named {
                    name,
                    from_indexed_access: true,
                    ..
                },
                Some(DictType {
                    key,
                    value,
                    dict_polarity,
                    ..
                }),
            ) if !ignore_dicts => {
                F::dict_read_check(
                    cx,
                    env,
                    *trace,
                    use_op,
                    (&type_of_key_name_with_env(env, name.dupe(), reason_op), key),
                )?;
                Ok(Some((
                    Property::new(PropertyInner::Field(Box::new(FieldData {
                        preferred_def_locs: None,
                        key_loc: None,
                        type_: value.dupe(),
                        polarity: *dict_polarity,
                    }))),
                    PropertySource::IndexerProperty,
                )))
            }
            (
                None,
                PropRef::Named {
                    name,
                    from_indexed_access: false,
                    ..
                },
                Some(DictType {
                    key,
                    value,
                    dict_polarity,
                    ..
                }),
            ) => {
                // Unlike an object indexer, this does not answer the access here: an
                // indexer declared on an earlier `extends` branch must not shadow a
                // property declared on a later one, so the candidate is held back until
                // the whole chain has been searched. See `IndexerFallbackData`.
                if !named_prop_matches_indexer::<F>(cx, env, trace, use_op, name, reason_op, key)? {
                    return Ok(None);
                }
                Ok(Some((
                    Property::new(PropertyInner::Field(Box::new(FieldData {
                        preferred_def_locs: None,
                        key_loc: None,
                        type_: value.dupe(),
                        polarity: *dict_polarity,
                    }))),
                    PropertySource::IndexerProperty,
                )))
            }
            (
                None,
                PropRef::Computed(k),
                Some(DictType {
                    key,
                    value,
                    dict_polarity,
                    ..
                }),
            ) if !ignore_dicts => {
                F::dict_read_check(cx, env, *trace, use_op, (k, key))?;
                Ok(Some((
                    Property::new(PropertyInner::Field(Box::new(FieldData {
                        preferred_def_locs: None,
                        key_loc: None,
                        type_: value.dupe(),
                        polarity: *dict_polarity,
                    }))),
                    PropertySource::IndexerProperty,
                )))
            }
            _ => Ok(None),
        }
    }

    /// Controls which indexer matches may be carried along a prototype-chain lookup.
    #[derive(Clone, Copy)]
    pub enum IndexerFallbackMode {
        /// Do not carry an indexer match.
        Disabled,
        /// Carry indexers matched by property access syntax.
        PropertyAccess,
        /// Carry indexers matched by property or indexed access syntax.
        PropertyAndIndexedAccess,
    }

    /// Like [`get_instance_prop`], but for a lookup that will continue up the prototype
    /// chain. An indexer that matched a named access may be returned separately so that an
    /// inherited declaration can take precedence over it.
    pub fn get_instance_prop_for_lookup<'cx, F: GetPropHelper>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        trace: &DepthTrace,
        use_op: &UseOp,
        ignore_dicts: bool,
        inst: &InstType,
        propref: &PropRef,
        reason_op: &Reason,
        indexer_fallback_mode: IndexerFallbackMode,
        obj: &Type,
    ) -> Result<
        (
            Option<(Property, PropertySource)>,
            Option<Box<IndexerFallbackData>>,
        ),
        FlowJsException,
    > {
        let defer_indexed_access = matches!(
            (indexer_fallback_mode, propref),
            (
                IndexerFallbackMode::PropertyAndIndexedAccess,
                PropRef::Named {
                    from_indexed_access: true,
                    ..
                } | PropRef::Computed(_)
            )
        );
        let mut property = get_instance_prop::<F>(
            cx,
            env,
            trace,
            use_op,
            ignore_dicts || defer_indexed_access,
            inst,
            propref,
            reason_op,
        )?;
        if property.is_none()
            && defer_indexed_access
            && let Some(DictType {
                key,
                value,
                dict_polarity,
                ..
            }) = &inst.inst_dict
        {
            let matches_indexer = match propref {
                PropRef::Named { name, .. } => {
                    named_prop_matches_indexer::<F>(cx, env, trace, use_op, name, reason_op, key)?
                }
                PropRef::Computed(prop) => {
                    computed_prop_matches_indexer::<F>(cx, env, trace, use_op, prop, key)?
                }
            };
            if matches_indexer {
                property = Some((
                    Property::new(PropertyInner::Field(Box::new(FieldData {
                        preferred_def_locs: None,
                        key_loc: None,
                        type_: value.dupe(),
                        polarity: *dict_polarity,
                    }))),
                    PropertySource::IndexerProperty,
                ));
            } else if !ignore_dicts {
                let property = get_instance_prop::<F>(
                    cx, env, trace, use_op, false, inst, propref, reason_op,
                )?;
                return Ok((property, None));
            }
        }

        let Some((property, PropertySource::IndexerProperty)) = property else {
            return Ok((property, None));
        };
        let defer_property_access = matches!(
            (indexer_fallback_mode, propref),
            (
                IndexerFallbackMode::PropertyAccess | IndexerFallbackMode::PropertyAndIndexedAccess,
                PropRef::Named {
                    from_indexed_access: false,
                    ..
                }
            )
        );
        if !defer_indexed_access && !defer_property_access {
            if matches!(
                (indexer_fallback_mode, propref),
                (
                    IndexerFallbackMode::Disabled,
                    PropRef::Named {
                        from_indexed_access: false,
                        ..
                    }
                )
            ) {
                return Ok((None, None));
            }
            return Ok((Some((property, PropertySource::IndexerProperty)), None));
        }

        Ok((
            None,
            Some(Box::new(IndexerFallbackData {
                property,
                obj: obj.dupe(),
                // The access is TS-related when either side is: the interface was declared
                // under TS semantics, or the access itself is written in a TS file.
                suppress_missing_error: inst
                    .strictness_kind
                    .join(cx.type_strictness_kind())
                    .is_typescript_loose(),
            })),
        ))
    }

    pub fn read_instance_prop<'cx, F: GetPropHelper>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        trace: &DepthTrace,
        use_op: &UseOp,
        instance_t: &Type,
        id: Option<i32>,
        method_accessible: bool,
        super_t: Type,
        lookup_kind: LookupKind,
        _hint: &type_::LazyHintT<Cx>,
        _skip_optional: bool,
        inst: &InstType,
        propref: &PropRef,
        reason_op: &Reason,
    ) -> Result<F::R, FlowJsException> {
        let (property, indexer_fallback) = get_instance_prop_for_lookup::<F>(
            cx,
            env,
            trace,
            use_op,
            true,
            inst,
            propref,
            reason_op,
            IndexerFallbackMode::PropertyAndIndexedAccess,
            instance_t,
        )?;
        if let Some(id) = id
            && (property.is_some() || indexer_fallback.is_some())
        {
            cx.test_prop_hit(id);
        }
        if let Some((p, _target_kind)) = property {
            let p = method_property_for_read(method_accessible, p);
            return perform_read_prop_action::<F>(
                cx,
                env,
                trace,
                use_op.dupe(),
                propref,
                property::property_type(&p),
                reason_op,
                &inst.inst_react_dro,
            );
        }
        let super_t = match name_of_propref(propref) {
            Some(name) if is_munged_prop_name(cx, &name) => {
                Type::new(TypeInner::ObjProtoT(reason_of_t(&super_t).dupe()))
            }
            _ => super_t,
        };
        let ids: FlowOrdSet<_> = [inst.own_props.dupe(), inst.proto_props.dupe()]
            .into_iter()
            .collect();
        F::cg_lookup(
            cx,
            env,
            *trace,
            instance_t.dupe(),
            method_accessible,
            super_t,
            CgLookupArgs {
                reason_op: reason_op.dupe(),
                lookup_kind,
                indexer_fallback,
                propref: propref.clone(),
                use_op: use_op.dupe(),
                ids,
            },
        )
    }

    pub fn on_enum_object_t<'cx, F: GetPropHelper>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        trace: &DepthTrace,
        enum_object_t: Type,
        enum_value_t: Type,
        enum_info: &type_::EnumConcreteInfo,
        access: &(UseOp, Reason, Option<i32>, (Reason, Name)),
    ) -> Result<F::R, FlowJsException> {
        let (_, access_reason, _, (prop_reason, member_name)) = access;
        let type_::EnumConcreteInfoInner {
            enum_name,
            members,
            representation_t,
            ..
        } = enum_info.deref();
        let error_invalid_access = |cx: &mut Context<'cx>,
                                    suggestion: Option<FlowSmolStr>|
         -> Result<F::R, FlowJsException> {
            add_output_with_env(
                cx,
                env,
                flow_typing_errors::error_message::ErrorMessage::EEnumError(
                    flow_typing_errors::error_message::EnumErrorKind::EnumInvalidMemberAccess(
                        Box::new(EnumInvalidMemberAccessData {
                            member: EnumMemberAccess::Named {
                                name: member_name.dupe(),
                                suggestion,
                            },
                            member_loc: prop_reason.loc().dupe(),
                            enum_: EnumReferenceData {
                                loc: type_util::ref_loc_of_t(&enum_object_t).dupe(),
                                name: Some(enum_name.dupe()),
                            },
                        }),
                    ),
                ),
            )?;
            F::return_(
                cx,
                env,
                unknown_use(),
                *trace,
                any_t::error(access_reason.dupe()),
            )
        };
        // We guarantee in the parser that enum member names won't start with lowercase
        // "a" through "z", these are reserved for methods.
        let is_valid_member_name =
            |name: &str| -> bool { name.is_empty() || !name.as_bytes()[0].is_ascii_lowercase() };
        match member_name.as_str_opt() {
            Some(name) if is_valid_member_name(name) => {
                if members.contains_key(name) {
                    let enum_value_t = F::reposition(
                        cx,
                        env,
                        Some(*trace),
                        access_reason.loc().dupe(),
                        enum_value_t.dupe(),
                    )?;
                    F::return_(cx, env, unknown_use(), *trace, enum_value_t)
                } else {
                    let keys_owned: Vec<_> = members.keys().map(|k| k.dupe()).collect();
                    let keys: Vec<&flow_data_structure_wrapper::smol_str::FlowSmolStr> =
                        keys_owned.iter().collect();
                    let suggestion = typo_suggestion(&keys, name);
                    error_invalid_access(cx, suggestion)
                }
            }
            _ => {
                let t = enum_proto(
                    cx,
                    env,
                    access_reason.dupe(),
                    enum_object_t.dupe(),
                    enum_value_t.dupe(),
                    representation_t.dupe(),
                );
                F::cg_get_prop(cx, env, *trace, t, access.clone())
            }
        }
    }

    pub fn on_array_length<'cx, F: GetPropHelper>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        trace: &DepthTrace,
        reason: Reason,
        inexact: bool,
        arity: (i32, i32),
        loc: ALoc,
    ) -> Result<F::R, FlowJsException> {
        // Use definition as the reason for the length, as this is
        // the actual location where the length is in fact set.
        let t = type_util::tuple_length(reason, inexact, arity.0, arity.1);
        let t = F::reposition(cx, env, Some(*trace), loc, t)?;
        F::return_(cx, env, unknown_use(), *trace, t)
    }

    // Under no_unchecked_indexed_access=true, indexed access on computed props should include void.
    // However, get_obj_prop might be called in other contexts where union with void doesn't make sense.
    // e.g. indexed access type or write `obj[foo] = ...`
    //
    // never_union_void_on_computed_prop_access is the flag to disable the union void behavior.
    pub fn get_obj_prop<'cx, F: GetPropHelper>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        trace: &DepthTrace,
        use_op: &UseOp,
        skip_optional: bool,
        never_union_void_on_computed_prop_access: bool,
        o: &ObjType,
        propref: &PropRef,
        reason_op: &Reason,
    ) -> Result<Option<(PropertyType, PropertySource)>, FlowJsException> {
        let named_prop = match propref {
            PropRef::Named { name, .. } => cx.get_prop(o.props_tmap.dupe(), name),
            PropRef::Computed(_) => None,
        };
        let no_unchecked_indexed_access = cx.no_unchecked_indexed_access();
        let union_void_if_instructed = |t: Type| -> Type {
            if no_unchecked_indexed_access && !never_union_void_on_computed_prop_access {
                let r = reason_of_t(&t).dupe();
                Type::new(TypeInner::UnionT(
                    r.dupe(),
                    union_rep::make(
                        None,
                        union_rep::UnionKind::UnknownKind,
                        t,
                        void::why(r),
                        vec![].into(),
                    ),
                ))
            } else {
                t
            }
        };
        let dict_t = obj_type::get_dict_opt(&o.flags.obj_kind);
        match (propref, &named_prop, dict_t) {
            (_, Some(prop), _) => {
                // Property exists on this property map
                let field = match property::property_type(prop) {
                    PropertyType::OrdinaryField { type_, polarity } if skip_optional => {
                        match type_.deref() {
                            TypeInner::OptionalT {
                                type_: inner_type, ..
                            } => PropertyType::OrdinaryField {
                                type_: inner_type.dupe(),
                                polarity,
                            },
                            _ => PropertyType::OrdinaryField { type_, polarity },
                        }
                    }
                    field => field,
                };
                Ok(Some((field, PropertySource::PropertyMapProperty)))
            }
            (
                PropRef::Named { name, .. },
                None,
                Some(DictType {
                    key,
                    value,
                    dict_polarity,
                    ..
                }),
            ) if !is_dictionary_exempt(name) => {
                //   Dictionaries match all property reads
                if !named_prop_matches_indexer::<F>(cx, env, trace, use_op, name, reason_op, key)? {
                    return Ok(None);
                }
                let type_ = union_void_if_instructed(value.dupe());
                Ok(Some((
                    PropertyType::OrdinaryField {
                        type_,
                        polarity: *dict_polarity,
                    },
                    PropertySource::IndexerProperty,
                )))
            }
            (
                PropRef::Computed(k),
                None,
                Some(DictType {
                    key,
                    value,
                    dict_polarity,
                    ..
                }),
            ) => {
                F::dict_read_check(cx, env, *trace, use_op, (k, key))?;
                let type_ = union_void_if_instructed(value.dupe());
                Ok(Some((
                    PropertyType::OrdinaryField {
                        type_,
                        polarity: *dict_polarity,
                    },
                    PropertySource::IndexerProperty,
                )))
            }
            _ => Ok(None),
        }
    }

    pub fn read_obj_prop<'cx, F: GetPropHelper>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        trace: &DepthTrace,
        use_op: UseOp,
        from_annot: bool,
        skip_optional: bool,
        o: &ObjType,
        propref: &PropRef,
        reason_obj: Reason,
        reason_op: Reason,
        lookup_info: Option<(i32, LookupDefaultResult)>,
    ) -> Result<F::R, FlowJsException> {
        let l = Type::new(TypeInner::DefT(
            reason_obj.dupe(),
            DefT::new(DefTInner::ObjT(std::rc::Rc::new(o.clone()))),
        ));
        match get_obj_prop::<F>(
            cx,
            env,
            trace,
            &use_op,
            skip_optional,
            from_annot,
            o,
            propref,
            &reason_op,
        )? {
            Some((p, _target_kind)) => {
                if let Some((id, _)) = &lookup_info {
                    cx.test_prop_hit(*id);
                }
                perform_read_prop_action::<F>(
                    cx,
                    env,
                    trace,
                    use_op,
                    propref,
                    p,
                    &reason_op,
                    &o.flags.react_dro,
                )
            }
            None => match propref {
                PropRef::Named {
                    reason: reason_prop,
                    name: _,
                    from_indexed_access: _,
                } => {
                    let lookup_kind = match &lookup_info {
                        Some((id, lookup_default_tout))
                            if obj_type::is_exact(&o.flags.obj_kind) =>
                        {
                            let lookup_default = {
                                let r = reason_op
                                    .dupe()
                                    .replace_desc(flow_common::reason::VirtualReasonDesc::RVoid);
                                Some((
                                    Type::new(TypeInner::DefT(r, DefT::new(DefTInner::VoidT))),
                                    lookup_default_tout.clone(),
                                ))
                            };
                            LookupKind::NonstrictReturning(Box::new(NonstrictReturningData(
                                lookup_default,
                                Some((*id, (reason_prop.dupe(), l.dupe()))),
                            )))
                        }
                        _ => LookupKind::Strict(l.dupe()),
                    };
                    let data = CgLookupArgs {
                        reason_op,
                        lookup_kind,
                        // Objects resolve a named access through their own indexer
                        // immediately, so there is never anything to defer here.
                        indexer_fallback: None,
                        propref: propref.clone(),
                        use_op,
                        ids: [o.props_tmap.dupe()].into_iter().collect(),
                    };
                    F::cg_lookup(cx, env, *trace, l, true, o.proto_t.dupe(), data)
                }
                PropRef::Computed(elem_t) => match elem_t.deref() {
                    TypeInner::OpenT(_) | TypeInner::ImplicitInstantiationTvar(_) => {
                        let loc = type_util::loc_of_t(elem_t).dupe();
                        add_output_with_env(
                                    cx,
                                    env,
                                    flow_typing_errors::error_message::ErrorMessage::EInternal(Box::new((
                                        loc,
                                        flow_typing_errors::error_message::InternalError::PropRefComputedOpen,
                                    ))),
                                )?;
                        F::error_type(cx, env, *trace, reason_op)
                    }
                    TypeInner::GenericT(box GenericTData { bound, .. }) if matches!(bound.deref(), TypeInner::DefT(_, d) if matches!(d.deref(), DefTInner::SingletonStrT { .. })) =>
                    {
                        let loc = type_util::loc_of_t(elem_t).dupe();
                        add_output_with_env(
                                    cx,
                                    env,
                                    flow_typing_errors::error_message::ErrorMessage::EInternal(Box::new((
                                        loc,
                                        flow_typing_errors::error_message::InternalError::PropRefComputedLiteral,
                                    ))),
                                )?;
                        F::error_type(cx, env, *trace, reason_op)
                    }
                    TypeInner::DefT(_, d)
                        if matches!(d.deref(), DefTInner::SingletonStrT { .. }) =>
                    {
                        let loc = type_util::loc_of_t(elem_t).dupe();
                        add_output_with_env(
                                    cx,
                                    env,
                                    flow_typing_errors::error_message::ErrorMessage::EInternal(Box::new((
                                        loc,
                                        flow_typing_errors::error_message::InternalError::PropRefComputedLiteral,
                                    ))),
                                )?;
                        F::error_type(cx, env, *trace, reason_op)
                    }
                    TypeInner::GenericT(box GenericTData { bound, .. }) if matches!(bound.deref(), TypeInner::DefT(_, d) if matches!(d.deref(), DefTInner::SingletonNumT { .. })) =>
                    {
                        let value = match bound.deref() {
                            TypeInner::DefT(_, d) => match d.deref() {
                                DefTInner::SingletonNumT {
                                    value: NumberLiteral(value, _),
                                    ..
                                } => *value,
                                _ => unreachable!(),
                            },
                            _ => unreachable!(),
                        };
                        let reason_prop = reason_of_t(elem_t).dupe();
                        let kind = flow_typing_errors::intermediate_error_types::InvalidObjKey::kind_of_num_value(value);
                        add_output_with_env(
                                    cx,
                                    env,
                                    flow_typing_errors::error_message::ErrorMessage::EObjectComputedPropertyAccess(Box::new(EObjectComputedPropertyAccessData {
                                        object: type_reference_with_reason_for_error(&l, reason_obj.dupe()),
                                        property: type_reference_with_reason_for_error(elem_t, reason_prop),
                                        kind,
                                    })),
                                )?;
                        F::error_type(cx, env, *trace, reason_op)
                    }
                    TypeInner::DefT(_, d)
                        if matches!(d.deref(), DefTInner::SingletonNumT { .. }) =>
                    {
                        let value = match d.deref() {
                            DefTInner::SingletonNumT {
                                value: NumberLiteral(value, _),
                                ..
                            } => *value,
                            _ => unreachable!(),
                        };
                        let reason_prop = reason_of_t(elem_t).dupe();
                        let kind = flow_typing_errors::intermediate_error_types::InvalidObjKey::kind_of_num_value(value);
                        add_output_with_env(
                                    cx,
                                    env,
                                    flow_typing_errors::error_message::ErrorMessage::EObjectComputedPropertyAccess(Box::new(EObjectComputedPropertyAccessData {
                                        object: type_reference_with_reason_for_error(&l, reason_obj.dupe()),
                                        property: type_reference_with_reason_for_error(elem_t, reason_prop),
                                        kind,
                                    })),
                                )?;
                        F::error_type(cx, env, *trace, reason_op)
                    }
                    TypeInner::AnyT(_, src) => {
                        F::return_(cx, env, unknown_use(), *trace, any_t::why(*src, reason_op))
                    }
                    _ => {
                        let reason_prop = reason_of_t(elem_t).dupe();
                        add_output_with_env(
                                    cx,
                                    env,
                                    flow_typing_errors::error_message::ErrorMessage::EObjectComputedPropertyAccess(Box::new(EObjectComputedPropertyAccessData {
                                        object: type_reference_with_reason_for_error(&l, reason_obj.dupe()),
                                        property: type_reference_with_reason_for_error(elem_t, reason_prop),
                                        kind: flow_typing_errors::intermediate_error_types::InvalidObjKey::Other,
                                    })),
                                )?;
                        F::error_type(cx, env, *trace, reason_op)
                    }
                },
            },
        }
    }
}

// ***************
// * ElemT utils *
// ***************

// See docs GetPropsKit.get_obj_prop for explanation of never_union_void_on_computed_prop_access
pub fn array_elem_check<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    write_action: bool,
    never_union_void_on_computed_prop_access: bool,
    tuple: &Type,
    l: &Type,
    use_op: UseOp,
    reason: &Reason,
    reason_tup: &Reason,
    arrtype: &flow_typing_type::type_::ArrType,
) -> Result<(Type, bool, UseOp, Option<flow_typing_type::type_::ReactDro>), FlowJsException> {
    use std::ops::Deref;

    use dupe::Dupe;
    use flow_common::polarity::Polarity;
    use flow_typing_errors::error_message::ErrorMessage;
    use flow_typing_type::type_::ArrType;
    use flow_typing_type::type_::ArrayATData;
    use flow_typing_type::type_::DefTInner;
    use flow_typing_type::type_::ReactDro;
    use flow_typing_type::type_::TupleATData;
    use flow_typing_type::type_::TupleElement;
    use flow_typing_type::type_::VirtualFrameUseOp;
    use flow_typing_type::type_::VirtualUseOp;
    use flow_typing_type::type_::any_t;
    use flow_typing_type::type_::union_rep;
    use flow_typing_type::type_::void;
    use flow_typing_type::type_util::reason_of_t;

    let union_void_if_instructed = |elem_t: Type| -> Type {
        if cx.no_unchecked_indexed_access() && !never_union_void_on_computed_prop_access {
            let r = reason_of_t(&elem_t).dupe();
            let void_t = void::why(r.dupe());
            Type::new(TypeInner::UnionT(
                r,
                union_rep::make(
                    None,
                    union_rep::UnionKind::UnknownKind,
                    elem_t,
                    void_t,
                    vec![].into(),
                ),
            ))
        } else {
            elem_t
        }
    };

    let (elem_t, elements, rest, is_index_restricted, is_tuple, tuple_is_inexact, react_dro): (
        Type,
        Option<std::rc::Rc<[TupleElement]>>,
        Option<TupleElement>,
        bool,
        bool,
        bool,
        Option<ReactDro>,
    ) = match arrtype {
        ArrType::ArrayAT(box ArrayATData {
            elem_t,
            tuple_view,
            react_dro,
            ..
        }) => {
            let elements = tuple_view.as_ref().map(|tv| tv.elements.dupe());
            let elem_t = union_void_if_instructed(elem_t.dupe());
            (
                elem_t,
                elements,
                None,
                false,
                false,
                false,
                react_dro.clone(),
            )
        }
        ArrType::TupleAT(box TupleATData {
            elem_t,
            elements,
            arity: _,
            inexact,
            rest,
            react_dro,
            ..
        }) => {
            let elem_t = union_void_if_instructed(elem_t.dupe());
            // An index past the fixed elements accesses the rest element, which may not exist.
            let rest = rest.as_ref().map(|rest| TupleElement {
                t: if write_action {
                    rest.t.dupe()
                } else {
                    union_void_if_instructed(rest.t.dupe())
                },
                ..rest.clone()
            });
            (
                elem_t,
                Some(elements.dupe()),
                rest,
                true,
                true,
                *inexact,
                react_dro.clone(),
            )
        }
        ArrType::ROArrayAT(box ROArrayATData {
            elem_t, react_dro, ..
        }) => {
            let elem_t = union_void_if_instructed(elem_t.dupe());
            (elem_t, None, None, true, false, false, react_dro.clone())
        }
    };

    let (can_write_tuple, value, use_op): (bool, Type, UseOp) = match l.deref() {
        TypeInner::DefT(index_reason, def_t) => {
            if let DefTInner::SingletonNumT { value, .. } = def_t.deref() {
                let float_value = value.0;
                match &elements {
                    None => (false, elem_t.dupe(), use_op),
                    Some(elements) => {
                        let index_string =
                            flow_common::js_number::ecma_string_of_float(float_value);

                        match index_string.parse::<i64>() {
                            Ok(index) => {
                                let value_opt = usize::try_from(index)
                                    .ok()
                                    .and_then(|i| elements.get(i).or(rest.as_ref()));

                                match value_opt {
                                    Some(TupleElement {
                                        t,
                                        polarity,
                                        optional,
                                        name,
                                        reason: _,
                                    }) => {
                                        if write_action
                                            && !Polarity::compat(*polarity, Polarity::Negative)
                                        {
                                            add_output_with_env(
                                                cx,
                                                env,
                                                ErrorMessage::ETupleElementNotWritable(Box::new(
                                                    ETupleElementNotWritableData {
                                                        use_op: use_op.dupe(),
                                                        loc: reason.loc().dupe(),
                                                        index_def_loc: reason.def_loc().dupe(),
                                                        index: index as i32,
                                                        name: name.dupe(),
                                                    },
                                                )),
                                            )?;
                                        } else if !write_action
                                            && !Polarity::compat(*polarity, Polarity::Positive)
                                        {
                                            add_output_with_env(
                                                cx,
                                                env,
                                                ErrorMessage::ETupleElementNotReadable(Box::new(
                                                    ETupleElementNotReadableData {
                                                        use_op: use_op.dupe(),
                                                        loc: reason.loc().dupe(),
                                                        index_def_loc: reason.def_loc().dupe(),
                                                        index: index as i32,
                                                        name: name.dupe(),
                                                    },
                                                )),
                                            )?;
                                        }

                                        let (t, use_op) = if write_action {
                                            // We don't allowing writing `undefined` to optional tuple elements.
                                            // User can add `| void` to the element type if they want this behavior.
                                            let t = if *optional {
                                                if let TypeInner::OptionalT { type_, .. } =
                                                    t.deref()
                                                {
                                                    type_.dupe()
                                                } else {
                                                    t.dupe()
                                                }
                                            } else {
                                                t.dupe()
                                            };
                                            (
                                                t,
                                                VirtualUseOp::Frame(
                                                    std::sync::Arc::new(
                                                        VirtualFrameUseOp::TupleAssignment,
                                                    ),
                                                    std::sync::Arc::new(use_op),
                                                ),
                                            )
                                        } else {
                                            (t.dupe(), use_op)
                                        };
                                        (true, t, use_op)
                                    }
                                    None => {
                                        if is_tuple {
                                            add_output_with_env(
                                                cx,
                                                env,
                                                ErrorMessage::ETupleOutOfBounds(Box::new(
                                                    ETupleOutOfBoundsData {
                                                        use_op: use_op.dupe(),
                                                        loc: reason.loc().dupe(),
                                                        tuple: type_reference_with_reason_for_error(
                                                            tuple,
                                                            reason_tup.dupe(),
                                                        ),
                                                        inexact: tuple_is_inexact,
                                                        length: elements.len() as i32,
                                                        index: index_string.clone().into(),
                                                    },
                                                )),
                                            )?;
                                            let error_reason = Reason::new(
                                                VirtualReasonDesc::RTupleOutOfBoundsAccess(
                                                    index as i32,
                                                ),
                                                reason.loc().dupe(),
                                            );
                                            (true, any_t::error(error_reason), use_op)
                                        } else {
                                            (true, elem_t.dupe(), use_op)
                                        }
                                    }
                                }
                            }
                            Err(_) => {
                                // not an integer index
                                if is_tuple {
                                    add_output_with_env(
                                        cx,
                                        env,
                                        ErrorMessage::ETupleNonIntegerIndex(Box::new(
                                            ETupleNonIntegerIndexData {
                                                use_op: use_op.dupe(),
                                                loc: index_reason.loc().dupe(),
                                                index_def_loc: index_reason.def_loc().dupe(),
                                                index: index_string.into(),
                                            },
                                        )),
                                    )?;
                                    (true, any_t::error(reason.dupe()), use_op)
                                } else {
                                    (true, elem_t.dupe(), use_op)
                                }
                            }
                        }
                    }
                }
            } else {
                (false, elem_t.dupe(), use_op)
            }
        }
        _ => (false, elem_t.dupe(), use_op),
    };

    if let Some(dro) = &react_dro {
        if write_action {
            add_output_with_env(
                cx,
                env,
                ErrorMessage::EROArrayWrite(
                    reason.loc().dupe(),
                    VirtualUseOp::Frame(
                        std::sync::Arc::new(VirtualFrameUseOp::ReactDeepReadOnly(Box::new((
                            dro.0.dupe(),
                            dro.1.clone(),
                        )))),
                        std::sync::Arc::new(use_op.dupe()),
                    ),
                ),
            )?;
        }
    }

    if is_index_restricted && !can_write_tuple && write_action {
        let error = match &elements {
            Some(_) => ErrorMessage::ETupleUnsafeWrite {
                loc: reason.loc().dupe(),
                use_op: use_op.dupe(),
            },
            None => ErrorMessage::EROArrayWrite(reason.loc().dupe(), use_op.dupe()),
        };
        add_output_with_env(cx, env, error)?;
    }

    Ok((value, is_tuple, use_op, react_dro))
}

pub fn propref_for_elem_t<'cx>(
    cx: &mut Context<'cx>,
    l: &Type,
) -> flow_typing_type::type_::PropRef {
    propref_for_elem_t_with_env(cx, &FlowJsEnv::entry(), l)
}

pub fn propref_for_elem_t_with_env<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    l: &Type,
) -> flow_typing_type::type_::PropRef {
    use flow_common::js_number::ecma_string_of_float;
    use flow_common::js_number::is_float_safe_integer;
    use flow_typing_type::type_::DefTInner;
    use flow_typing_type::type_::PropRef;
    use flow_typing_type::type_util::mk_named_prop;

    match l.deref() {
        TypeInner::NominalT {
            reason,
            nominal_type,
        } => {
            if let Some(upper_t) = &nominal_type.upper_t {
                if let TypeInner::DefT(_, def) = upper_t.deref() {
                    if let DefTInner::SingletonStrT { value: name, .. } = def.deref() {
                        update_lit_type_from_annot(cx, env, l);
                        let name = Name::new(name.dupe());
                        let reason = reason
                            .dupe()
                            .replace_desc(VirtualReasonDesc::RProperty(Some(name.dupe())));
                        return mk_named_prop(reason, true, name);
                    }
                    if let DefTInner::SingletonNumT { value, .. } = def.deref() {
                        if is_float_safe_integer(value.0) {
                            update_lit_type_from_annot(cx, env, l);
                            let reason =
                                reason
                                    .dupe()
                                    .replace_desc(VirtualReasonDesc::RProperty(Some(Name::new(
                                        value.1.dupe(),
                                    ))));
                            let name = Name::new(ecma_string_of_float(value.0));
                            return mk_named_prop(reason, true, name);
                        }
                    }
                    if let DefTInner::UniqueSymbolT(data) = def.deref() {
                        let name = Name::symbol(data.symbol.dupe());
                        let reason = reason
                            .dupe()
                            .replace_desc(VirtualReasonDesc::RProperty(Some(name.dupe())));
                        return mk_named_prop(reason, true, name);
                    }
                }
            }
            PropRef::Computed(l.dupe())
        }
        TypeInner::GenericT(box GenericTData { bound, reason, .. }) => {
            if let TypeInner::DefT(_, def) = bound.deref() {
                if let DefTInner::SingletonStrT { value: name, .. } = def.deref() {
                    update_lit_type_from_annot(cx, env, l);
                    let name = Name::new(name.dupe());
                    let reason = reason
                        .dupe()
                        .replace_desc(VirtualReasonDesc::RProperty(Some(name.dupe())));
                    return mk_named_prop(reason, true, name);
                }
                if let DefTInner::SingletonNumT { value, .. } = def.deref() {
                    if is_float_safe_integer(value.0) {
                        update_lit_type_from_annot(cx, env, l);
                        let reason =
                            reason
                                .dupe()
                                .replace_desc(VirtualReasonDesc::RProperty(Some(Name::new(
                                    value.1.dupe(),
                                ))));
                        let name = Name::new(ecma_string_of_float(value.0));
                        return mk_named_prop(reason, true, name);
                    }
                }
                if let DefTInner::UniqueSymbolT(data) = def.deref() {
                    let name = Name::symbol(data.symbol.dupe());
                    let reason = reason
                        .dupe()
                        .replace_desc(VirtualReasonDesc::RProperty(Some(name.dupe())));
                    return mk_named_prop(reason, true, name);
                }
            }
            PropRef::Computed(l.dupe())
        }
        TypeInner::DefT(reason, def) => {
            if let DefTInner::SingletonStrT { value: name, .. } = def.deref() {
                update_lit_type_from_annot(cx, env, l);
                let name = Name::new(name.dupe());
                let reason = reason
                    .dupe()
                    .replace_desc(VirtualReasonDesc::RProperty(Some(name.dupe())));
                return mk_named_prop(reason, true, name);
            }
            if let DefTInner::UniqueSymbolT(data) = def.deref() {
                let name = Name::symbol(data.symbol.dupe());
                let reason = reason
                    .dupe()
                    .replace_desc(VirtualReasonDesc::RProperty(Some(name.dupe())));
                return mk_named_prop(reason, true, name);
            }
            if let DefTInner::SingletonNumT { value, .. } = def.deref() {
                if is_float_safe_integer(value.0) {
                    update_lit_type_from_annot(cx, env, l);
                    let reason = reason
                        .dupe()
                        .replace_desc(VirtualReasonDesc::RProperty(Some(Name::new(
                            value.1.dupe(),
                        ))));
                    let name = Name::new(ecma_string_of_float(value.0));
                    return mk_named_prop(reason, true, name);
                }
            }
            PropRef::Computed(l.dupe())
        }
        _ => PropRef::Computed(l.dupe()),
    }
}

pub fn keylist_of_props(
    env: &FlowJsEnv,
    props: &flow_typing_type::type_::properties::PropertiesMap,
    reason_op: &Reason,
    include_symbols: bool,
) -> Vec<Type> {
    use flow_typing_type::type_::DefT;
    use flow_typing_type::type_::DefTInner;

    let mut acc = Vec::new();
    for (name, _) in props.iter() {
        // A symbol-typed key has no string spelling. Type-level `keyof`
        // yields it as its `unique symbol` type (matched later by nominal
        // identity), but runtime enumeration (`Object.keys`/`Object.entries`)
        // omits symbol keys, matching JavaScript.
        let Name::Str(name) = name else {
            if include_symbols {
                acc.push(type_of_key_name_with_env(env, name.dupe(), reason_op));
            }
            continue;
        };
        let name = name.dupe();
        let reason = reason_op
            .dupe()
            .replace_desc_new(VirtualReasonDesc::RStringLit(name.dupe()));
        acc.push(Type::new(TypeInner::DefT(
            reason,
            DefT::new(DefTInner::SingletonStrT {
                from_annot: true,
                value: name,
            }),
        )));
    }
    acc
}

type KeySources = (Vec<properties::Id>, Vec<Type>);

fn empty_key_sources() -> KeySources {
    (Vec::new(), Vec::new())
}

fn append_key_sources(mut left: KeySources, right: KeySources) -> KeySources {
    left.0.extend(right.0);
    left.1.extend(right.1);
    left
}

fn instance_own_key_sources(inst: &InstType) -> KeySources {
    let dict_keys = inst
        .inst_dict
        .as_ref()
        .map(|dict| vec![dict.key.dupe()])
        .unwrap_or_default();
    (vec![inst.own_props.dupe()], dict_keys)
}

fn ts_interface_key_sources<E>(
    concretize: &mut dyn FnMut(&Reason, &Type) -> Result<Vec<Type>, E>,
    instance: &InstanceT,
) -> Result<KeySources, E> {
    fn collect_type<E>(
        concretize: &mut dyn FnMut(&Reason, &Type) -> Result<Vec<Type>, E>,
        seen: &mut BTreeSet<ALocId>,
        t: &Type,
    ) -> Result<KeySources, E> {
        use flow_typing_type::type_util::reason_of_t;

        concretize(reason_of_t(t), t)?
            .into_iter()
            .try_fold(empty_key_sources(), |acc, t| {
                Ok(append_key_sources(
                    acc,
                    collect_concrete(concretize, seen, &t)?,
                ))
            })
    }

    fn collect_concrete<E>(
        concretize: &mut dyn FnMut(&Reason, &Type) -> Result<Vec<Type>, E>,
        seen: &mut BTreeSet<ALocId>,
        t: &Type,
    ) -> Result<KeySources, E> {
        match t.deref() {
            TypeInner::DefT(_, def_t) => match def_t.deref() {
                DefTInner::InstanceT(instance) => collect_instance(concretize, seen, instance),
                DefTInner::ClassT(t) => collect_type(concretize, seen, t),
                _ => Ok(empty_key_sources()),
            },
            TypeInner::ThisInstanceT(box ThisInstanceTData { instance, .. }) => {
                collect_instance(concretize, seen, instance)
            }
            TypeInner::IntersectionT(_, rep) => {
                rep.members_iter().try_fold(empty_key_sources(), |acc, t| {
                    Ok(append_key_sources(acc, collect_type(concretize, seen, t)?))
                })
            }
            _ => Ok(empty_key_sources()),
        }
    }

    fn collect_instance<E>(
        concretize: &mut dyn FnMut(&Reason, &Type) -> Result<Vec<Type>, E>,
        seen: &mut BTreeSet<ALocId>,
        instance: &InstanceT,
    ) -> Result<KeySources, E> {
        if !matches!(instance.inst.inst_kind, InstanceKind::InterfaceKind { .. }) {
            return Ok(empty_key_sources());
        }

        if !seen.insert(instance.inst.class_id.dupe()) {
            return Ok(empty_key_sources());
        }

        let own_sources = instance_own_key_sources(&instance.inst);
        let super_sources = collect_type(concretize, seen, &instance.super_)?;
        Ok(append_key_sources(own_sources, super_sources))
    }

    let mut seen = BTreeSet::new();
    collect_instance(concretize, &mut seen, instance)
}

/// Returns property maps and dictionary key types that contribute to `keyof`/`$Keys`
/// for an instance type.
///
/// TS `keyof` sees inherited members, Flow `$Keys` sees only own ones. The choice is
/// keyed on where the interface was *declared*, not on the file being checked: `keyof`
/// resolves lazily, so a `.d.ts` interface is almost always keyed from a `.js` consumer.
pub fn key_sources_of_instance_t<E>(
    mut concretize: impl FnMut(&Reason, &Type) -> Result<Vec<Type>, E>,
    instance: &InstanceT,
) -> Result<KeySources, E> {
    if instance.inst.strictness_kind.is_typescript_loose()
        && matches!(instance.inst.inst_kind, InstanceKind::InterfaceKind { .. })
    {
        ts_interface_key_sources(&mut concretize, instance)
    } else {
        Ok(instance_own_key_sources(&instance.inst))
    }
}

pub fn keylist_of_prop_ids(
    cx: &mut Context<'_>,
    env: &FlowJsEnv,
    prop_ids: &[properties::Id],
    reason_op: &Reason,
    include_symbols: bool,
) -> Vec<Type> {
    prop_ids
        .iter()
        .flat_map(|id| keylist_of_props(env, &cx.find_props(id.dupe()), reason_op, include_symbols))
        .collect()
}

pub fn objt_to_obj_rest<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    props_tmap: flow_typing_type::type_::properties::Id,
    reachable_targs: Option<Rc<[(Type, flow_common::polarity::Polarity)]>>,
    obj_kind: flow_typing_type::type_::ObjKind,
    reason_op: &Reason,
    reason_obj: &Reason,
    xs: &[flow_data_structure_wrapper::smol_str::FlowSmolStr],
    literal: bool,
) -> Result<Type, FlowJsException> {
    use flow_common::polarity::Polarity;
    use flow_common::reason::Name;
    use flow_typing_errors::error_message::ErrorMessage;
    use flow_typing_type::type_::FieldData;
    use flow_typing_type::type_::Property;
    use flow_typing_type::type_::PropertyInner;
    use flow_typing_type::type_::VirtualRootUseOp;
    use flow_typing_type::type_::VirtualUseOp;
    use flow_typing_type::type_util::reason_of_t;

    let mut props = cx.find_props(props_tmap);
    for x in xs {
        props.remove(&Name::new(x.dupe()));
    }
    let use_op: UseOp = VirtualUseOp::Op(std::sync::Arc::new(VirtualRootUseOp::ObjectRest {
        op: reason_op
            .dupe()
            .replace_desc(reason_obj.desc(true).cloned()),
    }));

    let props = {
        let mut new_props = BTreeMap::new();
        for (name, p) in props.iter() {
            let new_prop = match p.deref() {
                PropertyInner::Field(fd) => {
                    if !Polarity::compat(fd.polarity, Polarity::Positive) {
                        add_output_with_env(
                            cx,
                            env,
                            ErrorMessage::EPropNotReadable(Box::new(EPropNotReadableData {
                                prop_loc: reason_of_t(&fd.type_).loc().dupe(),
                                prop_name: Some(name.dupe()),
                                use_op: use_op.dupe(),
                            })),
                        )?;
                    }
                    Property::new(PropertyInner::Field(Box::new(FieldData {
                        preferred_def_locs: fd.preferred_def_locs.clone(),
                        key_loc: fd.key_loc.dupe(),
                        type_: fd.type_.dupe(),
                        polarity: Polarity::Neutral,
                    })))
                }
                PropertyInner::Set { key_loc: _, type_ } => {
                    add_output_with_env(
                        cx,
                        env,
                        ErrorMessage::EPropNotReadable(Box::new(EPropNotReadableData {
                            prop_loc: reason_of_t(type_).loc().dupe(),
                            prop_name: Some(name.dupe()),
                            use_op: use_op.dupe(),
                        })),
                    )?;
                    p.dupe()
                }
                _ => p.dupe(),
            };
            new_props.insert(name.dupe(), new_prop);
        }
        flow_typing_type::type_::properties::PropertiesMap::from(new_props)
    };

    let proto = Type::new(TypeInner::ObjProtoT(reason_op.dupe()));
    Ok(crate::obj_type::mk_with_proto(
        cx,
        reason_op.dupe(),
        obj_kind,
        reachable_targs,
        None,
        Some(props),
        None,
        proto,
        literal,
    ))
}

// $Values

pub fn get_values_type_of_obj_t<'cx>(
    cx: &mut Context<'cx>,
    o: &flow_typing_type::type_::ObjType,
    reason: Reason,
) -> Type {
    use flow_typing_type::type_::ObjKind;
    use flow_typing_type::type_::property;
    use flow_typing_type::type_util::union_of_ts;
    use flow_typing_visitors::type_mapper::union_flatten;

    let flags = &o.flags;
    let tmap = o.props_tmap.dupe();
    // Find all of the props.
    let props = cx.find_props(tmap);

    // Get the read type for all readable properties and discard the rest.
    let mut ts: Vec<Type> = Vec::new();
    for (_, prop) in props.iter() {
        if let Some(t) = property::read_t(prop) {
            ts.push(t);
        }
    }
    // If the object has a dictionary value then add that to our types.
    if let ObjKind::Indexed(dict) = &flags.obj_kind {
        ts.push(dict.value.dupe());
    }
    // Create a union type from all our selected types.
    let ts = union_flatten(cx, ts);
    union_of_ts(reason, ts, None)
}

pub fn get_values_type_of_instance_t<'cx>(
    cx: &mut Context<'cx>,
    own_props: flow_typing_type::type_::properties::Id,
    dict: Option<&flow_typing_type::type_::DictType>,
    reason: Reason,
) -> Type {
    use flow_common::polarity::Polarity;
    use flow_typing_type::type_::property;
    use flow_typing_type::type_util::union_of_ts;
    use flow_typing_visitors::type_mapper::union_flatten;

    // Find all of the props.
    let props = cx.find_props(own_props);
    // Get the read type for all readable properties and discard the rest.
    let mut ts: Vec<Type> = Vec::new();
    for (_, prop) in props.iter() {
        if let Some(t) = property::read_t(prop) {
            ts.push(t);
        }
    }
    if let Some(d) = dict {
        if Polarity::compat(d.dict_polarity, Polarity::Positive) {
            ts.push(d.value.dupe());
        }
    }
    // Create a union type from all our selected types.
    let ts = union_flatten(cx, ts);
    union_of_ts(reason, ts, None)
}

pub fn any_mod_src_keep_placeholder(
    new_src: flow_typing_type::type_::AnySource,
    src: &flow_typing_type::type_::AnySource,
) -> flow_typing_type::type_::AnySource {
    use flow_typing_type::type_::AnySource;
    match src {
        AnySource::Placeholder => AnySource::Placeholder,
        _ => new_src,
    }
}

pub fn unary_negate_lit(
    annot_loc: ALoc,
    reason: Reason,
    value_raw: (f64, FlowSmolStr),
) -> (Reason, (f64, FlowSmolStr)) {
    let reason = reason
        .reposition(annot_loc.dupe())
        .annotate(annot_loc.dupe());
    let (value, raw) = flow_parser::ast_utils::negate_number_literal(value_raw);
    let reason = reason.update_desc(|d| {
        use flow_common::reason::VirtualReasonDesc::*;
        match &d {
            Some(RNumberLit(_)) => Some(RNumberLit(raw.dupe())),
            _ => d,
        }
    });
    (reason, (value, raw))
}

pub fn unary_negate_bigint_lit(
    annot_loc: ALoc,
    reason: Reason,
    value_raw: (Option<i64>, FlowSmolStr),
) -> (Reason, (Option<i64>, FlowSmolStr)) {
    let reason = reason
        .reposition(annot_loc.dupe())
        .annotate(annot_loc.dupe());
    let (value, raw) = flow_parser::ast_utils::negate_bigint_literal(value_raw);
    let reason = reason.update_desc(|d| {
        use flow_common::reason::VirtualReasonDesc::*;
        match &d {
            Some(RBigIntLit(_)) => Some(RBigIntLit(raw.dupe())),
            _ => d,
        }
    });
    (reason, (value, raw))
}

fn arithmetic_operand_data(t: &Type) -> EArithmeticOperandData<ALoc> {
    let operand = type_reference_for_error(t);
    EArithmeticOperandData {
        loc: operand.loc.dupe(),
        operand,
    }
}

fn arithmetic_type_reference_for_error(t: &Type) -> ErrorTypeReferenceData<ALoc> {
    let reason = reason_of_t(t);
    let type_desc = if matches!(reason.desc(true), Some(VirtualReasonDesc::RNullOrVoid)) {
        TypeOrTypeDescT::TypeDesc(ErrorTypeDesc::ReasonDesc(VirtualReasonDesc::RNullOrVoid))
    } else {
        type_or_type_desc_for_error(t)
    };
    ErrorTypeReferenceData {
        loc: reason.loc().dupe(),
        reference_loc: type_util::ref_loc_of_t(t).dupe(),
        type_desc,
    }
}

fn arithmetic_operand_error(t: &Type) -> ErrorMessage<ALoc> {
    ErrorMessage::EArithmeticOperand(Box::new(arithmetic_operand_data(t)))
}

pub fn flow_unary_arith<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    l: &Type,
    reason: Reason,
    kind: UnaryArithKind,
) -> Result<Type, FlowJsException> {
    use flow_typing_type::type_::DefT;
    use flow_typing_type::type_::DefTInner;
    use flow_typing_type::type_::UnaryArithKind::*;
    use flow_typing_type::type_::any_t;
    use flow_typing_type::type_::bigint_module_t;
    use flow_typing_type::type_::num_module_t;

    match (kind, l.deref()) {
        (Minus, TypeInner::DefT(lreason, def_t)) => match def_t.deref() {
            DefTInner::SingletonNumT { from_annot, value } => {
                let annot_loc = reason.loc().dupe();
                let (new_reason, (new_val, new_raw)) =
                    unary_negate_lit(annot_loc, lreason.dupe(), (value.0, value.1.dupe()));
                Ok(Type::new(TypeInner::DefT(
                    new_reason,
                    DefT::new(DefTInner::SingletonNumT {
                        from_annot: *from_annot,
                        value: flow_typing_type::type_::NumberLiteral(new_val, new_raw),
                    }),
                )))
            }
            DefTInner::NumGeneralT { .. } => Ok(l.dupe()),
            DefTInner::SingletonBigIntT { from_annot, value } => {
                let annot_loc = reason.loc().dupe();
                let (new_reason, (new_val, new_raw)) =
                    unary_negate_bigint_lit(annot_loc, lreason.dupe(), (value.0, value.1.dupe()));
                Ok(Type::new(TypeInner::DefT(
                    new_reason,
                    DefT::new(DefTInner::SingletonBigIntT {
                        from_annot: *from_annot,
                        value: flow_typing_type::type_::BigIntLiteral(new_val, new_raw),
                    }),
                )))
            }
            DefTInner::BigIntGeneralT { .. } => Ok(l.dupe()),
            _ => {
                add_output_with_env(cx, env, arithmetic_operand_error(l))?;
                Ok(any_t::error(reason))
            }
        },
        (Plus, TypeInner::DefT(reason_bigint, def_t))
            if matches!(def_t.deref(), DefTInner::BigIntGeneralT { .. }) =>
        {
            add_output_with_env(
                cx,
                env,
                ErrorMessage::EBigIntNumCoerce(Box::new(type_reference_for_error(l))),
            )?;
            Ok(any_t::error(reason))
        }
        (Plus, _) => Ok(num_module_t::why(reason)),
        (BitNot, TypeInner::DefT(_, def_t))
            if matches!(
                def_t.deref(),
                DefTInner::NumGeneralT { .. } | DefTInner::SingletonNumT { .. }
            ) =>
        {
            Ok(num_module_t::why(reason))
        }
        (BitNot, TypeInner::DefT(_, def_t))
            if matches!(
                def_t.deref(),
                DefTInner::BigIntGeneralT { .. } | DefTInner::SingletonBigIntT { .. }
            ) =>
        {
            Ok(bigint_module_t::why(reason))
        }
        (Update, TypeInner::DefT(_, def_t))
            if matches!(
                def_t.deref(),
                DefTInner::NumGeneralT { .. } | DefTInner::SingletonNumT { .. }
            ) =>
        {
            Ok(num_module_t::why(reason))
        }
        (Update, TypeInner::DefT(_, def_t))
            if matches!(
                def_t.deref(),
                DefTInner::BigIntGeneralT { .. } | DefTInner::SingletonBigIntT { .. }
            ) =>
        {
            Ok(bigint_module_t::why(reason))
        }
        (_, TypeInner::AnyT(_, src)) => {
            let src = any_mod_src_keep_placeholder(AnySource::Untyped, src);
            Ok(any_t::why(src, reason))
        }
        (_, _) => {
            add_output_with_env(cx, env, arithmetic_operand_error(l))?;
            Ok(any_t::error(reason))
        }
    }
}

pub fn flow_arith<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    reason: Reason,
    l: &Type,
    r: &Type,
    kind: ArithKind,
) -> Result<Type, FlowJsException> {
    use flow_typing_type::type_::DefTInner;
    use flow_typing_type::type_::any_t;
    use flow_typing_type::type_::arith_kind::ArithKindInner;
    use flow_typing_type::type_::bigint_module_t;
    use flow_typing_type::type_::empty_t;
    use flow_typing_type::type_::num_module_t;
    use flow_typing_type::type_::str_module_t;

    let op = kind.1;

    match (l.deref(), r.deref()) {
        (TypeInner::AnyT(_, src), _) | (_, TypeInner::AnyT(_, src)) => Ok(any_t::why(*src, reason)),
        // empty <> _ , _ <> empty
        (TypeInner::DefT(_, def_t), _) if matches!(def_t.deref(), DefTInner::EmptyT) => {
            Ok(empty_t::why(reason))
        }
        (_, TypeInner::DefT(_, def_t)) if matches!(def_t.deref(), DefTInner::EmptyT) => {
            Ok(empty_t::why(reason))
        }
        // num <> num
        (TypeInner::DefT(_, l_def), TypeInner::DefT(_, r_def))
            if matches!(
                l_def.deref(),
                DefTInner::NumGeneralT { .. } | DefTInner::SingletonNumT { .. }
            ) && matches!(
                r_def.deref(),
                DefTInner::NumGeneralT { .. } | DefTInner::SingletonNumT { .. }
            ) =>
        {
            Ok(num_module_t::why(reason))
        }
        (TypeInner::DefT(_, def_t), _)
            if op == ArithKindInner::RShift3
                && matches!(
                    def_t.deref(),
                    DefTInner::BigIntGeneralT { .. } | DefTInner::SingletonBigIntT { .. }
                ) =>
        {
            add_output_with_env(
                cx,
                env,
                ErrorMessage::EBigIntRShift3(Box::new(type_reference_for_error(l))),
            )?;
            Ok(any_t::error(reason))
        }
        (TypeInner::DefT(_, l_def), TypeInner::DefT(_, r_def))
            if matches!(
                l_def.deref(),
                DefTInner::BigIntGeneralT { .. } | DefTInner::SingletonBigIntT { .. }
            ) && matches!(
                r_def.deref(),
                DefTInner::BigIntGeneralT { .. } | DefTInner::SingletonBigIntT { .. }
            ) =>
        {
            Ok(bigint_module_t::why(reason))
        }
        (TypeInner::DefT(_, l_def), TypeInner::DefT(_, r_def))
            if op == ArithKindInner::Plus
                && ((matches!(
                    l_def.deref(),
                    DefTInner::StrGeneralT { .. } | DefTInner::SingletonStrT { .. }
                ) && matches!(
                    r_def.deref(),
                    DefTInner::StrGeneralT { .. } | DefTInner::SingletonStrT { .. }
                )) || (matches!(
                    l_def.deref(),
                    DefTInner::StrGeneralT { .. } | DefTInner::SingletonStrT { .. }
                ) && matches!(
                    r_def.deref(),
                    DefTInner::NumGeneralT { .. } | DefTInner::SingletonNumT { .. }
                )) || (matches!(
                    l_def.deref(),
                    DefTInner::NumGeneralT { .. } | DefTInner::SingletonNumT { .. }
                ) && matches!(
                    r_def.deref(),
                    DefTInner::StrGeneralT { .. } | DefTInner::SingletonStrT { .. }
                ))) =>
        {
            Ok(str_module_t::why(reason))
        }
        (TypeInner::TemplateLiteralT { .. }, TypeInner::TemplateLiteralT { .. })
            if op == ArithKindInner::Plus =>
        {
            Ok(str_module_t::why(reason))
        }
        (TypeInner::TemplateLiteralT { .. }, TypeInner::DefT(_, r_def))
            if op == ArithKindInner::Plus
                && matches!(
                    r_def.deref(),
                    DefTInner::StrGeneralT { .. }
                        | DefTInner::SingletonStrT { .. }
                        | DefTInner::NumGeneralT { .. }
                        | DefTInner::SingletonNumT { .. }
                ) =>
        {
            Ok(str_module_t::why(reason))
        }
        (TypeInner::DefT(_, l_def), TypeInner::TemplateLiteralT { .. })
            if op == ArithKindInner::Plus
                && matches!(
                    l_def.deref(),
                    DefTInner::StrGeneralT { .. }
                        | DefTInner::SingletonStrT { .. }
                        | DefTInner::NumGeneralT { .. }
                        | DefTInner::SingletonNumT { .. }
                ) =>
        {
            Ok(str_module_t::why(reason))
        }
        _ => {
            add_output_with_env(
                cx,
                env,
                ErrorMessage::EInvalidBinaryArith(Box::new(EInvalidBinaryArithData {
                    loc: reason.loc().dupe(),
                    left: arithmetic_type_reference_for_error(l),
                    right: arithmetic_type_reference_for_error(r),
                    kind,
                })),
            )?;
            Ok(any_t::error(reason))
        }
    }
}

pub fn is_same_instance_type(
    i1: &flow_typing_type::type_::InstanceT,
    i2: &flow_typing_type::type_::InstanceT,
) -> bool {
    i1.inst.class_id == i2.inst.class_id && i1.inst.class_id != flow_aloc::ALocId::none()
}

/// Whether two annotations resolve to the same instance type without type arguments. Each
/// annotation site of an interface gets its own `this`-specialized copy of the instance, so
/// this compares nominally, as `InstanceT ~> InstanceT` does.
pub fn is_same_non_generic_instance_annot(cx: &mut Context<'_>, t1: &Type, t2: &Type) -> bool {
    let mut resolve = |t: &Type| match t.deref() {
        TypeInner::AnnotT(_, t, _) => cx.find_resolved(t),
        _ => cx.find_resolved(t),
    };
    let (Some(t1), Some(t2)) = (resolve(t1), resolve(t2)) else {
        return false;
    };
    match (t1.deref(), t2.deref()) {
        (TypeInner::DefT(_, d1), TypeInner::DefT(_, d2)) => match (d1.deref(), d2.deref()) {
            (DefTInner::InstanceT(i1), DefTInner::InstanceT(i2)) => {
                is_same_instance_type(i1, i2)
                    && i1.inst.type_args.is_empty()
                    && i2.inst.type_args.is_empty()
            }
            _ => false,
        },
        _ => false,
    }
}

// TypeAppT ~> TypeAppT has special behavior that flows the type arguments directly
// instead of evaluating the types and then flowing the results to each other. This is
// a bug that should be removed because it breaks transitivity, which can cause errors
// on code like:
// type ReadOnly<O> = {+[key in keyof O]: O[key]};
// type O1 = {foo: number};
// type O2 = {foo: string | number};
// declare const x: ReadOnly<O1>;
// (x: ReadOnly<O2>); // ERROR
//
// In order to prevent this in common mapped type and other cases, we do a best-effort
// check to see if the RHS contains a mapped type, other utility type, or a template
// literal type (whose subtyping is not captured by parameter variance). This traversal
// is extremely limited and does not attempt to be exhaustive.
pub fn wraps_utility_type<'cx>(cx: &mut Context<'cx>, tin: &Type) -> bool {
    use std::collections::BTreeSet;

    let mut seen_open_id: BTreeSet<i32> = BTreeSet::new();
    let mut seen_eval_id: BTreeSet<flow_typing_type::type_::eval::Id> = BTreeSet::new();

    fn loop_inner<'cx>(
        cx: &mut Context<'cx>,
        t: &Type,
        seen_open_id: &mut BTreeSet<i32>,
        seen_eval_id: &mut BTreeSet<flow_typing_type::type_::eval::Id>,
    ) -> bool {
        use flow_typing_type::type_::DefTInner;
        use flow_typing_type::type_::Destructor;
        use flow_typing_type::type_::DestructorConditionalTypeData;
        use flow_typing_type::type_::PolyTData;
        use flow_typing_type::type_::constraint::Constraints;

        match t.deref() {
            TypeInner::EvalT {
                type_,
                defer_use_t,
                id: _,
            } if matches!(type_.deref(), TypeInner::GenericT(..))
                && matches!(defer_use_t.2.deref(), Destructor::ReadOnlyType) =>
            {
                // Handles special cases targeting tests/typeapp_opt/stylex.js
                true
            }
            TypeInner::EvalT {
                type_: _,
                defer_use_t,
                id: _,
            } if matches!(
                defer_use_t.2.deref(),
                Destructor::ConditionalType(box DestructorConditionalTypeData { true_t, .. }) if matches!(true_t.deref(), TypeInner::GenericT(..))
            ) =>
            {
                true
            }
            TypeInner::EvalT {
                type_: _,
                defer_use_t,
                id: _,
            } if matches!(
                defer_use_t.2.deref(),
                Destructor::ConditionalType(box DestructorConditionalTypeData { false_t, .. }) if matches!(false_t.deref(), TypeInner::GenericT(..))
            ) =>
            {
                true
            }
            TypeInner::EvalT {
                type_: inner_t,
                defer_use_t,
                id,
            } if matches!(defer_use_t.2.deref(), Destructor::ReadOnlyType) => {
                if seen_eval_id.contains(id) {
                    false
                } else {
                    seen_eval_id.insert(id.dupe());
                    loop_inner(cx, inner_t, seen_open_id, seen_eval_id)
                }
            }
            TypeInner::EvalT {
                type_: _,
                defer_use_t,
                id: _,
            } if matches!(
                defer_use_t.2.deref(),
                Destructor::MappedType(box flow_typing_type::type_::DestructorMappedTypeData {
                    ..
                })
            ) =>
            {
                true
            }
            // Transparent type aliases whose body is a template literal type cannot
            // be checked by parameter-variance comparison alone: template literal
            // subtyping admits relationships (e.g. prefix extension) that aren't
            // captured by pointwise covariance/contravariance of the type arguments.
            // Treat these like other utility types — skip the TypeAppT-vs-TypeAppT
            // fast path and force expansion of both sides so the template-literal
            // subtyping rules in subtyping_kit can run.
            TypeInner::TemplateLiteralT { .. } => true,
            TypeInner::DefT(_, def_t) => match def_t.deref() {
                DefTInner::TypeT(_, inner_t) => loop_inner(cx, inner_t, seen_open_id, seen_eval_id),
                DefTInner::PolyT(box PolyTData { t_out, .. }) => {
                    loop_inner(cx, t_out, seen_open_id, seen_eval_id)
                }
                _ => false,
            },
            _ if let Some(node) = type_util::constraint_node_id(t) => {
                let (root_id, constraints) = cx.find_constraints(node.id());
                if seen_open_id.contains(&root_id) {
                    false
                } else {
                    seen_open_id.insert(root_id);
                    match &constraints {
                        Constraints::FullyResolved(s) => {
                            let resolved = cx.force_fully_resolved_tvar(s);
                            loop_inner(cx, &resolved, seen_open_id, seen_eval_id)
                        }
                        Constraints::Resolved(inner_t) => {
                            loop_inner(cx, inner_t, seen_open_id, seen_eval_id)
                        }
                        _ => false,
                    }
                }
            }
            TypeInner::TypeAppT(box TypeAppTData {
                reason: _,
                use_op: _,
                type_,
                targs: _,
                from_value: _,
                use_desc: _,
            }) => loop_inner(cx, type_, seen_open_id, seen_eval_id),
            _ => false,
        }
    }

    loop_inner(cx, tin, &mut seen_open_id, &mut seen_eval_id)
}

// Tuples

pub fn validate_tuple_elements<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    reason_tuple: &Reason,
    error_on_req_after_opt: bool,
    elements: &[flow_typing_type::type_::TupleElement],
) -> Result<(bool, (usize, usize)), FlowJsException> {
    use flow_typing_errors::error_message::ErrorMessage;
    use flow_typing_type::type_::TupleElement;

    let mut valid = true;
    let mut num_req: usize = 0;
    let mut num_opt: usize = 0;
    let mut prev_element: Option<&TupleElement> = None;

    for element in elements {
        let optional = element.optional;
        let reason_element = &element.reason;

        if optional {
            num_opt += 1;
            prev_element = Some(element);
        } else {
            valid = match prev_element {
                Some(TupleElement {
                    name: optional_name,
                    optional: true,
                    reason: reason_optional,
                    ..
                }) if valid => {
                    if error_on_req_after_opt {
                        add_output_with_env(
                            cx,
                            env,
                            ErrorMessage::ETupleRequiredAfterOptional(Box::new(
                                ETupleRequiredAfterOptionalData {
                                    tuple_loc: reason_tuple.loc().dupe(),
                                    required: TupleElementReferenceData {
                                        loc: reason_element.loc().dupe(),
                                        name: element.name.dupe(),
                                    },
                                    optional: TupleElementReferenceData {
                                        loc: reason_optional.loc().dupe(),
                                        name: optional_name.dupe(),
                                    },
                                },
                            )),
                        )?;
                    }
                    false
                }
                _ => valid,
            };
            num_req += 1;
            prev_element = Some(element);
        }
    }

    let arity = (num_req, num_req + num_opt);
    Ok((valid, arity))
}

pub fn mk_tuple_type<'cx, F>(
    cx: &mut Context<'cx>,
    strictness_kind: TypeStrictnessKind,
    id: flow_typing_type::type_::eval::Id,
    mk_type_destructor: F,
    inexact: bool,
    reason: Reason,
    elements: Vec<flow_typing_type::type_::UnresolvedParam>,
) -> Result<Type, FlowJsException>
where
    F: FnOnce(
        &mut Context<'cx>,
        UseOp,
        Reason,
        Type,
        flow_typing_type::type_::Destructor,
        flow_typing_type::type_::eval::Id,
    ) -> Result<Type, FlowJsException>,
{
    use flow_typing_type::type_::ArrType;
    use flow_typing_type::type_::DefT;
    use flow_typing_type::type_::DefTInner;
    use flow_typing_type::type_::Destructor;
    use flow_typing_type::type_::DestructorSpreadTupleTypeData;
    use flow_typing_type::type_::ResolvedArgData;
    use flow_typing_type::type_::ResolvedParam;
    use flow_typing_type::type_::TupleATData;
    use flow_typing_type::type_::TupleElement;
    use flow_typing_type::type_::UnresolvedArgData;
    use flow_typing_type::type_::UnresolvedParam;
    use flow_typing_type::type_::any_t;
    use flow_typing_type::type_::mixed_t;
    use flow_typing_type::type_util::reason_of_t;
    use flow_typing_type::type_util::tuple_ts_of_elements;
    use flow_typing_type::type_util::union_of_ts;

    let env = &FlowJsEnv::entry();
    let mut resolved_rev: flow_data_structure_wrapper::list::FlowOcamlList<(
        TupleElement,
        Option<flow_typing_generics::GenericId>,
    )> = flow_data_structure_wrapper::list::FlowOcamlList::new();
    let mut unresolved_rev: flow_data_structure_wrapper::list::FlowOcamlList<UnresolvedParam> =
        flow_data_structure_wrapper::list::FlowOcamlList::new();
    let mut first_spread: Option<(Reason, Type)> = None;

    for el in elements {
        match (&el, &first_spread) {
            (UnresolvedParam::UnresolvedArg(box UnresolvedArgData(tuple_el, generic)), None) => {
                resolved_rev.push_front((tuple_el.clone(), generic.clone()));
            }
            (UnresolvedParam::UnresolvedSpreadArg(t), None) => {
                first_spread = Some((reason_of_t(t).dupe(), t.dupe()));
            }
            (_, Some(_)) => {
                unresolved_rev.push_front(el);
            }
        }
    }

    match first_spread {
        Some((reason_spread, spread_t)) => {
            let mut unresolved = unresolved_rev;
            unresolved.reverse();
            let resolved_rev: flow_data_structure_wrapper::list::FlowOcamlList<ResolvedParam> =
                resolved_rev
                    .iter()
                    .map(|(el, generic)| {
                        ResolvedParam::ResolvedArg(Box::new(ResolvedArgData(
                            el.clone(),
                            generic.clone(),
                        )))
                    })
                    .collect();

            mk_type_destructor(
                cx,
                unknown_use(),
                reason.dupe(),
                spread_t,
                Destructor::SpreadTupleType(Box::new(DestructorSpreadTupleTypeData {
                    reason_tuple: reason,
                    reason_spread,
                    inexact,
                    resolved: resolved_rev,
                    unresolved,
                })),
                id,
            )
        }
        None => {
            let elements: Vec<TupleElement> = {
                let mut r = resolved_rev;
                r.reverse();
                r.iter().map(|(el, _)| el.clone()).collect()
            };
            let (valid, arity) = validate_tuple_elements(cx, env, &reason, true, &elements)?;
            let arity = (arity.0 as i32, arity.1 as i32);

            if valid {
                let elem_t = if inexact {
                    let reason_mixed = reason
                        .dupe()
                        .replace_desc(VirtualReasonDesc::RTupleUnknownElementFromInexact);
                    mixed_t::make(reason_mixed)
                } else {
                    let ts = tuple_ts_of_elements(&elements);
                    let elem_t_reason = reason.dupe().replace_desc(None);
                    union_of_ts(elem_t_reason, ts, None)
                };
                Ok(Type::new(TypeInner::DefT(
                    reason,
                    DefT::new(DefTInner::ArrT(std::rc::Rc::new(ArrType::TupleAT(
                        Box::new(TupleATData {
                            elem_t,
                            elements: elements.into(),
                            arity,
                            inexact,
                            rest: None,
                            react_dro: None,
                            strictness_kind,
                            literal: false,
                            fresh: false,
                        }),
                    )))),
                )))
            } else {
                Ok(any_t::error(reason))
            }
        }
    }
}

pub mod render_types {
    use std::ops::Deref;
    use std::rc::Rc;

    use dupe::Dupe;
    use flow_aloc::ALoc;
    use flow_common::reason::Reason;
    use flow_common::reason::VirtualReasonDesc;
    use flow_common::reason::mk_reason;
    use flow_data_structure_wrapper::ord_map::FlowOrdMap;
    use flow_typing_context::Context;
    use flow_typing_errors::error_message::EInvalidRendersTypeArgumentData;
    use flow_typing_errors::error_message::ErrorMessage;
    use flow_typing_errors::intermediate_error_types::InvalidRenderTypeKind;
    use flow_typing_type::type_::CanonicalRendersForm;
    use flow_typing_type::type_::ComponentKind;
    use flow_typing_type::type_::DefT;
    use flow_typing_type::type_::DefTInner;
    use flow_typing_type::type_::GenericTData;
    use flow_typing_type::type_::PolyTData;
    use flow_typing_type::type_::ReactAbstractComponentTData;
    use flow_typing_type::type_::RendersVariant;
    use flow_typing_type::type_::Type;
    use flow_typing_type::type_::TypeInner;
    use flow_typing_type::type_::any_t;
    use flow_typing_type::type_::type_collector::TypeCollector;
    use flow_typing_type::type_util::loc_of_t;
    use flow_typing_type::type_util::union_of_ts;
    use flow_utils_concurrency::job_error::JobError;
    use vec1::Vec1;

    use crate::flow_js_utils::add_output_non_speculating;
    use crate::flow_js_utils::builtin_react_renders_exactly_nominal_id;
    use crate::flow_js_utils::type_reference_for_error;
    use crate::type_subst;
    use crate::type_subst::Purpose;

    fn allow_child_renders(
        parent_variant: &RendersVariant,
        child_variant: &RendersVariant,
    ) -> bool {
        match (parent_variant, child_variant) {
            (RendersVariant::RendersNormal, RendersVariant::RendersNormal) => true,
            (
                RendersVariant::RendersNormal,
                RendersVariant::RendersMaybe | RendersVariant::RendersStar,
            ) => false,
            (
                RendersVariant::RendersMaybe,
                RendersVariant::RendersNormal | RendersVariant::RendersMaybe,
            ) => true,
            (RendersVariant::RendersMaybe, RendersVariant::RendersStar) => false,
            (RendersVariant::RendersStar, _) => true,
        }
    }

    fn ast_render_variant_of_render_variant(
        variant: &RendersVariant,
    ) -> flow_parser::ast::types::RendersVariant {
        match variant {
            RendersVariant::RendersNormal => flow_parser::ast::types::RendersVariant::Normal,
            RendersVariant::RendersMaybe => flow_parser::ast::types::RendersVariant::Maybe,
            RendersVariant::RendersStar => flow_parser::ast::types::RendersVariant::Star,
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum PotentialFixableErrorKind {
        InvalidRendersNullVoidFalse,
        InvalidRendersIterable,
    }

    struct ErrorAcc {
        potential_fixable_error_acc: Option<(Vec1<Type>, PotentialFixableErrorKind)>,
        normal_errors: Vec<ErrorMessage<ALoc>>,
    }

    struct RenderTypeNormalizationContext<'a, 'cx, F, G>
    where
        F: Fn(&mut Context<'cx>, &Type) -> Result<Vec<Type>, JobError>,
        G: Fn(&mut Context<'cx>, &Type) -> Result<bool, JobError>,
    {
        cx: &'a mut Context<'cx>,
        type_collector: TypeCollector,
        error_acc: ErrorAcc,
        arg_loc: ALoc,
        result_reason: Reason,
        renders_variant: RendersVariant,
        concretize: F,
        is_iterable_for_better_error: G,
    }

    fn merge_error_acc_with_potential_fixable_error<'cx, F, G>(
        normalization_cx: &mut RenderTypeNormalizationContext<'_, 'cx, F, G>,
        new_error: (Type, PotentialFixableErrorKind),
    ) where
        F: Fn(&mut Context<'cx>, &Type) -> Result<Vec<Type>, JobError>,
        G: Fn(&mut Context<'cx>, &Type) -> Result<bool, JobError>,
    {
        let error_acc = &mut normalization_cx.error_acc;
        let (r, k2) = new_error;
        error_acc.potential_fixable_error_acc = match error_acc.potential_fixable_error_acc.take() {
            None => Some((Vec1::new(r), k2)),
            Some((mut rs, k1)) => {
                let k = match (k1, k2) {
                    (
                        PotentialFixableErrorKind::InvalidRendersNullVoidFalse,
                        PotentialFixableErrorKind::InvalidRendersNullVoidFalse,
                    ) => PotentialFixableErrorKind::InvalidRendersNullVoidFalse,
                    _ => PotentialFixableErrorKind::InvalidRendersIterable,
                };
                rs.push(r);
                Some((rs, k))
            }
        };
    }

    fn merge_error_acc_with_normal_error<'cx, F, G>(
        normalization_cx: &mut RenderTypeNormalizationContext<'_, 'cx, F, G>,
        e: ErrorMessage<ALoc>,
    ) where
        F: Fn(&mut Context<'cx>, &Type) -> Result<Vec<Type>, JobError>,
        G: Fn(&mut Context<'cx>, &Type) -> Result<bool, JobError>,
    {
        let error_acc = &mut normalization_cx.error_acc;
        error_acc.normal_errors.push(e);
    }

    fn on_concretized_renders_normalization<'cx, F, G>(
        normalization_cx: &mut RenderTypeNormalizationContext<'_, 'cx, F, G>,
        resolved_elem: &Type,
        t: Type,
    ) -> Result<(), JobError>
    where
        F: Fn(&mut Context<'cx>, &Type) -> Result<Vec<Type>, JobError>,
        G: Fn(&mut Context<'cx>, &Type) -> Result<bool, JobError>,
    {
        match t.deref() {
            TypeInner::DefT(_, def_t) => match def_t.deref() {
                DefTInner::RendersT(renders_form) => match renders_form.deref() {
                    CanonicalRendersForm::NominalRenders { .. } => {
                        normalization_cx.type_collector.add(t.dupe());
                        return Ok(());
                    }
                    CanonicalRendersForm::StructuralRenders {
                        renders_variant: child_variant,
                        renders_structural_type,
                    } => {
                        if allow_child_renders(&normalization_cx.renders_variant, child_variant) {
                            for concretized in (normalization_cx.concretize)(
                                normalization_cx.cx,
                                renders_structural_type,
                            )? {
                                on_concretized_renders_normalization(
                                    normalization_cx,
                                    resolved_elem,
                                    concretized,
                                )?;
                            }
                        } else {
                            normalization_cx
                                .type_collector
                                .add(any_t::error(normalization_cx.result_reason.dupe()));
                            merge_error_acc_with_normal_error(
                                normalization_cx,
                                ErrorMessage::EInvalidRendersTypeArgument(Box::new(
                                    EInvalidRendersTypeArgumentData {
                                        loc: normalization_cx.arg_loc.dupe(),
                                        renders_variant: ast_render_variant_of_render_variant(
                                            &normalization_cx.renders_variant,
                                        ),
                                        invalid_render_type_kind:
                                            InvalidRenderTypeKind::UncategorizedInvalidRenders,
                                        invalid_types: Vec1::new(type_reference_for_error(&t)),
                                    },
                                )),
                            );
                        }
                        return Ok(());
                    }
                    CanonicalRendersForm::DefaultRenders => {
                        normalization_cx
                            .type_collector
                            .add(any_t::error(normalization_cx.result_reason.dupe()));
                        merge_error_acc_with_normal_error(
                            normalization_cx,
                            ErrorMessage::EInvalidRendersTypeArgument(Box::new(
                                EInvalidRendersTypeArgumentData {
                                    loc: normalization_cx.arg_loc.dupe(),
                                    renders_variant: ast_render_variant_of_render_variant(
                                        &normalization_cx.renders_variant,
                                    ),
                                    invalid_render_type_kind:
                                        InvalidRenderTypeKind::UncategorizedInvalidRenders,
                                    invalid_types: Vec1::new(type_reference_for_error(&t)),
                                },
                            )),
                        );
                        return Ok(());
                    }
                    _ => {}
                },
                _ => {}
            },
            _ => {}
        }
        normalization_cx
            .type_collector
            .add(any_t::error(normalization_cx.result_reason.dupe()));
        merge_error_acc_with_normal_error(
            normalization_cx,
            ErrorMessage::EInvalidRendersTypeArgument(Box::new(EInvalidRendersTypeArgumentData {
                loc: normalization_cx.arg_loc.dupe(),
                renders_variant: ast_render_variant_of_render_variant(
                    &normalization_cx.renders_variant,
                ),
                invalid_render_type_kind: InvalidRenderTypeKind::InvalidRendersStructural(
                    type_reference_for_error(&t),
                ),
                invalid_types: Vec1::new(type_reference_for_error(resolved_elem)),
            })),
        );
        Ok(())
    }

    fn on_concretized_component_normalization<'cx, F, G>(
        normalization_cx: &mut RenderTypeNormalizationContext<'_, 'cx, F, G>,
        resolved_elem: &Type,
        t: Type,
    ) -> Result<(), JobError>
    where
        F: Fn(&mut Context<'cx>, &Type) -> Result<Vec<Type>, JobError>,
        G: Fn(&mut Context<'cx>, &Type) -> Result<bool, JobError>,
    {
        match t.deref() {
            TypeInner::DefT(_, def_t) => match def_t.deref() {
                DefTInner::PolyT(box PolyTData {
                    tparams_loc,
                    tparams,
                    t_out,
                    ..
                }) => {
                    let mut subst_map = FlowOrdMap::new();
                    for tparam in tparams.iter() {
                        subst_map.insert(
                            tparam.name.dupe(),
                            flow_typing_type::type_::unsoundness::at(
                                flow_typing_type::type_::UnsoundnessKind::Unchecked,
                                tparams_loc.dupe(),
                            ),
                        );
                    }
                    let t_out = type_subst::subst(
                        normalization_cx.cx,
                        None,
                        true,
                        false,
                        Purpose::Normal,
                        &subst_map,
                        t_out.dupe(),
                    );
                    on_concretized_component_normalization(normalization_cx, resolved_elem, t_out)?;
                }
                DefTInner::ReactAbstractComponentT(box ReactAbstractComponentTData {
                    component_kind: ComponentKind::Nominal(renders_id, renders_name, _),
                    renders: renders_super,
                    ..
                }) => {
                    let reason = mk_reason(
                        VirtualReasonDesc::RType(renders_name.dupe()),
                        normalization_cx.result_reason.loc().dupe(),
                    );
                    let new_t = Type::new(TypeInner::DefT(
                        reason,
                        DefT::new(DefTInner::RendersT(Rc::new(
                            CanonicalRendersForm::NominalRenders {
                                renders_id: renders_id.dupe(),
                                renders_name: renders_name.dupe(),
                                renders_super: renders_super.dupe(),
                            },
                        ))),
                    ));
                    normalization_cx.type_collector.add(new_t);
                }
                DefTInner::ReactAbstractComponentT(box ReactAbstractComponentTData {
                    component_kind: ComponentKind::Structural,
                    renders: render_type,
                    ..
                }) => {
                    for concretized in
                        (normalization_cx.concretize)(normalization_cx.cx, render_type)?
                    {
                        on_concretized_renders_normalization(
                            normalization_cx,
                            resolved_elem,
                            concretized,
                        )?;
                    }
                }
                _ => {
                    normalization_cx
                        .type_collector
                        .add(any_t::error(normalization_cx.result_reason.dupe()));
                    merge_error_acc_with_normal_error(
                        normalization_cx,
                        ErrorMessage::EInvalidRendersTypeArgument(Box::new(
                            EInvalidRendersTypeArgumentData {
                                loc: normalization_cx.arg_loc.dupe(),
                                renders_variant: ast_render_variant_of_render_variant(
                                    &normalization_cx.renders_variant,
                                ),
                                invalid_render_type_kind:
                                    InvalidRenderTypeKind::InvalidRendersNonNominalElement(
                                        type_reference_for_error(&t),
                                    ),
                                invalid_types: Vec1::new(type_reference_for_error(resolved_elem)),
                            },
                        )),
                    );
                }
            },
            _ => {
                normalization_cx
                    .type_collector
                    .add(any_t::error(normalization_cx.result_reason.dupe()));
                merge_error_acc_with_normal_error(
                    normalization_cx,
                    ErrorMessage::EInvalidRendersTypeArgument(Box::new(
                        EInvalidRendersTypeArgumentData {
                            loc: normalization_cx.arg_loc.dupe(),
                            renders_variant: ast_render_variant_of_render_variant(
                                &normalization_cx.renders_variant,
                            ),
                            invalid_render_type_kind:
                                InvalidRenderTypeKind::InvalidRendersNonNominalElement(
                                    type_reference_for_error(&t),
                                ),
                            invalid_types: Vec1::new(type_reference_for_error(resolved_elem)),
                        },
                    )),
                );
            }
        }
        Ok(())
    }

    fn on_concretized_bad_non_element_normalization<'cx, F, G>(
        normalization_cx: &mut RenderTypeNormalizationContext<'_, 'cx, F, G>,
        t: Type,
    ) -> Result<(), JobError>
    where
        F: Fn(&mut Context<'cx>, &Type) -> Result<Vec<Type>, JobError>,
        G: Fn(&mut Context<'cx>, &Type) -> Result<bool, JobError>,
    {
        match &*t {
            TypeInner::DefT(_, def_t)
                if matches!(
                    &**def_t,
                    DefTInner::SingletonBoolT { value: false, .. }
                        | DefTInner::NullT
                        | DefTInner::VoidT
                ) =>
            {
                normalization_cx
                    .type_collector
                    .add(any_t::error(normalization_cx.result_reason.dupe()));
                merge_error_acc_with_potential_fixable_error(
                    normalization_cx,
                    (
                        t.dupe(),
                        PotentialFixableErrorKind::InvalidRendersNullVoidFalse,
                    ),
                );
            }
            TypeInner::DefT(_, def_t) if matches!(&**def_t, DefTInner::ArrT(_)) => {
                merge_error_acc_with_potential_fixable_error(
                    normalization_cx,
                    (t.dupe(), PotentialFixableErrorKind::InvalidRendersIterable),
                );
            }
            _ => {
                normalization_cx
                    .type_collector
                    .add(any_t::error(normalization_cx.result_reason.dupe()));
                if (normalization_cx.is_iterable_for_better_error)(normalization_cx.cx, &t)? {
                    merge_error_acc_with_potential_fixable_error(
                        normalization_cx,
                        (t.dupe(), PotentialFixableErrorKind::InvalidRendersIterable),
                    );
                } else {
                    merge_error_acc_with_normal_error(
                        normalization_cx,
                        ErrorMessage::EInvalidRendersTypeArgument(Box::new(
                            EInvalidRendersTypeArgumentData {
                                loc: normalization_cx.arg_loc.dupe(),
                                renders_variant: ast_render_variant_of_render_variant(
                                    &normalization_cx.renders_variant,
                                ),
                                invalid_render_type_kind:
                                    InvalidRenderTypeKind::UncategorizedInvalidRenders,
                                invalid_types: Vec1::new(type_reference_for_error(&t)),
                            },
                        )),
                    );
                }
            }
        }
        Ok(())
    }

    fn on_concretized_element_normalization<'cx, F, G>(
        normalization_cx: &mut RenderTypeNormalizationContext<'_, 'cx, F, G>,
        t: Type,
    ) -> Result<(), JobError>
    where
        F: Fn(&mut Context<'cx>, &Type) -> Result<Vec<Type>, JobError>,
        G: Fn(&mut Context<'cx>, &Type) -> Result<bool, JobError>,
    {
        match &*t {
            TypeInner::GenericT(box GenericTData { .. }) => {
                normalization_cx
                    .type_collector
                    .add(any_t::error(normalization_cx.result_reason.dupe()));
                merge_error_acc_with_normal_error(
                    normalization_cx,
                    ErrorMessage::EInvalidRendersTypeArgument(Box::new(
                        EInvalidRendersTypeArgumentData {
                            loc: normalization_cx.arg_loc.dupe(),
                            renders_variant: ast_render_variant_of_render_variant(
                                &normalization_cx.renders_variant,
                            ),
                            invalid_render_type_kind: InvalidRenderTypeKind::InvalidRendersGenericT,
                            invalid_types: Vec1::new(type_reference_for_error(&t)),
                        },
                    )),
                );
            }
            TypeInner::DefT(reason, def_t) => {
                if let DefTInner::SingletonStrT { value, .. } = &**def_t {
                    if value.as_str() == "svg" {
                        normalization_cx
                            .type_collector
                            .add(Type::new(TypeInner::DefT(
                                reason.dupe(),
                                DefT::new(DefTInner::RendersT(Rc::new(
                                    CanonicalRendersForm::IntrinsicRenders("svg".into()),
                                ))),
                            )));
                        return Ok(());
                    }
                }
                on_concretized_bad_non_element_normalization(normalization_cx, t.dupe())?;
            }
            TypeInner::NominalT { nominal_type, .. } => {
                let nominal_id = &nominal_type.nominal_id;
                let nominal_type_args = &nominal_type.nominal_type_args;
                if builtin_react_renders_exactly_nominal_id(normalization_cx.cx).as_ref()
                    == Some(nominal_id)
                {
                    if let Some((_, _, component_t, _)) = nominal_type_args.first() {
                        for concretized in
                            (normalization_cx.concretize)(normalization_cx.cx, component_t)?
                        {
                            on_concretized_component_normalization(
                                normalization_cx,
                                &t,
                                concretized,
                            )?;
                        }
                        return Ok(());
                    }
                }
                on_concretized_bad_non_element_normalization(normalization_cx, t.dupe())?;
            }
            _ => {
                on_concretized_bad_non_element_normalization(normalization_cx, t.dupe())?;
            }
        }
        Ok(())
    }

    fn normalize_render_type_argument<'cx, F, G>(
        cx: &mut Context<'cx>,
        arg_loc: ALoc,
        reason: Reason,
        renders_variant: RendersVariant,
        concretize: F,
        is_iterable_for_better_error: G,
        input: Type,
    ) -> Result<Type, JobError>
    where
        F: Fn(&mut Context<'cx>, &Type) -> Result<Vec<Type>, JobError>,
        G: Fn(&mut Context<'cx>, &Type) -> Result<bool, JobError>,
    {
        let type_collector = TypeCollector::create();
        let mut normalization_cx = RenderTypeNormalizationContext {
            cx,
            type_collector,
            error_acc: ErrorAcc {
                potential_fixable_error_acc: None,
                normal_errors: Vec::new(),
            },
            arg_loc: arg_loc.dupe(),
            result_reason: reason.dupe(),
            renders_variant,
            concretize,
            is_iterable_for_better_error,
        };
        for concretized in (normalization_cx.concretize)(normalization_cx.cx, &input)? {
            on_concretized_element_normalization(&mut normalization_cx, concretized)?;
        }
        let RenderTypeNormalizationContext {
            cx,
            type_collector,
            error_acc,
            renders_variant,
            ..
        } = normalization_cx;
        if let Some((mut invalid_types, kind)) = error_acc.potential_fixable_error_acc {
            invalid_types.reverse();
            add_output_non_speculating(
                cx,
                ErrorMessage::EInvalidRendersTypeArgument(Box::new(
                    EInvalidRendersTypeArgumentData {
                        loc: arg_loc.dupe(),
                        renders_variant: ast_render_variant_of_render_variant(&renders_variant),
                        invalid_render_type_kind: match kind {
                            PotentialFixableErrorKind::InvalidRendersNullVoidFalse => {
                                InvalidRenderTypeKind::InvalidRendersNullVoidFalse
                            }
                            PotentialFixableErrorKind::InvalidRendersIterable => {
                                InvalidRenderTypeKind::InvalidRendersIterable
                            }
                        },
                        invalid_types: Vec1::try_from_vec(
                            invalid_types
                                .into_iter()
                                .map(|type_| type_reference_for_error(&type_))
                                .collect(),
                        )
                        .unwrap(),
                    },
                )),
            );
        }
        for e in error_acc.normal_errors {
            add_output_non_speculating(cx, e.clone());
        }
        let collected = type_collector.collect_to_vec();
        let renders_structural_type = union_of_ts(reason.dupe(), collected, None);
        Ok(Type::new(TypeInner::DefT(
            reason,
            DefT::new(DefTInner::RendersT(Rc::new(
                CanonicalRendersForm::StructuralRenders {
                    renders_variant,
                    renders_structural_type,
                },
            ))),
        )))
    }

    pub fn mk_non_generic_render_type<'cx, F, G>(
        cx: &mut Context<'cx>,
        reason: Reason,
        renders_variant: RendersVariant,
        force_post_component: bool,
        concretize: F,
        is_iterable_for_better_error: G,
        t: Type,
    ) -> Type
    where
        F: for<'a> Fn(&mut Context<'a>, &Type) -> Result<Vec<Type>, JobError> + 'static,
        G: for<'a> Fn(&mut Context<'a>, &Type) -> Result<bool, JobError> + 'static,
    {
        let reason_inner = reason.dupe();
        let renders_variant_inner = renders_variant.clone();
        flow_typing_tvar::mk_fully_resolved_lazy(
            cx,
            reason,
            force_post_component,
            move |cx: &mut Context<'_>| {
                cx.run_in_signature_tvar_env(|cx| {
                    let arg_loc = loc_of_t(&t).dupe();
                    normalize_render_type_argument(
                        cx,
                        arg_loc,
                        reason_inner,
                        renders_variant_inner,
                        &concretize,
                        &is_iterable_for_better_error,
                        t,
                    )
                })
            },
        )
    }
}

pub mod callee_recorder {
    use dupe::Dupe;
    use flow_common::reason::Reason;
    use flow_lazy::ContextFamily;
    use flow_typing_flow_js_env::FlowJsEnv;
    use flow_typing_type::type_::CallTData;
    use flow_typing_type::type_::SpecializedCallee;
    use flow_typing_type::type_::Type;
    use flow_typing_type::type_::UseT;
    use flow_typing_type::type_util::union_of_ts;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Kind {
        Tast,
        SigHelp,
        All,
    }

    fn add_tast(env: &FlowJsEnv, l: Type, specialized_callee: &SpecializedCallee) {
        match env.speculation_id() {
            Some(id) if Some(id.dupe()) != specialized_callee.init_speculation_state => {
                // It is possible that the call we are inspecting was initiated in a speculative
                // state. It is important to compare with the state during the beginning of the
                // call to determine if this is a true speculative candidate.
                specialized_callee
                    .speculative_candidates
                    .borrow_mut()
                    .push_front((l, id.dupe()));
            }
            _ => {
                specialized_callee.finalized.borrow_mut().push_front(l);
            }
        }
    }

    // For signature-help, we are intereseted in all branches of intersections, so
    // we include intersections in the accumulated result. Note that we discard nested
    // intersection types. These would appear under speculation, so we can effectively
    // enforce this constraint by checking that we are not in a speculation enviornment.
    // Also we skip voided out results in case of optional chaining.
    fn add_signature_help(env: &FlowJsEnv, l: Type, specialized_callee: &SpecializedCallee) {
        if env.speculation_id().is_none() {
            specialized_callee.sig_help.borrow_mut().push_front(l);
        }
    }

    fn do_tast(kind: Kind) -> bool {
        match kind {
            Kind::Tast | Kind::All => true,
            Kind::SigHelp => false,
        }
    }

    fn do_sig_help(kind: Kind) -> bool {
        match kind {
            Kind::All | Kind::SigHelp => true,
            Kind::Tast => false,
        }
    }

    pub fn add_callee(
        env: &FlowJsEnv,
        kind: Kind,
        l: Type,
        specialized_callee: Option<&SpecializedCallee>,
    ) {
        if let Some(specialized_callee) = specialized_callee {
            // Avoid recording results computed during implicit instantiation. We redo the
            // call after the instantiation portion using the concretized results. We will
            // record that latter call result.
            if !env.in_implicit_instantiation() {
                if do_tast(kind) {
                    add_tast(env, l.dupe(), specialized_callee);
                }
                if do_sig_help(kind) {
                    add_signature_help(env, l, specialized_callee);
                }
            }
        }
    }

    pub fn add_callee_use<CX: ContextFamily>(env: &FlowJsEnv, kind: Kind, l: Type, u: &UseT<CX>) {
        use flow_typing_type::type_::UseTInner;
        match &**u {
            UseTInner::CallT(box CallTData { call_action, .. }) => match call_action.as_ref() {
                flow_typing_type::type_::CallAction::Funcalltype(
                    box flow_typing_type::type_::FuncallType {
                        call_specialized_callee,
                        ..
                    },
                ) => {
                    add_callee(env, kind, l, call_specialized_callee.as_ref());
                }
                _ => {}
            },
            _ => {}
        }
    }

    pub fn type_for_sig_help(reason: Reason, specialized_callee: &SpecializedCallee) -> Type {
        let sig_help = specialized_callee.sig_help.borrow();
        union_of_ts(reason, sig_help.iter().cloned().collect(), None)
    }

    pub fn type_for_tast(reason: Reason, specialized_callee: &SpecializedCallee) -> Type {
        let finalized = specialized_callee.finalized.borrow();
        union_of_ts(reason, finalized.iter().cloned().collect(), None)
    }

    pub fn type_for_tast_opt(
        reason: Reason,
        specialized_callee: &SpecializedCallee,
    ) -> Option<Type> {
        let finalized = specialized_callee.finalized.borrow();
        if finalized.is_empty() {
            None
        } else {
            Some(union_of_ts(
                reason,
                finalized.iter().cloned().collect(),
                None,
            ))
        }
    }
}
