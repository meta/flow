/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::fmt::Debug;
use std::sync::Arc;

use dupe::Dupe;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;

use crate::ty::AnyErrorKind;
use crate::ty::AnyKind;
use crate::ty::ArrT;
use crate::ty::BotKind;
use crate::ty::BuiltinOrSymbol;
use crate::ty::ComponentProps;
use crate::ty::Decl;
use crate::ty::DeclEnumDeclData;
use crate::ty::DeclModuleDeclData;
use crate::ty::DeclNamespaceDeclData;
use crate::ty::DeclNominalComponentDeclData;
use crate::ty::DeclTypeAliasDeclData;
use crate::ty::Dict;
use crate::ty::Elt;
use crate::ty::FlattenedComponentProp;
use crate::ty::FunEffect;
use crate::ty::FunParam;
use crate::ty::FunT;
use crate::ty::GenericT;
use crate::ty::InterfaceT;
use crate::ty::MappedTypeHomomorphicFlag;
use crate::ty::MappedTypeOptionalFlag;
use crate::ty::MappedTypeVariance;
use crate::ty::NamedProp;
use crate::ty::ObjKind;
use crate::ty::ObjT;
use crate::ty::OpaqueBounds;
use crate::ty::Polarity;
use crate::ty::Prop;
use crate::ty::ReturnT;
use crate::ty::TupleElement;
use crate::ty::Ty;
use crate::ty::TypeParam;
use crate::ty::UnsoundnessKind;
use crate::ty::UpperBoundKind;
use crate::ty::debug_string_of_generic_kind;
use crate::ty::string_of_utility_ctor;
use crate::ty::types_of_utility;
use crate::ty_symbol::ImportMode;
use crate::ty_symbol::ImportedIdent;
use crate::ty_symbol::Provenance;
use crate::ty_symbol::Symbol;

pub fn string_of_polarity(p: Polarity) -> &'static str {
    match p {
        Polarity::Negative => "Negative",
        Polarity::Neutral => "Neutral",
        Polarity::Positive => "Positive",
    }
}

pub fn string_of_ctor_t<L>(t: &Ty<L>) -> &'static str {
    match t {
        Ty::Truncated => "Truncated",
        Ty::Bound(_) => "Bound",
        Ty::Generic(_) => "Generic",
        Ty::Any(AnyKind::Annotated(_)) => "Explicit Any",
        Ty::Any(_) => "Implicit Any",
        Ty::Top => "Top",
        Ty::Bot(_) => "Bot",
        Ty::Void => "Void",
        Ty::Undefined => "Undefined",
        Ty::TsVoid => "TsVoid",
        Ty::Null => "Null",
        Ty::Symbol => "Symbol",
        Ty::UniqueSymbol => "UniqueSymbol",
        Ty::Num => "Num",
        Ty::Str => "Str",
        Ty::Bool => "Bool",
        Ty::BigInt => "BigInt",
        Ty::NumLit(_) => "NumLit",
        Ty::StrLit(_) => "StrLit",
        Ty::BoolLit(_) => "BoolLit",
        Ty::BigIntLit(_) => "BigIntLit",
        Ty::Fun(_) => "Fun",
        Ty::Obj(_) => "Obj",
        Ty::Arr(_) => "Arr",
        Ty::Tup { .. } => "Tup",
        Ty::Union(_, _, _, _) => "Union",
        Ty::Inter(_, _, _) => "Inter",
        Ty::InlineInterface(_) => "InlineInterface",
        Ty::TypeOf(_) => "Typeof",
        Ty::Utility(_) => "Utility",
        Ty::IndexedAccess { .. } => "IndexedAccess",
        Ty::Conditional { .. } => "Conditional",
        Ty::Infer(_) => "Infer",
        Ty::Component { .. } => "Component",
        Ty::Renders(_, _) => "Renders",
        Ty::TemplateLiteral { .. } => "TemplateLiteral",
    }
}

pub fn string_of_ctor_decl<L>(d: &Decl<L>) -> &'static str {
    match d {
        Decl::TypeAliasDecl(..) => "TypeAlias",
        Decl::ClassDecl(..) => "ClassDecl",
        Decl::InterfaceDecl(..) => "InterfaceDecl",
        Decl::RecordDecl(..) => "RecordDecl",
        Decl::ModuleDecl(..) => "Module",
        Decl::NamespaceDecl(..) => "NamespaceDecl",
        Decl::VariableDecl(..) => "VariableDecl",
        Decl::NominalComponentDecl(..) => "NominalComponentDecl",
        Decl::EnumDecl(..) => "EnumDecl",
    }
}

fn dump_any_error_kind(kind: Option<AnyErrorKind>) -> &'static str {
    match kind {
        Some(AnyErrorKind::UnresolvedName) => "UnresolvedName",
        Some(AnyErrorKind::MissingAnnotation) => "MissingAnnotation",
        None => "<None>",
    }
}

pub fn dump_any_unsoundness_kind(kind: UnsoundnessKind) -> &'static str {
    match kind {
        UnsoundnessKind::BoundFunctionThis => "BoundFunctionThis",
        UnsoundnessKind::Constructor => "Constructor",
        UnsoundnessKind::DummyStatic => "DummyStatic",
        UnsoundnessKind::Exports => "Exports",
        UnsoundnessKind::InferenceHooks => "InferenceHooks",
        UnsoundnessKind::InstanceOfRefinement => "InstanceOfRefinement",
        UnsoundnessKind::Merged => "Merged",
        UnsoundnessKind::ResolveSpread => "ResolveSpread",
        UnsoundnessKind::Unchecked => "Unchecked",
        UnsoundnessKind::Unimplemented => "Unimplemented",
        UnsoundnessKind::UnresolvedType => "UnresolvedType",
        UnsoundnessKind::NonBindingPattern => "NonBindingPattern",
    }
}

fn dump_any_kind<L>(kind: &AnyKind<L>) -> String {
    match kind {
        AnyKind::Annotated(_) => "Annotated".to_string(),
        AnyKind::AnyError(kind) => format!("AnyError ({})", dump_any_error_kind(*kind)),
        AnyKind::Recursive => "Recursive".to_string(),
        AnyKind::Unsound(kind) => format!("Unsound ({})", dump_any_unsoundness_kind(*kind)),
        AnyKind::Untyped => "Untyped".to_string(),
        AnyKind::Placeholder => "Placeholder".to_string(),
    }
}

const DEFAULT_DEPTH: i32 = 10;

fn cut_off(s: &str, limit: usize) -> String {
    if s.len() > limit {
        format!("{} ...", &s[..limit - 1])
    } else {
        s.to_string()
    }
}

fn dump_bot_upper_bound_kind<L>(kind: &UpperBoundKind<L>) -> String {
    match kind {
        UpperBoundKind::NoUpper => "NoUpper".to_string(),
        UpperBoundKind::SomeKnownUpper(_) => "SomeKnownUpper".to_string(),
        UpperBoundKind::SomeUnknownUpper(u) => format!("SomeUnknownUpper ({})", u),
    }
}

fn dump_bot_kind<L>(kind: &BotKind<L>) -> String {
    match kind {
        BotKind::EmptyType => "EmptyType".to_string(),
        BotKind::NoLowerWithUpper(u) => {
            format!("NoLowerWithUpper ({})", dump_bot_upper_bound_kind(u))
        }
    }
}

fn dump_import_mode(mode: ImportMode) -> &'static str {
    match mode {
        ImportMode::ValueMode => "value",
        ImportMode::TypeMode => "type",
        ImportMode::TypeofMode => "typeof",
    }
}

fn dump_import_ident<L>(ident: &ImportedIdent<L>) -> String {
    let ImportedIdent(_, id, mode) = ident;
    format!("`{}` ({})", id, dump_import_mode(*mode))
}

fn ctor_of_provenance<L>(p: &Provenance<L>) -> String {
    match p {
        Provenance::Local => "Local".to_string(),
        Provenance::Remote(info) => match &info.imported_as {
            Some(ii) => format!("Imported as {}", dump_import_ident(ii)),
            None => "Remote".to_string(),
        },
        Provenance::Library(info) => match &info.imported_as {
            Some(_) => "Library (Imported)".to_string(),
            None => "Library (Remote)".to_string(),
        },
        Provenance::Builtin => "Builtin".to_string(),
    }
}

