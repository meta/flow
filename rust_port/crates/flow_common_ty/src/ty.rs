/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::collections::BTreeSet;
use std::sync::Arc;

use dupe::Dupe;
use flow_aloc::ALoc;
use flow_common::reason::Name;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_parser::ast::types::TypeGuardKind;
use flow_parser::loc::Loc;
use flow_parser::loc_sig::LocSig;

use crate::ty_symbol::ImportMode;
use crate::ty_symbol::Symbol;

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    serde::Serialize,
    serde::Deserialize
)]
pub enum Ty<L> {
    Bound(Box<(L, String)>),
    Generic(Box<GenericT<L>>),
    Any(AnyKind<L>),
    Top,
    Bot(BotKind<L>),
    /// `undefined`, as Flow spells it without `experimental.ts_void`.
    Void,
    /// `undefined`, under `experimental.ts_void`.
    Undefined,
    /// TypeScript's `void`, under `experimental.ts_void`.
    TsVoid,
    Null,
    Symbol,
    UniqueSymbol,
    Num,
    Str,
    Bool,
    BigInt,
    NumLit(String),
    StrLit(FlowSmolStr),
    BoolLit(bool),
    BigIntLit(String),
    Fun(Box<FunT<L>>),
    Obj(Box<ObjT<L>>),
    Arr(ArrT<L>),
    Tup {
        elements: Arc<[TupleElement<L>]>,
        inexact: bool,
    },
    Union(
        bool, /* from annotation */
        Arc<Ty<L>>,
        Arc<Ty<L>>,
        Arc<[Arc<Ty<L>>]>,
    ),
    Inter(Arc<Ty<L>>, Arc<Ty<L>>, Arc<[Arc<Ty<L>>]>),
    InlineInterface(Box<InterfaceT<L>>),
    TypeOf(Box<(BuiltinOrSymbol<L>, Option<Arc<[Arc<Ty<L>>]>>)>),
    Utility(Utility<L>),
    IndexedAccess {
        _object: Arc<Ty<L>>,
        index: Arc<Ty<L>>,
        optional: bool,
    },
    Conditional {
        check_type: Arc<Ty<L>>,
        extends_type: Arc<Ty<L>>,
        true_type: Arc<Ty<L>>,
        false_type: Arc<Ty<L>>,
    },
    Infer(Box<(Symbol<L>, Option<Arc<Ty<L>>>)>),
    Component {
        regular_props: ComponentProps<L>,
        renders: Option<Arc<Ty<L>>>,
    },
    Renders(Arc<Ty<L>>, RendersKind),
    TemplateLiteral {
        quasis: Arc<[FlowSmolStr]>,
        types: Arc<[Arc<Ty<L>>]>,
    },
    Truncated,
}

/* Recursive variable */
pub type GenericT<L> = (Symbol<L>, GenKind, Option<Arc<[Arc<Ty<L>>]>>);

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    serde::Serialize,
    serde::Deserialize
)]
pub enum AnyKind<L> {
    Annotated(L),
    AnyError(Option<AnyErrorKind>),
    Recursive,
    Unsound(UnsoundnessKind),
    Untyped,
    Placeholder,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    serde::Serialize,
    serde::Deserialize
)]
pub enum AnyErrorKind {
    UnresolvedName,
    MissingAnnotation,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    serde::Serialize,
    serde::Deserialize
)]
pub enum UnsoundnessKind {
    BoundFunctionThis,
    Constructor,
    DummyStatic,
    Exports,
    InferenceHooks,
    InstanceOfRefinement,
    Merged,
    ResolveSpread,
    Unchecked,
    Unimplemented,
    UnresolvedType,
    NonBindingPattern,
}

/* The purpose of adding this distinction is to enable normalized types to mimic
 * the behavior of the signature optimizer when exporting types that contain
 * tvars with no lower bounds.
 */
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    serde::Serialize,
    serde::Deserialize
)]
pub enum UpperBoundKind<L> {
    /* No upper bounds are exported as `any` */
    NoUpper,
    /* Some upper bound use type. */
    SomeKnownUpper(Arc<Ty<L>>),
    /* If the above case fails we resort to this last case. */
    SomeUnknownUpper(String),
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    serde::Serialize,
    serde::Deserialize
)]
pub enum BotKind<L> {
    /* Type.Empty */
    EmptyType,
    /* A tvar with no lower bounds */
    NoLowerWithUpper(UpperBoundKind<L>),
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    serde::Serialize,
    serde::Deserialize
)]
pub enum GenKind {
    ClassKind,
    InterfaceKind,
    TypeAliasKind,
    EnumKind,
    ComponentKind,
    RecordKind,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    serde::Serialize,
    serde::Deserialize
)]
pub enum FunEffect {
    Hook,
    Arbitrary,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    serde::Serialize,
    serde::Deserialize
)]
pub struct FunT<L> {
    pub fun_this_param: Option<Arc<Ty<L>>>,
    pub fun_params: Arc<[(Option<FlowSmolStr>, Arc<Ty<L>>, FunParam)]>,
    pub fun_rest_param: Option<(Option<FlowSmolStr>, Arc<Ty<L>>)>,
    pub fun_return: ReturnT<L>,
    pub fun_type_params: Option<Arc<[TypeParam<L>]>>,
    pub fun_static: Arc<Ty<L>>,
    pub fun_effect: FunEffect,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    serde::Serialize,
    serde::Deserialize
)]
pub enum ReturnT<L> {
    ReturnType(Arc<Ty<L>>),
    /// The payload is `None` only for a bare `asserts x`.
    TypeGuard(TypeGuardKind, FlowSmolStr, Option<Arc<Ty<L>>>),
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    serde::Serialize,
    serde::Deserialize
)]
pub enum ObjKind<L> {
    ExactObj,
    InexactObj,
    IndexedObj(Dict<L>),
    MappedTypeObj,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    serde::Serialize,
    serde::Deserialize
)]
pub struct ObjT<L> {
    pub obj_def_loc: Option<L>,
    pub obj_props: Arc<[Prop<L>]>,
    pub obj_kind: ObjKind<L>,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    serde::Serialize,
    serde::Deserialize
)]
pub struct ArrT<L> {
    pub arr_readonly: bool,
    pub arr_elt_t: Arc<Ty<L>>,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    serde::Serialize,
    serde::Deserialize
)]
pub enum TupleElement<L> {
    TupleElement {
        name: Option<FlowSmolStr>,
        t: Arc<Ty<L>>,
        polarity: Polarity,
        optional: bool,
    },
    TupleSpread {
        name: Option<FlowSmolStr>,
        t: Arc<Ty<L>>,
    },
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    serde::Serialize,
    serde::Deserialize
)]
pub struct InterfaceT<L> {
    pub if_extends: Arc<[GenericT<L>]>,
    pub if_props: Arc<[Prop<L>]>,
    pub if_dict: Option<Dict<L>>,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    serde::Serialize,
    serde::Deserialize
)]
pub struct FunParam {
    pub prm_optional: bool,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    serde::Serialize,
    serde::Deserialize
)]
pub enum Prop<L> {
    NamedProp {
        name: Name,
        prop: NamedProp<L>,
        inherited: bool,
        source: PropSource,
        def_locs: Arc<[L]>,
    },
    CallProp(FunT<L>),
    SpreadProp(Arc<Ty<L>>),
    MappedTypeProp {
        key_tparam: TypeParam<L>,
        source: Arc<Ty<L>>,
        prop: Arc<Ty<L>>,
        name_type: Option<Arc<Ty<L>>>,
        flags: MappedTypeFlags,
        homomorphic: MappedTypeHomomorphicFlag<L>,
    },
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    serde::Serialize,
    serde::Deserialize
)]
pub enum MappedTypeHomomorphicFlag<L> {
    Homomorphic,
    SemiHomomorphic(Arc<Ty<L>>),
    Unspecialized,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    serde::Serialize,
    serde::Deserialize
)]
pub enum NamedProp<L> {
    Field {
        t: Arc<Ty<L>>,
        polarity: Polarity,
        optional: bool,
    },
    Method {
        fun: FunT<L>,
        optional: bool,
    },
    Get(Arc<Ty<L>>),
    Set(Arc<Ty<L>>),
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    serde::Serialize,
    serde::Deserialize
)]
pub enum PropSource {
    Interface,
    PrimitiveProto(FlowSmolStr),
    Other,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    serde::Serialize,
    serde::Deserialize
)]
pub enum MappedTypeOptionalFlag {
    RemoveOptional,
    KeepOptionality,
    MakeOptional,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    serde::Serialize,
    serde::Deserialize
)]
pub enum MappedTypeVariance {
    OverrideVariance(Polarity),
    RemoveVariance(Polarity),
    KeepVariance,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    serde::Serialize,
    serde::Deserialize
)]
pub struct MappedTypeFlags {
    pub optional: MappedTypeOptionalFlag,
    pub variance: MappedTypeVariance,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    serde::Serialize,
    serde::Deserialize
)]
pub struct Dict<L> {
    pub dict_polarity: Polarity,
    pub dict_name: Option<FlowSmolStr>,
    pub dict_key: Arc<Ty<L>>,
    pub dict_value: Arc<Ty<L>>,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    serde::Serialize,
    serde::Deserialize
)]
pub struct TypeParam<L> {
    pub tp_name: FlowSmolStr,
    pub tp_bound: Option<Arc<Ty<L>>>,
    pub tp_polarity: Polarity,
    pub tp_default: Option<Arc<Ty<L>>>,
    pub tp_const: bool,
}

/* https://flow.org/en/docs/types/utilities/ */
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    serde::Serialize,
    serde::Deserialize
)]
pub enum Utility<L> {
    Keys(Arc<Ty<L>>),
    Values(Arc<Ty<L>>),
    ReadOnly(Arc<Ty<L>>),
    Partial(Arc<Ty<L>>),
    Required(Arc<Ty<L>>),
    Exact(Arc<Ty<L>>),
    Omit(Arc<Ty<L>>, Arc<Ty<L>>),
    ElementType(Arc<Ty<L>>, Arc<Ty<L>>),
    Enum(Arc<Ty<L>>),
    NonMaybeType(Arc<Ty<L>>),
    ObjKeyMirror(Arc<Ty<L>>),
    Class(Arc<Ty<L>>),
    Uppercase(Arc<Ty<L>>),
    Lowercase(Arc<Ty<L>>),
    Capitalize(Arc<Ty<L>>),
    Uncapitalize(Arc<Ty<L>>),
    /* React utils */
    ReactElementConfigType(Arc<Ty<L>>),
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    serde::Serialize,
    serde::Deserialize
)]
pub enum ComponentProps<L> {
    UnflattenedComponentProps(Arc<Ty<L>>),
    FlattenedComponentProps {
        props: Arc<[FlattenedComponentProp<L>]>,
        inexact: bool,
        dict: Option<Dict<L>>,
    },
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    serde::Serialize,
    serde::Deserialize
)]
pub enum FlattenedComponentProp<L> {
    FlattenedComponentProp {
        name: Name,
        optional: bool,
        def_locs: Arc<[L]>,
        t: Arc<Ty<L>>,
    },
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    serde::Serialize,
    serde::Deserialize
)]
pub enum RendersKind {
    RendersNormal,
    RendersMaybe,
    RendersStar,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    serde::Serialize,
    serde::Deserialize
)]
pub enum Polarity {
    Positive,
    Negative,
    Neutral,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    serde::Serialize,
    serde::Deserialize
)]
pub enum BuiltinOrSymbol<L> {
    FunProto,
    ObjProto,
    TSymbol(Symbol<L>),
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    serde::Serialize,
    serde::Deserialize
)]
pub struct DeclTypeAliasDeclData<L> {
    pub import: bool,
    pub name: Symbol<L>,
    pub tparams: Option<Arc<[TypeParam<L>]>>,
    pub type_: Option<Arc<Ty<L>>>,
    /// Set for an opaque type alias. `type_` is then its underlying type, when
    /// visible from where the alias is printed.
    pub opaque: Option<OpaqueBounds<L>>,
}

/// The bounds of an opaque type alias: `opaque type T super L extends U`.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    serde::Serialize,
    serde::Deserialize
)]
pub struct OpaqueBounds<L> {
    pub lower: Option<Arc<Ty<L>>>,
    pub upper: Option<Arc<Ty<L>>>,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    serde::Serialize,
    serde::Deserialize
)]
pub struct DeclEnumDeclData<L> {
    pub name: Symbol<L>,
    pub members: Option<Arc<[FlowSmolStr]>>,
    pub has_unknown_members: bool,
    pub truncated_members_count: i64,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    serde::Serialize,
    serde::Deserialize
)]
pub struct DeclNominalComponentDeclData<L> {
    pub name: Symbol<L>,
    pub tparams: Option<Arc<[TypeParam<L>]>>,
    /* Used to show instantiation at JSX creation site. */
    pub targs: Option<Arc<[Arc<Ty<L>>]>>,
    pub props: ComponentProps<L>,
    pub renders: Option<Ty<L>>,
    pub is_type: bool,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    serde::Serialize,
    serde::Deserialize
)]
pub struct DeclNamespaceDeclData<L> {
    pub name: Option<Symbol<L>>,
    pub exports: Arc<[Decl<L>]>,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    serde::Serialize,
    serde::Deserialize
)]
pub struct DeclModuleDeclData<L> {
    pub name: Option<Symbol<L>>,
    pub exports: Arc<[Decl<L>]>,
    pub default: Option<Arc<Ty<L>>>,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    serde::Serialize,
    serde::Deserialize
)]
pub enum Decl<L> {
    VariableDecl(Box<(Name, Arc<Ty<L>>)>),
    TypeAliasDecl(Box<DeclTypeAliasDeclData<L>>),
    ClassDecl(Box<(Symbol<L>, Option<Arc<[TypeParam<L>]>>)>),
    InterfaceDecl(Box<(Symbol<L>, Option<Arc<[TypeParam<L>]>>)>),
    RecordDecl(Box<(Symbol<L>, Option<Arc<[TypeParam<L>]>>)>),
    EnumDecl(Box<DeclEnumDeclData<L>>),
    NominalComponentDecl(Box<DeclNominalComponentDeclData<L>>),
    NamespaceDecl(Box<DeclNamespaceDeclData<L>>),
    ModuleDecl(Box<DeclModuleDeclData<L>>),
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    serde::Serialize,
    serde::Deserialize
)]
pub enum Elt<L> {
    Type(Arc<Ty<L>>),
    Decl(Decl<L>),
}

pub type ALocTy = Arc<Ty<ALoc>>;

pub type ALocDecl = Decl<ALoc>;

pub type ALocElt = Elt<ALoc>;

pub type ALocAnyKind = AnyKind<ALoc>;

pub type ALocBotKind = BotKind<ALoc>;

pub type ALocUpperBoundKind = UpperBoundKind<ALoc>;

pub type ALocFunT = FunT<ALoc>;

pub type ALocObjT = ObjT<ALoc>;

pub type ALocArrT = ArrT<ALoc>;

pub type ALocInterfaceT = InterfaceT<ALoc>;

pub type ALocDict = Dict<ALoc>;

pub type ALocTypeParam = TypeParam<ALoc>;

pub type ALocUtility = Utility<ALoc>;

pub type ALocComponentProps = ComponentProps<ALoc>;

pub type ALocGenericT = GenericT<ALoc>;

/// The syntactic form that introduced a binding, when the hover target is the
/// binding's own name. Hover uses it to print a declaration (`const a: A`,
/// `(method) A.m(): void`) where it would otherwise print a bare type.
#[derive(Debug, Clone, Copy, Dupe, PartialEq, Eq)]
pub enum BinderKind {
    Var,
    Let,
    Const,
    Function,
    Method,
    Constructor,
    Getter,
    Setter,
    Property,
    Index,
    EnumMember,
    Parameter,
    TypeParameter,
}

#[derive(Debug, Clone, Dupe, PartialEq, Eq)]
pub struct Binder {
    pub kind: BinderKind,
    pub name: FlowSmolStr,
    /// Enclosing class or record, for members: the `A` in `(method) A.m()`.
    pub owner: Option<FlowSmolStr>,
    pub type_parameter_context: Option<Arc<TypeParameterContext<ALocElt>>>,
    /// Signatures the declaration has beyond the one being printed. A call to an
    /// overloaded function prints the signature the call selected, so without a
    /// count nothing says the other signatures exist.
    pub overloads: u32,
    /// The type arguments a call instantiated the signature with: the `<number>`
    /// in `function f<number>(x: number): number`.
    pub targs: Option<Arc<[ALocTy]>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeParameterContext<T> {
    pub name: FlowSmolStr,
    pub declaration: T,
}

/// Which statement introduced an alias, for the line hover prints under it.
#[derive(Debug, Clone, Copy, Dupe, PartialEq, Eq)]
pub enum AliasKind {
    Import,
    Export,
}

/// The specifier that introduces an imported binding.
#[derive(Debug, Clone, Dupe, PartialEq, Eq)]
pub enum ImportSpecifier {
    Default,
    Named { remote_name: FlowSmolStr },
    Namespace,
}

/// The import declaration information that is not recoverable from its local
/// binding alone.
#[derive(Debug, Clone, Dupe, PartialEq, Eq)]
pub struct ImportProvenance {
    pub mode: ImportMode,
    pub specifier: ImportSpecifier,
    pub source: FlowSmolStr,
}

/// An imported or exported name stands for a declaration elsewhere. Hover frames
/// that declaration under an `(alias)` head and names the statement below it.
#[derive(Debug, Clone, Dupe, PartialEq, Eq)]
pub struct Alias {
    pub kind: AliasKind,
    /// The local name, as the import or export statement writes it.
    pub name: FlowSmolStr,
    pub import: Option<ImportProvenance>,
}

pub struct TypeAtPosResult {
    pub ty: ALocElt,
    pub refs: Option<BTreeSet<Symbol<Loc>>>,
    pub binder: Option<Binder>,
    pub alias: Option<Alias>,
}

/* Type destructors */

pub fn bk_union<L: Dupe>(t: &Arc<Ty<L>>) -> Vec<Arc<Ty<L>>> {
    bk_union_with_flattened(false, t)
}

pub fn bk_union_with_flattened<L: Dupe>(flattened: bool, t: &Arc<Ty<L>>) -> Vec<Arc<Ty<L>>> {
    match t.as_ref() {
        Ty::Union(_, t1, t2, rest) if flattened => {
            let mut result = vec![t1.dupe(), t2.dupe()];
            result.extend(rest.iter().cloned());
            result
        }
        Ty::Union(_, t1, t2, rest) => {
            let mut result = Vec::new();
            result.extend(bk_union(t1));
            result.extend(bk_union(t2));
            for t in rest.iter() {
                result.extend(bk_union(t));
            }
            result
        }
        _ => vec![t.dupe()],
    }
}

pub fn bk_inter<L: Clone>(t: Ty<L>) -> Vec<Arc<Ty<L>>> {
    match t {
        Ty::Inter(t1, t2, rest) => {
            let mut result = vec![t1, t2];
            result.extend(rest.iter().cloned());
            result
        }
        t => vec![Arc::new(t)],
    }
}

/* Type constructors */

pub fn mk_union<L: Dupe>(from_annotation: bool, ts: Vec<Arc<Ty<L>>>) -> Option<Arc<Ty<L>>> {
    mk_union_with_flattened(from_annotation, false, ts)
}

pub fn mk_union_with_flattened<L: Dupe>(
    from_annotation: bool,
    flattened: bool,
    ts: Vec<Arc<Ty<L>>>,
) -> Option<Arc<Ty<L>>> {
    let mut flattened_ts: Vec<Arc<Ty<L>>> = Vec::new();
    for t in ts {
        flattened_ts.extend(bk_union_with_flattened(flattened, &t));
    }
    match flattened_ts.len() {
        0 => None,
        1 => Some(flattened_ts.remove(0)),
        _ => {
            let t1 = flattened_ts.remove(0);
            let t2 = flattened_ts.remove(0);
            Some(Arc::new(Ty::Union(
                from_annotation,
                t1,
                t2,
                Arc::from(flattened_ts),
            )))
        }
    }
}

pub fn mk_inter<L>(mut ts: Vec<Arc<Ty<L>>>) -> Option<Arc<Ty<L>>> {
    match ts.len() {
        0 => None,
        1 => Some(ts.remove(0)),
        _ => {
            let t1 = ts.remove(0);
            let t2 = ts.remove(0);
            Some(Arc::new(Ty::Inter(t1, t2, Arc::from(ts))))
        }
    }
}

pub fn explicit_any() -> ALocTy {
    Arc::new(Ty::Any(AnyKind::Annotated(ALoc::none())))
}

pub fn is_dynamic<L>(t: &Ty<L>) -> bool {
    matches!(t, Ty::Any(_))
}

pub fn mk_maybe<L: Dupe>(t: Ty<L>) -> Ty<L> {
    Ty::Union(
        false,
        Arc::new(t),
        Arc::new(Ty::Null),
        Arc::from(vec![Arc::new(Ty::Void)]),
    )
}

pub fn mk_generic_class<L>(sym: Symbol<L>, targs: Option<Arc<[Arc<Ty<L>>]>>) -> Ty<L> {
    Ty::Generic(Box::new((sym, GenKind::ClassKind, targs)))
}

pub fn mk_generic_interface<L>(sym: Symbol<L>, targs: Option<Arc<[Arc<Ty<L>>]>>) -> Ty<L> {
    Ty::Generic(Box::new((sym, GenKind::InterfaceKind, targs)))
}

pub fn mk_generic_talias<L>(sym: Symbol<L>, targs: Option<Arc<[Arc<Ty<L>>]>>) -> Ty<L> {
    Ty::Generic(Box::new((sym, GenKind::TypeAliasKind, targs)))
}

pub fn mk_exact<L>(ty: Ty<L>) -> Ty<L> {
    match ty {
        Ty::Obj(o) => {
            let obj_kind = match o.obj_kind {
                ObjKind::InexactObj => ObjKind::ExactObj,
                _ => o.obj_kind,
            };
            Ty::Obj(Box::new(ObjT { obj_kind, ..*o }))
        }
        // Not applicable
        Ty::Truncated
        | Ty::Any(_)
        | Ty::Top
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
        | Ty::BigInt
        | Ty::NumLit(_)
        | Ty::StrLit(_)
        | Ty::BoolLit(_)
        | Ty::BigIntLit(_)
        | Ty::Fun(_)
        | Ty::Arr(_)
        | Ty::Tup { .. }
        | Ty::InlineInterface(_)
        | Ty::Infer(_)
        | Ty::Component { .. }
        | Ty::Renders(_, _)
        | Ty::TemplateLiteral { .. } => ty,
        // Do not nest $Exact
        Ty::Utility(Utility::Exact(_)) => ty,
        // Wrap in $Exact<...>
        Ty::Generic(_)
        | Ty::Bound(_)
        | Ty::Union(_, _, _, _)
        | Ty::Inter(_, _, _)
        | Ty::TypeOf(_)
        | Ty::Utility(_)
        | Ty::IndexedAccess { .. }
        | Ty::Conditional { .. } => Ty::Utility(Utility::Exact(Arc::new(ty))),
    }
}

pub fn mk_array<L>(readonly: bool, t: Ty<L>) -> Ty<L> {
    Ty::Arr(ArrT {
        arr_readonly: readonly,
        arr_elt_t: Arc::new(t),
    })
}

pub fn debug_string_of_generic_kind(kind: GenKind) -> &'static str {
    match kind {
        GenKind::ClassKind => "class",
        GenKind::InterfaceKind => "interface",
        GenKind::TypeAliasKind => "type alias",
        GenKind::EnumKind => "enum",
        GenKind::ComponentKind => "component",
        GenKind::RecordKind => "record",
    }
}

pub fn string_of_utility_ctor<L>(u: &Utility<L>) -> &'static str {
    match u {
        Utility::Keys(_) => "$Keys",
        Utility::Values(_) => "Values",
        Utility::ReadOnly(_) => "Readonly",
        Utility::Partial(_) => "Partial",
        Utility::Required(_) => "Required",
        Utility::Exact(_) => "$Exact",
        Utility::Enum(_) => "Enum",
        Utility::Omit(_, _) => "Omit",
        Utility::ElementType(_, _) => "$ElementType",
        Utility::NonMaybeType(_) => "NonNullable",
        Utility::ObjKeyMirror(_) => "$KeyMirror",
        Utility::Class(_) => "Class",
        Utility::Uppercase(_) => "Uppercase",
        Utility::Lowercase(_) => "Lowercase",
        Utility::Capitalize(_) => "Capitalize",
        Utility::Uncapitalize(_) => "Uncapitalize",
        Utility::ReactElementConfigType(_) => "React$ElementConfig",
    }
}

pub fn types_of_utility<L: Dupe>(u: &Utility<L>) -> Option<Vec<Arc<Ty<L>>>> {
    match u {
        Utility::Keys(t)
        | Utility::Values(t)
        | Utility::ReadOnly(t)
        | Utility::Partial(t)
        | Utility::Required(t)
        | Utility::Exact(t)
        | Utility::Enum(t)
        | Utility::NonMaybeType(t)
        | Utility::ObjKeyMirror(t)
        | Utility::Class(t)
        | Utility::ReactElementConfigType(t) => Some(vec![t.dupe()]),
        Utility::Omit(t1, t2) | Utility::ElementType(t1, t2) => Some(vec![t1.dupe(), t2.dupe()]),
        Utility::Uppercase(t)
        | Utility::Lowercase(t)
        | Utility::Capitalize(t)
        | Utility::Uncapitalize(t) => Some(vec![t.dupe()]),
    }
}

pub fn string_of_prop_source(source: &PropSource) -> String {
    match source {
        PropSource::Interface => "interface".to_string(),
        PropSource::PrimitiveProto(s) => format!("{}.prototype", s),
        PropSource::Other => "other".to_string(),
    }
}

use crate::ty_ancestors::Monoid;
use crate::ty_ancestors::StructuralMismatch;
use crate::ty_ancestors::TyEndoBase;
use crate::ty_ancestors::TyIter2Base;
use crate::ty_ancestors::TyIterBase;
use crate::ty_ancestors::TyMapBase;
use crate::ty_ancestors::TyMapReduceBase;
use crate::ty_ancestors::TyReduceBase;

pub trait TyIterTy<L, Env>: TyIterBase<Env, L>
where
    L: Dupe,
{
    fn default_on_t(&mut self, env: &Env, t: &Ty<L>) {
        match t {
            Ty::Bound(data) => {
                let (_loc, s) = data.as_ref();
                self.on_string(env, s);
            }
            Ty::Generic(g) => {
                self.on_generic_t(env, g);
            }
            Ty::Any(ak) => {
                self.on_any_kind(env, ak);
            }
            Ty::Truncated
            | Ty::Top
            | Ty::Void
            | Ty::Undefined
            | Ty::TsVoid
            | Ty::Null
            | Ty::Symbol
            | Ty::UniqueSymbol
            | Ty::Num
            | Ty::Str
            | Ty::Bool
            | Ty::BigInt => {}
            Ty::Bot(bk) => {
                self.on_bot_kind(env, bk);
            }
            Ty::NumLit(s) | Ty::BigIntLit(s) => {
                self.on_string(env, s);
            }
            Ty::StrLit(s) => {
                self.on_string(env, s);
            }
            Ty::BoolLit(b) => {
                self.on_bool(env, *b);
            }
            Ty::Fun(fun_t) => {
                self.on_fun_t(env, fun_t);
            }
            Ty::Obj(obj_t) => {
                self.on_obj_t(env, obj_t);
            }
            Ty::Arr(arr_t) => {
                self.on_arr_t(env, arr_t);
            }
            Ty::Tup { elements, inexact } => {
                self.on_bool(env, *inexact);
                for elem in elements.iter() {
                    self.on_tuple_element(env, elem);
                }
            }
            Ty::Union(from_annotation, t1, t2, rest) => {
                self.on_bool(env, *from_annotation);
                self.on_t(env, t1);
                self.on_t(env, t2);
                for t in rest.iter() {
                    self.on_t(env, t);
                }
            }
            Ty::Inter(t1, t2, rest) => {
                self.on_t(env, t1);
                self.on_t(env, t2);
                for t in rest.iter() {
                    self.on_t(env, t);
                }
            }
            Ty::InlineInterface(iface) => {
                self.on_interface_t(env, iface);
            }
            Ty::TypeOf(data) => {
                let (bos, targs) = data.as_ref();
                self.on_builtin_or_symbol(env, bos);
                if let Some(ts) = targs {
                    for t in ts.iter() {
                        self.on_t(env, t);
                    }
                }
            }
            Ty::Utility(u) => {
                self.on_utility(env, u);
            }
            Ty::IndexedAccess {
                _object,
                index,
                optional,
            } => {
                self.on_t(env, _object);
                self.on_t(env, index);
                self.on_bool(env, *optional);
            }
            Ty::Conditional {
                check_type,
                extends_type,
                true_type,
                false_type,
            } => {
                self.on_t(env, check_type);
                self.on_t(env, extends_type);
                self.on_t(env, true_type);
                self.on_t(env, false_type);
            }
            Ty::Infer(data) => {
                let (sym, bound) = data.as_ref();
                self.on_symbol(env, sym);
                if let Some(b) = bound {
                    self.on_t(env, b);
                }
            }
            Ty::Component {
                regular_props,
                renders,
            } => {
                self.on_component_props(env, regular_props);
                if let Some(r) = renders {
                    self.on_t(env, r);
                }
            }
            Ty::Renders(t, _kind) => {
                self.on_t(env, t);
            }
            Ty::TemplateLiteral { quasis: _, types } => {
                for t in types.iter() {
                    self.on_t(env, t);
                }
            }
        }
    }

    fn on_t(&mut self, env: &Env, t: &Ty<L>) {
        self.default_on_t(env, t);
    }

    fn default_on_decl(&mut self, env: &Env, d: &Decl<L>) {
        match d {
            Decl::VariableDecl(box (name, t)) => {
                self.on_name(env, name);
                self.on_t(env, t);
            }
            Decl::TypeAliasDecl(box DeclTypeAliasDeclData {
                import,
                name,
                tparams,
                type_,
                opaque,
            }) => {
                self.on_bool(env, *import);
                self.on_symbol(env, name);
                if let Some(tps) = tparams {
                    for tp in tps.iter() {
                        self.on_type_param(env, tp);
                    }
                }
                if let Some(t) = type_ {
                    self.on_t(env, t);
                }
                if let Some(OpaqueBounds { lower, upper }) = opaque {
                    for t in [lower, upper].into_iter().flatten() {
                        self.on_t(env, t);
                    }
                }
            }
            Decl::ClassDecl(box (sym, tparams))
            | Decl::InterfaceDecl(box (sym, tparams))
            | Decl::RecordDecl(box (sym, tparams)) => {
                self.on_symbol(env, sym);
                if let Some(tps) = tparams {
                    for tp in tps.iter() {
                        self.on_type_param(env, tp);
                    }
                }
            }
            Decl::EnumDecl(box DeclEnumDeclData {
                name,
                members,
                has_unknown_members,
                truncated_members_count,
            }) => {
                self.on_symbol(env, name);
                if let Some(ms) = members {
                    for member in ms.iter() {
                        self.on_string(env, member.as_str());
                    }
                }
                self.on_bool(env, *has_unknown_members);
                self.on_int(env, *truncated_members_count);
            }
            Decl::NominalComponentDecl(box DeclNominalComponentDeclData {
                name,
                tparams,
                targs,
                props,
                renders,
                is_type,
            }) => {
                self.on_symbol(env, name);
                if let Some(tps) = tparams {
                    for tp in tps.iter() {
                        self.on_type_param(env, tp);
                    }
                }
                if let Some(tas) = targs {
                    for t in tas.iter() {
                        self.on_t(env, t);
                    }
                }
                self.on_component_props(env, props);
                if let Some(r) = renders {
                    self.on_t(env, r);
                }
                self.on_bool(env, *is_type);
            }
            Decl::NamespaceDecl(box DeclNamespaceDeclData { name, exports }) => {
                if let Some(n) = name {
                    self.on_symbol(env, n);
                }
                for d in exports.iter() {
                    self.on_decl(env, d);
                }
            }
            Decl::ModuleDecl(box DeclModuleDeclData {
                name,
                exports,
                default,
            }) => {
                if let Some(n) = name {
                    self.on_symbol(env, n);
                }
                for d in exports.iter() {
                    self.on_decl(env, d);
                }
                if let Some(t) = default {
                    self.on_t(env, t);
                }
            }
        }
    }

    fn on_decl(&mut self, env: &Env, d: &Decl<L>) {
        self.default_on_decl(env, d);
    }

    fn on_elt(&mut self, env: &Env, e: &Elt<L>) {
        match e {
            Elt::Type(t) => self.on_t(env, t),
            Elt::Decl(d) => self.on_decl(env, d),
        }
    }

    fn on_generic_t(&mut self, env: &Env, g: &GenericT<L>) {
        let (sym, _kind, targs) = g;
        self.on_symbol(env, sym);
        if let Some(ts) = targs {
            for t in ts.iter() {
                self.on_t(env, t);
            }
        }
    }

    fn on_any_kind(&mut self, _env: &Env, ak: &AnyKind<L>) {
        match ak {
            AnyKind::Annotated(_loc) => {}
            AnyKind::AnyError(_)
            | AnyKind::Recursive
            | AnyKind::Unsound(_)
            | AnyKind::Untyped
            | AnyKind::Placeholder => {}
        }
    }

    fn on_bot_kind(&mut self, env: &Env, bk: &BotKind<L>) {
        match bk {
            BotKind::EmptyType => {}
            BotKind::NoLowerWithUpper(ubk) => {
                self.on_upper_bound_kind(env, ubk);
            }
        }
    }

    fn on_upper_bound_kind(&mut self, env: &Env, ubk: &UpperBoundKind<L>) {
        match ubk {
            UpperBoundKind::NoUpper => {}
            UpperBoundKind::SomeKnownUpper(t) => {
                self.on_t(env, t);
            }
            UpperBoundKind::SomeUnknownUpper(s) => {
                self.on_string(env, s);
            }
        }
    }

    fn on_fun_t(&mut self, env: &Env, f: &FunT<L>) {
        if let Some(ty) = &f.fun_this_param {
            self.on_t(env, ty);
        }
        for (name_opt, ty, _param) in f.fun_params.iter() {
            if let Some(_name) = name_opt {}
            self.on_t(env, ty);
        }
        if let Some((_, ty)) = &f.fun_rest_param {
            self.on_t(env, ty);
        }
        self.on_return_t(env, &f.fun_return);
        if let Some(tps) = &f.fun_type_params {
            for tp in tps.iter() {
                self.on_type_param(env, tp);
            }
        }
        self.on_t(env, &f.fun_static);
    }

    fn on_return_t(&mut self, env: &Env, r: &ReturnT<L>) {
        match r {
            ReturnT::ReturnType(t) => {
                self.on_t(env, t);
            }
            ReturnT::TypeGuard(_kind, _name, t) => {
                if let Some(t) = t {
                    self.on_t(env, t);
                }
            }
        }
    }

    fn on_obj_t(&mut self, env: &Env, o: &ObjT<L>) {
        for prop in o.obj_props.iter() {
            self.on_prop(env, prop);
        }
        self.on_obj_kind(env, &o.obj_kind);
    }

    fn on_obj_kind(&mut self, env: &Env, ok: &ObjKind<L>) {
        match ok {
            ObjKind::ExactObj | ObjKind::InexactObj | ObjKind::MappedTypeObj => {}
            ObjKind::IndexedObj(dict) => {
                self.on_dict(env, dict);
            }
        }
    }

    fn on_arr_t(&mut self, env: &Env, a: &ArrT<L>) {
        self.on_t(env, &a.arr_elt_t);
    }

    fn on_interface_t(&mut self, env: &Env, i: &InterfaceT<L>) {
        for g in i.if_extends.iter() {
            self.on_generic_t(env, g);
        }
        for prop in i.if_props.iter() {
            self.on_prop(env, prop);
        }
        if let Some(dict) = &i.if_dict {
            self.on_dict(env, dict);
        }
    }

    fn on_prop(&mut self, env: &Env, p: &Prop<L>) {
        match p {
            Prop::NamedProp {
                name,
                prop,
                inherited: _,
                source: _,
                def_locs: _,
            } => {
                self.on_name(env, name);
                self.on_named_prop(env, prop);
            }
            Prop::CallProp(fun_t) => {
                self.on_fun_t(env, fun_t);
            }
            Prop::SpreadProp(t) => {
                self.on_t(env, t);
            }
            Prop::MappedTypeProp {
                key_tparam,
                source,
                prop,
                name_type,
                flags: _,
                homomorphic,
            } => {
                self.on_type_param(env, key_tparam);
                self.on_t(env, source);
                self.on_t(env, prop);
                if let Some(nt) = name_type {
                    self.on_t(env, nt);
                }
                self.on_mapped_type_homomorphic_flag(env, homomorphic);
            }
        }
    }

    fn on_named_prop(&mut self, env: &Env, np: &NamedProp<L>) {
        match np {
            NamedProp::Field {
                t,
                polarity: _,
                optional: _,
            } => {
                self.on_t(env, t);
            }
            NamedProp::Method { fun: fun_t, .. } => {
                self.on_fun_t(env, fun_t);
            }
            NamedProp::Get(t) | NamedProp::Set(t) => {
                self.on_t(env, t);
            }
        }
    }

    fn on_mapped_type_homomorphic_flag(&mut self, env: &Env, flag: &MappedTypeHomomorphicFlag<L>) {
        match flag {
            MappedTypeHomomorphicFlag::Homomorphic | MappedTypeHomomorphicFlag::Unspecialized => {}
            MappedTypeHomomorphicFlag::SemiHomomorphic(t) => {
                self.on_t(env, t);
            }
        }
    }

    fn on_type_param(&mut self, env: &Env, tp: &TypeParam<L>) {
        if let Some(bound) = &tp.tp_bound {
            self.on_t(env, bound);
        }
        if let Some(default) = &tp.tp_default {
            self.on_t(env, default);
        }
    }

    fn on_dict(&mut self, env: &Env, d: &Dict<L>) {
        self.on_t(env, &d.dict_key);
        self.on_t(env, &d.dict_value);
    }

    fn on_tuple_element(&mut self, env: &Env, te: &TupleElement<L>) {
        match te {
            TupleElement::TupleElement {
                name: _,
                t,
                polarity: _,
                optional: _,
            } => {
                self.on_t(env, t);
            }
            TupleElement::TupleSpread { name: _, t } => {
                self.on_t(env, t);
            }
        }
    }

    fn on_builtin_or_symbol(&mut self, env: &Env, bos: &BuiltinOrSymbol<L>) {
        match bos {
            BuiltinOrSymbol::FunProto | BuiltinOrSymbol::ObjProto => {}
            BuiltinOrSymbol::TSymbol(sym) => {
                self.on_symbol(env, sym);
            }
        }
    }

    fn on_utility(&mut self, env: &Env, u: &Utility<L>) {
        match u {
            Utility::Keys(t)
            | Utility::Values(t)
            | Utility::ReadOnly(t)
            | Utility::Partial(t)
            | Utility::Required(t)
            | Utility::Exact(t)
            | Utility::Enum(t)
            | Utility::NonMaybeType(t)
            | Utility::ObjKeyMirror(t)
            | Utility::Class(t)
            | Utility::ReactElementConfigType(t) => {
                self.on_t(env, t);
            }
            Utility::Omit(t1, t2) | Utility::ElementType(t1, t2) => {
                self.on_t(env, t1);
                self.on_t(env, t2);
            }
            Utility::Uppercase(t)
            | Utility::Lowercase(t)
            | Utility::Capitalize(t)
            | Utility::Uncapitalize(t) => {
                self.on_t(env, t);
            }
        }
    }

    fn on_component_props(&mut self, env: &Env, cp: &ComponentProps<L>) {
        match cp {
            ComponentProps::UnflattenedComponentProps(t) => {
                self.on_t(env, t);
            }
            ComponentProps::FlattenedComponentProps {
                props,
                inexact: _,
                dict,
            } => {
                for prop in props.iter() {
                    self.on_flattened_component_prop(env, prop);
                }
                if let Some(dict) = dict {
                    self.on_dict(env, dict);
                }
            }
        }
    }

    fn on_flattened_component_prop(&mut self, env: &Env, fcp: &FlattenedComponentProp<L>) {
        match fcp {
            FlattenedComponentProp::FlattenedComponentProp {
                name,
                optional: _,
                def_locs: _,
                t,
            } => {
                self.on_name(env, name);
                self.on_t(env, t);
            }
        }
    }
}