fn dump_symbol<L: Debug>(sym: &Symbol<L>) -> String {
    format!(
        "{} ({}:{:?})",
        sym.sym_name,
        ctor_of_provenance(&sym.sym_provenance),
        sym.sym_def_loc
    )
}

fn builtin_value<L: Debug>(v: &BuiltinOrSymbol<L>) -> String {
    match v {
        BuiltinOrSymbol::FunProto => "Function.prototype".to_string(),
        BuiltinOrSymbol::ObjProto => "Object.prototype".to_string(),
        BuiltinOrSymbol::TSymbol(s) => dump_symbol(s),
    }
}

fn dump_property_polarity(p: Polarity) -> &'static str {
    match p {
        Polarity::Positive => "readonly ",
        Polarity::Negative => "writeonly ",
        Polarity::Neutral => "",
    }
}

fn dump_type_param_polarity(p: Polarity) -> &'static str {
    match p {
        Polarity::Positive => "out",
        Polarity::Negative => "in",
        Polarity::Neutral => "",
    }
}

fn dump_param_opt(p: &FunParam) -> &'static str {
    if p.prm_optional { "?" } else { "" }
}

fn dump_param<L: Debug + Clone + Dupe>(
    depth: i32,
    name: &Option<FlowSmolStr>,
    t: &Ty<L>,
    p: &FunParam,
) -> String {
    match name {
        Some(s) => format!("{}{}: {}", s, dump_param_opt(p), dump_t(depth, t)),
        None => format!("{}{}", dump_param_opt(p), dump_t(depth, t)),
    }
}

fn dump_rest_params<L: Debug + Clone + Dupe>(
    depth: i32,
    rest: Option<&(Option<FlowSmolStr>, Arc<Ty<L>>)>,
) -> String {
    match rest {
        Some((name, t)) => format!(
            "...{}",
            dump_param(
                depth,
                name,
                t,
                &FunParam {
                    prm_optional: false
                }
            )
        ),
        None => String::new(),
    }
}

fn dump_bound<L: Debug + Clone + Dupe>(depth: i32, bound: Option<&Ty<L>>) -> String {
    match bound {
        Some(t) => format!(" extends {}", dump_t(depth, t)),
        None => String::new(),
    }
}

fn dump_type_param<L: Debug + Clone + Dupe>(depth: i32, tp: &TypeParam<L>) -> String {
    format!(
        "TParam({}, {}, {})",
        dump_type_param_polarity(tp.tp_polarity),
        tp.tp_name,
        dump_bound(depth, tp.tp_bound.as_deref())
    )
}

fn dump_type_params<L: Debug + Clone + Dupe>(depth: i32, tps: Option<&[TypeParam<L>]>) -> String {
    match tps {
        Some([]) => String::new(),
        Some(ps) => {
            let params: Vec<String> = ps.iter().map(|tp| dump_type_param(depth, tp)).collect();
            format!("TypeParams({})", params.join(", "))
        }
        None => String::new(),
    }
}

fn dump_fun_t<L: Debug + Clone + Dupe>(depth: i32, f: &FunT<L>) -> String {
    let params: Vec<String> = f
        .fun_this_param
        .iter()
        .map(|t| format!("this: {}", dump_t(depth, t)))
        .chain(
            f.fun_params
                .iter()
                .map(|(name, t, p)| dump_param(depth, name, t, p)),
        )
        .collect();
    format!(
        "Fun({}, {}, {}, {}, out: {}, hook: {})",
        dump_type_params(depth, f.fun_type_params.as_deref()),
        params.join(", "),
        dump_rest_params(depth, f.fun_rest_param.as_ref()),
        dump_t(depth, &f.fun_static),
        dump_return_t(depth, &f.fun_return),
        f.fun_effect == FunEffect::Hook
    )
}

fn dump_return_t<L: Debug + Clone + Dupe>(depth: i32, ret: &ReturnT<L>) -> String {
    match ret {
        ReturnT::ReturnType(t) => dump_t(depth, t),
        ReturnT::TypeGuard(kind, x, t) => {
            let prefix = crate::ty::type_guard_kind_prefix(*kind);
            match t {
                Some(t) => format!("{}{} is {}", prefix, x, dump_t(depth, t)),
                None => format!("{}{}", prefix, x),
            }
        }
    }
}

fn dump_tuple_element<L: Debug + Clone + Dupe>(
    depth: i32,
    i: usize,
    name: &Option<FlowSmolStr>,
    t: &Ty<L>,
    polarity: Polarity,
    optional: bool,
) -> String {
    if name.is_none() && polarity == Polarity::Neutral && !optional {
        dump_t(depth, t)
    } else {
        let name_str = name
            .clone()
            .unwrap_or_else(|| FlowSmolStr::new(format!("element_{}", i)));
        let opt_str = if optional { "?" } else { "" };
        format!(
            "{}{}{}: {}",
            dump_property_polarity(polarity),
            name_str,
            opt_str,
            dump_t(depth, t)
        )
    }
}

fn dump_tuple_spread<L: Debug + Clone + Dupe>(
    depth: i32,
    name: &Option<FlowSmolStr>,
    t: &Ty<L>,
) -> String {
    let name_str = match name {
        Some(n) => format!("{}: ", n),
        None => String::new(),
    };
    format!("...{}{}", name_str, dump_t(depth, t))
}

fn dump_field<L: Debug + Clone + Dupe>(
    depth: i32,
    x: &str,
    t: &Ty<L>,
    polarity: Polarity,
    optional: bool,
) -> String {
    let opt_str = if optional { "?" } else { "" };
    format!(
        "{}{}{}: {}",
        dump_property_polarity(polarity),
        x,
        opt_str,
        dump_t(depth, t)
    )
}

fn dump_prop<L: Debug + Clone + Dupe>(depth: i32, prop: &Prop<L>) -> String {
    match prop {
        Prop::NamedProp { name, prop, .. } => {
            dump_named_prop(depth, &name.display_smol_str(), prop)
        }
        Prop::CallProp(f) => dump_fun_t(depth, f),
        Prop::SpreadProp(t) => dump_spread(depth, t),
        Prop::MappedTypeProp {
            key_tparam,
            source,
            prop,
            name_type,
            flags,
            homomorphic,
        } => dump_mapped_type(
            depth,
            key_tparam,
            source,
            prop,
            name_type.as_deref(),
            flags,
            homomorphic,
        ),
    }
}

fn dump_named_prop<L: Debug + Clone + Dupe>(depth: i32, x: &str, prop: &NamedProp<L>) -> String {
    match prop {
        NamedProp::Field {
            t,
            polarity,
            optional,
        } => dump_field(depth, x, t, *polarity, *optional),
        NamedProp::Method { fun, optional } => format!(
            "{}{}",
            if *optional { "optional " } else { "" },
            dump_fun_t(depth, fun)
        ),
        NamedProp::Get(t) => format!("get {}", dump_t(depth, t)),
        NamedProp::Set(t) => format!("set {}", dump_t(depth, t)),
    }
}

fn dump_dict<L: Debug + Clone + Dupe>(depth: i32, d: &Dict<L>) -> String {
    let name_str = match &d.dict_name {
        Some(n) => format!("{}: ", n),
        None => String::new(),
    };
    format!(
        "{}[{}{}]: {}",
        dump_property_polarity(d.dict_polarity),
        name_str,
        dump_t(depth, &d.dict_key),
        dump_t(depth, &d.dict_value)
    )
}

fn dump_spread<L: Debug + Clone + Dupe>(depth: i32, t: &Ty<L>) -> String {
    format!("...{}", dump_t(depth, t))
}

fn dump_mapped_type_variance(v: MappedTypeVariance) -> String {
    match v {
        MappedTypeVariance::OverrideVariance(pol) => dump_property_polarity(pol).to_string(),
        MappedTypeVariance::RemoveVariance(Polarity::Positive) => "-readonly ".to_string(),
        MappedTypeVariance::RemoveVariance(Polarity::Negative | Polarity::Neutral) => String::new(),
        MappedTypeVariance::KeepVariance => String::new(),
    }
}