pub trait TyIter2Ty<L, Env>: TyIter2Base<Env, L>
where
    L: Dupe,
{
    fn on_bound_locs(&mut self, _env: &Env, _l1: &L, _l2: &L) -> Result<(), StructuralMismatch> {
        Ok(())
    }

    fn on_t(&mut self, env: &Env, t1: &Ty<L>, t2: &Ty<L>) -> Result<(), StructuralMismatch> {
        if std::ptr::eq(t1, t2) {
            return Ok(());
        }

        match (t1, t2) {
            (Ty::Bound(d1), Ty::Bound(d2)) => {
                let (l1, s1) = d1.as_ref();
                let (l2, s2) = d2.as_ref();
                self.on_bound_locs(env, l1, l2)?;
                self.on_string(env, s1, s2)?;
                Ok(())
            }
            (Ty::Generic(g1), Ty::Generic(g2)) => self.on_generic_t(env, g1, g2),
            (Ty::Any(ak1), Ty::Any(ak2)) => self.on_any_kind(env, ak1, ak2),
            (Ty::Truncated, Ty::Truncated)
            | (Ty::Top, Ty::Top)
            | (Ty::Void, Ty::Void)
            | (Ty::Undefined, Ty::Undefined)
            | (Ty::TsVoid, Ty::TsVoid)
            | (Ty::Null, Ty::Null)
            | (Ty::Symbol, Ty::Symbol)
            | (Ty::UniqueSymbol, Ty::UniqueSymbol)
            | (Ty::Num, Ty::Num)
            | (Ty::Str, Ty::Str)
            | (Ty::Bool, Ty::Bool)
            | (Ty::BigInt, Ty::BigInt) => Ok(()),
            (Ty::Bot(bk1), Ty::Bot(bk2)) => self.on_bot_kind(env, bk1, bk2),
            (Ty::NumLit(s1), Ty::NumLit(s2)) | (Ty::BigIntLit(s1), Ty::BigIntLit(s2)) => {
                self.on_string(env, s1, s2)
            }
            (Ty::StrLit(s1), Ty::StrLit(s2)) => self.on_string(env, s1, s2),
            (Ty::BoolLit(b1), Ty::BoolLit(b2)) => self.on_bool(env, *b1, *b2),
            (Ty::Fun(f1), Ty::Fun(f2)) => self.on_fun_t(env, f1, f2),
            (Ty::Obj(o1), Ty::Obj(o2)) => self.on_obj_t(env, o1, o2),
            (Ty::Arr(a1), Ty::Arr(a2)) => self.on_arr_t(env, a1, a2),
            (
                Ty::Tup {
                    elements: e1,
                    inexact: i1,
                },
                Ty::Tup {
                    elements: e2,
                    inexact: i2,
                },
            ) => {
                self.on_bool(env, *i1, *i2)?;
                self.on_list(
                    |s, e, elem1, elem2| s.on_tuple_element(e, elem1, elem2),
                    env,
                    e1,
                    e2,
                )
            }
            (Ty::Union(a1, t1_1, t1_2, r1), Ty::Union(a2, t2_1, t2_2, r2)) => {
                self.on_bool(env, *a1, *a2)?;
                self.on_t(env, t1_1, t2_1)?;
                self.on_t(env, t1_2, t2_2)?;
                self.on_list(|s, e, x, y| s.on_t(e, x, y), env, r1, r2)
            }
            (Ty::Inter(t1_1, t1_2, r1), Ty::Inter(t2_1, t2_2, r2)) => {
                self.on_t(env, t1_1, t2_1)?;
                self.on_t(env, t1_2, t2_2)?;
                self.on_list(|s, e, x, y| s.on_t(e, x, y), env, r1, r2)
            }
            (Ty::InlineInterface(i1), Ty::InlineInterface(i2)) => self.on_interface_t(env, i1, i2),
            (Ty::TypeOf(d1), Ty::TypeOf(d2)) => {
                let (bos1, targs1) = d1.as_ref();
                let (bos2, targs2) = d2.as_ref();
                self.on_builtin_or_symbol(env, bos1, bos2)?;
                self.on_option_vec_ty(env, targs1, targs2)
            }
            (Ty::Utility(u1), Ty::Utility(u2)) => self.on_utility(env, u1, u2),
            (
                Ty::IndexedAccess {
                    _object: o1,
                    index: i1,
                    optional: opt1,
                },
                Ty::IndexedAccess {
                    _object: o2,
                    index: i2,
                    optional: opt2,
                },
            ) => {
                self.on_t(env, o1, o2)?;
                self.on_t(env, i1, i2)?;
                self.on_bool(env, *opt1, *opt2)
            }
            (
                Ty::Conditional {
                    check_type: c1,
                    extends_type: e1,
                    true_type: tt1,
                    false_type: ft1,
                },
                Ty::Conditional {
                    check_type: c2,
                    extends_type: e2,
                    true_type: tt2,
                    false_type: ft2,
                },
            ) => {
                self.on_t(env, c1, c2)?;
                self.on_t(env, e1, e2)?;
                self.on_t(env, tt1, tt2)?;
                self.on_t(env, ft1, ft2)
            }
            (Ty::Infer(d1), Ty::Infer(d2)) => {
                let (s1, b1) = d1.as_ref();
                let (s2, b2) = d2.as_ref();
                self.on_symbol(env, s1, s2)?;
                self.on_option(|s, e, t1, t2| s.on_t(e, t1, t2), env, b1, b2)
            }
            (
                Ty::Component {
                    regular_props: rp1,
                    renders: r1,
                },
                Ty::Component {
                    regular_props: rp2,
                    renders: r2,
                },
            ) => {
                self.on_component_props(env, rp1, rp2)?;
                self.on_option(|s, e, t1, t2| s.on_t(e, t1, t2), env, r1, r2)
            }
            (Ty::Renders(t1, k1), Ty::Renders(t2, k2)) => {
                self.on_t(env, t1, t2)?;
                self.on_renders_kind(env, k1, k2)
            }
            (
                Ty::TemplateLiteral {
                    quasis: q1,
                    types: ts1,
                },
                Ty::TemplateLiteral {
                    quasis: q2,
                    types: ts2,
                },
            ) => {
                if q1 == q2 {
                    self.on_list(|s, e, t1, t2| s.on_t(e, t1, t2), env, ts1, ts2)
                } else {
                    Err(StructuralMismatch::Difference(0))
                }
            }
            _ => self.fail_t(env, t1, t2),
        }
    }

    fn fail_t(&mut self, _env: &Env, t1: &Ty<L>, t2: &Ty<L>) -> Result<(), StructuralMismatch> {
        StructuralMismatch::fail_gen(tag_of_t, t1, t2)
    }

    fn on_decl(&mut self, env: &Env, d1: &Decl<L>, d2: &Decl<L>) -> Result<(), StructuralMismatch> {
        match (d1, d2) {
            (Decl::VariableDecl(box (n1, t1)), Decl::VariableDecl(box (n2, t2))) => {
                self.on_name(env, n1, n2)?;
                self.on_t(env, t1, t2)
            }
            (
                Decl::TypeAliasDecl(box DeclTypeAliasDeclData {
                    import: i1,
                    name: n1,
                    tparams: tp1,
                    type_: ty1,
                    opaque: o1,
                }),
                Decl::TypeAliasDecl(box DeclTypeAliasDeclData {
                    import: i2,
                    name: n2,
                    tparams: tp2,
                    type_: ty2,
                    opaque: o2,
                }),
            ) => {
                self.on_bool(env, *i1, *i2)?;
                self.on_symbol(env, n1, n2)?;
                self.on_option_vec_type_param(env, tp1, tp2)?;
                self.on_option(|s, e, t1, t2| s.on_t(e, t1, t2), env, ty1, ty2)?;
                self.on_option(
                    |s, e, b1: &OpaqueBounds<L>, b2: &OpaqueBounds<L>| {
                        s.on_option(|s, e, t1, t2| s.on_t(e, t1, t2), e, &b1.lower, &b2.lower)?;
                        s.on_option(|s, e, t1, t2| s.on_t(e, t1, t2), e, &b1.upper, &b2.upper)
                    },
                    env,
                    o1,
                    o2,
                )
            }
            (Decl::ClassDecl(box (s1, tp1)), Decl::ClassDecl(box (s2, tp2)))
            | (Decl::InterfaceDecl(box (s1, tp1)), Decl::InterfaceDecl(box (s2, tp2)))
            | (Decl::RecordDecl(box (s1, tp1)), Decl::RecordDecl(box (s2, tp2))) => {
                self.on_symbol(env, s1, s2)?;
                self.on_option_vec_type_param(env, tp1, tp2)
            }
            (
                Decl::EnumDecl(box DeclEnumDeclData {
                    name: n1,
                    members: m1,
                    has_unknown_members: hum1,
                    truncated_members_count: tmc1,
                }),
                Decl::EnumDecl(box DeclEnumDeclData {
                    name: n2,
                    members: m2,
                    has_unknown_members: hum2,
                    truncated_members_count: tmc2,
                }),
            ) => {
                self.on_symbol(env, n1, n2)?;
                match (m1, m2) {
                    (Some(v1), Some(v2)) => self.on_list(
                        |s, e, m1, m2| s.on_string(e, m1.as_str(), m2.as_str()),
                        env,
                        v1,
                        v2,
                    )?,
                    (None, None) => {}
                    _ => self.fail_option(env, m1, m2)?,
                }
                self.on_bool(env, *hum1, *hum2)?;
                self.on_int(env, *tmc1, *tmc2)
            }
            (
                Decl::NominalComponentDecl(box DeclNominalComponentDeclData {
                    name: n1,
                    tparams: tp1,
                    targs: ta1,
                    props: p1,
                    renders: r1,
                    is_type: it1,
                }),
                Decl::NominalComponentDecl(box DeclNominalComponentDeclData {
                    name: n2,
                    tparams: tp2,
                    targs: ta2,
                    props: p2,
                    renders: r2,
                    is_type: it2,
                }),
            ) => {
                self.on_symbol(env, n1, n2)?;
                self.on_option_vec_type_param(env, tp1, tp2)?;
                self.on_option_vec_ty(env, ta1, ta2)?;
                self.on_component_props(env, p1, p2)?;
                self.on_option(|s, e, t1, t2| s.on_t(e, t1, t2), env, r1, r2)?;
                self.on_bool(env, *it1, *it2)
            }
            (
                Decl::NamespaceDecl(box DeclNamespaceDeclData {
                    name: n1,
                    exports: e1,
                }),
                Decl::NamespaceDecl(box DeclNamespaceDeclData {
                    name: n2,
                    exports: e2,
                }),
            ) => {
                self.on_option(|s, e, s1, s2| s.on_symbol(e, s1, s2), env, n1, n2)?;
                self.on_list(|s, e, d1, d2| s.on_decl(e, d1, d2), env, e1, e2)
            }
            (
                Decl::ModuleDecl(box DeclModuleDeclData {
                    name: n1,
                    exports: e1,
                    default: d1,
                }),
                Decl::ModuleDecl(box DeclModuleDeclData {
                    name: n2,
                    exports: e2,
                    default: d2,
                }),
            ) => {
                self.on_option(|s, e, s1, s2| s.on_symbol(e, s1, s2), env, n1, n2)?;
                self.on_list(|s, e, d1, d2| s.on_decl(e, d1, d2), env, e1, e2)?;
                self.on_option(|s, e, t1, t2| s.on_t(e, t1, t2), env, d1, d2)
            }
            _ => self.fail_decl(env, d1, d2),
        }
    }

    fn fail_decl(
        &mut self,
        _env: &Env,
        d1: &Decl<L>,
        d2: &Decl<L>,
    ) -> Result<(), StructuralMismatch> {
        StructuralMismatch::fail_gen(tag_of_decl, d1, d2)
    }

    fn on_elt(&mut self, env: &Env, e1: &Elt<L>, e2: &Elt<L>) -> Result<(), StructuralMismatch> {
        match (e1, e2) {
            (Elt::Type(t1), Elt::Type(t2)) => self.on_t(env, t1, t2),
            (Elt::Decl(d1), Elt::Decl(d2)) => self.on_decl(env, d1, d2),
            _ => self.fail_elt(env, e1, e2),
        }
    }

    fn on_generic_t(
        &mut self,
        env: &Env,
        g1: &GenericT<L>,
        g2: &GenericT<L>,
    ) -> Result<(), StructuralMismatch> {
        let (sym1, kind1, targs1) = g1;
        let (sym2, kind2, targs2) = g2;
        self.on_symbol(env, sym1, sym2)?;
        if kind1 != kind2 {
            return self.fail_gen_kind(env, kind1, kind2);
        }
        self.on_option_vec_ty(env, targs1, targs2)
    }

    fn on_any_kind(
        &mut self,
        env: &Env,
        ak1: &AnyKind<L>,
        ak2: &AnyKind<L>,
    ) -> Result<(), StructuralMismatch> {
        match (ak1, ak2) {
            (AnyKind::Annotated(_), AnyKind::Annotated(_)) => Ok(()),
            (AnyKind::AnyError(e1), AnyKind::AnyError(e2)) => self.on_option(
                |s, e, ek1, ek2| {
                    if ek1 != ek2 {
                        s.fail_any_error_kind(e, ek1, ek2)
                    } else {
                        Ok(())
                    }
                },
                env,
                e1,
                e2,
            ),
            (AnyKind::Recursive, AnyKind::Recursive) => Ok(()),
            (AnyKind::Unsound(u1), AnyKind::Unsound(u2)) => {
                if u1 != u2 {
                    self.fail_unsoundness_kind(env, u1, u2)
                } else {
                    Ok(())
                }
            }
            (AnyKind::Untyped, AnyKind::Untyped) => Ok(()),
            (AnyKind::Placeholder, AnyKind::Placeholder) => Ok(()),
            _ => self.fail_any_kind(env, ak1, ak2),
        }
    }

    fn on_bot_kind(
        &mut self,
        env: &Env,
        bk1: &BotKind<L>,
        bk2: &BotKind<L>,
    ) -> Result<(), StructuralMismatch> {
        match (bk1, bk2) {
            (BotKind::EmptyType, BotKind::EmptyType) => Ok(()),
            (BotKind::NoLowerWithUpper(u1), BotKind::NoLowerWithUpper(u2)) => {
                self.on_upper_bound_kind(env, u1, u2)
            }
            _ => self.fail_bot_kind(env, bk1, bk2),
        }
    }

    fn on_upper_bound_kind(
        &mut self,
        env: &Env,
        ubk1: &UpperBoundKind<L>,
        ubk2: &UpperBoundKind<L>,
    ) -> Result<(), StructuralMismatch> {
        match (ubk1, ubk2) {
            (UpperBoundKind::NoUpper, UpperBoundKind::NoUpper) => Ok(()),
            (UpperBoundKind::SomeKnownUpper(t1), UpperBoundKind::SomeKnownUpper(t2)) => {
                self.on_t(env, t1, t2)
            }
            (UpperBoundKind::SomeUnknownUpper(s1), UpperBoundKind::SomeUnknownUpper(s2)) => {
                self.on_string(env, s1, s2)
            }
            _ => self.fail_upper_bound_kind(env, ubk1, ubk2),
        }
    }

    fn on_fun_t(
        &mut self,
        env: &Env,
        f1: &FunT<L>,
        f2: &FunT<L>,
    ) -> Result<(), StructuralMismatch> {
        self.on_option(
            |s, e, t1, t2| s.on_t(e, t1, t2),
            env,
            &f1.fun_this_param,
            &f2.fun_this_param,
        )?;
        self.on_list(
            |s, e, (n1, ty1, fp1), (n2, ty2, fp2)| {
                s.on_option(|s, e, n1, n2| s.on_string(e, n1, n2), e, n1, n2)?;
                s.on_t(e, ty1, ty2)?;
                s.on_bool(e, fp1.prm_optional, fp2.prm_optional)
            },
            env,
            &f1.fun_params,
            &f2.fun_params,
        )?;
        self.on_option(
            |s, e, (n1, t1), (n2, t2)| {
                s.on_option(|s, e, n1, n2| s.on_string(e, n1, n2), e, n1, n2)?;
                s.on_t(e, t1, t2)
            },
            env,
            &f1.fun_rest_param,
            &f2.fun_rest_param,
        )?;
        self.on_return_t(env, &f1.fun_return, &f2.fun_return)?;
        self.on_option_vec_type_param(env, &f1.fun_type_params, &f2.fun_type_params)?;
        self.on_t(env, &f1.fun_static, &f2.fun_static)?;
        self.on_fun_effect(env, &f1.fun_effect, &f2.fun_effect)
    }

    fn on_return_t(
        &mut self,
        env: &Env,
        r1: &ReturnT<L>,
        r2: &ReturnT<L>,
    ) -> Result<(), StructuralMismatch> {
        match (r1, r2) {
            (ReturnT::ReturnType(t1), ReturnT::ReturnType(t2)) => self.on_t(env, t1, t2),
            (ReturnT::TypeGuard(k1, _, Some(t1)), ReturnT::TypeGuard(k2, _, Some(t2)))
                if k1 == k2 =>
            {
                self.on_t(env, t1, t2)
            }
            (ReturnT::TypeGuard(k1, _, None), ReturnT::TypeGuard(k2, _, None)) if k1 == k2 => {
                Ok(())
            }
            _ => self.fail_return_t(env, r1, r2),
        }
    }

    /* Take advantage of pointer equality at type nodes to short circut */
    fn on_obj_t(
        &mut self,
        env: &Env,
        obj1: &ObjT<L>,
        obj2: &ObjT<L>,
    ) -> Result<(), StructuralMismatch> {
        let obj1_no_def_loc = ObjT {
            obj_def_loc: None,
            obj_kind: obj1.obj_kind.clone(),
            obj_props: obj1.obj_props.dupe(),
        };
        let obj2_no_def_loc = ObjT {
            obj_def_loc: None,
            obj_kind: obj2.obj_kind.clone(),
            obj_props: obj2.obj_props.dupe(),
        };
        self.on_obj_t_impl(env, &obj1_no_def_loc, &obj2_no_def_loc)
    }

    fn on_obj_kind(
        &mut self,
        env: &Env,
        ok1: &ObjKind<L>,
        ok2: &ObjKind<L>,
    ) -> Result<(), StructuralMismatch> {
        match (ok1, ok2) {
            (ObjKind::ExactObj, ObjKind::ExactObj)
            | (ObjKind::InexactObj, ObjKind::InexactObj)
            | (ObjKind::MappedTypeObj, ObjKind::MappedTypeObj) => Ok(()),
            (ObjKind::IndexedObj(d1), ObjKind::IndexedObj(d2)) => self.on_dict(env, d1, d2),
            _ => self.fail_obj_kind(env, ok1, ok2),
        }
    }

    fn on_arr_t(
        &mut self,
        env: &Env,
        a1: &ArrT<L>,
        a2: &ArrT<L>,
    ) -> Result<(), StructuralMismatch> {
        self.on_bool(env, a1.arr_readonly, a2.arr_readonly)?;
        self.on_t(env, &a1.arr_elt_t, &a2.arr_elt_t)
    }

    fn on_interface_t(
        &mut self,
        env: &Env,
        i1: &InterfaceT<L>,
        i2: &InterfaceT<L>,
    ) -> Result<(), StructuralMismatch> {
        self.on_list(
            |s, e, g1, g2| s.on_generic_t(e, g1, g2),
            env,
            &i1.if_extends,
            &i2.if_extends,
        )?;
        self.on_list(
            |s, e, p1, p2| s.on_prop(e, p1, p2),
            env,
            &i1.if_props,
            &i2.if_props,
        )?;
        self.on_option(
            |s, e, d1, d2| s.on_dict(e, d1, d2),
            env,
            &i1.if_dict,
            &i2.if_dict,
        )
    }

    fn on_prop(&mut self, env: &Env, p1: &Prop<L>, p2: &Prop<L>) -> Result<(), StructuralMismatch> {
        match (p1, p2) {
            (
                Prop::NamedProp {
                    name: n1,
                    prop: np1,
                    inherited: inh1,
                    source: src1,
                    def_locs: dl1,
                },
                Prop::NamedProp {
                    name: n2,
                    prop: np2,
                    inherited: inh2,
                    source: src2,
                    def_locs: dl2,
                },
            ) => self
                .on_named_prop_wrapper(env, n1, n2, np1, np2, *inh1, *inh2, src1, src2, dl1, dl2),
            (Prop::CallProp(f1), Prop::CallProp(f2)) => self.on_fun_t(env, f1, f2),
            (Prop::SpreadProp(t1), Prop::SpreadProp(t2)) => self.on_t(env, t1, t2),
            (
                Prop::MappedTypeProp {
                    key_tparam: k1,
                    source: s1,
                    prop: pr1,
                    name_type: nt1,
                    flags: _,
                    homomorphic: h1,
                },
                Prop::MappedTypeProp {
                    key_tparam: k2,
                    source: s2,
                    prop: pr2,
                    name_type: nt2,
                    flags: _,
                    homomorphic: h2,
                },
            ) => {
                self.on_type_param(env, k1, k2)?;
                self.on_t(env, s1, s2)?;
                self.on_t(env, pr1, pr2)?;
                match (nt1, nt2) {
                    (Some(a), Some(b)) => self.on_t(env, a, b)?,
                    (None, None) => {}
                    _ => return self.fail_prop(env, p1, p2),
                }
                self.on_mapped_type_homomorphic_flag(env, h1, h2)
            }
            _ => self.fail_prop(env, p1, p2),
        }
    }

    fn on_named_prop(
        &mut self,
        env: &Env,
        np1: &NamedProp<L>,
        np2: &NamedProp<L>,
    ) -> Result<(), StructuralMismatch> {
        match (np1, np2) {
            (
                NamedProp::Field {
                    t: t1,
                    polarity: p1,
                    optional: o1,
                },
                NamedProp::Field {
                    t: t2,
                    polarity: p2,
                    optional: o2,
                },
            ) => {
                if p1 != p2 {
                    return self.fail_polarity(env, p1, p2);
                }
                self.on_bool(env, *o1, *o2)?;
                self.on_t(env, t1, t2)
            }
            (
                NamedProp::Method {
                    fun: f1,
                    optional: o1,
                },
                NamedProp::Method {
                    fun: f2,
                    optional: o2,
                },
            ) => {
                self.on_bool(env, *o1, *o2)?;
                self.on_fun_t(env, f1, f2)
            }
            (NamedProp::Get(t1), NamedProp::Get(t2)) | (NamedProp::Set(t1), NamedProp::Set(t2)) => {
                self.on_t(env, t1, t2)
            }
            _ => self.fail_named_prop(env, np1, np2),
        }
    }

    fn on_mapped_type_homomorphic_flag(
        &mut self,
        env: &Env,
        f1: &MappedTypeHomomorphicFlag<L>,
        f2: &MappedTypeHomomorphicFlag<L>,
    ) -> Result<(), StructuralMismatch> {
        match (f1, f2) {
            (MappedTypeHomomorphicFlag::Homomorphic, MappedTypeHomomorphicFlag::Homomorphic)
            | (
                MappedTypeHomomorphicFlag::Unspecialized,
                MappedTypeHomomorphicFlag::Unspecialized,
            ) => Ok(()),
            (
                MappedTypeHomomorphicFlag::SemiHomomorphic(t1),
                MappedTypeHomomorphicFlag::SemiHomomorphic(t2),
            ) => self.on_t(env, t1, t2),
            _ => self.fail_mapped_type_homomorphic_flag(env, f1, f2),
        }
    }

    fn on_type_param(
        &mut self,
        env: &Env,
        tp1: &TypeParam<L>,
        tp2: &TypeParam<L>,
    ) -> Result<(), StructuralMismatch> {
        self.on_string(env, &tp1.tp_name, &tp2.tp_name)?;
        if tp1.tp_polarity != tp2.tp_polarity {
            return self.fail_polarity(env, &tp1.tp_polarity, &tp2.tp_polarity);
        }
        self.on_bool(env, tp1.tp_const, tp2.tp_const)?;
        self.on_option(
            |s, e, b1, b2| s.on_t(e, b1, b2),
            env,
            &tp1.tp_bound,
            &tp2.tp_bound,
        )?;
        self.on_option(
            |s, e, d1, d2| s.on_t(e, d1, d2),
            env,
            &tp1.tp_default,
            &tp2.tp_default,
        )
    }

    fn on_dict(&mut self, env: &Env, d1: &Dict<L>, d2: &Dict<L>) -> Result<(), StructuralMismatch> {
        if d1.dict_polarity != d2.dict_polarity {
            return self.fail_polarity(env, &d1.dict_polarity, &d2.dict_polarity);
        }
        self.on_t(env, &d1.dict_key, &d2.dict_key)?;
        self.on_t(env, &d1.dict_value, &d2.dict_value)
    }

    fn on_tuple_element(
        &mut self,
        env: &Env,
        te1: &TupleElement<L>,
        te2: &TupleElement<L>,
    ) -> Result<(), StructuralMismatch> {
        match (te1, te2) {
            (
                TupleElement::TupleElement {
                    name: n1,
                    t: t1,
                    polarity: p1,
                    optional: o1,
                },
                TupleElement::TupleElement {
                    name: n2,
                    t: t2,
                    polarity: p2,
                    optional: o2,
                },
            ) => {
                self.on_option(|s, e, n1, n2| s.on_string(e, n1, n2), env, n1, n2)?;
                self.on_t(env, t1, t2)?;
                if p1 != p2 {
                    return self.fail_polarity(env, p1, p2);
                }
                self.on_bool(env, *o1, *o2)
            }
            (
                TupleElement::TupleSpread { name: n1, t: t1 },
                TupleElement::TupleSpread { name: n2, t: t2 },
            ) => {
                self.on_option(|s, e, n1, n2| s.on_string(e, n1, n2), env, n1, n2)?;
                self.on_t(env, t1, t2)
            }
            _ => self.fail_tuple_element(env, te1, te2),
        }
    }

    fn on_builtin_or_symbol(
        &mut self,
        env: &Env,
        bos1: &BuiltinOrSymbol<L>,
        bos2: &BuiltinOrSymbol<L>,
    ) -> Result<(), StructuralMismatch> {
        match (bos1, bos2) {
            (BuiltinOrSymbol::FunProto, BuiltinOrSymbol::FunProto)
            | (BuiltinOrSymbol::ObjProto, BuiltinOrSymbol::ObjProto) => Ok(()),
            (BuiltinOrSymbol::TSymbol(s1), BuiltinOrSymbol::TSymbol(s2)) => {
                self.on_symbol(env, s1, s2)
            }
            _ => self.fail_builtin_or_symbol(env, bos1, bos2),
        }
    }

    fn on_utility(
        &mut self,
        env: &Env,
        u1: &Utility<L>,
        u2: &Utility<L>,
    ) -> Result<(), StructuralMismatch> {
        match (u1, u2) {
            (Utility::Keys(t1), Utility::Keys(t2))
            | (Utility::Values(t1), Utility::Values(t2))
            | (Utility::ReadOnly(t1), Utility::ReadOnly(t2))
            | (Utility::Partial(t1), Utility::Partial(t2))
            | (Utility::Required(t1), Utility::Required(t2))
            | (Utility::Exact(t1), Utility::Exact(t2))
            | (Utility::Enum(t1), Utility::Enum(t2))
            | (Utility::NonMaybeType(t1), Utility::NonMaybeType(t2))
            | (Utility::ObjKeyMirror(t1), Utility::ObjKeyMirror(t2))
            | (Utility::Class(t1), Utility::Class(t2))
            | (Utility::ReactElementConfigType(t1), Utility::ReactElementConfigType(t2)) => {
                self.on_t(env, t1, t2)
            }
            (Utility::Omit(a1, b1), Utility::Omit(a2, b2))
            | (Utility::ElementType(a1, b1), Utility::ElementType(a2, b2)) => {
                self.on_t(env, a1, a2)?;
                self.on_t(env, b1, b2)
            }
            (Utility::Uppercase(t1), Utility::Uppercase(t2))
            | (Utility::Lowercase(t1), Utility::Lowercase(t2))
            | (Utility::Capitalize(t1), Utility::Capitalize(t2))
            | (Utility::Uncapitalize(t1), Utility::Uncapitalize(t2)) => self.on_t(env, t1, t2),
            _ => self.fail_utility(env, u1, u2),
        }
    }

    fn on_component_props(
        &mut self,
        env: &Env,
        cp1: &ComponentProps<L>,
        cp2: &ComponentProps<L>,
    ) -> Result<(), StructuralMismatch> {
        match (cp1, cp2) {
            (
                ComponentProps::UnflattenedComponentProps(t1),
                ComponentProps::UnflattenedComponentProps(t2),
            ) => self.on_t(env, t1, t2),
            (
                ComponentProps::FlattenedComponentProps {
                    props: p1,
                    inexact: i1,
                    dict: dict1,
                },
                ComponentProps::FlattenedComponentProps {
                    props: p2,
                    inexact: i2,
                    dict: dict2,
                },
            ) => {
                self.on_bool(env, *i1, *i2)?;
                match (dict1, dict2) {
                    (Some(d1), Some(d2)) => self.on_dict(env, d1, d2)?,
                    (None, None) => {}
                    _ => return self.fail_component_props(cp1, cp2),
                }
                self.on_list(
                    |s, e, fcp1, fcp2| s.on_flattened_component_prop(e, fcp1, fcp2),
                    env,
                    p1,
                    p2,
                )
            }
            _ => self.fail_component_props(cp1, cp2),
        }
    }

    fn on_fun_effect(
        &mut self,
        _env: &Env,
        fe1: &FunEffect,
        fe2: &FunEffect,
    ) -> Result<(), StructuralMismatch> {
        StructuralMismatch::fail_gen(tag_of_fun_effect, fe1, fe2)
    }

    fn fail_component_props(
        &mut self,
        cp1: &ComponentProps<L>,
        cp2: &ComponentProps<L>,
    ) -> Result<(), StructuralMismatch> {
        StructuralMismatch::fail_gen(tag_of_component_props, cp1, cp2)
    }

    fn on_renders_kind(
        &mut self,
        _env: &Env,
        rk1: &RendersKind,
        rk2: &RendersKind,
    ) -> Result<(), StructuralMismatch> {
        StructuralMismatch::fail_gen(tag_of_renders_kind, rk1, rk2)
    }

    fn on_flattened_component_prop(
        &mut self,
        env: &Env,
        fcp1: &FlattenedComponentProp<L>,
        fcp2: &FlattenedComponentProp<L>,
    ) -> Result<(), StructuralMismatch> {
        match (fcp1, fcp2) {
            (
                FlattenedComponentProp::FlattenedComponentProp {
                    name: n1,
                    optional: o1,
                    def_locs: _,
                    t: t1,
                },
                FlattenedComponentProp::FlattenedComponentProp {
                    name: n2,
                    optional: o2,
                    def_locs: _,
                    t: t2,
                },
            ) => {
                self.on_name(env, n1, n2)?;
                self.on_bool(env, *o1, *o2)?;
                // def_locs: L is generic, cannot call on_aloc here
                // (same limitation as Bound and AnyKind::Annotated)
                self.on_t(env, t1, t2)
            }
        }
    }

    fn on_option_vec_ty(
        &mut self,
        env: &Env,
        o1: &Option<Arc<[Arc<Ty<L>>]>>,
        o2: &Option<Arc<[Arc<Ty<L>>]>>,
    ) -> Result<(), StructuralMismatch> {
        match (o1, o2) {
            (None, None) => Ok(()),
            (Some(v1), Some(v2)) => self.on_list(|s, e, t1, t2| s.on_t(e, t1, t2), env, v1, v2),
            _ => self.fail_option(env, o1, o2),
        }
    }

    fn on_option_vec_type_param(
        &mut self,
        env: &Env,
        o1: &Option<Arc<[TypeParam<L>]>>,
        o2: &Option<Arc<[TypeParam<L>]>>,
    ) -> Result<(), StructuralMismatch> {
        match (o1, o2) {
            (None, None) => Ok(()),
            (Some(v1), Some(v2)) => {
                self.on_list(|s, e, tp1, tp2| s.on_type_param(e, tp1, tp2), env, v1, v2)
            }
            _ => self.fail_option(env, o1, o2),
        }
    }

    fn fail_elt(&mut self, _env: &Env, e1: &Elt<L>, e2: &Elt<L>) -> Result<(), StructuralMismatch> {
        StructuralMismatch::fail_gen(tag_of_elt, e1, e2)
    }

    fn fail_any_kind(
        &mut self,
        _env: &Env,
        ak1: &AnyKind<L>,
        ak2: &AnyKind<L>,
    ) -> Result<(), StructuralMismatch> {
        StructuralMismatch::fail_gen(tag_of_any_kind, ak1, ak2)
    }

    fn fail_upper_bound_kind(
        &mut self,
        _env: &Env,
        ubk1: &UpperBoundKind<L>,
        ubk2: &UpperBoundKind<L>,
    ) -> Result<(), StructuralMismatch> {
        StructuralMismatch::fail_gen(tag_of_upper_bound_kind, ubk1, ubk2)
    }

    fn fail_bot_kind(
        &mut self,
        _env: &Env,
        bk1: &BotKind<L>,
        bk2: &BotKind<L>,
    ) -> Result<(), StructuralMismatch> {
        StructuralMismatch::fail_gen(tag_of_bot_kind, bk1, bk2)
    }

    fn fail_gen_kind(
        &mut self,
        _env: &Env,
        gk1: &GenKind,
        gk2: &GenKind,
    ) -> Result<(), StructuralMismatch> {
        StructuralMismatch::fail_gen(tag_of_gen_kind, gk1, gk2)
    }

    fn fail_obj_kind(
        &mut self,
        _env: &Env,
        ok1: &ObjKind<L>,
        ok2: &ObjKind<L>,
    ) -> Result<(), StructuralMismatch> {
        StructuralMismatch::fail_gen(tag_of_obj_kind, ok1, ok2)
    }

    fn fail_tuple_element(
        &mut self,
        _env: &Env,
        te1: &TupleElement<L>,
        te2: &TupleElement<L>,
    ) -> Result<(), StructuralMismatch> {
        StructuralMismatch::fail_gen(tag_of_tuple_element, te1, te2)
    }

    fn fail_prop(
        &mut self,
        _env: &Env,
        p1: &Prop<L>,
        p2: &Prop<L>,
    ) -> Result<(), StructuralMismatch> {
        StructuralMismatch::fail_gen(tag_of_prop, p1, p2)
    }

    fn fail_named_prop(
        &mut self,
        _env: &Env,
        np1: &NamedProp<L>,
        np2: &NamedProp<L>,
    ) -> Result<(), StructuralMismatch> {
        StructuralMismatch::fail_gen(tag_of_named_prop, np1, np2)
    }

    fn on_prop_source(
        &mut self,
        env: &Env,
        ps1: &PropSource,
        ps2: &PropSource,
    ) -> Result<(), StructuralMismatch> {
        match (ps1, ps2) {
            (PropSource::Interface, PropSource::Interface)
            | (PropSource::Other, PropSource::Other) => Ok(()),
            (PropSource::PrimitiveProto(s1), PropSource::PrimitiveProto(s2)) => {
                self.on_string(env, s1, s2)
            }
            _ => self.fail_prop_source(env, ps1, ps2),
        }
    }

    fn fail_prop_source(
        &mut self,
        _env: &Env,
        ps1: &PropSource,
        ps2: &PropSource,
    ) -> Result<(), StructuralMismatch> {
        StructuralMismatch::fail_gen(tag_of_prop_source, ps1, ps2)
    }

    fn fail_utility(
        &mut self,
        _env: &Env,
        u1: &Utility<L>,
        u2: &Utility<L>,
    ) -> Result<(), StructuralMismatch> {
        StructuralMismatch::fail_gen(tag_of_utility, u1, u2)
    }

    fn fail_polarity(
        &mut self,
        _env: &Env,
        p1: &Polarity,
        p2: &Polarity,
    ) -> Result<(), StructuralMismatch> {
        StructuralMismatch::fail_gen(tag_of_polarity, p1, p2)
    }

    fn fail_unsoundness_kind(
        &mut self,
        _env: &Env,
        uk1: &UnsoundnessKind,
        uk2: &UnsoundnessKind,
    ) -> Result<(), StructuralMismatch> {
        StructuralMismatch::fail_gen(tag_of_unsoundness_kind, uk1, uk2)
    }

    fn fail_builtin_or_symbol(
        &mut self,
        _env: &Env,
        bos1: &BuiltinOrSymbol<L>,
        bos2: &BuiltinOrSymbol<L>,
    ) -> Result<(), StructuralMismatch> {
        StructuralMismatch::fail_gen(tag_of_builtin_or_symbol, bos1, bos2)
    }

    fn fail_return_t(
        &mut self,
        _env: &Env,
        rt1: &ReturnT<L>,
        rt2: &ReturnT<L>,
    ) -> Result<(), StructuralMismatch> {
        StructuralMismatch::fail_gen(tag_of_return_t, rt1, rt2)
    }

    fn fail_any_error_kind(
        &mut self,
        _env: &Env,
        aek1: &AnyErrorKind,
        aek2: &AnyErrorKind,
    ) -> Result<(), StructuralMismatch> {
        StructuralMismatch::fail_gen(tag_of_any_error_kind, aek1, aek2)
    }

    fn fail_mapped_type_homomorphic_flag(
        &mut self,
        _env: &Env,
        mthf1: &MappedTypeHomomorphicFlag<L>,
        mthf2: &MappedTypeHomomorphicFlag<L>,
    ) -> Result<(), StructuralMismatch> {
        StructuralMismatch::fail_gen(tag_of_mapped_type_homomorphic_flag, mthf1, mthf2)
    }

    fn fail_mapped_type_optional_flag(
        &mut self,
        _env: &Env,
        mtof1: &MappedTypeOptionalFlag,
        mtof2: &MappedTypeOptionalFlag,
    ) -> Result<(), StructuralMismatch> {
        StructuralMismatch::fail_gen(tag_of_mapped_type_optional_flag, mtof1, mtof2)
    }

    fn fail_mapped_type_variance(
        &mut self,
        _env: &Env,
        mtv1: &MappedTypeVariance,
        mtv2: &MappedTypeVariance,
    ) -> Result<(), StructuralMismatch> {
        StructuralMismatch::fail_gen(tag_of_mapped_type_variance, mtv1, mtv2)
    }

    fn on_t_impl(&mut self, env: &Env, t1: &Ty<L>, t2: &Ty<L>) -> Result<(), StructuralMismatch> {
        self.on_t(env, t1, t2)
    }

    fn on_obj_t_impl(
        &mut self,
        env: &Env,
        obj1: &ObjT<L>,
        obj2: &ObjT<L>,
    ) -> Result<(), StructuralMismatch> {
        self.on_list(
            |s, e, p1, p2| s.on_prop(e, p1, p2),
            env,
            &obj1.obj_props,
            &obj2.obj_props,
        )?;
        self.on_obj_kind(env, &obj1.obj_kind, &obj2.obj_kind)
    }

    fn on_named_prop_wrapper(
        &mut self,
        env: &Env,
        name1: &Name,
        name2: &Name,
        prop1: &NamedProp<L>,
        prop2: &NamedProp<L>,
        inherited1: bool,
        inherited2: bool,
        source1: &PropSource,
        source2: &PropSource,
        _def_locs1: &[L],
        _def_locs2: &[L],
    ) -> Result<(), StructuralMismatch> {
        self.on_name(env, name1, name2)?;
        self.on_named_prop(env, prop1, prop2)?;
        self.on_bool(env, inherited1, inherited2)?;
        self.on_prop_source(env, source1, source2)?;
        Ok(())
    }
}

pub trait TyReduceTy<L, Env>: TyReduceBase<Env, L>
where
    L: Dupe,
{
    fn on_t(&mut self, env: &Env, t: &Ty<L>) -> Self::Acc {
        match t {
            Ty::Bound(data) => {
                let (_loc, s) = data.as_ref();
                self.on_string(env, s)
            }
            Ty::Generic(g) => self.on_generic_t(env, g),
            Ty::Any(ak) => self.on_any_kind(env, ak),
            Ty::Truncated
            | Ty::Top
            | Ty::Void
            | Ty::Undefined
            | Ty::TsVoid
            | Ty::Null
            | Ty::Symbol
            | Ty::UniqueSymbol
            | Ty::Num
            | Ty::Str
            | Ty::Bool
            | Ty::BigInt => Self::Acc::zero(),
            Ty::Bot(bk) => self.on_bot_kind(env, bk),
            Ty::NumLit(s) | Ty::BigIntLit(s) => self.on_string(env, s),
            Ty::StrLit(s) => self.on_string(env, s),
            Ty::BoolLit(b) => self.on_bool(env, *b),
            Ty::Fun(fun_t) => self.on_fun_t(env, fun_t),
            Ty::Obj(obj_t) => self.on_obj_t(env, obj_t),
            Ty::Arr(arr_t) => self.on_arr_t(env, arr_t),
            Ty::Tup { elements, inexact } => {
                let mut acc = self.on_bool(env, *inexact);
                for elem in elements.iter() {
                    acc = Self::Acc::plus(acc, self.on_tuple_element(env, elem));
                }
                acc
            }
            Ty::Union(from_annotation, t1, t2, rest) => {
                let mut acc = self.on_bool(env, *from_annotation);
                acc = Self::Acc::plus(acc, self.on_t(env, t1));
                acc = Self::Acc::plus(acc, self.on_t(env, t2));
                for t in rest.iter() {
                    acc = Self::Acc::plus(acc, self.on_t(env, t));
                }
                acc
            }
            Ty::Inter(t1, t2, rest) => {
                let mut acc = self.on_t(env, t1);
                acc = Self::Acc::plus(acc, self.on_t(env, t2));
                for t in rest.iter() {
                    acc = Self::Acc::plus(acc, self.on_t(env, t));
                }
                acc
            }
            Ty::InlineInterface(iface) => self.on_interface_t(env, iface),
            Ty::TypeOf(data) => {
                let (bos, targs) = data.as_ref();
                let mut acc = self.on_builtin_or_symbol(env, bos);
                if let Some(ts) = targs {
                    for t in ts.iter() {
                        acc = Self::Acc::plus(acc, self.on_t(env, t));
                    }
                }
                acc
            }
            Ty::Utility(u) => self.on_utility(env, u),
            Ty::IndexedAccess {
                _object,
                index,
                optional,
            } => {
                let mut acc = self.on_t(env, _object);
                acc = Self::Acc::plus(acc, self.on_t(env, index));
                Self::Acc::plus(acc, self.on_bool(env, *optional))
            }
            Ty::Conditional {
                check_type,
                extends_type,
                true_type,
                false_type,
            } => {
                let mut acc = self.on_t(env, check_type);
                acc = Self::Acc::plus(acc, self.on_t(env, extends_type));
                acc = Self::Acc::plus(acc, self.on_t(env, true_type));
                Self::Acc::plus(acc, self.on_t(env, false_type))
            }
            Ty::Infer(data) => {
                let (sym, bound) = data.as_ref();
                let mut acc = self.on_symbol(env, sym);
                if let Some(b) = bound {
                    acc = Self::Acc::plus(acc, self.on_t(env, b));
                }
                acc
            }
            Ty::Component {
                regular_props,
                renders,
            } => {
                let mut acc = self.on_component_props(env, regular_props);
                if let Some(r) = renders {
                    acc = Self::Acc::plus(acc, self.on_t(env, r));
                }
                acc
            }
            Ty::Renders(t, _kind) => self.on_t(env, t),
            Ty::TemplateLiteral { quasis: _, types } => {
                types.iter().fold(Self::Acc::zero(), |acc, t| {
                    Self::Acc::plus(acc, self.on_t(env, t))
                })
            }
        }
    }

    fn on_decl(&mut self, env: &Env, d: &Decl<L>) -> Self::Acc {
        match d {
            Decl::VariableDecl(box (name, t)) => {
                Self::Acc::plus(self.on_name(env, name), self.on_t(env, t))
            }
            Decl::TypeAliasDecl(box DeclTypeAliasDeclData {
                import,
                name,
                tparams,
                type_,
                opaque,
            }) => {
                let mut acc = self.on_bool(env, *import);
                acc = Self::Acc::plus(acc, self.on_symbol(env, name));
                if let Some(tps) = tparams {
                    for tp in tps.iter() {
                        acc = Self::Acc::plus(acc, self.on_type_param(env, tp));
                    }
                }
                if let Some(t) = type_ {
                    acc = Self::Acc::plus(acc, self.on_t(env, t));
                }
                if let Some(OpaqueBounds { lower, upper }) = opaque {
                    for t in [lower, upper].into_iter().flatten() {
                        acc = Self::Acc::plus(acc, self.on_t(env, t));
                    }
                }
                acc
            }
            Decl::ClassDecl(box (sym, tparams))
            | Decl::InterfaceDecl(box (sym, tparams))
            | Decl::RecordDecl(box (sym, tparams)) => {
                let mut acc = self.on_symbol(env, sym);
                if let Some(tps) = tparams {
                    for tp in tps.iter() {
                        acc = Self::Acc::plus(acc, self.on_type_param(env, tp));
                    }
                }
                acc
            }
            Decl::EnumDecl(box DeclEnumDeclData {
                name,
                members,
                has_unknown_members,
                truncated_members_count,
            }) => {
                let mut acc = self.on_symbol(env, name);
                if let Some(ms) = members {
                    for member in ms.iter() {
                        acc = Self::Acc::plus(acc, self.on_string(env, member.as_str()));
                    }
                }
                acc = Self::Acc::plus(acc, self.on_bool(env, *has_unknown_members));
                Self::Acc::plus(acc, self.on_int(env, *truncated_members_count))
            }
            Decl::NominalComponentDecl(box DeclNominalComponentDeclData {
                name,
                tparams,
                targs,
                props,
                renders,
                is_type,
            }) => {
                let mut acc = self.on_symbol(env, name);
                if let Some(tps) = tparams {
                    for tp in tps.iter() {
                        acc = Self::Acc::plus(acc, self.on_type_param(env, tp));
                    }
                }
                if let Some(tas) = targs {
                    for t in tas.iter() {
                        acc = Self::Acc::plus(acc, self.on_t(env, t));
                    }
                }
                acc = Self::Acc::plus(acc, self.on_component_props(env, props));
                if let Some(r) = renders {
                    acc = Self::Acc::plus(acc, self.on_t(env, r));
                }
                Self::Acc::plus(acc, self.on_bool(env, *is_type))
            }
            Decl::NamespaceDecl(box DeclNamespaceDeclData { name, exports }) => {
                let mut acc = Self::Acc::zero();
                if let Some(n) = name {
                    acc = self.on_symbol(env, n);
                }
                for d in exports.iter() {
                    acc = Self::Acc::plus(acc, self.on_decl(env, d));
                }
                acc
            }
            Decl::ModuleDecl(box DeclModuleDeclData {
                name,
                exports,
                default,
            }) => {
                let mut acc = Self::Acc::zero();
                if let Some(n) = name {
                    acc = self.on_symbol(env, n);
                }
                for d in exports.iter() {
                    acc = Self::Acc::plus(acc, self.on_decl(env, d));
                }
                if let Some(t) = default {
                    acc = Self::Acc::plus(acc, self.on_t(env, t));
                }
                acc
            }
        }
    }

    fn on_elt(&mut self, env: &Env, e: &Elt<L>) -> Self::Acc {
        match e {
            Elt::Type(t) => self.on_t(env, t),
            Elt::Decl(d) => self.on_decl(env, d),
        }
    }

    fn on_generic_t(&mut self, env: &Env, g: &GenericT<L>) -> Self::Acc {
        let (sym, _kind, targs) = g;
        let mut acc = self.on_symbol(env, sym);
        if let Some(ts) = targs {
            for t in ts.iter() {
                acc = Self::Acc::plus(acc, self.on_t(env, t));
            }
        }
        acc
    }

    fn on_any_kind(&mut self, _env: &Env, _ak: &AnyKind<L>) -> Self::Acc {
        Self::Acc::zero()
    }

    fn on_bot_kind(&mut self, env: &Env, bk: &BotKind<L>) -> Self::Acc {
        match bk {
            BotKind::EmptyType => Self::Acc::zero(),
            BotKind::NoLowerWithUpper(ubk) => self.on_upper_bound_kind(env, ubk),
        }
    }

    fn on_upper_bound_kind(&mut self, env: &Env, ubk: &UpperBoundKind<L>) -> Self::Acc {
        match ubk {
            UpperBoundKind::NoUpper => Self::Acc::zero(),
            UpperBoundKind::SomeKnownUpper(t) => self.on_t(env, t),
            UpperBoundKind::SomeUnknownUpper(s) => self.on_string(env, s),
        }
    }

    fn on_fun_t(&mut self, env: &Env, f: &FunT<L>) -> Self::Acc {
        let mut acc = Self::Acc::zero();
        if let Some(ty) = &f.fun_this_param {
            acc = Self::Acc::plus(acc, self.on_t(env, ty));
        }
        for (_, ty, _) in f.fun_params.iter() {
            acc = Self::Acc::plus(acc, self.on_t(env, ty));
        }
        if let Some((_, ty)) = &f.fun_rest_param {
            acc = Self::Acc::plus(acc, self.on_t(env, ty));
        }
        acc = Self::Acc::plus(acc, self.on_return_t(env, &f.fun_return));
        if let Some(tps) = &f.fun_type_params {
            for tp in tps.iter() {
                acc = Self::Acc::plus(acc, self.on_type_param(env, tp));
            }
        }
        Self::Acc::plus(acc, self.on_t(env, &f.fun_static))
    }

    fn on_return_t(&mut self, env: &Env, r: &ReturnT<L>) -> Self::Acc {
        match r {
            ReturnT::ReturnType(t) => self.on_t(env, t),
            ReturnT::TypeGuard(_, _, t) => match t {
                Some(t) => self.on_t(env, t),
                None => Self::Acc::zero(),
            },
        }
    }

    fn on_obj_t(&mut self, env: &Env, o: &ObjT<L>) -> Self::Acc {
        let mut acc = Self::Acc::zero();
        for prop in o.obj_props.iter() {
            acc = Self::Acc::plus(acc, self.on_prop(env, prop));
        }
        Self::Acc::plus(acc, self.on_obj_kind(env, &o.obj_kind))
    }

    fn on_obj_kind(&mut self, env: &Env, ok: &ObjKind<L>) -> Self::Acc {
        match ok {
            ObjKind::ExactObj | ObjKind::InexactObj | ObjKind::MappedTypeObj => Self::Acc::zero(),
            ObjKind::IndexedObj(dict) => self.on_dict(env, dict),
        }
    }

    fn on_arr_t(&mut self, env: &Env, a: &ArrT<L>) -> Self::Acc {
        self.on_t(env, &a.arr_elt_t)
    }

    fn on_interface_t(&mut self, env: &Env, i: &InterfaceT<L>) -> Self::Acc {
        let mut acc = Self::Acc::zero();
        for g in i.if_extends.iter() {
            acc = Self::Acc::plus(acc, self.on_generic_t(env, g));
        }
        for prop in i.if_props.iter() {
            acc = Self::Acc::plus(acc, self.on_prop(env, prop));
        }
        if let Some(dict) = &i.if_dict {
            acc = Self::Acc::plus(acc, self.on_dict(env, dict));
        }
        acc
    }

    fn on_prop(&mut self, env: &Env, p: &Prop<L>) -> Self::Acc {
        match p {
            Prop::NamedProp {
                name,
                prop,
                inherited: _,
                source: _,
                def_locs: _,
            } => Self::Acc::plus(self.on_name(env, name), self.on_named_prop(env, prop)),
            Prop::CallProp(fun_t) => self.on_fun_t(env, fun_t),
            Prop::SpreadProp(t) => self.on_t(env, t),
            Prop::MappedTypeProp {
                key_tparam,
                source,
                prop,
                name_type,
                flags: _,
                homomorphic,
            } => {
                let mut acc = self.on_type_param(env, key_tparam);
                acc = Self::Acc::plus(acc, self.on_t(env, source));
                acc = Self::Acc::plus(acc, self.on_t(env, prop));
                if let Some(nt) = name_type {
                    acc = Self::Acc::plus(acc, self.on_t(env, nt));
                }
                Self::Acc::plus(acc, self.on_mapped_type_homomorphic_flag(env, homomorphic))
            }
        }
    }

    fn on_named_prop(&mut self, env: &Env, np: &NamedProp<L>) -> Self::Acc {
        match np {
            NamedProp::Field {
                t,
                polarity: _,
                optional: _,
            } => self.on_t(env, t),
            NamedProp::Method { fun: fun_t, .. } => self.on_fun_t(env, fun_t),
            NamedProp::Get(t) | NamedProp::Set(t) => self.on_t(env, t),
        }
    }

    fn on_mapped_type_homomorphic_flag(
        &mut self,
        env: &Env,
        flag: &MappedTypeHomomorphicFlag<L>,
    ) -> Self::Acc {
        match flag {
            MappedTypeHomomorphicFlag::Homomorphic | MappedTypeHomomorphicFlag::Unspecialized => {
                Self::Acc::zero()
            }
            MappedTypeHomomorphicFlag::SemiHomomorphic(t) => self.on_t(env, t),
        }
    }

    fn on_type_param(&mut self, env: &Env, tp: &TypeParam<L>) -> Self::Acc {
        let mut acc = Self::Acc::zero();
        if let Some(bound) = &tp.tp_bound {
            acc = Self::Acc::plus(acc, self.on_t(env, bound));
        }
        if let Some(default) = &tp.tp_default {
            acc = Self::Acc::plus(acc, self.on_t(env, default));
        }
        acc
    }

    fn on_dict(&mut self, env: &Env, d: &Dict<L>) -> Self::Acc {
        Self::Acc::plus(self.on_t(env, &d.dict_key), self.on_t(env, &d.dict_value))
    }

    fn on_tuple_element(&mut self, env: &Env, te: &TupleElement<L>) -> Self::Acc {
        match te {
            TupleElement::TupleElement {
                name: _,
                t,
                polarity: _,
                optional: _,
            }
            | TupleElement::TupleSpread { name: _, t } => self.on_t(env, t),
        }
    }

    fn on_builtin_or_symbol(&mut self, env: &Env, bos: &BuiltinOrSymbol<L>) -> Self::Acc {
        match bos {
            BuiltinOrSymbol::FunProto | BuiltinOrSymbol::ObjProto => Self::Acc::zero(),
            BuiltinOrSymbol::TSymbol(sym) => self.on_symbol(env, sym),
        }
    }

    fn on_utility(&mut self, env: &Env, u: &Utility<L>) -> Self::Acc {
        match u {
            Utility::Keys(t)
            | Utility::Values(t)
            | Utility::ReadOnly(t)
            | Utility::Partial(t)
            | Utility::Required(t)
            | Utility::Exact(t)
            | Utility::Enum(t)
            | Utility::NonMaybeType(t)
            | Utility::ObjKeyMirror(t)
            | Utility::Class(t)
            | Utility::ReactElementConfigType(t) => self.on_t(env, t),
            Utility::Omit(t1, t2) | Utility::ElementType(t1, t2) => {
                Self::Acc::plus(self.on_t(env, t1), self.on_t(env, t2))
            }
            Utility::Uppercase(t)
            | Utility::Lowercase(t)
            | Utility::Capitalize(t)
            | Utility::Uncapitalize(t) => self.on_t(env, t),
        }
    }

    fn on_component_props(&mut self, env: &Env, cp: &ComponentProps<L>) -> Self::Acc {
        match cp {
            ComponentProps::UnflattenedComponentProps(t) => self.on_t(env, t),
            ComponentProps::FlattenedComponentProps {
                props,
                inexact: _,
                dict,
            } => {
                let mut acc = Self::Acc::zero();
                for prop in props.iter() {
                    acc = Self::Acc::plus(acc, self.on_flattened_component_prop(env, prop));
                }
                if let Some(dict) = dict {
                    acc = Self::Acc::plus(acc, self.on_dict(env, dict));
                }
                acc
            }
        }
    }

    fn on_flattened_component_prop(
        &mut self,
        env: &Env,
        fcp: &FlattenedComponentProp<L>,
    ) -> Self::Acc {
        match fcp {
            FlattenedComponentProp::FlattenedComponentProp {
                name,
                optional: _,
                def_locs: _,
                t,
            } => Self::Acc::plus(self.on_name(env, name), self.on_t(env, t)),
        }
    }
}