fn dump_mapped_type<L: Debug + Clone + Dupe>(
    depth: i32,
    key_tparam: &TypeParam<L>,
    source: &Ty<L>,
    prop: &Ty<L>,
    name_type: Option<&Ty<L>>,
    flags: &crate::ty::MappedTypeFlags,
    homomorphic: &MappedTypeHomomorphicFlag<L>,
) -> String {
    let keyof = match homomorphic {
        MappedTypeHomomorphicFlag::Homomorphic => "keyof ",
        _ => "",
    };
    let optional_str = match flags.optional {
        MappedTypeOptionalFlag::KeepOptionality => "",
        MappedTypeOptionalFlag::MakeOptional => "?",
        MappedTypeOptionalFlag::RemoveOptional => "-?",
    };
    let as_str = match name_type {
        None => String::new(),
        Some(nt) => format!(" as {}", dump_t(depth, nt)),
    };
    format!(
        "{}[{} in {}{}{}]{}: {}",
        dump_mapped_type_variance(flags.variance),
        key_tparam.tp_name,
        keyof,
        dump_t(depth, source),
        as_str,
        optional_str,
        dump_t(depth, prop)
    )
}

fn dump_obj<L: Debug + Clone + Dupe>(depth: i32, o: &ObjT<L>) -> String {
    let props: Vec<String> = o.obj_props.iter().map(|p| dump_prop(depth, p)).collect();
    let props_str = props.join(", ");
    match &o.obj_kind {
        ObjKind::ExactObj => format!("{{{}}}", props_str),
        ObjKind::InexactObj => format!("{{{}, ...}}", props_str),
        ObjKind::IndexedObj(d) => format!("{{{}, {}}}", props_str, dump_dict(depth, d)),
        ObjKind::MappedTypeObj => format!("{{{}}}", props_str),
    }
}

fn dump_arr<L: Debug + Clone + Dupe>(depth: i32, a: &ArrT<L>) -> String {
    let ctor = if a.arr_readonly {
        "ReadonlyArray"
    } else {
        "Array"
    };
    format!("{}<{}>", ctor, dump_t(depth, &a.arr_elt_t))
}

fn dump_generic<L: Debug + Clone + Dupe>(depth: i32, g: &GenericT<L>) -> String {
    let (s, kind, ts) = g;
    format!(
        "Generic ({}, kind= {}, params={})",
        dump_symbol(s),
        debug_string_of_generic_kind(*kind),
        dump_generics(depth, ts.as_deref())
    )
}

fn dump_generics<L: Debug + Clone + Dupe>(depth: i32, ts: Option<&[Arc<Ty<L>>]>) -> String {
    match ts {
        Some(ts) => {
            let types: Vec<String> = ts.iter().map(|t| dump_t(depth, t)).collect();
            format!("<{}>", types.join(", "))
        }
        None => String::new(),
    }
}

fn utility_ctor<L>(u: &crate::ty::Utility<L>) -> &'static str {
    match u {
        crate::ty::Utility::Keys(_) => "keyof",
        crate::ty::Utility::ElementType(_, _) => "indexed_access",
        _ => string_of_utility_ctor(u),
    }
}

fn dump_utility<L: Debug + Clone + Dupe>(depth: i32, u: &crate::ty::Utility<L>) -> String {
    match u {
        crate::ty::Utility::Keys(t) => return format!("keyof {}", dump_t(depth, t)),
        crate::ty::Utility::ElementType(object, index) => {
            return format!("{}[{}]", dump_t(depth, object), dump_t(depth, index));
        }
        _ => {}
    }
    let ctor = utility_ctor(u);
    let ts = types_of_utility(u);
    match ts {
        None => ctor.to_string(),
        Some(ts) if ts.is_empty() => ctor.to_string(),
        Some(ts) => {
            let types: Vec<String> = ts.iter().map(|t| dump_t(depth, t)).collect();
            format!("{} ({})", ctor, types.join(", "))
        }
    }
}

fn dump_t<L: Debug + Clone + Dupe>(depth: i32, t: &Ty<L>) -> String {
    if depth < 0 {
        return "...".to_string();
    }
    let depth = depth - 1;
    match t {
        Ty::Truncated => "Truncated".to_string(),
        Ty::Bound(data) => {
            let (_, s) = data.as_ref();
            format!("Bound({})", s)
        }
        Ty::Generic(g) => dump_generic(depth, g),
        Ty::Any(kind) => format!("Any ({})", dump_any_kind(kind)),
        Ty::Top => "Top".to_string(),
        Ty::Bot(k) => format!("Bot ({})", dump_bot_kind(k)),
        Ty::Void => "Void".to_string(),
        Ty::Undefined => "Undefined".to_string(),
        Ty::TsVoid => "TsVoid".to_string(),
        Ty::Null => "Null".to_string(),
        Ty::Symbol => "Symbol".to_string(),
        Ty::UniqueSymbol => "UniqueSymbol".to_string(),
        Ty::Num => "Num".to_string(),
        Ty::NumLit(s) => format!("\"{}\"", s),
        Ty::Str => "Str".to_string(),
        Ty::StrLit(s) => format!("\"{}\"", s),
        Ty::Bool => "Bool".to_string(),
        Ty::BoolLit(b) => format!("\"{}\"", b),
        Ty::BigInt => "BigInt".to_string(),
        Ty::BigIntLit(s) => format!("\"{}\"", s),
        Ty::Fun(f) => dump_fun_t(depth, f),
        Ty::Obj(o) => dump_obj(depth, o),
        Ty::Arr(a) => dump_arr(depth, a),
        Ty::Tup { elements, inexact } => {
            let els: Vec<String> = elements
                .iter()
                .enumerate()
                .map(|(i, el)| match el {
                    TupleElement::TupleElement {
                        name,
                        t,
                        polarity,
                        optional,
                    } => dump_tuple_element(depth, i, name, t, *polarity, *optional),
                    TupleElement::TupleSpread { t, name } => dump_tuple_spread(depth, name, t),
                })
                .collect();
            let inexact_str = if *inexact { ", ..." } else { "" };
            format!("Tup ({}{})", els.join(","), inexact_str)
        }
        Ty::Union(_, t1, t2, rest) => {
            let mut ts: Vec<&Ty<L>> = vec![t1.as_ref(), t2.as_ref()];
            ts.extend(rest.iter().take(8).map(|t| t.as_ref()));
            let types: Vec<String> = ts.iter().map(|t| dump_t(depth, t)).collect();
            format!("Union ({})", types.join(", "))
        }
        Ty::Inter(t1, t2, rest) => {
            let mut ts: Vec<&Ty<L>> = vec![t1.as_ref(), t2.as_ref()];
            ts.extend(rest.iter().map(|t| t.as_ref()));
            let types: Vec<String> = ts.iter().map(|t| dump_t(depth, t)).collect();
            format!("Inter ({})", types.join(", "))
        }
        Ty::InlineInterface(iface) => {
            let InterfaceT {
                if_extends,
                if_props,
                if_dict,
            } = iface.as_ref();
            let dict_str = match if_dict {
                Some(d) => dump_dict(depth, d),
                None => String::new(),
            };
            let extends: Vec<String> = if_extends.iter().map(|g| dump_generic(depth, g)).collect();
            let props: Vec<String> = if_props.iter().map(|p| dump_prop(depth, p)).collect();
            format!(
                "InlineInterface ({}, {{ {} {} }})",
                extends.join(", "),
                props.join(", "),
                dict_str
            )
        }
        Ty::TypeOf(data) => {
            let (v, ts) = data.as_ref();
            format!(
                "Typeof ({}, {})",
                builtin_value(v),
                dump_generics(depth, ts.as_deref())
            )
        }
        Ty::Utility(u) => dump_utility(depth, u),
        Ty::IndexedAccess {
            _object,
            index,
            optional,
        } => format!(
            "IndexedAccess ({}) ({}) (optional={})",
            dump_t(depth, _object),
            dump_t(depth, index),
            optional
        ),
        Ty::Conditional {
            check_type,
            extends_type,
            true_type,
            false_type,
        } => format!(
            "Conditional ({}, {}, {}, {})",
            dump_t(depth, check_type),
            dump_t(depth, extends_type),
            dump_t(depth, true_type),
            dump_t(depth, false_type)
        ),
        Ty::Infer(data) => {
            let (s, b) = data.as_ref();
            let bound_str = match b {
                Some(t) => dump_t(depth, t),
                None => "None".to_string(),
            };
            format!("Infer ({}, {})", dump_symbol(s), bound_str)
        }
        Ty::Component {
            regular_props,
            renders,
        } => {
            let props = match regular_props {
                ComponentProps::UnflattenedComponentProps(t) => {
                    vec![format!("...{}", dump_t(depth, t))]
                }
                ComponentProps::FlattenedComponentProps {
                    props,
                    inexact,
                    dict,
                } => {
                    let mut prop_strs: Vec<String> = props
                        .iter()
                        .map(|p| match p {
                            FlattenedComponentProp::FlattenedComponentProp {
                                name,
                                optional,
                                t,
                                ..
                            } => {
                                let opt = if *optional { "?" } else { "" };
                                format!("{}{}: {}", name, opt, dump_t(depth, t))
                            }
                        })
                        .collect();
                    if *inexact {
                        prop_strs.push("...{...}".to_string());
                    }
                    if let Some(dict) = dict {
                        prop_strs.push(format!(
                            "...{{[{}]: {}}}",
                            dump_t(depth, &dict.dict_key),
                            dump_t(depth, &dict.dict_value)
                        ));
                    }
                    prop_strs
                }
            };
            let renders_str = match renders {
                Some(r) => dump_t(depth, r),
                None => "<missing>".to_string(),
            };
            format!("Component({}): {}", props.join(", "), renders_str)
        }
        Ty::Renders(t, _) => format!("Renders ({})", dump_t(depth, t)),
        Ty::TemplateLiteral { quasis, types } => {
            let types_str = types
                .iter()
                .map(|t| dump_t(depth, t))
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "TemplateLiteral(quasis=[{}], types=[{}])",
                quasis
                    .iter()
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>()
                    .join("; "),
                types_str
            )
        }
    }
}