pub trait TyMapTy<L, Env>: TyMapBase<Env, L>
where
    L: Dupe,
{
    fn on_t(&mut self, env: &Env, t: Ty<L>) -> Ty<L>;
    fn on_decl(&mut self, env: &Env, d: Decl<L>) -> Decl<L>;
    fn on_elt(&mut self, env: &Env, e: Elt<L>) -> Elt<L>;
}

pub trait TyEndoTy<L, Env>: TyEndoBase<Env, L>
where
    L: Dupe,
{
    fn default_on_t(&mut self, env: &Env, t: Arc<Ty<L>>) -> Arc<Ty<L>> {
        match t.as_ref() {
            Ty::Bound(data) => {
                let (loc, s) = data.as_ref();
                let s_new = self.on_string(env, s.clone());
                Arc::new(Ty::Bound(Box::new((loc.clone(), s_new))))
            }
            Ty::Generic(g) => {
                let g_new = self.on_generic_t(env, (**g).clone());
                Arc::new(Ty::Generic(Box::new(g_new)))
            }
            Ty::Any(ak) => {
                let ak_new = self.on_any_kind(env, ak.clone());
                Arc::new(Ty::Any(ak_new))
            }
            Ty::Truncated
            | Ty::Top
            | Ty::Void
            | Ty::Undefined
            | Ty::TsVoid
            | Ty::Null
            | Ty::Symbol
            | Ty::UniqueSymbol
            | Ty::Num
            | Ty::Str
            | Ty::Bool
            | Ty::BigInt => t,
            Ty::Bot(bk) => {
                let bk_new = self.on_bot_kind(env, bk.clone());
                Arc::new(Ty::Bot(bk_new))
            }
            Ty::NumLit(s) => {
                let s_new = self.on_string(env, s.clone());
                Arc::new(Ty::NumLit(s_new))
            }
            Ty::BigIntLit(s) => {
                let s_new = self.on_string(env, s.clone());
                Arc::new(Ty::BigIntLit(s_new))
            }
            Ty::StrLit(s) => {
                let s_new: FlowSmolStr = self.on_string(env, s.to_string()).into();
                Arc::new(Ty::StrLit(s_new))
            }
            Ty::BoolLit(b) => {
                let b_new = self.on_bool(env, *b);
                Arc::new(Ty::BoolLit(b_new))
            }
            Ty::Fun(fun_t) => {
                let fun_t_new = self.on_fun_t(env, (**fun_t).clone());
                Arc::new(Ty::Fun(Box::new(fun_t_new)))
            }
            Ty::Obj(obj_t) => {
                let obj_t_new = self.on_obj_t(env, (**obj_t).clone());
                Arc::new(Ty::Obj(Box::new(obj_t_new)))
            }
            Ty::Arr(arr_t) => {
                let arr_t_new = self.on_arr_t(env, arr_t.clone());
                Arc::new(Ty::Arr(arr_t_new))
            }
            Ty::Tup { elements, inexact } => {
                let inexact_new = self.on_bool(env, *inexact);
                let elements_new: Arc<[_]> = self
                    .on_list(|s, e, elem| s.on_tuple_element(e, elem), env, elements)
                    .into();
                Arc::new(Ty::Tup {
                    elements: elements_new,
                    inexact: inexact_new,
                })
            }
            Ty::Union(from_annotation, t1, t2, rest) => {
                let from_annotation_new = self.on_bool(env, *from_annotation);
                let t1_new = self.on_t(env, t1.dupe());
                let t2_new = self.on_t(env, t2.dupe());
                let rest_new: Arc<[_]> = self.on_list(|s, e, t| s.on_t(e, t), env, rest).into();
                Arc::new(Ty::Union(from_annotation_new, t1_new, t2_new, rest_new))
            }
            Ty::Inter(t1, t2, rest) => {
                let t1_new = self.on_t(env, t1.dupe());
                let t2_new = self.on_t(env, t2.dupe());
                let rest_new: Arc<[_]> = self.on_list(|s, e, t| s.on_t(e, t), env, rest).into();
                Arc::new(Ty::Inter(t1_new, t2_new, rest_new))
            }
            Ty::InlineInterface(iface) => {
                let iface_new = self.on_interface_t(env, (**iface).clone());
                Arc::new(Ty::InlineInterface(Box::new(iface_new)))
            }
            Ty::TypeOf(data) => {
                let (bos, targs) = data.as_ref();
                let bos_new = self.on_builtin_or_symbol(env, bos.clone());
                let targs_new = targs.as_ref().map(|ts| {
                    let v: Arc<[_]> = self.on_list(|s2, e2, t| s2.on_t(e2, t), env, ts).into();
                    v
                });
                Arc::new(Ty::TypeOf(Box::new((bos_new, targs_new))))
            }
            Ty::Utility(u) => {
                let u_new = self.on_utility(env, u.clone());
                Arc::new(Ty::Utility(u_new))
            }
            Ty::IndexedAccess {
                _object,
                index,
                optional,
            } => {
                let object_new = self.on_t(env, _object.dupe());
                let index_new = self.on_t(env, index.dupe());
                let optional_new = self.on_bool(env, *optional);
                Arc::new(Ty::IndexedAccess {
                    _object: object_new,
                    index: index_new,
                    optional: optional_new,
                })
            }
            Ty::Conditional {
                check_type,
                extends_type,
                true_type,
                false_type,
            } => {
                let check_type_new = self.on_t(env, check_type.dupe());
                let extends_type_new = self.on_t(env, extends_type.dupe());
                let true_type_new = self.on_t(env, true_type.dupe());
                let false_type_new = self.on_t(env, false_type.dupe());
                Arc::new(Ty::Conditional {
                    check_type: check_type_new,
                    extends_type: extends_type_new,
                    true_type: true_type_new,
                    false_type: false_type_new,
                })
            }
            Ty::Infer(data) => {
                let (sym, bound) = data.as_ref();
                let sym_new = self.on_symbol(env, sym.clone());
                let bound_new = self.on_option(|s, e, b| s.on_t(e, b), env, bound.clone());
                Arc::new(Ty::Infer(Box::new((sym_new, bound_new))))
            }
            Ty::Component {
                regular_props,
                renders,
            } => {
                let regular_props_new = self.on_component_props(env, regular_props.clone());
                let renders_new = self.on_option(|s, e, r| s.on_t(e, r), env, renders.clone());
                Arc::new(Ty::Component {
                    regular_props: regular_props_new,
                    renders: renders_new,
                })
            }
            Ty::Renders(t, kind) => {
                let t_new = self.on_t(env, t.dupe());
                let kind_new = self.on_renders_kind(env, *kind);
                Arc::new(Ty::Renders(t_new, kind_new))
            }
            Ty::TemplateLiteral { quasis, types } => {
                let types_new: Arc<[_]> = self.on_list(|s, e, t| s.on_t(e, t), env, types).into();
                Arc::new(Ty::TemplateLiteral {
                    quasis: quasis.clone(),
                    types: types_new,
                })
            }
        }
    }

    fn on_t(&mut self, env: &Env, t: Arc<Ty<L>>) -> Arc<Ty<L>> {
        self.default_on_t(env, t)
    }

    fn on_t_owned(&mut self, env: &Env, t: Ty<L>) -> Ty<L> {
        let result_arc = self.on_t(env, Arc::new(t));
        (*result_arc).clone()
    }

    fn default_on_decl(&mut self, env: &Env, d: Decl<L>) -> Decl<L> {
        match d {
            Decl::VariableDecl(box (name, t)) => {
                let name_new = self.on_name(env, name);
                let t_new = self.on_t(env, t);
                Decl::VariableDecl(Box::new((name_new, t_new)))
            }
            Decl::TypeAliasDecl(box DeclTypeAliasDeclData {
                import,
                name,
                tparams,
                type_,
                opaque,
            }) => {
                let import_new = self.on_bool(env, import);
                let name_new = self.on_symbol(env, name);
                let tparams_new = tparams.map(|tps| -> Arc<[_]> {
                    self.on_list(|s2, e2, tp| s2.on_type_param(e2, tp), env, &tps)
                        .into()
                });
                let type_new = self.on_option(|s, e, t| s.on_t(e, t), env, type_);
                let opaque_new = opaque.map(|OpaqueBounds { lower, upper }| OpaqueBounds {
                    lower: self.on_option(|s, e, t| s.on_t(e, t), env, lower),
                    upper: self.on_option(|s, e, t| s.on_t(e, t), env, upper),
                });
                Decl::TypeAliasDecl(Box::new(DeclTypeAliasDeclData {
                    import: import_new,
                    name: name_new,
                    tparams: tparams_new,
                    type_: type_new,
                    opaque: opaque_new,
                }))
            }
            Decl::ClassDecl(box (sym, tparams)) => {
                let sym_new = self.on_symbol(env, sym);
                let tparams_new = tparams.map(|tps| -> Arc<[_]> {
                    self.on_list(|s2, e2, tp| s2.on_type_param(e2, tp), env, &tps)
                        .into()
                });
                Decl::ClassDecl(Box::new((sym_new, tparams_new)))
            }
            Decl::InterfaceDecl(box (sym, tparams)) => {
                let sym_new = self.on_symbol(env, sym);
                let tparams_new = tparams.map(|tps| -> Arc<[_]> {
                    self.on_list(|s2, e2, tp| s2.on_type_param(e2, tp), env, &tps)
                        .into()
                });
                Decl::InterfaceDecl(Box::new((sym_new, tparams_new)))
            }
            Decl::RecordDecl(box (sym, tparams)) => {
                let sym_new = self.on_symbol(env, sym);
                let tparams_new = tparams.map(|tps| -> Arc<[_]> {
                    self.on_list(|s2, e2, tp| s2.on_type_param(e2, tp), env, &tps)
                        .into()
                });
                Decl::RecordDecl(Box::new((sym_new, tparams_new)))
            }
            Decl::EnumDecl(box DeclEnumDeclData {
                name,
                members,
                has_unknown_members,
                truncated_members_count,
            }) => {
                let name_new = self.on_symbol(env, name);
                let members_new = members.map(|ms| -> Arc<[_]> {
                    self.on_list(
                        |s2, e2, member| -> FlowSmolStr {
                            s2.on_string(e2, member.to_string()).into()
                        },
                        env,
                        &ms,
                    )
                    .into()
                });
                let has_unknown_members_new = self.on_bool(env, has_unknown_members);
                let truncated_members_count_new = self.on_int(env, truncated_members_count);
                Decl::EnumDecl(Box::new(DeclEnumDeclData {
                    name: name_new,
                    members: members_new,
                    has_unknown_members: has_unknown_members_new,
                    truncated_members_count: truncated_members_count_new,
                }))
            }
            Decl::NominalComponentDecl(box DeclNominalComponentDeclData {
                name,
                tparams,
                targs,
                props,
                renders,
                is_type,
            }) => {
                let name_new = self.on_symbol(env, name);
                let tparams_new = tparams.map(|tps| -> Arc<[_]> {
                    self.on_list(|s2, e2, tp| s2.on_type_param(e2, tp), env, &tps)
                        .into()
                });
                let targs_new = targs.map(|tas| -> Arc<[_]> {
                    self.on_list(|s2, e2, t| s2.on_t(e2, t), env, &tas).into()
                });
                let props_new = self.on_component_props(env, props);
                let renders_new = self.on_option(|s, e, r| s.on_t_owned(e, r), env, renders);
                let is_type_new = self.on_bool(env, is_type);
                Decl::NominalComponentDecl(Box::new(DeclNominalComponentDeclData {
                    name: name_new,
                    tparams: tparams_new,
                    targs: targs_new,
                    props: props_new,
                    renders: renders_new,
                    is_type: is_type_new,
                }))
            }
            Decl::NamespaceDecl(box DeclNamespaceDeclData { name, exports }) => {
                let name_new = self.on_option(|s, e, n| s.on_symbol(e, n), env, name);
                let exports_new: Arc<[_]> = self
                    .on_list(|s, e, d| s.on_decl(e, d), env, &exports)
                    .into();
                Decl::NamespaceDecl(Box::new(DeclNamespaceDeclData {
                    name: name_new,
                    exports: exports_new,
                }))
            }
            Decl::ModuleDecl(box DeclModuleDeclData {
                name,
                exports,
                default,
            }) => {
                let name_new = self.on_option(|s, e, n| s.on_symbol(e, n), env, name);
                let exports_new: Arc<[_]> = self
                    .on_list(|s, e, d| s.on_decl(e, d), env, &exports)
                    .into();
                let default_new = self.on_option(|s, e, t| s.on_t(e, t), env, default);
                Decl::ModuleDecl(Box::new(DeclModuleDeclData {
                    name: name_new,
                    exports: exports_new,
                    default: default_new,
                }))
            }
        }
    }

    fn on_decl(&mut self, env: &Env, d: Decl<L>) -> Decl<L> {
        self.default_on_decl(env, d)
    }

    fn on_elt(&mut self, env: &Env, e: Elt<L>) -> Elt<L> {
        match e {
            Elt::Type(t) => Elt::Type(self.on_t(env, t)),
            Elt::Decl(d) => Elt::Decl(self.on_decl(env, d)),
        }
    }

    fn on_generic_t(&mut self, env: &Env, g: GenericT<L>) -> GenericT<L> {
        let (sym, kind, targs) = g;
        let sym_new = self.on_symbol(env, sym);
        let kind_new = self.on_gen_kind(env, kind);
        let targs_new = targs
            .map(|ts| -> Arc<[_]> { self.on_list(|s2, e2, t| s2.on_t(e2, t), env, &ts).into() });
        (sym_new, kind_new, targs_new)
    }

    fn on_any_kind(&mut self, env: &Env, ak: AnyKind<L>) -> AnyKind<L> {
        match ak {
            AnyKind::Annotated(loc) => AnyKind::Annotated(loc),
            AnyKind::AnyError(ae_opt) => {
                let ae_opt_new = self.on_option(|s, e, ae| s.on_any_error_kind(e, ae), env, ae_opt);
                AnyKind::AnyError(ae_opt_new)
            }
            AnyKind::Recursive => AnyKind::Recursive,
            AnyKind::Unsound(uk) => {
                let uk_new = self.on_unsoundness_kind(env, uk);
                AnyKind::Unsound(uk_new)
            }
            AnyKind::Untyped => AnyKind::Untyped,
            AnyKind::Placeholder => AnyKind::Placeholder,
        }
    }

    fn on_any_error_kind(&mut self, _env: &Env, aek: AnyErrorKind) -> AnyErrorKind {
        aek
    }

    fn on_unsoundness_kind(&mut self, _env: &Env, uk: UnsoundnessKind) -> UnsoundnessKind {
        uk
    }

    fn on_bot_kind(&mut self, env: &Env, bk: BotKind<L>) -> BotKind<L> {
        match bk {
            BotKind::EmptyType => BotKind::EmptyType,
            BotKind::NoLowerWithUpper(ubk) => {
                let ubk_new = self.on_upper_bound_kind(env, ubk);
                BotKind::NoLowerWithUpper(ubk_new)
            }
        }
    }

    fn on_gen_kind(&mut self, _env: &Env, gk: GenKind) -> GenKind {
        gk
    }

    fn on_fun_effect(&mut self, _env: &Env, fe: FunEffect) -> FunEffect {
        fe
    }

    fn on_upper_bound_kind(&mut self, env: &Env, ubk: UpperBoundKind<L>) -> UpperBoundKind<L> {
        match ubk {
            UpperBoundKind::NoUpper => UpperBoundKind::NoUpper,
            UpperBoundKind::SomeKnownUpper(t) => {
                let t_new = self.on_t(env, t);
                UpperBoundKind::SomeKnownUpper(t_new)
            }
            UpperBoundKind::SomeUnknownUpper(s) => {
                let s_new = self.on_string(env, s);
                UpperBoundKind::SomeUnknownUpper(s_new)
            }
        }
    }

    fn on_fun_t(&mut self, env: &Env, f: FunT<L>) -> FunT<L> {
        let fun_this_param_new = f.fun_this_param.map(|ty| self.on_t(env, ty));
        let fun_params_new: Arc<[_]> = f
            .fun_params
            .iter()
            .cloned()
            .map(|(name_opt, ty, param)| {
                let ty_new = self.on_t(env, ty);
                let param_new = self.on_fun_param(env, param);
                (name_opt, ty_new, param_new)
            })
            .collect::<Vec<_>>()
            .into();
        let fun_rest_param_new = f.fun_rest_param.map(|(name, ty)| {
            let ty_new = self.on_t(env, ty);
            (name, ty_new)
        });
        let fun_return_new = self.on_return_t(env, f.fun_return);
        let fun_type_params_new = f.fun_type_params.map(|tps| -> Arc<[_]> {
            self.on_list(|s2, e2, tp| s2.on_type_param(e2, tp), env, &tps)
                .into()
        });
        let fun_static_new = self.on_t(env, f.fun_static);
        let fun_effect_new = self.on_fun_effect(env, f.fun_effect);
        FunT {
            fun_this_param: fun_this_param_new,
            fun_params: fun_params_new,
            fun_rest_param: fun_rest_param_new,
            fun_return: fun_return_new,
            fun_type_params: fun_type_params_new,
            fun_static: fun_static_new,
            fun_effect: fun_effect_new,
        }
    }

    fn on_return_t(&mut self, env: &Env, r: ReturnT<L>) -> ReturnT<L> {
        match r {
            ReturnT::ReturnType(t) => {
                let t_new = self.on_t(env, t);
                ReturnT::ReturnType(t_new)
            }
            ReturnT::TypeGuard(kind, name, t) => {
                let t_new = t.map(|t| self.on_t(env, t));
                ReturnT::TypeGuard(kind, name, t_new)
            }
        }
    }

    fn on_fun_param(&mut self, _env: &Env, fp: FunParam) -> FunParam {
        fp
    }

    fn on_obj_t(&mut self, env: &Env, o: ObjT<L>) -> ObjT<L> {
        let obj_props_new: Arc<[_]> = self
            .on_list(|s, e, p| s.on_prop(e, p), env, &o.obj_props)
            .into();
        let obj_kind_new = self.on_obj_kind(env, o.obj_kind);
        ObjT {
            obj_def_loc: o.obj_def_loc,
            obj_props: obj_props_new,
            obj_kind: obj_kind_new,
        }
    }

    fn on_obj_kind(&mut self, env: &Env, ok: ObjKind<L>) -> ObjKind<L> {
        match ok {
            ObjKind::ExactObj => ObjKind::ExactObj,
            ObjKind::InexactObj => ObjKind::InexactObj,
            ObjKind::MappedTypeObj => ObjKind::MappedTypeObj,
            ObjKind::IndexedObj(dict) => {
                let dict_new = self.on_dict(env, dict);
                ObjKind::IndexedObj(dict_new)
            }
        }
    }

    fn on_arr_t(&mut self, env: &Env, a: ArrT<L>) -> ArrT<L> {
        let arr_elt_t_new = self.on_t(env, a.arr_elt_t);
        ArrT {
            arr_readonly: a.arr_readonly,
            arr_elt_t: arr_elt_t_new,
        }
    }

    fn on_interface_t(&mut self, env: &Env, i: InterfaceT<L>) -> InterfaceT<L> {
        let if_extends_new: Arc<[_]> = self
            .on_list(|s, e, g| s.on_generic_t(e, g), env, &i.if_extends)
            .into();
        let if_props_new: Arc<[_]> = self
            .on_list(|s, e, p| s.on_prop(e, p), env, &i.if_props)
            .into();
        let if_dict_new = self.on_option(|s, e, d| s.on_dict(e, d), env, i.if_dict);
        InterfaceT {
            if_extends: if_extends_new,
            if_props: if_props_new,
            if_dict: if_dict_new,
        }
    }

    fn on_prop(&mut self, env: &Env, p: Prop<L>) -> Prop<L> {
        match p {
            Prop::NamedProp {
                name,
                prop,
                inherited,
                source,
                def_locs,
            } => {
                let name_new = self.on_name(env, name);
                let prop_new = self.on_named_prop(env, prop);
                let source_new = self.on_prop_source(env, source);
                Prop::NamedProp {
                    name: name_new,
                    prop: prop_new,
                    inherited,
                    source: source_new,
                    def_locs,
                }
            }
            Prop::CallProp(fun_t) => {
                let fun_t_new = self.on_fun_t(env, fun_t);
                Prop::CallProp(fun_t_new)
            }
            Prop::SpreadProp(t) => {
                let t_new = self.on_t(env, t);
                Prop::SpreadProp(t_new)
            }
            Prop::MappedTypeProp {
                key_tparam,
                source,
                prop,
                name_type,
                flags,
                homomorphic,
            } => {
                let key_tparam_new = self.on_type_param(env, key_tparam);
                let source_new = self.on_t(env, source);
                let prop_new = self.on_t(env, prop);
                let name_type_new = name_type.map(|nt| self.on_t(env, nt));
                let flags_new = self.on_mapped_type_flags(env, flags);
                let homomorphic_new = self.on_mapped_type_homomorphic_flag(env, homomorphic);
                Prop::MappedTypeProp {
                    key_tparam: key_tparam_new,
                    source: source_new,
                    prop: prop_new,
                    name_type: name_type_new,
                    flags: flags_new,
                    homomorphic: homomorphic_new,
                }
            }
        }
    }

    fn on_named_prop(&mut self, env: &Env, np: NamedProp<L>) -> NamedProp<L> {
        match np {
            NamedProp::Field {
                t,
                polarity,
                optional,
            } => {
                let t_new = self.on_t(env, t);
                let polarity_new = self.on_polarity(env, polarity);
                NamedProp::Field {
                    t: t_new,
                    polarity: polarity_new,
                    optional,
                }
            }
            NamedProp::Method {
                fun: fun_t,
                optional,
            } => {
                let fun_t_new = self.on_fun_t(env, fun_t);
                NamedProp::Method {
                    fun: fun_t_new,
                    optional,
                }
            }
            NamedProp::Get(t) => {
                let t_new = self.on_t(env, t);
                NamedProp::Get(t_new)
            }
            NamedProp::Set(t) => {
                let t_new = self.on_t(env, t);
                NamedProp::Set(t_new)
            }
        }
    }

    fn on_prop_source(&mut self, _env: &Env, ps: PropSource) -> PropSource {
        ps
    }

    fn on_mapped_type_homomorphic_flag(
        &mut self,
        env: &Env,
        flag: MappedTypeHomomorphicFlag<L>,
    ) -> MappedTypeHomomorphicFlag<L> {
        match flag {
            MappedTypeHomomorphicFlag::Homomorphic => MappedTypeHomomorphicFlag::Homomorphic,
            MappedTypeHomomorphicFlag::Unspecialized => MappedTypeHomomorphicFlag::Unspecialized,
            MappedTypeHomomorphicFlag::SemiHomomorphic(t) => {
                let t_new = self.on_t(env, t);
                MappedTypeHomomorphicFlag::SemiHomomorphic(t_new)
            }
        }
    }

    fn on_mapped_type_optional_flag(
        &mut self,
        _env: &Env,
        flag: MappedTypeOptionalFlag,
    ) -> MappedTypeOptionalFlag {
        flag
    }

    fn on_mapped_type_flags(&mut self, env: &Env, flags: MappedTypeFlags) -> MappedTypeFlags {
        let optional_new = self.on_mapped_type_optional_flag(env, flags.optional);
        let variance_new = self.on_mapped_type_variance(env, flags.variance);
        MappedTypeFlags {
            optional: optional_new,
            variance: variance_new,
        }
    }

    fn on_mapped_type_variance(&mut self, _env: &Env, v: MappedTypeVariance) -> MappedTypeVariance {
        v
    }

    fn on_polarity(&mut self, _env: &Env, p: Polarity) -> Polarity {
        p
    }

    fn on_type_param(&mut self, env: &Env, tp: TypeParam<L>) -> TypeParam<L> {
        let tp_bound_new = self.on_option(|s, e, b| s.on_t(e, b), env, tp.tp_bound);
        let tp_default_new = self.on_option(|s, e, d| s.on_t(e, d), env, tp.tp_default);
        let tp_polarity_new = self.on_polarity(env, tp.tp_polarity);
        TypeParam {
            tp_name: tp.tp_name,
            tp_bound: tp_bound_new,
            tp_default: tp_default_new,
            tp_polarity: tp_polarity_new,
            tp_const: tp.tp_const,
        }
    }

    fn on_dict(&mut self, env: &Env, d: Dict<L>) -> Dict<L> {
        let dict_polarity_new = self.on_polarity(env, d.dict_polarity);
        let dict_key_new = self.on_t(env, d.dict_key);
        let dict_value_new = self.on_t(env, d.dict_value);
        Dict {
            dict_polarity: dict_polarity_new,
            dict_name: d.dict_name,
            dict_key: dict_key_new,
            dict_value: dict_value_new,
        }
    }

    fn on_tuple_element(&mut self, env: &Env, te: TupleElement<L>) -> TupleElement<L> {
        match te {
            TupleElement::TupleElement {
                name,
                t,
                polarity,
                optional,
            } => {
                let t_new = self.on_t(env, t);
                let polarity_new = self.on_polarity(env, polarity);
                TupleElement::TupleElement {
                    name,
                    t: t_new,
                    polarity: polarity_new,
                    optional,
                }
            }
            TupleElement::TupleSpread { name, t } => {
                let t_new = self.on_t(env, t);
                TupleElement::TupleSpread { name, t: t_new }
            }
        }
    }

    fn on_builtin_or_symbol(&mut self, env: &Env, bos: BuiltinOrSymbol<L>) -> BuiltinOrSymbol<L> {
        match bos {
            BuiltinOrSymbol::FunProto => BuiltinOrSymbol::FunProto,
            BuiltinOrSymbol::ObjProto => BuiltinOrSymbol::ObjProto,
            BuiltinOrSymbol::TSymbol(sym) => {
                let sym_new = self.on_symbol(env, sym);
                BuiltinOrSymbol::TSymbol(sym_new)
            }
        }
    }

    fn on_utility(&mut self, env: &Env, u: Utility<L>) -> Utility<L> {
        match u {
            Utility::Keys(t) => {
                let t_new = self.on_t(env, t);
                Utility::Keys(t_new)
            }
            Utility::Values(t) => {
                let t_new = self.on_t(env, t);
                Utility::Values(t_new)
            }
            Utility::ReadOnly(t) => {
                let t_new = self.on_t(env, t);
                Utility::ReadOnly(t_new)
            }
            Utility::Partial(t) => {
                let t_new = self.on_t(env, t);
                Utility::Partial(t_new)
            }
            Utility::Required(t) => {
                let t_new = self.on_t(env, t);
                Utility::Required(t_new)
            }
            Utility::Exact(t) => {
                let t_new = self.on_t(env, t);
                Utility::Exact(t_new)
            }
            Utility::Enum(t) => {
                let t_new = self.on_t(env, t);
                Utility::Enum(t_new)
            }
            Utility::NonMaybeType(t) => {
                let t_new = self.on_t(env, t);
                Utility::NonMaybeType(t_new)
            }
            Utility::ObjKeyMirror(t) => {
                let t_new = self.on_t(env, t);
                Utility::ObjKeyMirror(t_new)
            }
            Utility::Class(t) => {
                let t_new = self.on_t(env, t);
                Utility::Class(t_new)
            }
            Utility::ReactElementConfigType(t) => {
                let t_new = self.on_t(env, t);
                Utility::ReactElementConfigType(t_new)
            }
            Utility::Omit(t1, t2) => {
                let t1_new = self.on_t(env, t1);
                let t2_new = self.on_t(env, t2);
                Utility::Omit(t1_new, t2_new)
            }
            Utility::ElementType(t1, t2) => {
                let t1_new = self.on_t(env, t1);
                let t2_new = self.on_t(env, t2);
                Utility::ElementType(t1_new, t2_new)
            }
            Utility::Uppercase(t) => {
                let t_new = self.on_t(env, t);
                Utility::Uppercase(t_new)
            }
            Utility::Lowercase(t) => {
                let t_new = self.on_t(env, t);
                Utility::Lowercase(t_new)
            }
            Utility::Capitalize(t) => {
                let t_new = self.on_t(env, t);
                Utility::Capitalize(t_new)
            }
            Utility::Uncapitalize(t) => {
                let t_new = self.on_t(env, t);
                Utility::Uncapitalize(t_new)
            }
        }
    }

    fn on_component_props(&mut self, env: &Env, cp: ComponentProps<L>) -> ComponentProps<L> {
        match cp {
            ComponentProps::UnflattenedComponentProps(t) => {
                let t_new = self.on_t(env, t);
                ComponentProps::UnflattenedComponentProps(t_new)
            }
            ComponentProps::FlattenedComponentProps {
                props,
                inexact,
                dict,
            } => {
                let props_new: Arc<[_]> = self
                    .on_list(|s, e, p| s.on_flattened_component_prop(e, p), env, &props)
                    .into();
                ComponentProps::FlattenedComponentProps {
                    props: props_new,
                    inexact,
                    dict: dict.map(|d| self.on_dict(env, d)),
                }
            }
        }
    }

    fn on_flattened_component_prop(
        &mut self,
        env: &Env,
        fcp: FlattenedComponentProp<L>,
    ) -> FlattenedComponentProp<L> {
        match fcp {
            FlattenedComponentProp::FlattenedComponentProp {
                name,
                optional,
                def_locs,
                t,
            } => {
                let name_new = self.on_name(env, name);
                let t_new = self.on_t(env, t);
                FlattenedComponentProp::FlattenedComponentProp {
                    name: name_new,
                    optional,
                    def_locs,
                    t: t_new,
                }
            }
        }
    }

    fn on_renders_kind(&mut self, _env: &Env, rk: RendersKind) -> RendersKind {
        rk
    }
}

pub trait TyMapReduceTy<L, Env>: TyMapReduceBase<Env, L>
where
    L: Dupe,
{
    fn on_t(&mut self, env: &Env, t: Ty<L>) -> (Ty<L>, Self::Acc);
    fn on_decl(&mut self, env: &Env, d: Decl<L>) -> (Decl<L>, Self::Acc);
    fn on_elt(&mut self, env: &Env, e: Elt<L>) -> (Elt<L>, Self::Acc);
}

use std::marker::PhantomData;

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    serde::Serialize,
    serde::Deserialize
)]
pub struct Difference(pub i32);

/* Compare Ty.t for structural equality
This class can be overridden to define new forms of equality on types */
pub struct ComparatorTy<Env> {
    _phantom: PhantomData<Env>,
}

impl<Env> ComparatorTy<Env> {
    pub fn new() -> Self {
        ComparatorTy {
            _phantom: PhantomData,
        }
    }

    pub fn compare<L>(&mut self, env: &Env, t1: &Ty<L>, t2: &Ty<L>) -> i32
    where
        L: Dupe + PartialEq,
        Self: TyIter2Ty<L, Env>,
    {
        match self.on_t(env, t1, t2) {
            Ok(()) => 0,
            Err(StructuralMismatch::Difference(n)) => n,
            Err(StructuralMismatch::Mismatch) => panic!("unexpected StructuralMismatch::Mismatch"),
        }
    }