fn dump_decl<L: Debug + Clone + Dupe>(depth: i32, d: &Decl<L>) -> String {
    match d {
        Decl::VariableDecl(box (name, t)) => {
            format!("VariableDecl ({}, {})", name, dump_t(depth, t))
        }
        Decl::TypeAliasDecl(box DeclTypeAliasDeclData {
            import,
            name,
            tparams,
            type_,
            opaque,
        }) => {
            let type_str = match type_ {
                Some(t) => cut_off(&dump_t(depth, t), 1000),
                None => String::new(),
            };
            let dump_bound = |t: &Option<Arc<Ty<L>>>| match t {
                Some(t) => cut_off(&dump_t(depth, t), 1000),
                None => String::new(),
            };
            let opaque_str = match opaque {
                Some(OpaqueBounds { lower, upper }) => {
                    format!(
                        ", opaque (super={}, extends={})",
                        dump_bound(lower),
                        dump_bound(upper)
                    )
                }
                None => String::new(),
            };
            format!(
                "TypeAlias (import={}, {}, {}, {}{})",
                import,
                dump_symbol(name),
                dump_type_params(depth, tparams.as_deref()),
                type_str,
                opaque_str
            )
        }
        Decl::ClassDecl(box (s, ps)) => format!(
            "ClassDecl ({}) ({})",
            dump_symbol(s),
            dump_type_params(depth, ps.as_deref())
        ),
        Decl::InterfaceDecl(box (s, ps)) => format!(
            "InterfaceDecl ({}) ({})",
            dump_symbol(s),
            dump_type_params(depth, ps.as_deref())
        ),
        Decl::RecordDecl(box (s, ps)) => format!(
            "RecordDecl ({}) ({})",
            dump_symbol(s),
            dump_type_params(depth, ps.as_deref())
        ),
        Decl::EnumDecl(box DeclEnumDeclData {
            name,
            members,
            has_unknown_members,
            truncated_members_count,
        }) => {
            let members_str = match members {
                None => String::new(),
                Some(ms) => {
                    let ms_str = ms.iter().map(|m| m.as_str()).collect::<Vec<_>>().join(", ");
                    if *truncated_members_count > 0 {
                        format!(
                            " {{ {}, /* ... {} more members */ }}",
                            ms_str, truncated_members_count
                        )
                    } else if *has_unknown_members {
                        format!(" {{ {}, ... }}", ms_str)
                    } else {
                        format!(" {{ {} }}", ms_str)
                    }
                }
            };
            format!("Enum({}){}", dump_symbol(name), members_str)
        }
        Decl::NominalComponentDecl(box DeclNominalComponentDeclData {
            name,
            tparams,
            is_type,
            ..
        }) => format!(
            "NominalComponentDecl ({}, {}, {})",
            dump_symbol(name),
            dump_type_params(depth, tparams.as_deref()),
            is_type
        ),
        Decl::NamespaceDecl(box DeclNamespaceDeclData { name, exports }) => {
            let name_str = match name {
                Some(n) => dump_symbol(n),
                None => "<no name>".to_string(),
            };
            let exports_str: Vec<String> = exports.iter().map(|d| dump_decl(depth, d)).collect();
            format!("Namespace({}, {})", name_str, exports_str.join(", "))
        }
        Decl::ModuleDecl(box DeclModuleDeclData {
            name,
            exports,
            default: _,
        }) => {
            let name_str = match name {
                Some(n) => dump_symbol(n),
                None => "<no name>".to_string(),
            };
            let exports_str: Vec<String> = exports.iter().map(|d| dump_decl(depth, d)).collect();
            format!("Module({}, {})", name_str, exports_str.join(", "))
        }
    }
}

fn dump_elt<L: Debug + Clone + Dupe>(depth: i32, elt: &Elt<L>) -> String {
    match elt {
        Elt::Type(t) => format!("Type ({})", dump_t(depth, t)),
        Elt::Decl(d) => format!("Decl ({})", dump_decl(depth, d)),
    }
}

use std::path::Path;

use serde_json::Value as Json;

pub trait ALocToLoc<L> {
    fn loc_to_string(&self, loc: &L, strip_root: Option<&Path>) -> String;
}

pub struct AlocToLocFn<'a, F>
where
    F: Fn(&flow_aloc::ALoc) -> flow_parser::loc::Loc + 'a,
{
    pub f: F,
    _marker: std::marker::PhantomData<&'a ()>,
}