    pub fn compare_elt<L>(&mut self, env: &Env, e1: &Elt<L>, e2: &Elt<L>) -> i32
    where
        L: Dupe + PartialEq,
        Self: TyIter2Ty<L, Env>,
    {
        match self.on_elt(env, e1, e2) {
            Ok(()) => 0,
            Err(StructuralMismatch::Difference(n)) => n,
            Err(StructuralMismatch::Mismatch) => panic!("unexpected StructuralMismatch::Mismatch"),
        }
    }

    pub fn assert0(n: i32) -> Result<(), StructuralMismatch> {
        if n == 0 {
            Ok(())
        } else {
            Err(StructuralMismatch::Difference(n))
        }
    }

    /* The prototype of what should happen when overriding fail_* methods */
    pub fn fail_gen<T, F>(tag_of: F, x: &T, y: &T) -> Result<(), StructuralMismatch>
    where
        F: Fn(&T) -> i32,
    {
        Self::assert0(tag_of(x) - tag_of(y))
    }
}

impl<Env> Default for ComparatorTy<Env> {
    fn default() -> Self {
        Self::new()
    }
}

/* types will show up in unions and intersections in ascending order */
/* No two elements of each variant can be assigned the same tag */
pub fn tag_of_t<L>(_t: &Ty<L>) -> i32 {
    match _t {
        /* Roughly in order of increasing complexity */
        /* Favor litererals over base types */
        /* Favor user defined types over structural types */
        Ty::Bot(_) => 0,
        Ty::Top => 1,
        Ty::Any(_) => 2,
        Ty::Void => 3,
        Ty::Undefined => 4,
        Ty::TsVoid => 5,
        Ty::Null => 6,
        Ty::BoolLit(_) => 7,
        Ty::Bool => 8,
        Ty::NumLit(_) => 9,
        Ty::Num => 10,
        Ty::BigIntLit(_) => 11,
        Ty::BigInt => 12,
        Ty::StrLit(_) => 13,
        Ty::Str => 14,
        Ty::UniqueSymbol => 15,
        Ty::Symbol => 16,
        Ty::Bound(_) => 17,
        Ty::Generic(_) => 18,
        Ty::TypeOf(_) => 19,
        Ty::Utility(_) => 20,
        Ty::IndexedAccess { .. } => 21,
        Ty::Tup { .. } => 22,
        Ty::Arr(_) => 23,
        Ty::Fun(_) => 24,
        Ty::Obj(_) => 25,
        Ty::Inter(_, _, _) => 26,
        Ty::Union(_, _, _, _) => 27,
        Ty::InlineInterface(_) => 28,
        Ty::Conditional { .. } => 29,
        Ty::Infer { .. } => 30,
        Ty::Component { .. } => 31,
        Ty::Renders { .. } => 32,
        Ty::TemplateLiteral { .. } => 33,
        Ty::Truncated => 34,
    }
}

pub fn tag_of_decl<L>(_d: &Decl<L>) -> i32 {
    match _d {
        Decl::VariableDecl(..) => 0,
        Decl::TypeAliasDecl(..) => 1,
        Decl::ClassDecl(..) => 2,
        Decl::InterfaceDecl(..) => 3,
        Decl::RecordDecl(..) => 4,
        Decl::EnumDecl(..) => 5,
        Decl::NominalComponentDecl(..) => 6,
        Decl::NamespaceDecl(..) => 7,
        Decl::ModuleDecl(..) => 8,
    }
}

pub fn tag_of_elt<L>(_e: &Elt<L>) -> i32 {
    match _e {
        Elt::Type(_) => 0,
        Elt::Decl(_) => 1,
    }
}

pub fn tag_of_gen_kind(_g: &GenKind) -> i32 {
    match _g {
        GenKind::ClassKind => 0,
        GenKind::InterfaceKind => 1,
        GenKind::TypeAliasKind => 2,
        GenKind::EnumKind => 3,
        GenKind::ComponentKind => 4,
        GenKind::RecordKind => 5,
    }
}

pub fn tag_of_fun_effect(_fe: &FunEffect) -> i32 {
    match _fe {
        FunEffect::Hook => 0,
        FunEffect::Arbitrary => 1,
    }
}

pub fn tag_of_obj_kind<L>(_o: &ObjKind<L>) -> i32 {
    match _o {
        ObjKind::ExactObj => 0,
        ObjKind::InexactObj => 1,
        ObjKind::IndexedObj(_) => 2,
        ObjKind::MappedTypeObj => 3,
    }
}

pub fn tag_of_tuple_element<L>(_te: &TupleElement<L>) -> i32 {
    match _te {
        TupleElement::TupleElement { .. } => 0,
        TupleElement::TupleSpread { .. } => 1,
    }
}

pub fn tag_of_component_props<L>(_cp: &ComponentProps<L>) -> i32 {
    match _cp {
        ComponentProps::UnflattenedComponentProps(_) => 0,
        ComponentProps::FlattenedComponentProps { .. } => 1,
    }
}

pub fn tag_of_renders_kind(_rk: &RendersKind) -> i32 {
    match _rk {
        RendersKind::RendersNormal => 0,
        RendersKind::RendersMaybe => 1,
        RendersKind::RendersStar => 2,
    }
}

pub fn tag_of_any_kind<L>(_ak: &AnyKind<L>) -> i32 {
    match _ak {
        AnyKind::Annotated(_) => 0,
        AnyKind::AnyError(_) => 1,
        AnyKind::Recursive => 2,
        AnyKind::Unsound(_) => 3,
        AnyKind::Untyped => 4,
        AnyKind::Placeholder => 5,
    }
}

pub fn tag_of_unsoundness_kind(_uk: &UnsoundnessKind) -> i32 {
    match _uk {
        UnsoundnessKind::BoundFunctionThis => 0,
        UnsoundnessKind::Constructor => 2,
        UnsoundnessKind::DummyStatic => 3,
        UnsoundnessKind::Exports => 5,
        UnsoundnessKind::InferenceHooks => 7,
        UnsoundnessKind::InstanceOfRefinement => 8,
        UnsoundnessKind::Merged => 9,
        UnsoundnessKind::ResolveSpread => 10,
        UnsoundnessKind::Unchecked => 11,
        UnsoundnessKind::Unimplemented => 12,
        UnsoundnessKind::UnresolvedType => 13,
        UnsoundnessKind::NonBindingPattern => 14,
    }
}

pub fn tag_of_prop<L>(_p: &Prop<L>) -> i32 {
    match _p {
        Prop::NamedProp { .. } => 0,
        Prop::CallProp { .. } => 1,
        Prop::SpreadProp { .. } => 2,
        Prop::MappedTypeProp { .. } => 3,
    }
}

pub fn tag_of_named_prop<L>(_np: &NamedProp<L>) -> i32 {
    match _np {
        NamedProp::Field { .. } => 0,
        NamedProp::Method { .. } => 1,
        NamedProp::Get { .. } => 2,
        NamedProp::Set { .. } => 3,
    }
}

pub fn tag_of_prop_source(_ps: &PropSource) -> i32 {
    match _ps {
        PropSource::Interface => 0,
        PropSource::PrimitiveProto(_) => 1,
        PropSource::Other => 2,
    }
}

pub fn tag_of_utility<L>(_u: &Utility<L>) -> i32 {
    match _u {
        Utility::Keys(_) => 0,
        Utility::Values(_) => 1,
        Utility::ReadOnly(_) => 2,
        Utility::Exact(_) => 3,
        Utility::ElementType(_, _) => 7,
        Utility::NonMaybeType(_) => 8,
        Utility::Class(_) => 13,
        Utility::ReactElementConfigType(_) => 19,
        Utility::ObjKeyMirror(_) => 22,
        Utility::Partial(_) => 23,
        Utility::Required(_) => 24,
        Utility::Enum(_) => 27,
        Utility::Omit(_, _) => 30,
        Utility::Uppercase(_) => 31,
        Utility::Lowercase(_) => 32,
        Utility::Capitalize(_) => 33,
        Utility::Uncapitalize(_) => 34,
    }
}

pub fn tag_of_polarity(_p: &Polarity) -> i32 {
    match _p {
        Polarity::Positive => 0,
        Polarity::Negative => 1,
        Polarity::Neutral => 2,
    }
}

pub fn tag_of_bot_kind<L>(_bk: &BotKind<L>) -> i32 {
    match _bk {
        BotKind::EmptyType => 0,
        BotKind::NoLowerWithUpper(_) => 1,
    }
}

pub fn tag_of_upper_bound_kind<L>(_ubk: &UpperBoundKind<L>) -> i32 {
    match _ubk {
        UpperBoundKind::NoUpper => 0,
        UpperBoundKind::SomeKnownUpper(_) => 1,
        UpperBoundKind::SomeUnknownUpper(_) => 2,
    }
}

pub fn tag_of_builtin_or_symbol<L>(_bos: &BuiltinOrSymbol<L>) -> i32 {
    match _bos {
        BuiltinOrSymbol::FunProto => 0,
        BuiltinOrSymbol::ObjProto => 1,
        BuiltinOrSymbol::TSymbol(_) => 3,
    }
}

/// The keyword a type guard is printed with, including its trailing space.
pub fn type_guard_kind_prefix(kind: TypeGuardKind) -> &'static str {
    match kind {
        TypeGuardKind::Default => "",
        TypeGuardKind::Implies => "implies ",
        TypeGuardKind::Asserts => "asserts ",
    }
}

pub fn tag_of_return_t<L>(_rt: &ReturnT<L>) -> i32 {
    match _rt {
        ReturnT::ReturnType(_) => 0,
        ReturnT::TypeGuard { .. } => 1,
    }
}

pub fn tag_of_mapped_type_homomorphic_flag<L>(_mthf: &MappedTypeHomomorphicFlag<L>) -> i32 {
    match _mthf {
        MappedTypeHomomorphicFlag::Homomorphic => 0,
        MappedTypeHomomorphicFlag::SemiHomomorphic(_) => 1,
        MappedTypeHomomorphicFlag::Unspecialized => 2,
    }
}

pub fn tag_of_any_error_kind(_aek: &AnyErrorKind) -> i32 {
    match _aek {
        AnyErrorKind::UnresolvedName => 0,
        AnyErrorKind::MissingAnnotation => 1,
    }
}

pub fn tag_of_mapped_type_optional_flag(_mtof: &MappedTypeOptionalFlag) -> i32 {
    match _mtof {
        MappedTypeOptionalFlag::RemoveOptional => 0,
        MappedTypeOptionalFlag::KeepOptionality => 1,
        MappedTypeOptionalFlag::MakeOptional => 2,
    }
}

pub fn tag_of_mapped_type_variance(mtv: &MappedTypeVariance) -> i32 {
    match mtv {
        MappedTypeVariance::OverrideVariance(_) => 0,
        MappedTypeVariance::RemoveVariance(_) => 1,
        MappedTypeVariance::KeepVariance => 2,
    }
}

impl<Env> TyIter2Base<Env, ALoc> for ComparatorTy<Env> {}

impl<Env> TyIter2Ty<ALoc, Env> for ComparatorTy<Env> {
    fn on_any_kind(
        &mut self,
        env: &Env,
        ak1: &AnyKind<ALoc>,
        ak2: &AnyKind<ALoc>,
    ) -> Result<(), StructuralMismatch> {
        match (ak1, ak2) {
            (AnyKind::Annotated(l1), AnyKind::Annotated(l2)) => {
                use std::cmp::Ordering;
                match ALoc::compare(l1, l2) {
                    Ordering::Equal => Ok(()),
                    Ordering::Less => Err(StructuralMismatch::Difference(-1)),
                    Ordering::Greater => Err(StructuralMismatch::Difference(1)),
                }
            }
            (AnyKind::AnyError(e1), AnyKind::AnyError(e2)) => self.on_option(
                |s, e, ek1, ek2| {
                    if ek1 != ek2 {
                        s.fail_any_error_kind(e, ek1, ek2)
                    } else {
                        Ok(())
                    }
                },
                env,
                e1,
                e2,
            ),
            (AnyKind::Recursive, AnyKind::Recursive) => Ok(()),
            (AnyKind::Unsound(u1), AnyKind::Unsound(u2)) => {
                if u1 != u2 {
                    self.fail_unsoundness_kind(env, u1, u2)
                } else {
                    Ok(())
                }
            }
            (AnyKind::Untyped, AnyKind::Untyped) => Ok(()),
            (AnyKind::Placeholder, AnyKind::Placeholder) => Ok(()),
            _ => self.fail_any_kind(env, ak1, ak2),
        }
    }

    fn on_bound_locs(&mut self, env: &Env, l1: &ALoc, l2: &ALoc) -> Result<(), StructuralMismatch> {
        self.on_aloc(env, l1, l2)
    }
}

impl<L: Dupe> AnyKind<L> {
    pub fn map_locs<F, M>(&self, f: &F) -> AnyKind<M>
    where
        F: Fn(&L) -> M,
        M: Dupe,
    {
        match self {
            AnyKind::Annotated(loc) => AnyKind::Annotated(f(loc)),
            AnyKind::AnyError(kind) => AnyKind::AnyError(*kind),
            AnyKind::Recursive => AnyKind::Recursive,
            AnyKind::Unsound(kind) => AnyKind::Unsound(*kind),
            AnyKind::Untyped => AnyKind::Untyped,
            AnyKind::Placeholder => AnyKind::Placeholder,
        }
    }
}

impl<L: Dupe> UpperBoundKind<L> {
    pub fn map_locs<F, M>(&self, f: &F) -> UpperBoundKind<M>
    where
        F: Fn(&L) -> M,
        M: Dupe,
    {
        match self {
            UpperBoundKind::NoUpper => UpperBoundKind::NoUpper,
            UpperBoundKind::SomeKnownUpper(arc_ty) => {
                UpperBoundKind::SomeKnownUpper(Arc::new(arc_ty.as_ref().map_locs(f)))
            }
            UpperBoundKind::SomeUnknownUpper(s) => UpperBoundKind::SomeUnknownUpper(s.clone()),
        }
    }
}

impl<L: Dupe> BotKind<L> {
    pub fn map_locs<F, M>(&self, f: &F) -> BotKind<M>
    where
        F: Fn(&L) -> M,
        M: Dupe,
    {
        match self {
            BotKind::EmptyType => BotKind::EmptyType,
            BotKind::NoLowerWithUpper(upper) => BotKind::NoLowerWithUpper(upper.map_locs(f)),
        }
    }
}

impl<L: Dupe> BuiltinOrSymbol<L> {
    pub fn map_locs<F, M>(&self, f: &F) -> BuiltinOrSymbol<M>
    where
        F: Fn(&L) -> M,
        M: Dupe,
    {
        match self {
            BuiltinOrSymbol::FunProto => BuiltinOrSymbol::FunProto,
            BuiltinOrSymbol::ObjProto => BuiltinOrSymbol::ObjProto,
            BuiltinOrSymbol::TSymbol(sym) => BuiltinOrSymbol::TSymbol(sym.map_locs(f)),
        }
    }
}

impl<L: Dupe> ReturnT<L> {
    pub fn map_locs<F, M>(&self, f: &F) -> ReturnT<M>
    where
        F: Fn(&L) -> M,
        M: Dupe,
    {
        match self {
            ReturnT::ReturnType(arc_ty) => {
                ReturnT::ReturnType(Arc::new(arc_ty.as_ref().map_locs(f)))
            }
            ReturnT::TypeGuard(kind, name, arc_ty) => ReturnT::TypeGuard(
                *kind,
                name.clone(),
                arc_ty
                    .as_ref()
                    .map(|arc_ty| Arc::new(arc_ty.as_ref().map_locs(f))),
            ),
        }
    }
}

impl<L: Dupe> Dict<L> {
    pub fn map_locs<F, M>(&self, f: &F) -> Dict<M>
    where
        F: Fn(&L) -> M,
        M: Dupe,
    {
        Dict {
            dict_polarity: self.dict_polarity,
            dict_name: self.dict_name.clone(),
            dict_key: Arc::new(self.dict_key.as_ref().map_locs(f)),
            dict_value: Arc::new(self.dict_value.as_ref().map_locs(f)),
        }
    }
}

impl<L: Dupe> TypeParam<L> {
    pub fn map_locs<F, M>(&self, f: &F) -> TypeParam<M>
    where
        F: Fn(&L) -> M,
        M: Dupe,
    {
        TypeParam {
            tp_name: self.tp_name.clone(),
            tp_bound: self
                .tp_bound
                .as_ref()
                .map(|arc_ty| Arc::new(arc_ty.as_ref().map_locs(f))),
            tp_polarity: self.tp_polarity,
            tp_default: self
                .tp_default
                .as_ref()
                .map(|arc_ty| Arc::new(arc_ty.as_ref().map_locs(f))),
            tp_const: self.tp_const,
        }
    }
}

impl<L: Dupe> MappedTypeHomomorphicFlag<L> {
    pub fn map_locs<F, M>(&self, f: &F) -> MappedTypeHomomorphicFlag<M>
    where
        F: Fn(&L) -> M,
        M: Dupe,
    {
        match self {
            MappedTypeHomomorphicFlag::Homomorphic => MappedTypeHomomorphicFlag::Homomorphic,
            MappedTypeHomomorphicFlag::SemiHomomorphic(arc_ty) => {
                MappedTypeHomomorphicFlag::SemiHomomorphic(Arc::new(arc_ty.as_ref().map_locs(f)))
            }
            MappedTypeHomomorphicFlag::Unspecialized => MappedTypeHomomorphicFlag::Unspecialized,
        }
    }
}

impl<L: Dupe> NamedProp<L> {
    pub fn map_locs<F, M>(&self, f: &F) -> NamedProp<M>
    where
        F: Fn(&L) -> M,
        M: Dupe,
    {
        match self {
            NamedProp::Field {
                t,
                polarity,
                optional,
            } => NamedProp::Field {
                t: Arc::new(t.as_ref().map_locs(f)),
                polarity: *polarity,
                optional: *optional,
            },
            NamedProp::Method { fun, optional } => NamedProp::Method {
                fun: fun.map_locs(f),
                optional: *optional,
            },
            NamedProp::Get(arc_ty) => NamedProp::Get(Arc::new(arc_ty.as_ref().map_locs(f))),
            NamedProp::Set(arc_ty) => NamedProp::Set(Arc::new(arc_ty.as_ref().map_locs(f))),
        }
    }
}

impl<L: Dupe> Prop<L> {
    pub fn map_locs<F, M>(&self, f: &F) -> Prop<M>
    where
        F: Fn(&L) -> M,
        M: Dupe,
    {
        match self {
            Prop::NamedProp {
                name,
                prop,
                inherited,
                source,
                def_locs,
            } => Prop::NamedProp {
                name: name.dupe(),
                prop: prop.map_locs(f),
                inherited: *inherited,
                source: source.clone(),
                def_locs: def_locs.iter().map(f).collect::<Vec<_>>().into(),
            },
            Prop::CallProp(fun) => Prop::CallProp(fun.map_locs(f)),
            Prop::SpreadProp(arc_ty) => Prop::SpreadProp(Arc::new(arc_ty.as_ref().map_locs(f))),
            Prop::MappedTypeProp {
                key_tparam,
                source,
                prop,
                name_type,
                flags,
                homomorphic,
            } => Prop::MappedTypeProp {
                key_tparam: key_tparam.map_locs(f),
                source: Arc::new(source.as_ref().map_locs(f)),
                prop: Arc::new(prop.as_ref().map_locs(f)),
                name_type: name_type
                    .as_ref()
                    .map(|nt| Arc::new(nt.as_ref().map_locs(f))),
                flags: *flags,
                homomorphic: homomorphic.map_locs(f),
            },
        }
    }
}

impl<L: Dupe> FunT<L> {
    pub fn map_locs<F, M>(&self, f: &F) -> FunT<M>
    where
        F: Fn(&L) -> M,
        M: Dupe,
    {
        FunT {
            fun_this_param: self
                .fun_this_param
                .as_ref()
                .map(|arc_ty| Arc::new(arc_ty.as_ref().map_locs(f))),
            fun_params: self
                .fun_params
                .iter()
                .map(|(name, arc_ty, param)| {
                    (name.clone(), Arc::new(arc_ty.as_ref().map_locs(f)), *param)
                })
                .collect::<Vec<_>>()
                .into(),
            fun_rest_param: self
                .fun_rest_param
                .as_ref()
                .map(|(name, arc_ty)| (name.clone(), Arc::new(arc_ty.as_ref().map_locs(f)))),
            fun_return: self.fun_return.map_locs(f),
            fun_type_params: self.fun_type_params.as_ref().map(|tparams| -> Arc<[_]> {
                tparams
                    .iter()
                    .map(|tp| tp.map_locs(f))
                    .collect::<Vec<_>>()
                    .into()
            }),
            fun_static: Arc::new(self.fun_static.as_ref().map_locs(f)),
            fun_effect: self.fun_effect,
        }
    }
}

impl<L: Dupe> ObjKind<L> {
    pub fn map_locs<F, M>(&self, f: &F) -> ObjKind<M>
    where
        F: Fn(&L) -> M,
        M: Dupe,
    {
        match self {
            ObjKind::ExactObj => ObjKind::ExactObj,
            ObjKind::InexactObj => ObjKind::InexactObj,
            ObjKind::IndexedObj(dict) => ObjKind::IndexedObj(dict.map_locs(f)),
            ObjKind::MappedTypeObj => ObjKind::MappedTypeObj,
        }
    }
}

impl<L: Dupe> ObjT<L> {
    pub fn map_locs<F, M>(&self, f: &F) -> ObjT<M>
    where
        F: Fn(&L) -> M,
        M: Dupe,
    {
        ObjT {
            obj_def_loc: self.obj_def_loc.as_ref().map(f),
            obj_props: self
                .obj_props
                .iter()
                .map(|p| p.map_locs(f))
                .collect::<Vec<_>>()
                .into(),
            obj_kind: self.obj_kind.map_locs(f),
        }
    }
}

impl<L: Dupe> ArrT<L> {
    pub fn map_locs<F, M>(&self, f: &F) -> ArrT<M>
    where
        F: Fn(&L) -> M,
        M: Dupe,
    {
        ArrT {
            arr_readonly: self.arr_readonly,
            arr_elt_t: Arc::new(self.arr_elt_t.as_ref().map_locs(f)),
        }
    }
}

impl<L: Dupe> TupleElement<L> {
    pub fn map_locs<F, M>(&self, f: &F) -> TupleElement<M>
    where
        F: Fn(&L) -> M,
        M: Dupe,
    {
        match self {
            TupleElement::TupleElement {
                name,
                t,
                polarity,
                optional,
            } => TupleElement::TupleElement {
                name: name.clone(),
                t: Arc::new(t.as_ref().map_locs(f)),
                polarity: *polarity,
                optional: *optional,
            },
            TupleElement::TupleSpread { name, t } => TupleElement::TupleSpread {
                name: name.clone(),
                t: Arc::new(t.as_ref().map_locs(f)),
            },
        }
    }
}

impl<L: Dupe> InterfaceT<L> {
    pub fn map_locs<F, M>(&self, f: &F) -> InterfaceT<M>
    where
        F: Fn(&L) -> M,
        M: Dupe,
    {
        InterfaceT {
            if_extends: self
                .if_extends
                .iter()
                .map(|(sym, kind, targs)| {
                    (
                        sym.map_locs(f),
                        *kind,
                        targs.as_ref().map(|args| -> Arc<[_]> {
                            args.iter()
                                .map(|arc_ty| Arc::new(arc_ty.as_ref().map_locs(f)))
                                .collect::<Vec<_>>()
                                .into()
                        }),
                    )
                })
                .collect::<Vec<_>>()
                .into(),
            if_props: self
                .if_props
                .iter()
                .map(|p| p.map_locs(f))
                .collect::<Vec<_>>()
                .into(),
            if_dict: self.if_dict.as_ref().map(|d| d.map_locs(f)),
        }
    }
}

impl<L: Dupe> Utility<L> {
    pub fn map_locs<F, M>(&self, f: &F) -> Utility<M>
    where
        F: Fn(&L) -> M,
        M: Dupe,
    {
        match self {
            Utility::Keys(arc_ty) => Utility::Keys(Arc::new(arc_ty.as_ref().map_locs(f))),
            Utility::Values(arc_ty) => Utility::Values(Arc::new(arc_ty.as_ref().map_locs(f))),
            Utility::ReadOnly(arc_ty) => Utility::ReadOnly(Arc::new(arc_ty.as_ref().map_locs(f))),
            Utility::Partial(arc_ty) => Utility::Partial(Arc::new(arc_ty.as_ref().map_locs(f))),
            Utility::Required(arc_ty) => Utility::Required(Arc::new(arc_ty.as_ref().map_locs(f))),
            Utility::Exact(arc_ty) => Utility::Exact(Arc::new(arc_ty.as_ref().map_locs(f))),
            Utility::Omit(ty1, ty2) => Utility::Omit(
                Arc::new(ty1.as_ref().map_locs(f)),
                Arc::new(ty2.as_ref().map_locs(f)),
            ),
            Utility::ElementType(ty1, ty2) => Utility::ElementType(
                Arc::new(ty1.as_ref().map_locs(f)),
                Arc::new(ty2.as_ref().map_locs(f)),
            ),
            Utility::Enum(arc_ty) => Utility::Enum(Arc::new(arc_ty.as_ref().map_locs(f))),
            Utility::NonMaybeType(arc_ty) => {
                Utility::NonMaybeType(Arc::new(arc_ty.as_ref().map_locs(f)))
            }
            Utility::ObjKeyMirror(arc_ty) => {
                Utility::ObjKeyMirror(Arc::new(arc_ty.as_ref().map_locs(f)))
            }
            Utility::Class(arc_ty) => Utility::Class(Arc::new(arc_ty.as_ref().map_locs(f))),
            Utility::Uppercase(arc_ty) => Utility::Uppercase(Arc::new(arc_ty.as_ref().map_locs(f))),
            Utility::Lowercase(arc_ty) => Utility::Lowercase(Arc::new(arc_ty.as_ref().map_locs(f))),
            Utility::Capitalize(arc_ty) => {
                Utility::Capitalize(Arc::new(arc_ty.as_ref().map_locs(f)))
            }
            Utility::Uncapitalize(arc_ty) => {
                Utility::Uncapitalize(Arc::new(arc_ty.as_ref().map_locs(f)))
            }
            Utility::ReactElementConfigType(arc_ty) => {
                Utility::ReactElementConfigType(Arc::new(arc_ty.as_ref().map_locs(f)))
            }
        }
    }
}

impl<L: Dupe> FlattenedComponentProp<L> {
    pub fn map_locs<F, M>(&self, f: &F) -> FlattenedComponentProp<M>
    where
        F: Fn(&L) -> M,
        M: Dupe,
    {
        match self {
            FlattenedComponentProp::FlattenedComponentProp {
                name,
                optional,
                def_locs,
                t,
            } => FlattenedComponentProp::FlattenedComponentProp {
                name: name.dupe(),
                optional: *optional,
                def_locs: def_locs.iter().map(f).collect::<Vec<_>>().into(),
                t: Arc::new(t.as_ref().map_locs(f)),
            },
        }
    }
}

impl<L: Dupe> ComponentProps<L> {
    pub fn map_locs<F, M>(&self, f: &F) -> ComponentProps<M>
    where
        F: Fn(&L) -> M,
        M: Dupe,
    {
        match self {
            ComponentProps::UnflattenedComponentProps(arc_ty) => {
                ComponentProps::UnflattenedComponentProps(Arc::new(arc_ty.as_ref().map_locs(f)))
            }
            ComponentProps::FlattenedComponentProps {
                props,
                inexact,
                dict,
            } => ComponentProps::FlattenedComponentProps {
                props: props
                    .iter()
                    .map(|p| p.map_locs(f))
                    .collect::<Vec<_>>()
                    .into(),
                inexact: *inexact,
                dict: dict.as_ref().map(|d| d.map_locs(f)),
            },
        }
    }
}

// Full implementation of Ty::map_locs for location mapping
impl<L: Dupe> Ty<L> {
    pub fn map_locs<F, M>(&self, f: &F) -> Ty<M>
    where
        F: Fn(&L) -> M,
        M: Dupe,
    {
        match self {
            Ty::Truncated => Ty::Truncated,
            Ty::Bound(data) => {
                let (loc, name) = data.as_ref();
                Ty::Bound(Box::new((f(loc), name.clone())))
            }
            Ty::Generic(g) => {
                let (sym, kind, targs) = g.as_ref();
                Ty::Generic(Box::new((
                    sym.map_locs(f),
                    *kind,
                    targs.as_ref().map(|args| {
                        args.iter()
                            .map(|arc_ty| Arc::new(arc_ty.as_ref().map_locs(f)))
                            .collect::<Vec<_>>()
                            .into()
                    }),
                )))
            }
            Ty::Any(any_kind) => Ty::Any(any_kind.map_locs(f)),
            Ty::Top => Ty::Top,
            Ty::Bot(bot_kind) => Ty::Bot(bot_kind.map_locs(f)),
            Ty::Void => Ty::Void,
            Ty::Undefined => Ty::Undefined,
            Ty::TsVoid => Ty::TsVoid,
            Ty::Null => Ty::Null,
            Ty::Symbol => Ty::Symbol,
            Ty::UniqueSymbol => Ty::UniqueSymbol,
            Ty::Num => Ty::Num,
            Ty::Str => Ty::Str,
            Ty::Bool => Ty::Bool,
            Ty::BigInt => Ty::BigInt,
            Ty::NumLit(s) => Ty::NumLit(s.clone()),
            Ty::StrLit(s) => Ty::StrLit(s.dupe()),
            Ty::BoolLit(b) => Ty::BoolLit(*b),
            Ty::BigIntLit(s) => Ty::BigIntLit(s.clone()),
            Ty::Fun(fun) => Ty::Fun(Box::new(fun.map_locs(f))),
            Ty::Obj(obj) => Ty::Obj(Box::new(obj.map_locs(f))),
            Ty::Arr(arr) => Ty::Arr(arr.map_locs(f)),
            Ty::Tup { elements, inexact } => Ty::Tup {
                elements: elements
                    .iter()
                    .map(|e| e.map_locs(f))
                    .collect::<Vec<_>>()
                    .into(),
                inexact: *inexact,
            },
            Ty::Union(from_annotation, ty1, ty2, tys) => Ty::Union(
                *from_annotation,
                Arc::new(ty1.as_ref().map_locs(f)),
                Arc::new(ty2.as_ref().map_locs(f)),
                tys.iter()
                    .map(|arc_ty| Arc::new(arc_ty.as_ref().map_locs(f)))
                    .collect::<Vec<_>>()
                    .into(),
            ),
            Ty::Inter(ty1, ty2, tys) => Ty::Inter(
                Arc::new(ty1.as_ref().map_locs(f)),
                Arc::new(ty2.as_ref().map_locs(f)),
                tys.iter()
                    .map(|arc_ty| Arc::new(arc_ty.as_ref().map_locs(f)))
                    .collect::<Vec<_>>()
                    .into(),
            ),
            Ty::InlineInterface(iface) => Ty::InlineInterface(Box::new(iface.map_locs(f))),
            Ty::TypeOf(data) => {
                let (builtin_or_symbol, targs) = data.as_ref();
                Ty::TypeOf(Box::new((
                    builtin_or_symbol.map_locs(f),
                    targs.as_ref().map(|args| {
                        args.iter()
                            .map(|arc_ty| Arc::new(arc_ty.as_ref().map_locs(f)))
                            .collect::<Vec<_>>()
                            .into()
                    }),
                )))
            }
            Ty::Utility(utility) => Ty::Utility(utility.map_locs(f)),
            Ty::IndexedAccess {
                _object,
                index,
                optional,
            } => Ty::IndexedAccess {
                _object: Arc::new(_object.as_ref().map_locs(f)),
                index: Arc::new(index.as_ref().map_locs(f)),
                optional: *optional,
            },
            Ty::Conditional {
                check_type,
                extends_type,
                true_type,
                false_type,
            } => Ty::Conditional {
                check_type: Arc::new(check_type.as_ref().map_locs(f)),
                extends_type: Arc::new(extends_type.as_ref().map_locs(f)),
                true_type: Arc::new(true_type.as_ref().map_locs(f)),
                false_type: Arc::new(false_type.as_ref().map_locs(f)),
            },
            Ty::Infer(data) => {
                let (sym, bound) = data.as_ref();
                Ty::Infer(Box::new((
                    sym.map_locs(f),
                    bound
                        .as_ref()
                        .map(|arc_ty| Arc::new(arc_ty.as_ref().map_locs(f))),
                )))
            }
            Ty::Component {
                regular_props,
                renders,
            } => Ty::Component {
                regular_props: regular_props.map_locs(f),
                renders: renders
                    .as_ref()
                    .map(|arc_ty| Arc::new(arc_ty.as_ref().map_locs(f))),
            },
            Ty::Renders(arc_ty, kind) => Ty::Renders(Arc::new(arc_ty.as_ref().map_locs(f)), *kind),
            Ty::TemplateLiteral { quasis, types } => Ty::TemplateLiteral {
                quasis: quasis.clone(),
                types: types
                    .iter()
                    .map(|arc_ty| Arc::new(arc_ty.as_ref().map_locs(f)))
                    .collect(),
            },
        }
    }
}

impl Monoid for BTreeSet<Symbol<Loc>> {
    fn zero() -> Self {
        BTreeSet::new()
    }

    fn plus(mut a: Self, b: Self) -> Self {
        a.extend(b);
        a
    }
}

impl<F> TyReduceTy<ALoc, ()> for SymbolCollector<F> where F: Fn(&ALoc) -> Loc {}

struct SymbolCollector<F> {
    loc_of_aloc: F,
}

impl<F> TyReduceBase<(), ALoc> for SymbolCollector<F>
where
    F: Fn(&ALoc) -> Loc,
{
    type Acc = BTreeSet<Symbol<Loc>>;

    fn on_symbol(&mut self, _env: &(), sym: &Symbol<ALoc>) -> Self::Acc {
        let mapped_sym = sym.map_locs(&self.loc_of_aloc);
        let mut set = BTreeSet::new();
        set.insert(mapped_sym);
        set
    }
}

/// Returns the symbol printed as the declaration's head, if it has one.
pub fn head_symbol_of_decl(d: &Decl<ALoc>) -> Option<&Symbol<ALoc>> {
    match d {
        Decl::VariableDecl(..) => None,
        Decl::TypeAliasDecl(box DeclTypeAliasDeclData { name, .. }) => Some(name),
        Decl::ClassDecl(box (sym, _))
        | Decl::InterfaceDecl(box (sym, _))
        | Decl::RecordDecl(box (sym, _)) => Some(sym),
        Decl::EnumDecl(box DeclEnumDeclData { name, .. }) => Some(name),
        Decl::NominalComponentDecl(box DeclNominalComponentDeclData { name, .. }) => Some(name),
        Decl::NamespaceDecl(box DeclNamespaceDeclData { name, .. })
        | Decl::ModuleDecl(box DeclModuleDeclData { name, .. }) => name.as_ref(),
    }
}

pub fn symbols_of_elt<F>(loc_of_aloc: F, elt: &Elt<ALoc>) -> BTreeSet<Symbol<Loc>>
where
    F: Fn(&ALoc) -> Loc,
{
    let mut collector = SymbolCollector { loc_of_aloc };

    match elt {
        Elt::Type(t) => collector.on_t(&(), t),
        Elt::Decl(d) => match d {
            Decl::VariableDecl(..)
            | Decl::TypeAliasDecl(..)
            | Decl::ClassDecl(..)
            | Decl::InterfaceDecl(..)
            | Decl::RecordDecl(..)
            | Decl::EnumDecl(..)
            | Decl::NominalComponentDecl(..) => collector.on_decl(&(), d),
            Decl::NamespaceDecl(box DeclNamespaceDeclData { name, exports: _ })
            | Decl::ModuleDecl(box DeclModuleDeclData {
                name,
                exports: _,
                default: _,
            }) => match name {
                Some(sym) => {
                    let mapped_sym = sym.map_locs(&collector.loc_of_aloc);
                    let mut set = BTreeSet::new();
                    set.insert(mapped_sym);
                    set
                }
                None => BTreeSet::new(),
            },
        },
    }
}

/// Collects the symbols printed inside an element, excluding a declaration's own name.
pub fn symbols_of_elt_without_decl_head<F>(loc_of_aloc: F, elt: &Elt<ALoc>) -> BTreeSet<Symbol<Loc>>
where
    F: Fn(&ALoc) -> Loc,
{
    let mut refs = symbols_of_elt(&loc_of_aloc, elt);
    if let Elt::Decl(d) = elt
        && let Some(head) = head_symbol_of_decl(d)
    {
        refs.remove(&head.map_locs(&loc_of_aloc));
    }
    refs
}