impl<'a, F> AlocToLocFn<'a, F>
where
    F: Fn(&flow_aloc::ALoc) -> flow_parser::loc::Loc + 'a,
{
    pub fn new(f: F) -> Self {
        Self {
            f,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<'a, F> ALocToLoc<flow_aloc::ALoc> for AlocToLocFn<'a, F>
where
    F: Fn(&flow_aloc::ALoc) -> flow_parser::loc::Loc + 'a,
{
    fn loc_to_string(&self, loc: &flow_aloc::ALoc, strip_root: Option<&Path>) -> String {
        let strip_root_str = strip_root.and_then(|p| p.to_str());
        let concrete = (self.f)(loc);
        flow_common::reason::string_of_loc(strip_root_str, &concrete)
    }
}

/// Formats an ALoc directly (its keyed-or-concrete representation) without concretizing.
pub struct AlocOnlyConverter;

impl ALocToLoc<flow_aloc::ALoc> for AlocOnlyConverter {
    fn loc_to_string(&self, loc: &flow_aloc::ALoc, strip_root: Option<&Path>) -> String {
        let strip_root_str = strip_root.and_then(|p| p.to_str());
        flow_common::reason::string_of_aloc(strip_root_str, loc)
    }
}

fn json_of_provenance<L>(
    converter: &dyn ALocToLoc<L>,
    loc: &L,
    p: &Provenance<L>,
    strip_root: Option<&Path>,
) -> Json {
    Json::Object(serde_json::Map::from_iter(vec![
        (
            "kind".to_string(),
            Json::String(ctor_of_provenance(p).to_string()),
        ),
        (
            "loc".to_string(),
            Json::String(converter.loc_to_string(loc, strip_root)),
        ),
    ]))
}

fn json_of_symbol<L>(
    converter: &dyn ALocToLoc<L>,
    sym: &Symbol<L>,
    strip_root: Option<&Path>,
) -> Json {
    Json::Object(serde_json::Map::from_iter(vec![
        (
            "provenance".to_string(),
            json_of_provenance(converter, &sym.sym_def_loc, &sym.sym_provenance, strip_root),
        ),
        ("name".to_string(), Json::String(sym.sym_name.to_string())),
    ]))
}

fn json_of_builtin_value<L>(
    converter: &dyn ALocToLoc<L>,
    v: &BuiltinOrSymbol<L>,
    strip_root: Option<&Path>,
) -> Json {
    match v {
        BuiltinOrSymbol::FunProto => Json::String("Function.prototype".to_string()),
        BuiltinOrSymbol::ObjProto => Json::String("Object.prototype".to_string()),
        BuiltinOrSymbol::TSymbol(s) => json_of_symbol(converter, s, strip_root),
    }
}

fn json_of_polarity(polarity: Polarity) -> Json {
    Json::String(string_of_polarity(polarity).to_string())
}

fn json_of_mapped_type_variance(v: MappedTypeVariance) -> Json {
    match v {
        MappedTypeVariance::OverrideVariance(pol) => json_of_polarity(pol),
        MappedTypeVariance::RemoveVariance(pol) => {
            Json::String(format!("-{}", string_of_polarity(pol)))
        }
        MappedTypeVariance::KeepVariance => Json::String("".to_string()),
    }
}

fn json_of_targs<L: Debug + Clone + Dupe>(
    converter: &dyn ALocToLoc<L>,
    targs_opt: Option<&[Arc<Ty<L>>]>,
    strip_root: Option<&Path>,
) -> Vec<(String, Json)> {
    match targs_opt {
        Some(targs) => {
            let arr: Vec<Json> = targs
                .iter()
                .map(|t| json_of_t(converter, t, strip_root))
                .collect();
            vec![("typeArgs".to_string(), Json::Array(arr))]
        }
        None => vec![],
    }
}

fn json_of_typeparam<L: Debug + Clone + Dupe>(
    converter: &dyn ALocToLoc<L>,
    tp: &TypeParam<L>,
    strip_root: Option<&Path>,
) -> Json {
    let mut obj = vec![
        ("name".to_string(), Json::String(tp.tp_name.to_string())),
        (
            "bound".to_string(),
            tp.tp_bound
                .as_ref()
                .map(|b| json_of_t(converter, b.as_ref(), strip_root))
                .unwrap_or(Json::Null),
        ),
        ("polarity".to_string(), json_of_polarity(tp.tp_polarity)),
        ("const".to_string(), Json::Bool(tp.tp_const)),
    ];
    if let Some(default) = &tp.tp_default {
        obj.push((
            "default".to_string(),
            json_of_t(converter, default.as_ref(), strip_root),
        ));
    }
    Json::Object(serde_json::Map::from_iter(obj))
}

fn json_of_type_params<L: Debug + Clone + Dupe>(
    converter: &dyn ALocToLoc<L>,
    ps: Option<&[TypeParam<L>]>,
    strip_root: Option<&Path>,
) -> Json {
    match ps {
        None => Json::Null,
        Some(tparams) => {
            let arr: Vec<Json> = tparams
                .iter()
                .map(|tp| json_of_typeparam(converter, tp, strip_root))
                .collect();
            Json::Array(arr)
        }
    }
}

fn json_of_return_t<L: Debug + Clone + Dupe>(
    converter: &dyn ALocToLoc<L>,
    ret: &ReturnT<L>,
    strip_root: Option<&Path>,
) -> Json {
    match ret {
        ReturnT::ReturnType(t) => Json::Object(serde_json::Map::from_iter(vec![(
            "type_".to_string(),
            json_of_t(converter, t, strip_root),
        )])),
        ReturnT::TypeGuard(kind, x, t) => Json::Object(serde_json::Map::from_iter(vec![(
            "type_guard".to_string(),
            Json::Object(serde_json::Map::from_iter(vec![
                (
                    "kind".to_string(),
                    Json::String(format!("{:?}", kind).to_lowercase()),
                ),
                ("type_parameter".to_string(), Json::String(x.to_string())),
                (
                    "type_".to_string(),
                    match t {
                        Some(t) => json_of_t(converter, t, strip_root),
                        None => Json::Null,
                    },
                ),
            ])),
        )])),
    }
}

fn json_of_fun_t<L: Debug + Clone + Dupe>(
    converter: &dyn ALocToLoc<L>,
    f: &FunT<L>,
    strip_root: Option<&Path>,
) -> Vec<(String, Json)> {
    let param_types: Vec<Json> = f
        .fun_params
        .iter()
        .map(|(_, t, _)| json_of_t(converter, t, strip_root))
        .collect();
    let param_names: Vec<Json> = f
        .fun_params
        .iter()
        .rev()
        .map(|(name, _, _)| match name {
            Some(n) => Json::String(n.to_string()),
            None => Json::String("_".to_string()),
        })
        .collect();
    let rest_param = match &f.fun_rest_param {
        None => Json::Null,
        Some((name, t)) => {
            let mut obj = vec![(
                "restParamType".to_string(),
                json_of_t(converter, t.as_ref(), strip_root),
            )];
            if let Some(n) = name {
                obj.push(("restParamName".to_string(), Json::String(n.to_string())));
            }
            Json::Object(serde_json::Map::from_iter(obj))
        }
    };
    let mut result = vec![
        (
            "typeParams".to_string(),
            json_of_type_params(converter, f.fun_type_params.as_deref(), strip_root),
        ),
        ("paramTypes".to_string(), Json::Array(param_types)),
        ("paramNames".to_string(), Json::Array(param_names)),
        ("restParam".to_string(), rest_param),
        (
            "returnType".to_string(),
            json_of_return_t(converter, &f.fun_return, strip_root),
        ),
        (
            "staticType".to_string(),
            json_of_t(converter, &f.fun_static, strip_root),
        ),
        (
            "functionHook".to_string(),
            Json::Bool(f.fun_effect == FunEffect::Hook),
        ),
    ];
    if let Some(t) = &f.fun_this_param {
        result.push(("thisType".to_string(), json_of_t(converter, t, strip_root)));
    }
    result
}

fn json_of_dict<L: Debug + Clone + Dupe>(
    converter: &dyn ALocToLoc<L>,
    d: &Dict<L>,
    strip_root: Option<&Path>,
) -> Json {
    Json::Object(serde_json::Map::from_iter(vec![
        ("polarity".to_string(), json_of_polarity(d.dict_polarity)),
        (
            "name".to_string(),
            Json::String(d.dict_name.as_deref().unwrap_or("_").to_string()),
        ),
        (
            "key".to_string(),
            json_of_t(converter, &d.dict_key, strip_root),
        ),
        (
            "value".to_string(),
            json_of_t(converter, &d.dict_value, strip_root),
        ),
    ]))
}

fn json_of_named_prop<L: Debug + Clone + Dupe>(
    converter: &dyn ALocToLoc<L>,
    p: &NamedProp<L>,
    strip_root: Option<&Path>,
) -> Json {
    match p {
        NamedProp::Field {
            t,
            polarity,
            optional,
        } => Json::Object(serde_json::Map::from_iter(vec![
            ("kind".to_string(), Json::String("Field".to_string())),
            ("type".to_string(), json_of_t(converter, t, strip_root)),
            ("polarity".to_string(), json_of_polarity(*polarity)),
            ("optional".to_string(), Json::Bool(*optional)),
        ])),
        NamedProp::Method { fun: f, optional } => Json::Object(serde_json::Map::from_iter(vec![
            ("kind".to_string(), Json::String("Method".to_string())),
            ("optional".to_string(), Json::Bool(*optional)),
            (
                "funtype".to_string(),
                Json::Object(serde_json::Map::from_iter(json_of_fun_t(
                    converter, f, strip_root,
                ))),
            ),
        ])),
        NamedProp::Get(t) => Json::Object(serde_json::Map::from_iter(vec![
            ("kind".to_string(), Json::String("Get".to_string())),
            ("type".to_string(), json_of_t(converter, t, strip_root)),
        ])),
        NamedProp::Set(t) => Json::Object(serde_json::Map::from_iter(vec![
            ("kind".to_string(), Json::String("Set".to_string())),
            ("type".to_string(), json_of_t(converter, t, strip_root)),
        ])),
    }
}

fn json_of_prop<L: Debug + Clone + Dupe>(
    converter: &dyn ALocToLoc<L>,
    prop: &Prop<L>,
    strip_root: Option<&Path>,
) -> Json {
    match prop {
        Prop::NamedProp {
            name,
            prop,
            inherited,
            source,
            def_locs,
        } => {
            let def_locs_arr: Vec<Json> = def_locs
                .iter()
                .map(|loc| Json::String(converter.loc_to_string(loc, strip_root)))
                .collect();
            Json::Object(serde_json::Map::from_iter(vec![
                ("kind".to_string(), Json::String("NamedProp".to_string())),
                (
                    "prop".to_string(),
                    Json::Object(serde_json::Map::from_iter(vec![
                        ("name".to_string(), Json::String(name.to_string())),
                        (
                            "prop".to_string(),
                            json_of_named_prop(converter, prop, strip_root),
                        ),
                        ("inherited".to_string(), Json::Bool(*inherited)),
                        (
                            "source".to_string(),
                            Json::String(crate::ty::string_of_prop_source(source).to_string()),
                        ),
                        ("def_locs".to_string(), Json::Array(def_locs_arr)),
                    ])),
                ),
            ]))
        }
        Prop::CallProp(ft) => Json::Object(serde_json::Map::from_iter(vec![
            ("kind".to_string(), Json::String("CallProp".to_string())),
            (
                "prop".to_string(),
                Json::Object(serde_json::Map::from_iter(json_of_fun_t(
                    converter, ft, strip_root,
                ))),
            ),
        ])),
        Prop::SpreadProp(t) => Json::Object(serde_json::Map::from_iter(vec![
            ("kind".to_string(), Json::String("SpreadProp".to_string())),
            ("prop".to_string(), json_of_t(converter, t, strip_root)),
        ])),
        Prop::MappedTypeProp {
            key_tparam,
            source,
            prop,
            name_type,
            flags,
            homomorphic,
        } => {
            let optional_str = match flags.optional {
                MappedTypeOptionalFlag::KeepOptionality => "KeepOptionality",
                MappedTypeOptionalFlag::RemoveOptional => "RemoveOptional",
                MappedTypeOptionalFlag::MakeOptional => "MakeOptional",
            };
            let homomorphic_str = match homomorphic {
                MappedTypeHomomorphicFlag::Homomorphic => "Homomorphic",
                MappedTypeHomomorphicFlag::Unspecialized => "Unspecialized",
                MappedTypeHomomorphicFlag::SemiHomomorphic(_) => "SemiHomomorphic",
            };
            Json::Object(serde_json::Map::from_iter(vec![
                (
                    "kind".to_string(),
                    Json::String("MappedTypeProp".to_string()),
                ),
                (
                    "prop".to_string(),
                    Json::Object(serde_json::Map::from_iter(vec![
                        (
                            "key_tparam".to_string(),
                            Json::String(key_tparam.tp_name.to_string()),
                        ),
                        (
                            "source".to_string(),
                            json_of_t(converter, source, strip_root),
                        ),
                        (
                            "homomorphic".to_string(),
                            Json::String(homomorphic_str.to_string()),
                        ),
                        ("prop".to_string(), json_of_t(converter, prop, strip_root)),
                        (
                            "name_type".to_string(),
                            match name_type {
                                None => Json::Null,
                                Some(nt) => json_of_t(converter, nt, strip_root),
                            },
                        ),
                        (
                            "flags".to_string(),
                            Json::Object(serde_json::Map::from_iter(vec![
                                (
                                    "variance".to_string(),
                                    json_of_mapped_type_variance(flags.variance),
                                ),
                                (
                                    "optional".to_string(),
                                    Json::String(optional_str.to_string()),
                                ),
                            ])),
                        ),
                    ])),
                ),
            ]))
        }
    }
}

fn json_of_obj_t<L: Debug + Clone + Dupe>(
    converter: &dyn ALocToLoc<L>,
    o: &ObjT<L>,
    strip_root: Option<&Path>,
) -> Vec<(String, Json)> {
    let obj_kind = match &o.obj_kind {
        ObjKind::ExactObj => Json::String("Exact".to_string()),
        ObjKind::InexactObj => Json::String("Inexact".to_string()),
        ObjKind::IndexedObj(d) => json_of_dict(converter, d, strip_root),
        ObjKind::MappedTypeObj => Json::String("MappedType".to_string()),
    };
    let props: Vec<Json> = o
        .obj_props
        .iter()
        .map(|p| json_of_prop(converter, p, strip_root))
        .collect();
    vec![
        (
            "def_loc".to_string(),
            o.obj_def_loc
                .as_ref()
                .map(|loc| Json::String(converter.loc_to_string(loc, strip_root)))
                .unwrap_or(Json::Null),
        ),
        ("obj_kind".to_string(), obj_kind),
        ("props".to_string(), Json::Array(props)),
    ]
}

fn json_of_generic<L: Debug + Clone + Dupe>(
    converter: &dyn ALocToLoc<L>,
    g: &GenericT<L>,
    strip_root: Option<&Path>,
) -> Vec<(String, Json)> {
    let (s, k, targs_opt) = g;
    let mut result = json_of_targs(converter, targs_opt.as_deref(), strip_root);
    result.push(("type".to_string(), json_of_symbol(converter, s, strip_root)));
    result.push((
        "generic_kind".to_string(),
        Json::String(debug_string_of_generic_kind(*k).to_string()),
    ));
    result
}

fn json_of_component<L: Debug + Clone + Dupe>(
    converter: &dyn ALocToLoc<L>,
    regular_props: &ComponentProps<L>,
    renders: &Option<Arc<Ty<L>>>,
    strip_root: Option<&Path>,
) -> Vec<(String, Json)> {
    let props = match regular_props {
        ComponentProps::UnflattenedComponentProps(t) => {
            Json::Object(serde_json::Map::from_iter(vec![
                ("kind".to_string(), Json::String("unflattened".to_string())),
                ("type".to_string(), json_of_t(converter, t, strip_root)),
            ]))
        }
        ComponentProps::FlattenedComponentProps {
            props,
            inexact,
            dict,
        } => {
            let props_arr: Vec<Json> = props
                .iter()
                .map(|p| match p {
                    FlattenedComponentProp::FlattenedComponentProp {
                        name, optional, t, ..
                    } => Json::Object(serde_json::Map::from_iter(vec![
                        ("name".to_string(), Json::String(name.to_string())),
                        ("optional".to_string(), Json::Bool(*optional)),
                        ("type".to_string(), json_of_t(converter, t, strip_root)),
                    ])),
                })
                .collect();
            Json::Object(serde_json::Map::from_iter(vec![
                ("kind".to_string(), Json::String("flattened".to_string())),
                ("types".to_string(), Json::Array(props_arr)),
                ("inexact".to_string(), Json::Bool(*inexact)),
                (
                    "dict".to_string(),
                    dict.as_ref()
                        .map(|d| json_of_dict(converter, d, strip_root))
                        .unwrap_or(Json::Null),
                ),
            ]))
        }
    };
    vec![
        ("regularProps".to_string(), props),
        (
            "renders".to_string(),
            renders
                .as_ref()
                .map(|r| json_of_t(converter, r.as_ref(), strip_root))
                .unwrap_or(Json::Null),
        ),
    ]
}

fn json_of_utility<L: Debug + Clone + Dupe>(
    converter: &dyn ALocToLoc<L>,
    u: &crate::ty::Utility<L>,
    strip_root: Option<&Path>,
) -> Vec<(String, Json)> {
    let ctor = utility_ctor(u);
    let mut result = vec![("kind".to_string(), Json::String(ctor.to_string()))];
    if let Some(ts) = types_of_utility(u) {
        let targs: Vec<Json> = ts
            .iter()
            .map(|t| json_of_t(converter, t, strip_root))
            .collect();
        result.push(("typeArgs".to_string(), Json::Array(targs)));
    }
    result
}

fn json_of_t_list<L: Debug + Clone + Dupe>(
    converter: &dyn ALocToLoc<L>,
    t: &Ty<L>,
    strip_root: Option<&Path>,
) -> Vec<(String, Json)> {
    match t {
        Ty::Truncated => vec![("truncated".to_string(), Json::Bool(true))],
        Ty::Bound(data) => {
            let (_, name) = data.as_ref();
            vec![("bound".to_string(), Json::String(name.clone()))]
        }
        Ty::Generic(g) => json_of_generic(converter, g, strip_root),
        Ty::Any(AnyKind::Annotated(_)) => {
            vec![("any".to_string(), Json::String("explicit".to_string()))]
        }
        Ty::Any(_) => vec![("any".to_string(), Json::String("implicit".to_string()))],
        Ty::Top
        | Ty::Bot(_)
        | Ty::Void
        | Ty::Undefined
        | Ty::TsVoid
        | Ty::Null
        | Ty::Symbol
        | Ty::UniqueSymbol
        | Ty::Num
        | Ty::Str
        | Ty::Bool
        | Ty::BigInt => {
            vec![]
        }
        Ty::NumLit(s) => vec![("literal".to_string(), Json::String(s.clone()))],
        Ty::StrLit(s) => vec![("literal".to_string(), Json::String(s.to_string()))],
        Ty::BoolLit(b) => vec![("literal".to_string(), Json::Bool(*b))],
        Ty::BigIntLit(s) => vec![("literal".to_string(), Json::String(s.clone()))],
        Ty::Fun(f) => json_of_fun_t(converter, f, strip_root),
        Ty::Obj(o) => json_of_obj_t(converter, o, strip_root),
        Ty::Arr(arr) => {
            vec![
                ("readonly".to_string(), Json::Bool(arr.arr_readonly)),
                (
                    "type".to_string(),
                    json_of_t(converter, &arr.arr_elt_t, strip_root),
                ),
            ]
        }
        Ty::Tup { elements, inexact } => {
            let elements_arr: Vec<Json> = elements
                .iter()
                .map(|el| match el {
                    TupleElement::TupleElement {
                        t,
                        name,
                        polarity,
                        optional,
                    } => Json::Object(serde_json::Map::from_iter(vec![
                        ("kind".to_string(), Json::String("TupleElement".to_string())),
                        (
                            "name".to_string(),
                            Json::String(name.as_deref().unwrap_or("").to_string()),
                        ),
                        ("t".to_string(), json_of_t(converter, t, strip_root)),
                        ("optional".to_string(), Json::Bool(*optional)),
                        ("polarity".to_string(), json_of_polarity(*polarity)),
                    ])),
                    TupleElement::TupleSpread { t, name } => {
                        Json::Object(serde_json::Map::from_iter(vec![
                            ("kind".to_string(), Json::String("TupleSpread".to_string())),
                            (
                                "name".to_string(),
                                Json::String(name.as_deref().unwrap_or("").to_string()),
                            ),
                            ("t".to_string(), json_of_t(converter, t, strip_root)),
                        ]))
                    }
                })
                .collect();
            vec![
                ("elements".to_string(), Json::Array(elements_arr)),
                ("inexact".to_string(), Json::Bool(*inexact)),
            ]
        }
        Ty::Union(_, t0, t1, rest) => {
            let mut all = vec![t0.as_ref(), t1.as_ref()];
            all.extend(rest.iter().map(|t| t.as_ref()));
            let types: Vec<Json> = all
                .iter()
                .map(|t| json_of_t(converter, t, strip_root))
                .collect();
            vec![("types".to_string(), Json::Array(types))]
        }
        Ty::Inter(t0, t1, rest) => {
            let mut all = vec![t0.as_ref(), t1.as_ref()];
            all.extend(rest.iter().map(|t| t.as_ref()));
            let types: Vec<Json> = all
                .iter()
                .map(|t| json_of_t(converter, t, strip_root))
                .collect();
            vec![("types".to_string(), Json::Array(types))]
        }
        Ty::InlineInterface(iface) => {
            let InterfaceT {
                if_extends,
                if_props,
                if_dict,
            } = iface.as_ref();
            let extends: Vec<Json> = if_extends
                .iter()
                .map(|g| {
                    Json::Object(serde_json::Map::from_iter(json_of_generic(
                        converter, g, strip_root,
                    )))
                })
                .collect();
            let props: Vec<Json> = if_props
                .iter()
                .map(|p| json_of_prop(converter, p, strip_root))
                .collect();
            vec![
                ("extends".to_string(), Json::Array(extends)),
                ("body".to_string(), Json::Array(props)),
                (
                    "dict".to_string(),
                    if_dict
                        .as_ref()
                        .map(|d| json_of_dict(converter, d, strip_root))
                        .unwrap_or(Json::Null),
                ),
            ]
        }
        Ty::TypeOf(data) => {
            let (b, targs) = data.as_ref();
            let mut result = vec![(
                "name".to_string(),
                json_of_builtin_value(converter, b, strip_root),
            )];
            result.extend(json_of_targs(converter, targs.as_deref(), strip_root));
            result
        }
        Ty::Utility(u) => json_of_utility(converter, u, strip_root),
        Ty::IndexedAccess {
            _object,
            index,
            optional,
        } => {
            vec![
                (
                    "object".to_string(),
                    json_of_t(converter, _object, strip_root),
                ),
                ("index".to_string(), json_of_t(converter, index, strip_root)),
                ("optional".to_string(), Json::Bool(*optional)),
            ]
        }
        Ty::Conditional {
            check_type,
            extends_type,
            true_type,
            false_type,
        } => {
            vec![
                (
                    "check".to_string(),
                    json_of_t(converter, check_type, strip_root),
                ),
                (
                    "extends".to_string(),
                    json_of_t(converter, extends_type, strip_root),
                ),
                (
                    "true".to_string(),
                    json_of_t(converter, true_type, strip_root),
                ),
                (
                    "false".to_string(),
                    json_of_t(converter, false_type, strip_root),
                ),
            ]
        }
        Ty::Infer(data) => {
            let (s, b) = data.as_ref();
            vec![
                ("name".to_string(), json_of_symbol(converter, s, strip_root)),
                (
                    "bound".to_string(),
                    b.as_ref()
                        .map(|t| json_of_t(converter, t.as_ref(), strip_root))
                        .unwrap_or(Json::Null),
                ),
            ]
        }
        Ty::Component {
            regular_props,
            renders,
        } => json_of_component(converter, regular_props, renders, strip_root),
        Ty::Renders(t, variant) => {
            let variant_str = match variant {
                crate::ty::RendersKind::RendersNormal => "normal",
                crate::ty::RendersKind::RendersMaybe => "maybe",
                crate::ty::RendersKind::RendersStar => "star",
            };
            vec![
                ("argument".to_string(), json_of_t(converter, t, strip_root)),
                ("variant".to_string(), Json::String(variant_str.to_string())),
            ]
        }
        Ty::TemplateLiteral { quasis, types } => {
            vec![
                (
                    "quasis".to_string(),
                    Json::Array(quasis.iter().map(|s| Json::String(s.to_string())).collect()),
                ),
                (
                    "types".to_string(),
                    Json::Array(
                        types
                            .iter()
                            .map(|t| json_of_t(converter, t, strip_root))
                            .collect(),
                    ),
                ),
            ]
        }
    }
}

fn json_of_t<L: Debug + Clone + Dupe>(
    converter: &dyn ALocToLoc<L>,
    t: &Ty<L>,
    strip_root: Option<&Path>,
) -> Json {
    let mut obj = vec![(
        "kind".to_string(),
        Json::String(string_of_ctor_t(t).to_string()),
    )];
    obj.extend(json_of_t_list(converter, t, strip_root));
    Json::Object(serde_json::Map::from_iter(obj))
}

fn json_of_class_decl<L: Debug + Clone + Dupe>(
    converter: &dyn ALocToLoc<L>,
    name: &Symbol<L>,
    tparams: &Option<Arc<[TypeParam<L>]>>,
    strip_root: Option<&Path>,
) -> Vec<(String, Json)> {
    vec![
        (
            "name".to_string(),
            json_of_symbol(converter, name, strip_root),
        ),
        (
            "typeParams".to_string(),
            json_of_type_params(converter, tparams.as_deref(), strip_root),
        ),
    ]
}

fn json_of_interface_decl<L: Debug + Clone + Dupe>(
    converter: &dyn ALocToLoc<L>,
    name: &Symbol<L>,
    tparams: &Option<Arc<[TypeParam<L>]>>,
    strip_root: Option<&Path>,
) -> Vec<(String, Json)> {
    vec![
        (
            "name".to_string(),
            json_of_symbol(converter, name, strip_root),
        ),
        (
            "typeParams".to_string(),
            json_of_type_params(converter, tparams.as_deref(), strip_root),
        ),
    ]
}

fn json_of_record_decl<L: Debug + Clone + Dupe>(
    converter: &dyn ALocToLoc<L>,
    name: &Symbol<L>,
    tparams: &Option<Arc<[TypeParam<L>]>>,
    strip_root: Option<&Path>,
) -> Vec<(String, Json)> {
    vec![
        (
            "name".to_string(),
            json_of_symbol(converter, name, strip_root),
        ),
        (
            "typeParams".to_string(),
            json_of_type_params(converter, tparams.as_deref(), strip_root),
        ),
    ]
}

fn json_of_nominal_component_decl<L: Debug + Clone + Dupe>(
    converter: &dyn ALocToLoc<L>,
    name: &Symbol<L>,
    tparams: &Option<Arc<[TypeParam<L>]>>,
    props: &ComponentProps<L>,
    renders: &Option<Ty<L>>,
    is_type: bool,
    strip_root: Option<&Path>,
) -> Vec<(String, Json)> {
    let mut result = vec![
        (
            "name".to_string(),
            json_of_symbol(converter, name, strip_root),
        ),
        (
            "typeParams".to_string(),
            json_of_type_params(converter, tparams.as_deref(), strip_root),
        ),
        ("isType".to_string(), Json::Bool(is_type)),
    ];
    let renders_arc = renders.as_ref().map(|r| Arc::new(r.clone()));
    result.extend(json_of_component(
        converter,
        props,
        &renders_arc,
        strip_root,
    ));
    result
}

fn json_of_namespace<L>(
    converter: &dyn ALocToLoc<L>,
    name: &Option<Symbol<L>>,
    strip_root: Option<&Path>,
) -> Vec<(String, Json)> {
    vec![(
        "name".to_string(),
        name.as_ref()
            .map(|n| json_of_symbol(converter, n, strip_root))
            .unwrap_or(Json::Null),
    )]
}

fn json_of_module<L>(
    converter: &dyn ALocToLoc<L>,
    name: &Option<Symbol<L>>,
    strip_root: Option<&Path>,
) -> Vec<(String, Json)> {
    vec![(
        "name".to_string(),
        name.as_ref()
            .map(|n| json_of_symbol(converter, n, strip_root))
            .unwrap_or(Json::Null),
    )]
}

fn json_of_decl<L: Debug + Clone + Dupe>(
    converter: &dyn ALocToLoc<L>,
    d: &Decl<L>,
    strip_root: Option<&Path>,
) -> Vec<(String, Json)> {
    match d {
        Decl::VariableDecl(box (name, t)) => {
            vec![
                ("name".to_string(), Json::String(name.to_string())),
                ("type_".to_string(), json_of_t(converter, t, strip_root)),
            ]
        }
        Decl::TypeAliasDecl(box DeclTypeAliasDeclData {
            name,
            tparams,
            type_,
            ..
        }) => {
            vec![
                (
                    "name".to_string(),
                    json_of_symbol(converter, name, strip_root),
                ),
                (
                    "typeParams".to_string(),
                    json_of_type_params(converter, tparams.as_deref(), strip_root),
                ),
                (
                    "body".to_string(),
                    type_
                        .as_ref()
                        .map(|t| json_of_t(converter, t, strip_root))
                        .unwrap_or(Json::Null),
                ),
            ]
        }
        Decl::ClassDecl(box (s, ps)) => json_of_class_decl(converter, s, ps, strip_root),
        Decl::InterfaceDecl(box (s, ps)) => json_of_interface_decl(converter, s, ps, strip_root),
        Decl::RecordDecl(box (s, ps)) => json_of_record_decl(converter, s, ps, strip_root),
        Decl::EnumDecl(box DeclEnumDeclData {
            name,
            members,
            has_unknown_members,
            truncated_members_count,
        }) => vec![
            (
                "name".to_string(),
                json_of_symbol(converter, name, strip_root),
            ),
            (
                "members".to_string(),
                match members {
                    Some(ms) => Json::Array(
                        ms.iter()
                            .map(|m| Json::String(m.to_string()))
                            .collect::<Vec<_>>(),
                    ),
                    None => Json::Null,
                },
            ),
            (
                "has_unknown_members".to_string(),
                Json::Bool(*has_unknown_members),
            ),
            (
                "truncated_members_count".to_string(),
                Json::Number(serde_json::Number::from(*truncated_members_count)),
            ),
        ],
        Decl::NominalComponentDecl(box DeclNominalComponentDeclData {
            name,
            tparams,
            props,
            renders,
            is_type,
            ..
        }) => json_of_nominal_component_decl(
            converter, name, tparams, props, renders, *is_type, strip_root,
        ),
        Decl::NamespaceDecl(box DeclNamespaceDeclData { name, .. }) => {
            json_of_namespace(converter, name, strip_root)
        }
        Decl::ModuleDecl(box DeclModuleDeclData { name, .. }) => {
            json_of_module(converter, name, strip_root)
        }
    }
}

pub fn json_of_elt<L: Debug + Clone + Dupe>(
    converter: &dyn ALocToLoc<L>,
    elt: &Elt<L>,
    strip_root: Option<&Path>,
) -> Json {
    let payload = match elt {
        Elt::Type(t) => json_of_t_list(converter, t, strip_root),
        Elt::Decl(d) => json_of_decl(converter, d, strip_root),
    };
    let kind = match elt {
        Elt::Type(t) => string_of_ctor_t(t),
        Elt::Decl(d) => string_of_ctor_decl(d),
    };
    let mut obj = vec![("kind".to_string(), Json::String(kind.to_string()))];
    obj.extend(payload);
    Json::Object(serde_json::Map::from_iter(obj))
}

/// Compact JSON for `elt`, returned as a `Box<RawValue>` so callers can embed it
/// inside a serde-serializable struct without going through `serde_json::Value`
/// (which would deduplicate the two `"kind"` entries that
/// `json_of_utility` produces — outer wrapper + inner tag, OCaml `ty_debug.rs`
/// lines 516 + 831-836).
pub fn json_of_elt_raw<L: Debug + Clone + Dupe>(
    converter: &dyn ALocToLoc<L>,
    elt: &Elt<L>,
    strip_root: Option<&Path>,
) -> Box<serde_json::value::RawValue> {
    if let Elt::Type(t) = elt
        && let Ty::Utility(u) = t.as_ref()
    {
        let ctor = utility_ctor(u);
        let type_args_value: Option<Json> = types_of_utility(u).map(|ts| {
            Json::Array(
                ts.iter()
                    .map(|t| json_of_t(converter, t, strip_root))
                    .collect(),
            )
        });
        let mut buf = Vec::new();
        {
            use serde::Serializer;
            use serde::ser::SerializeMap;
            let mut serializer = serde_json::Serializer::new(&mut buf);
            let len = if type_args_value.is_some() { 3 } else { 2 };
            let mut map = serializer
                .serialize_map(Some(len))
                .expect("json_of_elt_raw: serialize_map");
            map.serialize_entry("kind", "Utility")
                .expect("json_of_elt_raw: kind=Utility");
            map.serialize_entry("kind", ctor)
                .expect("json_of_elt_raw: kind=ctor");
            if let Some(ref ta) = type_args_value {
                map.serialize_entry("typeArgs", ta)
                    .expect("json_of_elt_raw: typeArgs");
            }
            SerializeMap::end(map).expect("json_of_elt_raw: end map");
        }
        let s = String::from_utf8(buf).expect("json_of_elt_raw: utf-8");
        return serde_json::value::RawValue::from_string(s).expect("json_of_elt_raw: valid JSON");
    }
    let value = json_of_elt(converter, elt, strip_root);
    serde_json::value::to_raw_value(&value).expect("json_of_elt_raw: to_raw_value")
}

#[allow(non_snake_case)]
pub fn dump_t_EXPOSES_ABSTRACT_LOCS<L: Debug + Clone + Dupe>(t: &Ty<L>) -> String {
    dump_t(DEFAULT_DEPTH, t)
}

#[allow(non_snake_case)]
pub fn dump_elt_EXPOSES_ABSTRACT_LOCS<L: Debug + Clone + Dupe>(elt: &Elt<L>) -> String {
    dump_elt(DEFAULT_DEPTH, elt)
}
