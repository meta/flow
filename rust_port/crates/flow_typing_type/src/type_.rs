/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

// Some types represent definitions. These include numbers, strings, booleans,
// functions, classes, objects, arrays, and such. The shape of these types
// should be fairly obvious.
// Other types represent uses. These include function applications, class
// instantiations, property accesses, element accesses, operations such as
// addition, predicate refinements, etc. The shape of these types is somewhat
// trickier, but do follow a pattern. Typically, such a type consists of the
// arguments to the operation, and a type variable capturing the result of the
// operation. A full understanding of the semantics of such types requires a
// look at the subtyping relation, described in the module Flow_js.

// Every type has (or should have, if not already) a "reason" for its
// existence. This information is captured in the type itself for now, but
// should be separated out in the future.
// Types that represent definitions point to the positions of such
// definitions (or values). Types that represent uses point to the positions of
// such uses (or operations). These reasons are logged, chained, etc. by the
// implementation of the subtyping algorithm, that effectively constructs a
// proof of the typing derivation based on these reasons as axioms.

use std::borrow::Cow;
use std::cell::RefCell;
use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::HashSet;
use std::collections::VecDeque;
use std::hash::Hash;
use std::hash::Hasher;
use std::ops::Deref;
use std::ops::DerefPure;
use std::rc::Rc;
use std::sync::Arc;

use dupe::Dupe;
use flow_aloc::ALoc;
use flow_aloc::ALocId;
use flow_common::error_ref::ExpressionReferenceData;
use flow_common::error_ref::FunctionReferenceKind;
use flow_common::flow_symbol::Symbol;
use flow_common::hint::HintKind;
use flow_common::platform_set::PlatformSet;
use flow_common::polarity::Polarity;
use flow_common::reason::Name;
use flow_common::reason::Reason;
use flow_common::reason::SymbolID;
use flow_common::reason::VirtualReason;
use flow_common::reason::VirtualReasonDesc;
use flow_common::subst_name::SubstName;
pub use flow_common::type_strictness::TypeStrictnessKind;
use flow_common_utils::list_utils;
use flow_data_structure_wrapper::multi_level_map::MultiLevelMap;
use flow_data_structure_wrapper::ord_map::FlowOrdMap;
use flow_data_structure_wrapper::ord_set::FlowOrdSet;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_lazy::ContextFamily;
pub use flow_parser::ast::types::TypeGuardKind;
use vec1::Vec1;

#[derive(Debug, Clone, Dupe, PartialEq, Eq, Hash)]
pub struct Tvar(Reason, u32);

impl Tvar {
    pub fn new(reason: Reason, id: u32) -> Self {
        Self(reason, id)
    }

    pub fn reason(&self) -> &Reason {
        &self.0
    }

    pub fn id(&self) -> u32 {
        self.1
    }
}

impl PartialOrd for Tvar {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Tvar {
    #[inline]
    fn cmp(&self, other: &Self) -> Ordering {
        self.1.cmp(&other.1).then_with(|| self.0.cmp(&other.0))
    }
}

/// A set that emulates OCaml's immutable set semantics for tvar cycle detection.
///
/// In OCaml, `ISet.add` creates a new set without modifying the original.
/// This wrapper provides `with_added` to temporarily add an element during a
/// recursive call and automatically restore the set afterward, preventing bugs
/// where the caller forgets to remove the element.
pub struct TvarSeenSet<T: Hash>(HashSet<T>);

impl<T: Hash + Eq + Copy> TvarSeenSet<T> {
    pub fn new() -> Self {
        Self(HashSet::new())
    }

    pub fn contains(&self, value: &T) -> bool {
        self.0.contains(value)
    }

    /// Temporarily adds `value` to the set, calls `f`, and restores the set
    /// afterward. If `value` was already in the set, the closure still runs
    /// but no removal occurs (the set is unchanged).
    pub fn with_added<R>(&mut self, value: T, f: impl FnOnce(&mut Self) -> R) -> R {
        let inserted = self.0.insert(value);
        let result = f(self);
        if inserted {
            self.0.remove(&value);
        }
        result
    }
}

#[derive(Debug, Clone, Dupe, serde::Serialize, serde::Deserialize)]
pub struct NumberLiteral(pub f64, pub FlowSmolStr);

fn normalized_number_literal_bits(value: f64) -> u64 {
    if value == 0.0 {
        0.0f64.to_bits()
    } else if value.is_nan() {
        f64::NAN.to_bits()
    } else {
        value.to_bits()
    }
}

impl PartialEq for NumberLiteral {
    fn eq(&self, other: &Self) -> bool {
        normalized_number_literal_bits(self.0) == normalized_number_literal_bits(other.0)
    }
}

impl Eq for NumberLiteral {}

impl PartialOrd for NumberLiteral {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for NumberLiteral {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let lhs = f64::from_bits(normalized_number_literal_bits(self.0));
        let rhs = f64::from_bits(normalized_number_literal_bits(other.0));
        lhs.total_cmp(&rhs)
    }
}

impl std::hash::Hash for NumberLiteral {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        normalized_number_literal_bits(self.0).hash(state);
    }
}

#[derive(
    Debug,
    Clone,
    Dupe,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    serde::Serialize,
    serde::Deserialize
)]
pub struct BigIntLiteral(pub Option<i64>, pub FlowSmolStr);

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TypeAppTData {
    pub reason: Reason,
    pub use_op: UseOp,
    pub type_: Type,
    pub targs: Rc<[Type]>,
    pub from_value: bool,
    pub use_desc: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GenericTData {
    pub reason: Reason,
    pub name: SubstName,
    pub bound: Type,
    pub no_infer: bool,
    pub id: flow_typing_generics::GenericId,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
/// Metadata for the future implicit-instantiation constraint system.
/// Type visitors and mappers intentionally treat this node as atomic and do not traverse `bound`.
pub struct ImplicitInstantiationTvarData {
    pub reason: Reason,
    pub name: SubstName,
    pub bound: Type,
    pub id: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ThisInstanceTData {
    pub reason: Reason,
    pub instance: InstanceT,
    pub is_this: bool,
    pub subst_name: SubstName,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ThisTypeAppTData {
    pub reason: Reason,
    pub this_t: Type,
    pub type_: Type,
    pub targs: Option<Rc<[Type]>>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TypeInner {
    /// open type variable
    /// A type variable (tvar) is an OpenT(reason, id) where id is an int index
    /// into a context's graph: a context's graph is a map from tvar ids to nodes
    /// (see below).
    ///
    /// Note: ids are globally unique. tvars are "owned" by a single context,
    /// but that context and its tvars may later be merged into other contexts.
    OpenT(Tvar),
    /// def types
    DefT(Reason, DefT),
    /// type expression whose evaluation is deferred
    /// Usually a type expression is evaluated by splitting it into a def type
    /// and a use type, and flowing the former to the latter: the def type is the
    /// "main" argument, and the use type contains the type operation, other
    /// arguments, and a tvar to hold the result. However, sometimes a type
    /// expression may need to be kept in explicit form, with the type operation
    /// and other arguments split out into a "deferred" use type `defer_use_t`,
    /// whose evaluation state is tracked in the context by an identifier id:
    /// When defer_use_t is evaluated, id points to a tvar containing the result
    /// of evaluation. The explicit form simplifies other tasks, like
    /// substitution, but otherwise works in much the same way as usual.
    EvalT {
        type_: Type,
        defer_use_t: TypeDestructorT,
        id: eval::Id,
    },
    /// bound type variable
    GenericT(Box<GenericTData>),
    ImplicitInstantiationTvar(Box<ImplicitInstantiationTvarData>),
    /// this-abstracted instance. If `is_this` is true, then this literally comes from
    /// `this` as an annotation or expression, and should be fixed to an internal
    /// view of the class, which is a generic whose upper bound is the class.
    ThisInstanceT(Box<ThisInstanceTData>),
    /// this instantiation
    ThisTypeAppT(Box<ThisTypeAppTData>),
    /// type application
    TypeAppT(Box<TypeAppTData>),
    FunProtoT(Reason), //  Function.prototype
    ObjProtoT(Reason), //  Object.prototype
    /// Signifies the end of the prototype chain. Distinct from NullT when it
    /// appears as an upper bound of an object type, otherwise the same.
    NullProtoT(Reason),
    /// & types
    IntersectionT(Reason, inter_rep::InterRep),
    /// | types
    UnionT(Reason, union_rep::UnionRep),
    /// ? types
    MaybeT(Reason, Type),
    /// type of an optional parameter
    OptionalT {
        reason: Reason,
        type_: Type,
        use_desc: bool,
    },
    /// collects the keys of an object
    KeysT(Reason, Type),
    /// template literal types, e.g. `hello ${string}`
    TemplateLiteralT {
        reason: Reason,
        from_annot: bool,
        quasis: Vec<FlowSmolStr>,
        types: Vec<Type>,
    },
    // casing transform on a string-like type — `Uppercase<T>`, `Lowercase<T>`,
    //    `Capitalize<T>`, `Uncapitalize<T>`. Held in deferred form when `arg`
    //    isn't a literal we can rewrite at construction time (generic, `string`,
    //    template-literal interpolation), so the dependency on `arg` survives
    //    substitution.
    StringMappingT {
        reason: Reason,
        kind: StringMappingKind,
        arg: Type,
    },
    /// annotations
    /// A type that annotates a storage location performs two functions:
    ///
    /// - it constrains the types of values stored into the location
    ///
    /// - it masks the actual type of values retrieved from the location, giving
    ///   instead a pro forma type which all such values are considered as having.
    ///
    /// In the former role, the annotated type behaves as an upper bound
    /// interacting with inflowing lower bounds - these interactions may
    /// occur e.g. as a result of values being stored to type-annotated
    /// variables, or arguments flowing to type-annotated parameters.
    ///
    /// In the latter role, the annotated type behaves as a lower bound,
    /// flowing to sites where values stored in the annotated location are
    /// used (such as users of a variable, or users of a parameter within
    /// a function body).
    ///
    /// When a type annotation resolves immediately to a concrete type
    /// (say, number = NumT or string = StrT), this single type would
    /// suffice to perform both roles. However, when an annotation has
    /// not yet been resolved, we can't simply use a type variable as a
    /// placeholder as we can elsewhere.
    ///
    /// TL;DR type variables are conductors; annotated types are insulators. :)
    ///
    /// For an annotated type, we must collect incoming lower bounds and
    /// downstream upper bounds without allowing them to interact with each
    /// other. If we did, the annotation would be "translucent", leaking
    /// type information about incoming values - failing to perform the
    /// second of the two roles noted above.
    ///
    /// We accomplish the insulation by wrapping a tvar with AnnotT, and using a
    /// "slingshot" trick to grab lowers bounds, wait for the wrapped tvar to
    /// resolve to a type, then release the lower bounds to the resolved
    /// type. Meanwhile, the tvar itself flows to its upper bounds as usual.
    ///
    /// Note on usage: AnnotT can be used as a general wrapper for tvars as long
    /// as the wrapped tvars are 0->1. If instead the possible types of a
    /// wrapped tvar are T1 and T2, then the current rules would flow T1 | T2 to
    /// upper bounds, and would flow lower bounds to T1 & T2.
    AnnotT(Reason, Type, bool),
    /// Nominal type aliases. The nominal_type.nominal_id is its unique id, nominal_type.underlying_t is
    /// the underlying type, which we only allow access to when inside the file the opaque type
    /// was defined, and nominal_type.super_t is the super type, which we use when a NominalT is
    /// an upperbound in a file in which it was not defined. We also have
    /// nominal_type.nominal_arg_polarities and nominal_type.nominal_type_args to compare polymorphic
    /// opaque types. We need to keep track of these because underlying_t can be None if the opaque
    /// type is defined in a libdef. We also keep track of the name of the opaque type in
    /// nominal_type.name for pretty printing.
    NominalT {
        reason: Reason,
        nominal_type: Rc<NominalType>,
    },
    /// Stores both values and types in the same namespace
    NamespaceT(Rc<NamespaceType>),
    /// Here's to the crazy ones. The misfits. The rebels. The troublemakers.
    /// The round pegs in the square holes.
    AnyT(Reason, AnySource),
}

#[derive(Clone, Dupe)]
pub struct Type(Rc<TypeInner>);

impl Hash for Type {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

impl PartialEq for Type {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0) || self.0 == other.0
    }
}

impl Eq for Type {}

impl PartialOrd for Type {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Type {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        if Rc::ptr_eq(&self.0, &other.0) {
            std::cmp::Ordering::Equal
        } else {
            self.0.cmp(&other.0)
        }
    }
}

impl Deref for Type {
    type Target = TypeInner;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

// SAFETY: Deref repeatedly borrows the same Rc pointee without changing the wrapper.
unsafe impl DerefPure for Type {}

impl std::fmt::Debug for Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl Type {
    pub fn new(inner: TypeInner) -> Self {
        Self(Rc::new(inner))
    }

    pub fn ptr_eq(&self, other: &Type) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }

    /// Returns a mutable reference to the inner TypeInner.
    /// If this Type's Rc has refcount == 1, mutates in place (no allocation).
    /// If refcount > 1, clones the inner value first (copy-on-write).
    #[allow(dead_code)]
    pub(crate) fn make_mut(&mut self) -> &mut TypeInner {
        Rc::make_mut(&mut self.0)
    }
}

/// The kind of expression represented by `PolyT`.
#[derive(Debug, Clone, Copy, Dupe, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PolyTKind {
    /// A function, class, component, type alias, or opaque type.
    Regular,
    /// A mapped type's property or key expression.
    MappedType,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PolyTData {
    pub kind: PolyTKind,
    pub tparams_loc: ALoc,
    pub tparams: Rc<[TypeParam]>,
    pub t_out: Type,
    pub id: poly::Id,
    pub strictness_kind: TypeStrictnessKind,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ReactAbstractComponentTData {
    pub config: Type,
    pub renders: Type,
    pub component_kind: ComponentKind,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DefTInner {
    NumGeneralT(Literal),
    StrGeneralT(Literal),
    BoolGeneralT,
    BigIntGeneralT(Literal),
    EmptyT,
    MixedT(MixedFlavor),
    NullT,
    VoidT,
    SymbolT,
    UniqueSymbolT(UniqueSymbolTData),
    FunT(Type, Rc<FunType>),
    ObjT(Rc<ObjType>),
    ArrT(Rc<ArrType>),
    /// type of a class
    ClassT(Type),
    /// type of an instance of a class
    InstanceT(Rc<InstanceT>),
    /// singleton string, matches exactly a given string literal
    SingletonStrT {
        from_annot: bool,
        value: FlowSmolStr,
    },
    /// This type is only to be used to represent numeric-like object keys in the
    /// context of object-to-object subtyping. It allows numeric-like object keys
    /// to be a subtype of both `number` and `string`, so that `{1: true}` can be
    /// a subtyped of `{[number]: boolean}`. Do not use outside of this context!
    ///
    /// The second element of the `number_literal` tuple, which is the `string`
    /// representation, is the key name.
    NumericStrKeyT(NumberLiteral),
    /// matches exactly a given number literal, for some definition of "exactly"
    /// when it comes to floats...
    SingletonNumT {
        from_annot: bool,
        value: NumberLiteral,
    },
    /// singleton bool, matches exactly a given boolean literal
    SingletonBoolT {
        from_annot: bool,
        value: bool,
    },
    SingletonBigIntT {
        from_annot: bool,
        value: BigIntLiteral,
    },
    /// type aliases
    TypeT(TypeTKind, Type),
    /// A polymorphic type is like a type-level "function" that, when applied to
    /// lists of type arguments, generates types. Just like a function, a
    /// polymorphic type has a list of type parameters, represented as bound
    /// type variables. We say that type parameters are "universally quantified"
    /// (or "universal"): every substitution of type arguments for type
    /// parameters generates a type. Universal type parameters may specify subtype
    /// constraints ("bounds"), which must be satisfied by any types they may be
    /// substituted by.
    PolyT(Box<PolyTData>),
    ReactAbstractComponentT(Box<ReactAbstractComponentTData>),
    RendersT(Rc<CanonicalRendersForm>),
    EnumValueT(Rc<EnumInfo>),
    EnumObjectT {
        enum_value_t: Type,
        enum_info: Rc<EnumInfo>,
    },
}

#[derive(Debug, Clone, Dupe, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UniqueSymbolTData {
    pub symbol: SymbolID,
    /// Inferred singleton types are widened by natural inference when their
    /// identity is not needed. An explicit annotation (and an `as const`
    /// conversion) makes the identity non-widening.
    pub from_annot: bool,
}

#[derive(Debug, Clone, Dupe)]
pub struct DefT(Rc<DefTInner>);

impl Hash for DefT {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

impl PartialEq for DefT {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0) || self.0 == other.0
    }
}

impl Eq for DefT {}

impl PartialOrd for DefT {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for DefT {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        if Rc::ptr_eq(&self.0, &other.0) {
            std::cmp::Ordering::Equal
        } else {
            self.0.cmp(&other.0)
        }
    }
}

impl Deref for DefT {
    type Target = DefTInner;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

// SAFETY: Deref repeatedly borrows the same Rc pointee without changing the wrapper.
unsafe impl DerefPure for DefT {}

impl DefT {
    pub fn new(inner: DefTInner) -> Self {
        Self(Rc::new(inner))
    }

    pub fn ptr_eq(&self, other: &DefT) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

/// A syntactic render type "renders T" uses an EvalT to be translated into a canonical form.
/// The subtyping rules are much simpler to understand in these forms, so we use the render type
/// normalization logic defined in flow_js_utils to take a syntactic render type and turn it
/// into a RendersT (it will ALWAYS return a RendersT) of one of the canonical forms.
///
/// The Structural (t) form guarantees that if you evaluate t you will not
/// get back a RendersT. If the argument is a UnionT, none of the members of that UnionT
/// will be Strcutrual RendersTs, but there may be nominal RendersTs in the union.
///
/// Nominal render types make no guarantees about their super and they can be any type.
/// In practice, the only way to introduce Nominal render types is component syntax, and
/// components always use a render type as the `super`.
///
/// Given component Foo:
///  * Omitting render declaration would produce DefaultRenders
///  * renders Foo would produce NominalRenders
///  * renders (Foo | Foo) would produce a Structural UnionT with two Nominal elements
///  * renders (Foo | number) would produce a Structural UnionT with number and Nominal Foo
///  * renders number would produce a Structural number
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CanonicalRendersForm {
    IntrinsicRenders(FlowSmolStr),
    NominalRenders {
        renders_id: ALocId,
        renders_name: FlowSmolStr,
        renders_super: Type,
    },
    StructuralRenders {
        renders_variant: RendersVariant,
        renders_structural_type: Type,
    },
    DefaultRenders,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RendersVariant {
    RendersNormal,
    RendersMaybe,
    RendersStar,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ComponentKind {
    Structural,
    Nominal(ALocId, FlowSmolStr, Option<Rc<[Type]>>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum HintEvalResult {
    HintAvailable(Type, HintKind),
    NoHint,
    EncounteredPlaceholder(Option<Type>),
    DecompositionError,
}

pub type LazyHintCompute<CX = ()> = Rc<
    dyn for<'a> Fn(
        /* cx */ &mut <CX as ContextFamily>::Context<'a>,
        /* expected_only */ bool,
        /* skip_optional */ Option<bool>,
        /* reason */ Reason,
    ) -> Result<HintEvalResult, flow_utils_concurrency::job_error::JobError>,
>;

impl<CX: ContextFamily> std::fmt::Debug for LazyHintT<CX> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("LazyHintT")
            .field(&self.0)
            .field(&"<lazy_hint_compute>")
            .finish()
    }
}

pub struct LazyHintT<CX: ContextFamily = ()>(pub bool, pub LazyHintCompute<CX>);

impl<CX: ContextFamily> Clone for LazyHintT<CX> {
    fn clone(&self) -> Self {
        LazyHintT(self.0, self.1.clone())
    }
}

impl<CX: ContextFamily> PartialEq for LazyHintT<CX> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0 && Rc::ptr_eq(&self.1, &other.1)
    }
}

impl<CX: ContextFamily> Eq for LazyHintT<CX> {}

impl<CX: ContextFamily> PartialOrd for LazyHintT<CX> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<CX: ContextFamily> Ord for LazyHintT<CX> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match self.0.cmp(&other.0) {
            std::cmp::Ordering::Equal => {
                let self_ptr = Rc::as_ptr(&self.1) as *const () as usize;
                let other_ptr = Rc::as_ptr(&other.1) as *const () as usize;
                self_ptr.cmp(&other_ptr)
            }
            ordering => ordering,
        }
    }
}

impl<CX: ContextFamily> std::hash::Hash for LazyHintT<CX> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.hash(state);
        let ptr = Rc::as_ptr(&self.1) as *const () as usize;
        ptr.hash(state);
    }
}

/// destructors that extract parts of various kinds of types
#[derive(Debug, Clone, Dupe, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TypeDestructorT(Rc<TypeDestructorTInner>);

#[derive(Debug, Clone, Dupe, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TypeDestructorTInner(pub UseOp, pub Reason, pub Rc<Destructor>);

impl Deref for TypeDestructorT {
    type Target = TypeDestructorTInner;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl TypeDestructorT {
    pub fn new(inner: TypeDestructorTInner) -> Self {
        Self(Rc::new(inner))
    }

    pub fn ptr_eq(&self, other: &TypeDestructorT) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EnumConcreteInfoInner {
    pub enum_name: FlowSmolStr,
    pub enum_id: ALocId,
    pub members: FlowOrdMap<FlowSmolStr, ALoc>,
    pub representation_t: Type,
    pub has_unknown_members: bool,
}

#[derive(Debug, Clone, Dupe, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EnumConcreteInfo(Rc<EnumConcreteInfoInner>);

impl Deref for EnumConcreteInfo {
    type Target = EnumConcreteInfoInner;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl EnumConcreteInfo {
    pub fn new(inner: EnumConcreteInfoInner) -> Self {
        Self(Rc::new(inner))
    }

    pub fn ptr_eq(&self, other: &EnumConcreteInfo) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EnumInfoInner {
    ConcreteEnum(EnumConcreteInfo),
    AbstractEnum { representation_t: Type },
}

#[derive(Debug, Clone, Dupe, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EnumInfo(Rc<EnumInfoInner>);

impl std::ops::Deref for EnumInfo {
    type Target = EnumInfoInner;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl EnumInfo {
    pub fn new(inner: EnumInfoInner) -> Self {
        Self(Rc::new(inner))
    }

    pub fn enum_name(&self) -> Option<&FlowSmolStr> {
        match self.deref() {
            EnumInfoInner::ConcreteEnum(info) => Some(&info.enum_name),
            EnumInfoInner::AbstractEnum { .. } => None,
        }
    }

    pub fn ptr_eq(&self, other: &EnumInfo) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

#[derive(Debug, Clone, Copy, Dupe, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum StringMappingKind {
    StringMappingUppercase,
    StringMappingLowercase,
    StringMappingCapitalize,
    StringMappingUncapitalize,
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
pub struct ClassImplementsCheckData<L: Dupe + PartialEq + Eq + PartialOrd + Ord> {
    pub def: VirtualReason<L>,
    pub name: VirtualReason<L>,
    pub implements: VirtualReason<L>,
}

/// Compared without `def_t`: the type is what the error prints, and must not tell apart errors or
/// constraints that are otherwise the same.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FunMissingArgData<L: Dupe + PartialEq + Eq + PartialOrd + Ord> {
    pub n: i32,
    pub op: VirtualReason<L>,
    pub def: VirtualReason<L>,
    pub def_t: type_or_type_desc::TypeOrTypeDescT<L>,
}

impl<L: Dupe + PartialEq + Eq + PartialOrd + Ord> PartialEq for FunMissingArgData<L> {
    fn eq(&self, other: &Self) -> bool {
        self.n == other.n && self.op == other.op && self.def == other.def
    }
}

impl<L: Dupe + PartialEq + Eq + PartialOrd + Ord> Eq for FunMissingArgData<L> {}

impl<L: Dupe + PartialEq + Eq + PartialOrd + Ord + Hash> Hash for FunMissingArgData<L> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.n.hash(state);
        self.op.hash(state);
        self.def.hash(state);
    }
}

impl<L: Dupe + PartialEq + Eq + PartialOrd + Ord> PartialOrd for FunMissingArgData<L> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<L: Dupe + PartialEq + Eq + PartialOrd + Ord> Ord for FunMissingArgData<L> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.n
            .cmp(&other.n)
            .then_with(|| self.op.cmp(&other.op))
            .then_with(|| self.def.cmp(&other.def))
    }
}

/// Compared without `upper_t`: the type is what the error prints, and must not tell apart errors
/// or constraints that are otherwise the same.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FunImplicitReturnData<L: Dupe + PartialEq + Eq + PartialOrd + Ord> {
    pub fn_: VirtualReason<L>,
    pub upper: VirtualReason<L>,
    pub upper_t: type_or_type_desc::TypeOrTypeDescT<L>,
    pub type_guard: bool,
}

impl<L: Dupe + PartialEq + Eq + PartialOrd + Ord> PartialEq for FunImplicitReturnData<L> {
    fn eq(&self, other: &Self) -> bool {
        self.fn_ == other.fn_ && self.upper == other.upper && self.type_guard == other.type_guard
    }
}

impl<L: Dupe + PartialEq + Eq + PartialOrd + Ord> Eq for FunImplicitReturnData<L> {}

impl<L: Dupe + PartialEq + Eq + PartialOrd + Ord + Hash> Hash for FunImplicitReturnData<L> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.fn_.hash(state);
        self.upper.hash(state);
        self.type_guard.hash(state);
    }
}

impl<L: Dupe + PartialEq + Eq + PartialOrd + Ord> PartialOrd for FunImplicitReturnData<L> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<L: Dupe + PartialEq + Eq + PartialOrd + Ord> Ord for FunImplicitReturnData<L> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.fn_
            .cmp(&other.fn_)
            .then_with(|| self.upper.cmp(&other.upper))
            .then_with(|| self.type_guard.cmp(&other.type_guard))
    }
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
pub struct SetPropertyData<L: Dupe + PartialEq + Eq + PartialOrd + Ord> {
    pub lhs: VirtualReason<L>,
    pub prop: VirtualReason<L>,
    pub value: VirtualReason<L>,
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
pub struct ClassOwnProtoCheckData<L: Dupe + PartialEq + Eq + PartialOrd + Ord> {
    pub prop: Name,
    pub own_loc: Option<L>,
    pub proto_loc: Option<L>,
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
pub struct ClassMethodDefinitionData<L: Dupe + PartialEq + Eq + PartialOrd + Ord> {
    pub def_loc: L,
    pub method_name: FlowSmolStr,
    pub name: VirtualReason<L>,
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
pub struct ConformToCommonInterfaceData<L: Dupe + PartialEq + Eq + PartialOrd + Ord> {
    pub self_sig_loc: L,
    pub self_module_loc: L,
    pub originate_from_import: bool,
}

#[derive(
    Debug,
    Clone,
    Dupe,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    serde::Serialize,
    serde::Deserialize
)]
pub enum ImplicitInstantiationReferenceKind {
    Call(Option<FlowSmolStr>),
    Method(Option<FlowSmolStr>),
    Constructor(Option<FlowSmolStr>),
    ReactElement(Option<FlowSmolStr>),
    JSXFunction(FlowSmolStr),
    SuperCall,
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
pub struct FunCallData<L: Dupe + PartialEq + Eq + PartialOrd + Ord> {
    pub op: VirtualReason<L>,
    pub fn_: VirtualReason<L>,
    pub args: Arc<[VirtualReason<L>]>,
    pub implicit_instantiation_reference: ImplicitInstantiationReferenceKind,
    pub local: bool,
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
pub struct StandaloneCallThisData<L: Dupe + PartialEq + Eq + PartialOrd + Ord> {
    pub op: VirtualReason<L>,
    pub fn_: VirtualReason<L>,
    pub receiver_loc: L,
    pub receiver: Box<type_or_type_desc::TypeOrTypeDescT<L>>,
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
pub enum FunCallMethodCallee<L: Dupe + PartialEq + Eq + PartialOrd + Ord> {
    Reason(VirtualReason<L>),
    Method(FlowSmolStr),
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
pub struct FunCallMethodData<L: Dupe + PartialEq + Eq + PartialOrd + Ord> {
    pub op: VirtualReason<L>,
    pub fn_: FunCallMethodCallee<L>,
    pub prop: VirtualReason<L>,
    pub args: Arc<[VirtualReason<L>]>,
    pub implicit_instantiation_reference: ImplicitInstantiationReferenceKind,
    pub local: bool,
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
pub struct ReactCreateElementCallData<L: Dupe + PartialEq + Eq + PartialOrd + Ord> {
    pub op: VirtualReason<L>,
    pub component: VirtualReason<L>,
    pub children: L,
    pub implicit_instantiation_reference: ImplicitInstantiationReferenceKind,
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
pub struct JSXCreateElementData<L: Dupe + PartialEq + Eq + PartialOrd + Ord> {
    pub op: VirtualReason<L>,
    pub component: VirtualReason<L>,
    pub implicit_instantiation_reference: ImplicitInstantiationReferenceKind,
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
pub struct RecordCreateData<L: Dupe + PartialEq + Eq + PartialOrd + Ord> {
    pub op: VirtualReason<L>,
    pub constructor: VirtualReason<L>,
    pub properties: L,
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
pub struct SwitchRefinementCheckData<L: Dupe + PartialEq + Eq + PartialOrd + Ord> {
    pub test: L,
    pub discriminant: L,
}

/// What an `AssignVar` assigns to: a binding, or the annotation of a destructured
/// parameter, which has no binding of its own.
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
pub enum AssignVarTarget<L: Dupe + PartialEq + Eq + PartialOrd + Ord> {
    Variable(VirtualReason<L>),
    Annotation(type_or_type_desc::TypeOrTypeDescT<L>),
    /// The annotation of a destructuring declaration, printed as its type. The pattern has no
    /// binding of its own; its reason is the reason of the operation.
    Pattern {
        reason: VirtualReason<L>,
        annot: type_or_type_desc::TypeOrTypeDescT<L>,
    },
}

/// What an `AssignVar` assigns: a value referenced by its reason, or a default value, which is
/// printed as its type.
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
pub enum AssignVarInit<L: Dupe + PartialEq + Eq + PartialOrd + Ord> {
    Reason(VirtualReason<L>),
    Default(Box<AssignVarDefaultData<L>>),
}

/// Compared by location only: the type is what the error prints, and must not tell apart errors
/// that are otherwise the same.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AssignVarDefaultData<L: Dupe + PartialEq + Eq + PartialOrd + Ord> {
    pub loc: L,
    pub default: type_or_type_desc::TypeOrTypeDescT<L>,
}

impl<L: Dupe + PartialEq + Eq + PartialOrd + Ord> PartialEq for AssignVarDefaultData<L> {
    fn eq(&self, other: &Self) -> bool {
        self.loc == other.loc
    }
}

impl<L: Dupe + PartialEq + Eq + PartialOrd + Ord> Eq for AssignVarDefaultData<L> {}

impl<L: Dupe + PartialEq + Eq + PartialOrd + Ord + Hash> Hash for AssignVarDefaultData<L> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.loc.hash(state);
    }
}

impl<L: Dupe + PartialEq + Eq + PartialOrd + Ord> PartialOrd for AssignVarDefaultData<L> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<L: Dupe + PartialEq + Eq + PartialOrd + Ord> Ord for AssignVarDefaultData<L> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.loc.cmp(&other.loc)
    }
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
pub struct ClassExtendsCheckData<L: Dupe + PartialEq + Eq + PartialOrd + Ord> {
    pub def_loc: L,
    pub def: type_or_type_desc::TypeOrTypeDescT<L>,
    pub extends_loc: L,
    pub extends: type_or_type_desc::TypeOrTypeDescT<L>,
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
pub struct EvalMappedTypeData<L: Dupe + PartialEq + Eq + PartialOrd + Ord> {
    pub mapped_type_loc: L,
    pub mapped_type: type_or_type_desc::TypeOrTypeDescT<L>,
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
pub struct MappedTypeKeyCompatibilityData<L: Dupe + PartialEq + Eq + PartialOrd + Ord> {
    pub source_type_loc: L,
    pub source_type: type_or_type_desc::TypeOrTypeDescT<L>,
    pub mapped_type_loc: L,
    pub mapped_type: type_or_type_desc::TypeOrTypeDescT<L>,
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
pub struct IndexedTypeAccessData<L: Dupe + PartialEq + Eq + PartialOrd + Ord> {
    pub index_loc: L,
    pub object: type_or_type_desc::TypeOrTypeDescT<L>,
    pub index: type_or_type_desc::TypeOrTypeDescT<L>,
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
pub struct ComponentRestParamCompatibilityData<L: Dupe + PartialEq + Eq + PartialOrd + Ord> {
    pub rest_param_loc: L,
    pub rest_param: type_or_type_desc::TypeOrTypeDescT<L>,
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
pub struct PositiveTypeGuardConsistencyData<L: Dupe + PartialEq + Eq + PartialOrd + Ord> {
    pub reason: VirtualReason<L>,
    pub return_reason: VirtualReason<L>,
    pub param_loc: L,
    pub param_name: FlowSmolStr,
    pub guard_type_reference_loc: L,
    pub guard_type: type_or_type_desc::TypeOrTypeDescT<L>,
    pub is_return_false_statement: bool,
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
pub struct AssertionFunctionConsistencyData<L: Dupe + PartialEq + Eq + PartialOrd + Ord> {
    /// The `return`, or the `asserts` annotation when control reaches the end
    /// of the function body.
    pub exit_reason: VirtualReason<L>,
    pub param_loc: L,
    pub param_name: FlowSmolStr,
    pub guard_type_reference_loc: L,
    pub guard_type: type_or_type_desc::TypeOrTypeDescT<L>,
    pub implicit_return: bool,
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
pub struct TypeGuardIncompatibilityData<L: Dupe + PartialEq + Eq + PartialOrd + Ord> {
    pub guard_type_loc: L,
    pub guard_type_reference_loc: L,
    pub guard_type: type_or_type_desc::TypeOrTypeDescT<L>,
    pub param_name: FlowSmolStr,
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
pub struct ConstrainedAssignmentData<L: Dupe + PartialEq + Eq + PartialOrd + Ord> {
    pub name: FlowSmolStr,
    pub declaration: L,
    pub providers: Arc<[L]>,
}

/// Compared without `lower_t` and `upper_t`: the types are what the error prints, and must not
/// tell apart errors or constraints that are otherwise the same.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FunParamData<L: Dupe + PartialEq + Eq + PartialOrd + Ord> {
    pub n: i32,
    pub name: Option<FlowSmolStr>,
    pub lower: VirtualReason<L>,
    pub upper: VirtualReason<L>,
    pub lower_t: type_or_type_desc::TypeOrTypeDescT<L>,
    pub upper_t: type_or_type_desc::TypeOrTypeDescT<L>,
}

impl<L: Dupe + PartialEq + Eq + PartialOrd + Ord> PartialEq for FunParamData<L> {
    fn eq(&self, other: &Self) -> bool {
        self.n == other.n
            && self.name == other.name
            && self.lower == other.lower
            && self.upper == other.upper
    }
}

impl<L: Dupe + PartialEq + Eq + PartialOrd + Ord> Eq for FunParamData<L> {}

impl<L: Dupe + PartialEq + Eq + PartialOrd + Ord + Hash> Hash for FunParamData<L> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.n.hash(state);
        self.name.hash(state);
        self.lower.hash(state);
        self.upper.hash(state);
    }
}

impl<L: Dupe + PartialEq + Eq + PartialOrd + Ord> PartialOrd for FunParamData<L> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<L: Dupe + PartialEq + Eq + PartialOrd + Ord> Ord for FunParamData<L> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.n
            .cmp(&other.n)
            .then_with(|| self.name.cmp(&other.name))
            .then_with(|| self.lower.cmp(&other.lower))
            .then_with(|| self.upper.cmp(&other.upper))
    }
}

/// Compared without `lower_t` and `upper_t`: the types are what the error prints, and must not
/// tell apart errors or constraints that are otherwise the same.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FunReturnData<L: Dupe + PartialEq + Eq + PartialOrd + Ord> {
    pub lower: VirtualReason<L>,
    pub upper: VirtualReason<L>,
    pub lower_t: type_or_type_desc::TypeOrTypeDescT<L>,
    pub upper_t: type_or_type_desc::TypeOrTypeDescT<L>,
}

impl<L: Dupe + PartialEq + Eq + PartialOrd + Ord> PartialEq for FunReturnData<L> {
    fn eq(&self, other: &Self) -> bool {
        self.lower == other.lower && self.upper == other.upper
    }
}

impl<L: Dupe + PartialEq + Eq + PartialOrd + Ord> Eq for FunReturnData<L> {}

impl<L: Dupe + PartialEq + Eq + PartialOrd + Ord + Hash> Hash for FunReturnData<L> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.lower.hash(state);
        self.upper.hash(state);
    }
}

impl<L: Dupe + PartialEq + Eq + PartialOrd + Ord> PartialOrd for FunReturnData<L> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<L: Dupe + PartialEq + Eq + PartialOrd + Ord> Ord for FunReturnData<L> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.lower
            .cmp(&other.lower)
            .then_with(|| self.upper.cmp(&other.upper))
    }
}

/// Compared without `lower_t` and `upper_t`: the types are what the error prints, and must not
/// tell apart errors or constraints that are otherwise the same.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ArrayElementCompatibilityData<L: Dupe + PartialEq + Eq + PartialOrd + Ord> {
    pub lower: VirtualReason<L>,
    pub upper: VirtualReason<L>,
    pub lower_t: type_or_type_desc::TypeOrTypeDescT<L>,
    pub upper_t: type_or_type_desc::TypeOrTypeDescT<L>,
}

impl<L: Dupe + PartialEq + Eq + PartialOrd + Ord> PartialEq for ArrayElementCompatibilityData<L> {
    fn eq(&self, other: &Self) -> bool {
        self.lower == other.lower && self.upper == other.upper
    }
}

impl<L: Dupe + PartialEq + Eq + PartialOrd + Ord> Eq for ArrayElementCompatibilityData<L> {}

impl<L: Dupe + PartialEq + Eq + PartialOrd + Ord + Hash> Hash for ArrayElementCompatibilityData<L> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.lower.hash(state);
        self.upper.hash(state);
    }
}

impl<L: Dupe + PartialEq + Eq + PartialOrd + Ord> PartialOrd for ArrayElementCompatibilityData<L> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<L: Dupe + PartialEq + Eq + PartialOrd + Ord> Ord for ArrayElementCompatibilityData<L> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.lower
            .cmp(&other.lower)
            .then_with(|| self.upper.cmp(&other.upper))
    }
}

/// Compared without `lower_t` and `upper_t`: the types are what the error prints, and must not
/// tell apart errors or constraints that are otherwise the same.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct IndexerKeyCompatibilityData<L: Dupe + PartialEq + Eq + PartialOrd + Ord> {
    pub lower: VirtualReason<L>,
    pub upper: VirtualReason<L>,
    pub lower_t: type_or_type_desc::TypeOrTypeDescT<L>,
    pub upper_t: type_or_type_desc::TypeOrTypeDescT<L>,
}

impl<L: Dupe + PartialEq + Eq + PartialOrd + Ord> PartialEq for IndexerKeyCompatibilityData<L> {
    fn eq(&self, other: &Self) -> bool {
        self.lower == other.lower && self.upper == other.upper
    }
}

impl<L: Dupe + PartialEq + Eq + PartialOrd + Ord> Eq for IndexerKeyCompatibilityData<L> {}

impl<L: Dupe + PartialEq + Eq + PartialOrd + Ord + Hash> Hash for IndexerKeyCompatibilityData<L> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.lower.hash(state);
        self.upper.hash(state);
    }
}

impl<L: Dupe + PartialEq + Eq + PartialOrd + Ord> PartialOrd for IndexerKeyCompatibilityData<L> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<L: Dupe + PartialEq + Eq + PartialOrd + Ord> Ord for IndexerKeyCompatibilityData<L> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.lower
            .cmp(&other.lower)
            .then_with(|| self.upper.cmp(&other.upper))
    }
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
pub struct OpaqueTypeCustomErrorCompatibilityData<L: Dupe + PartialEq + Eq + PartialOrd + Ord> {
    pub lower: VirtualReason<L>,
    pub upper: VirtualReason<L>,
    pub lower_t: type_or_type_desc::TypeOrTypeDescT<L>,
    pub upper_t: type_or_type_desc::TypeOrTypeDescT<L>,
    pub custom_error_loc: L,
    pub name: FlowSmolStr,
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
pub struct UnionRepresentativeData<L: Dupe + PartialEq + Eq + PartialOrd + Ord> {
    pub loc: L,
    pub reference_loc: L,
    pub union: type_or_type_desc::TypeOrTypeDescT<L>,
}

/// Compared without `lower_t` and `upper_t`: the types are what the error prints, and must not
/// tell apart errors or constraints that are otherwise the same.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PropertyCompatibilityData<L: Dupe + PartialEq + Eq + PartialOrd + Ord> {
    pub prop: Option<Name>,
    pub lower: VirtualReason<L>,
    pub upper: VirtualReason<L>,
    pub lower_t: type_or_type_desc::TypeOrTypeDescT<L>,
    pub upper_t: type_or_type_desc::TypeOrTypeDescT<L>,
}

impl<L: Dupe + PartialEq + Eq + PartialOrd + Ord> PartialEq for PropertyCompatibilityData<L> {
    fn eq(&self, other: &Self) -> bool {
        self.prop == other.prop && self.lower == other.lower && self.upper == other.upper
    }
}

impl<L: Dupe + PartialEq + Eq + PartialOrd + Ord> Eq for PropertyCompatibilityData<L> {}

impl<L: Dupe + PartialEq + Eq + PartialOrd + Ord + Hash> Hash for PropertyCompatibilityData<L> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.prop.hash(state);
        self.lower.hash(state);
        self.upper.hash(state);
    }
}

impl<L: Dupe + PartialEq + Eq + PartialOrd + Ord> PartialOrd for PropertyCompatibilityData<L> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<L: Dupe + PartialEq + Eq + PartialOrd + Ord> Ord for PropertyCompatibilityData<L> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.prop
            .cmp(&other.prop)
            .then_with(|| self.lower.cmp(&other.lower))
            .then_with(|| self.upper.cmp(&other.upper))
    }
}

/// Compared without `lower_t` and `upper_t`: the types are what the error prints, and must not
/// tell apart errors or constraints that are otherwise the same.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TupleElementCompatibilityData<L: Dupe + PartialEq + Eq + PartialOrd + Ord> {
    pub n: i32,
    pub lower: VirtualReason<L>,
    pub upper: VirtualReason<L>,
    pub lower_t: type_or_type_desc::TypeOrTypeDescT<L>,
    pub upper_t: type_or_type_desc::TypeOrTypeDescT<L>,
}

impl<L: Dupe + PartialEq + Eq + PartialOrd + Ord> PartialEq for TupleElementCompatibilityData<L> {
    fn eq(&self, other: &Self) -> bool {
        self.n == other.n && self.lower == other.lower && self.upper == other.upper
    }
}

impl<L: Dupe + PartialEq + Eq + PartialOrd + Ord> Eq for TupleElementCompatibilityData<L> {}

impl<L: Dupe + PartialEq + Eq + PartialOrd + Ord + Hash> Hash for TupleElementCompatibilityData<L> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.n.hash(state);
        self.lower.hash(state);
        self.upper.hash(state);
    }
}

impl<L: Dupe + PartialEq + Eq + PartialOrd + Ord> PartialOrd for TupleElementCompatibilityData<L> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<L: Dupe + PartialEq + Eq + PartialOrd + Ord> Ord for TupleElementCompatibilityData<L> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.n
            .cmp(&other.n)
            .then_with(|| self.lower.cmp(&other.lower))
            .then_with(|| self.upper.cmp(&other.upper))
    }
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
pub struct TypeArgCompatibilityData<L: Dupe + PartialEq + Eq + PartialOrd + Ord> {
    pub name: SubstName,
    pub targ: VirtualReason<L>,
    pub lower: VirtualReason<L>,
    pub upper: VirtualReason<L>,
    pub polarity: Polarity,
}

#[derive(
    Debug,
    Clone,
    Copy,
    Dupe,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    serde::Serialize,
    serde::Deserialize
)]
pub enum MergedDeclarationConflict {
    Property,
    TypeParamDefault,
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
pub struct MergedDeclarationData<L: Dupe + PartialEq + Eq + PartialOrd + Ord> {
    pub conflict: MergedDeclarationConflict,
    pub first_decl_loc: L,
    pub first_decl: type_or_type_desc::TypeOrTypeDescT<L>,
    pub current_decl: VirtualReason<L>,
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
pub struct InitFieldData<L: Dupe + PartialEq + Eq + PartialOrd + Ord> {
    pub op: VirtualReason<L>,
    pub field: FlowSmolStr,
    pub body: VirtualReason<L>,
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
pub struct TypeofApplicationData<L: Dupe + PartialEq + Eq + PartialOrd + Ord> {
    pub reason: VirtualReason<L>,
    pub name: FlowSmolStr,
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
pub enum TypeApplicationTarget<L: Dupe + PartialEq + Eq + PartialOrd + Ord> {
    Reason(VirtualReason<L>),
    Typeof(Box<TypeofApplicationData<L>>),
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
pub enum VirtualRootUseOp<L: Dupe + PartialEq + Eq + PartialOrd + Ord> {
    ObjectAddComputedProperty {
        op: VirtualReason<L>,
    },
    ObjectSpread {
        op: VirtualReason<L>,
    },
    ObjectRest {
        op: VirtualReason<L>,
    },
    ObjectChain {
        op: VirtualReason<L>,
    },
    AssignVar {
        var: Option<Box<AssignVarTarget<L>>>,
        init: AssignVarInit<L>,
    },
    Cast {
        lower: VirtualReason<L>,
        upper: Box<type_or_type_desc::TypeOrTypeDescT<L>>,
    },
    ClassExtendsCheck(Box<ClassExtendsCheckData<L>>),
    ClassImplementsCheck(Box<ClassImplementsCheckData<L>>),
    ClassOwnProtoCheck(Box<ClassOwnProtoCheckData<L>>),
    ClassMethodDefinition(Box<ClassMethodDefinitionData<L>>),
    Coercion {
        from: VirtualReason<L>,
        target: VirtualReason<L>,
    },
    ConformToCommonInterface(Box<ConformToCommonInterfaceData<L>>),
    MergedDeclaration(Box<MergedDeclarationData<L>>),
    DeclareComponentRef {
        op: VirtualReason<L>,
    },
    DeleteProperty {
        lhs: VirtualReason<L>,
        prop: VirtualReason<L>,
    },
    DeleteVar {
        var: VirtualReason<L>,
    },
    FunCall(Box<FunCallData<L>>),
    FunCallMethod(Box<FunCallMethodData<L>>),
    FunReturnStatement {
        value: VirtualReason<L>,
    },
    FunImplicitReturn(Box<FunImplicitReturnData<L>>),
    GeneratorYield {
        value: VirtualReason<L>,
    },
    GetExport(VirtualReason<L>),
    GetProperty(VirtualReason<L>),
    IndexedTypeAccess(Box<IndexedTypeAccessData<L>>),
    InitField(Box<InitFieldData<L>>),
    InferBoundCompatibilityCheck {
        bound: VirtualReason<L>,
        infer: VirtualReason<L>,
    },
    JSXCreateElement(Box<JSXCreateElementData<L>>),
    ReactCreateElementCall(Box<ReactCreateElementCallData<L>>),
    ReactGetIntrinsic {
        literal: VirtualReason<L>,
    },
    RecordCreate(Box<RecordCreateData<L>>),
    Speculation(Arc<VirtualUseOp<L>>),
    TypeApplication {
        type_: TypeApplicationTarget<L>,
    },
    MakeReadOnly {
        loc: Box<L>,
    },
    SetProperty(Box<SetPropertyData<L>>),
    UpdateProperty {
        lhs: VirtualReason<L>,
        prop: VirtualReason<L>,
    },
    RefinementCheck {
        test: VirtualReason<L>,
        discriminant: VirtualReason<L>,
    },
    SwitchRefinementCheck(Box<SwitchRefinementCheckData<L>>),
    EvalMappedType(Box<EvalMappedTypeData<L>>),
    TypeGuardIncompatibility(Box<TypeGuardIncompatibilityData<L>>),
    RenderTypeInstantiation {
        render_type: VirtualReason<L>,
    },
    ComponentRestParamCompatibility(Box<ComponentRestParamCompatibilityData<L>>),
    PositiveTypeGuardConsistency(Box<PositiveTypeGuardConsistencyData<L>>),
    AssertionFunctionConsistency(Box<AssertionFunctionConsistencyData<L>>),
    UnknownUse,
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
pub enum VirtualFrameUseOp<L: Dupe + PartialEq + Eq + PartialOrd + Ord> {
    ConstrainedAssignment(Box<ConstrainedAssignmentData<L>>),
    ReactDeepReadOnly(Box<(L, DroType)>),
    ArrayElementCompatibility(Box<ArrayElementCompatibilityData<L>>),
    FunCompatibility {
        lower: VirtualReason<L>,
        upper: VirtualReason<L>,
    },
    StandaloneCallThis(Box<StandaloneCallThisData<L>>),
    FunMissingArg(Box<FunMissingArgData<L>>),
    FunParam(Box<FunParamData<L>>),
    FunRestParam {
        lower: VirtualReason<L>,
        upper: VirtualReason<L>,
    },
    FunReturn(Box<FunReturnData<L>>),
    ImplicitTypeParam,
    IndexerKeyCompatibility(Box<IndexerKeyCompatibilityData<L>>),
    OpaqueTypeLowerBoundCompatibility {
        lower: VirtualReason<L>,
        upper: VirtualReason<L>,
    },
    OpaqueTypeUpperBoundCompatibility {
        lower: VirtualReason<L>,
        upper: VirtualReason<L>,
    },
    OpaqueTypeCustomErrorCompatibility(Box<OpaqueTypeCustomErrorCompatibilityData<L>>),
    MappedTypeKeyCompatibility(Box<MappedTypeKeyCompatibilityData<L>>),
    PropertyCompatibility(Box<PropertyCompatibilityData<L>>),
    ReactConfigCheck,
    ReactGetConfig {
        polarity: Polarity,
    },
    TupleElementCompatibility(Box<TupleElementCompatibilityData<L>>),
    TupleAssignment,
    TypeArgCompatibility(Box<TypeArgCompatibilityData<L>>),
    TypeParamBound {
        name: SubstName,
    },
    OpaqueTypeLowerBound {
        opaque_t_reason: VirtualReason<L>,
    },
    OpaqueTypeUpperBound {
        opaque_t_reason: VirtualReason<L>,
    },
    TypeGuardCompatibility,
    RendersCompatibility,
    UnifyFlip,
    EnumRepresentationTypeCompatibility {
        lower: VirtualReason<L>,
        upper: VirtualReason<L>,
    },
    UnionRepresentative(Box<UnionRepresentativeData<L>>),
}

#[derive(Debug, Clone, Dupe)]
pub enum VirtualUseOp<L: Dupe + PartialEq + Eq + PartialOrd + Ord> {
    Op(Arc<VirtualRootUseOp<L>>),
    Frame(Arc<VirtualFrameUseOp<L>>, Arc<VirtualUseOp<L>>),
}

impl<L: Dupe + PartialEq + Eq + PartialOrd + Ord + std::hash::Hash> std::hash::Hash
    for VirtualUseOp<L>
{
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self).hash(state);
        match self {
            Self::Op(a) => a.hash(state),
            Self::Frame(a, b) => {
                a.hash(state);
                b.hash(state);
            }
        }
    }
}

impl<L: Dupe + PartialEq + Eq + PartialOrd + Ord> PartialEq for VirtualUseOp<L> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Op(a), Self::Op(b)) => Arc::ptr_eq(a, b) || a == b,
            (Self::Frame(a1, a2), Self::Frame(b1, b2)) => {
                (Arc::ptr_eq(a1, b1) || a1 == b1) && (Arc::ptr_eq(a2, b2) || a2 == b2)
            }
            _ => false,
        }
    }
}

impl<L: Dupe + PartialEq + Eq + PartialOrd + Ord> Eq for VirtualUseOp<L> {}

impl<L: Dupe + PartialEq + Eq + PartialOrd + Ord> PartialOrd for VirtualUseOp<L> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<L: Dupe + PartialEq + Eq + PartialOrd + Ord> Ord for VirtualUseOp<L> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match (self, other) {
            (Self::Op(a), Self::Op(b)) => {
                if Arc::ptr_eq(a, b) {
                    std::cmp::Ordering::Equal
                } else {
                    a.cmp(b)
                }
            }
            (Self::Op(_), Self::Frame(..)) => std::cmp::Ordering::Less,
            (Self::Frame(..), Self::Op(_)) => std::cmp::Ordering::Greater,
            (Self::Frame(a1, a2), Self::Frame(b1, b2)) => {
                let c = if Arc::ptr_eq(a1, b1) {
                    std::cmp::Ordering::Equal
                } else {
                    a1.cmp(b1)
                };
                match c {
                    std::cmp::Ordering::Equal => {
                        if Arc::ptr_eq(a2, b2) {
                            std::cmp::Ordering::Equal
                        } else {
                            a2.cmp(b2)
                        }
                    }
                    other => other,
                }
            }
        }
    }
}

impl<L> serde::Serialize for VirtualUseOp<L>
where
    L: Dupe + PartialEq + Eq + PartialOrd + Ord + serde::Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            Self::Op(op) => serializer.serialize_newtype_variant("VirtualUseOp", 0, "Op", &**op),
            Self::Frame(frame, use_op) => {
                #[derive(serde::Serialize)]
                struct FrameData<'a, L: Dupe + PartialEq + Eq + PartialOrd + Ord> {
                    frame: &'a VirtualFrameUseOp<L>,
                    use_op: &'a VirtualUseOp<L>,
                }

                serializer.serialize_newtype_variant(
                    "VirtualUseOp",
                    1,
                    "Frame",
                    &FrameData { frame, use_op },
                )
            }
        }
    }
}

impl<'de, L> serde::Deserialize<'de> for VirtualUseOp<L>
where
    L: Dupe + PartialEq + Eq + PartialOrd + Ord + serde::Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        struct FrameData<L: Dupe + PartialEq + Eq + PartialOrd + Ord> {
            frame: VirtualFrameUseOp<L>,
            use_op: VirtualUseOp<L>,
        }

        #[derive(serde::Deserialize)]
        enum VirtualUseOpSerde<L: Dupe + PartialEq + Eq + PartialOrd + Ord> {
            Op(VirtualRootUseOp<L>),
            Frame(FrameData<L>),
        }

        Ok(match VirtualUseOpSerde::deserialize(deserializer)? {
            VirtualUseOpSerde::Op(op) => Self::Op(Arc::new(op)),
            VirtualUseOpSerde::Frame(FrameData { frame, use_op }) => {
                Self::Frame(Arc::new(frame), Arc::new(use_op))
            }
        })
    }
}

pub type UseOp = VirtualUseOp<ALoc>;

pub type RootUseOp = VirtualRootUseOp<ALoc>;

pub type FrameUseOp = VirtualFrameUseOp<ALoc>;

pub struct PrivateMethodTData<CX: ContextFamily = ()> {
    pub use_op: UseOp,
    pub reason: Reason,
    pub prop_reason: Reason,
    pub name: FlowSmolStr,
    pub class_bindings: Rc<[ClassBinding]>,
    pub static_: bool,
    pub method_action: Box<MethodAction<CX>>,
}

impl<CX: ContextFamily> Clone for PrivateMethodTData<CX> {
    fn clone(&self) -> Self {
        PrivateMethodTData {
            use_op: self.use_op.clone(),
            reason: self.reason.clone(),
            prop_reason: self.prop_reason.clone(),
            name: self.name.clone(),
            class_bindings: self.class_bindings.clone(),
            static_: self.static_,
            method_action: self.method_action.clone(),
        }
    }
}

impl<CX: ContextFamily> PartialEq for PrivateMethodTData<CX> {
    fn eq(&self, other: &Self) -> bool {
        self.use_op == other.use_op
            && self.reason == other.reason
            && self.prop_reason == other.prop_reason
            && self.name == other.name
            && self.class_bindings == other.class_bindings
            && self.static_ == other.static_
            && self.method_action == other.method_action
    }
}

impl<CX: ContextFamily> Eq for PrivateMethodTData<CX> {}

impl<CX: ContextFamily> std::hash::Hash for PrivateMethodTData<CX> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.use_op.hash(state);
        self.reason.hash(state);
        self.prop_reason.hash(state);
        self.name.hash(state);
        self.class_bindings.hash(state);
        self.static_.hash(state);
        self.method_action.hash(state);
    }
}

impl<CX: ContextFamily> PartialOrd for PrivateMethodTData<CX> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<CX: ContextFamily> Ord for PrivateMethodTData<CX> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.use_op
            .cmp(&other.use_op)
            .then_with(|| self.reason.cmp(&other.reason))
            .then_with(|| self.prop_reason.cmp(&other.prop_reason))
            .then_with(|| self.name.cmp(&other.name))
            .then_with(|| self.class_bindings.cmp(&other.class_bindings))
            .then_with(|| self.static_.cmp(&other.static_))
            .then_with(|| self.method_action.cmp(&other.method_action))
    }
}

impl<CX: ContextFamily> std::fmt::Debug for PrivateMethodTData<CX> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PrivateMethodTData")
            .field("use_op", &self.use_op)
            .field("reason", &self.reason)
            .field("prop_reason", &self.prop_reason)
            .field("name", &self.name)
            .field("class_bindings", &self.class_bindings)
            .field("static_", &self.static_)
            .field("method_action", &self.method_action)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SetPrivatePropTData {
    pub use_op: UseOp,
    pub reason: Reason,
    pub name: FlowSmolStr,
    pub set_mode: SetMode,
    pub class_bindings: Rc<[ClassBinding]>,
    pub static_: bool,
    pub write_ctx: WriteCtx,
    pub tin: Type,
    pub tout: Option<Type>,
}

pub struct GetPropTData<CX: ContextFamily = ()> {
    pub use_op: UseOp,
    pub reason: Reason,
    pub id: Option<i32>,
    pub from_annot: bool,
    pub skip_optional: bool,
    pub propref: Box<PropRef>,
    pub tout: Box<Tvar>,
    pub hint: LazyHintT<CX>,
}

impl<CX: ContextFamily> Clone for GetPropTData<CX> {
    fn clone(&self) -> Self {
        GetPropTData {
            use_op: self.use_op.clone(),
            reason: self.reason.clone(),
            id: self.id,
            from_annot: self.from_annot,
            skip_optional: self.skip_optional,
            propref: self.propref.clone(),
            tout: self.tout.clone(),
            hint: self.hint.clone(),
        }
    }
}

impl<CX: ContextFamily> PartialEq for GetPropTData<CX> {
    fn eq(&self, other: &Self) -> bool {
        self.use_op == other.use_op
            && self.reason == other.reason
            && self.id == other.id
            && self.from_annot == other.from_annot
            && self.skip_optional == other.skip_optional
            && self.propref == other.propref
            && self.tout == other.tout
            && self.hint == other.hint
    }
}

impl<CX: ContextFamily> Eq for GetPropTData<CX> {}

impl<CX: ContextFamily> std::hash::Hash for GetPropTData<CX> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.use_op.hash(state);
        self.reason.hash(state);
        self.id.hash(state);
        self.from_annot.hash(state);
        self.skip_optional.hash(state);
        self.propref.hash(state);
        self.tout.hash(state);
        self.hint.hash(state);
    }
}

impl<CX: ContextFamily> PartialOrd for GetPropTData<CX> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<CX: ContextFamily> Ord for GetPropTData<CX> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.use_op
            .cmp(&other.use_op)
            .then_with(|| self.reason.cmp(&other.reason))
            .then_with(|| self.id.cmp(&other.id))
            .then_with(|| self.from_annot.cmp(&other.from_annot))
            .then_with(|| self.skip_optional.cmp(&other.skip_optional))
            .then_with(|| self.propref.cmp(&other.propref))
            .then_with(|| self.tout.cmp(&other.tout))
            .then_with(|| self.hint.cmp(&other.hint))
    }
}

impl<CX: ContextFamily> std::fmt::Debug for GetPropTData<CX> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GetPropTData")
            .field("use_op", &self.use_op)
            .field("reason", &self.reason)
            .field("id", &self.id)
            .field("from_annot", &self.from_annot)
            .field("skip_optional", &self.skip_optional)
            .field("propref", &self.propref)
            .field("tout", &self.tout)
            .field("hint", &self.hint)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GetPrivatePropTData {
    pub use_op: UseOp,
    pub reason: Reason,
    pub name: FlowSmolStr,
    pub class_bindings: Rc<[ClassBinding]>,
    pub static_: bool,
    pub tout: Box<Tvar>,
}

pub struct ConstructorTData<CX: ContextFamily = ()> {
    pub use_op: UseOp,
    pub reason: Reason,
    pub targs: Option<Rc<[Targ]>>,
    pub args: Rc<[CallArg]>,
    pub tout: Type,
    pub return_hint: LazyHintT<CX>,
    pub specialized_ctor: Option<SpecializedCallee>,
}

impl<CX: ContextFamily> Clone for ConstructorTData<CX> {
    fn clone(&self) -> Self {
        ConstructorTData {
            use_op: self.use_op.clone(),
            reason: self.reason.clone(),
            targs: self.targs.clone(),
            args: self.args.clone(),
            tout: self.tout.clone(),
            return_hint: self.return_hint.clone(),
            specialized_ctor: self.specialized_ctor.clone(),
        }
    }
}

impl<CX: ContextFamily> PartialEq for ConstructorTData<CX> {
    fn eq(&self, other: &Self) -> bool {
        self.use_op == other.use_op
            && self.reason == other.reason
            && self.targs == other.targs
            && self.args == other.args
            && self.tout == other.tout
            && self.return_hint == other.return_hint
            && self.specialized_ctor == other.specialized_ctor
    }
}

impl<CX: ContextFamily> Eq for ConstructorTData<CX> {}

impl<CX: ContextFamily> std::hash::Hash for ConstructorTData<CX> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.use_op.hash(state);
        self.reason.hash(state);
        self.targs.hash(state);
        self.args.hash(state);
        self.tout.hash(state);
        self.return_hint.hash(state);
        self.specialized_ctor.hash(state);
    }
}

impl<CX: ContextFamily> PartialOrd for ConstructorTData<CX> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<CX: ContextFamily> Ord for ConstructorTData<CX> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.use_op
            .cmp(&other.use_op)
            .then_with(|| self.reason.cmp(&other.reason))
            .then_with(|| self.targs.cmp(&other.targs))
            .then_with(|| self.args.cmp(&other.args))
            .then_with(|| self.tout.cmp(&other.tout))
            .then_with(|| self.return_hint.cmp(&other.return_hint))
            .then_with(|| self.specialized_ctor.cmp(&other.specialized_ctor))
    }
}

impl<CX: ContextFamily> std::fmt::Debug for ConstructorTData<CX> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConstructorTData")
            .field("use_op", &self.use_op)
            .field("reason", &self.reason)
            .field("targs", &self.targs)
            .field("args", &self.args)
            .field("tout", &self.tout)
            .field("return_hint", &self.return_hint)
            .field("specialized_ctor", &self.specialized_ctor)
            .finish()
    }
}

// Data structs for boxed UseTInner variants
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ConditionalTData {
    pub use_op: UseOp,
    pub reason: Reason,
    pub distributive_tparam_name: Option<SubstName>,
    pub infer_tparams: Rc<[TypeParam]>,
    pub extends_t: Type,
    pub true_t: Type,
    pub false_t: Type,
    pub tout: Box<Tvar>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SetElemTData {
    pub use_op: UseOp,
    pub reason: Reason,
    pub key_t: Type,
    pub set_mode: SetMode,
    pub tin: Type,
    pub tout: Option<Type>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GetElemTData {
    pub use_op: UseOp,
    pub reason: Reason,
    pub id: Option<i32>,
    pub from_annot: bool,
    pub skip_optional: bool,
    pub access_iterables: bool,
    pub key_t: Type,
    pub tout: Box<Tvar>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ReposUseTData {
    pub reason: Reason,
    pub use_desc: bool,
    pub use_op: UseOp,
    pub type_: Type,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SuperTData {
    pub use_op: UseOp,
    pub reason: Reason,
    pub derived_type: DerivedType,
}

pub struct SpecializeTData<CX: ContextFamily> {
    pub use_op: UseOp,
    pub reason: Reason,
    pub reason2: Reason,
    pub targs: Option<Rc<[Type]>>,
    pub upper: Option<Box<Cont<CX>>>,
}

impl<CX: ContextFamily> Clone for SpecializeTData<CX> {
    fn clone(&self) -> Self {
        Self {
            use_op: self.use_op.dupe(),
            reason: self.reason.dupe(),
            reason2: self.reason2.dupe(),
            targs: self.targs.dupe(),
            upper: self.upper.clone(),
        }
    }
}

impl<CX: ContextFamily> PartialEq for SpecializeTData<CX> {
    fn eq(&self, other: &Self) -> bool {
        self.use_op == other.use_op
            && self.reason == other.reason
            && self.reason2 == other.reason2
            && self.targs == other.targs
            && self.upper == other.upper
    }
}

impl<CX: ContextFamily> Eq for SpecializeTData<CX> {}

impl<CX: ContextFamily> Hash for SpecializeTData<CX> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.use_op.hash(state);
        self.reason.hash(state);
        self.reason2.hash(state);
        self.targs.hash(state);
        self.upper.hash(state);
    }
}

impl<CX: ContextFamily> PartialOrd for SpecializeTData<CX> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<CX: ContextFamily> Ord for SpecializeTData<CX> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.use_op
            .cmp(&other.use_op)
            .then_with(|| self.reason.cmp(&other.reason))
            .then_with(|| self.reason2.cmp(&other.reason2))
            .then_with(|| self.targs.cmp(&other.targs))
            .then_with(|| self.upper.cmp(&other.upper))
    }
}

impl<CX: ContextFamily> std::fmt::Debug for SpecializeTData<CX> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SpecializeTData")
            .field("use_op", &self.use_op)
            .field("reason", &self.reason)
            .field("reason2", &self.reason2)
            .field("targs", &self.targs)
            .field("upper", &self.upper)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ValueToTypeReferenceTData {
    pub use_op: UseOp,
    pub reason: Reason,
    pub kind: TypeTKind,
    pub tout: Box<Tvar>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LookupTData {
    pub reason: Reason,
    pub lookup_kind: Box<LookupKind>,
    /// An indexer property that already matched this access, held back until the
    /// prototype chain is exhausted so that an inherited declared property wins over
    /// the indexer. Travels alongside `lookup_kind`, which keeps describing what should
    /// happen if nothing at all is found.
    pub indexer_fallback: Option<Box<IndexerFallbackData>>,
    pub try_ts_on_failure: Rc<[Type]>,
    pub propref: Box<PropRef>,
    pub lookup_action: Box<LookupAction>,
    pub ids: Option<properties::Set>,
    pub method_accessible: bool,
    pub ignore_dicts: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ArrRestTData {
    pub literal: bool,
    pub use_op: UseOp,
    pub reason: Reason,
    pub index: i32,
    pub tout: Type,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct HasOwnPropTData {
    pub use_op: UseOp,
    pub reason: Reason,
    pub type_: Type,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MapTypeTData {
    pub use_op: UseOp,
    pub reason: Reason,
    pub type_map: TypeMap,
    pub tout: Type,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ConcretizeTData {
    pub reason: Reason,
    pub kind: ConcretizationKind,
    pub seen: concretize_seen::ConcretizeSeen,
    pub collector: type_collector::TypeCollector,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ResolveSpreadTData {
    pub use_op: UseOp,
    pub reason: Reason,
    pub resolve_spread_type: Box<ResolveSpreadType>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CondTData {
    pub loc: ALoc,
    pub opt_type: Option<Type>,
    pub true_t: Type,
    pub collector: type_collector::TypeCollector,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ExtendsUseTData {
    pub use_op: UseOp,
    pub reason: Reason,
    pub targs: Rc<[Type]>,
    pub true_t: Type,
    pub false_t: Type,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GetEnumTData {
    pub use_op: UseOp,
    pub loc: ALoc,
    pub orig_t: Option<Type>,
    pub kind: GetEnumKind,
    pub collector: type_collector::TypeCollector,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OptionalIndexedAccessTData {
    pub use_op: UseOp,
    pub reason: Reason,
    pub index: OptionalIndexedAccessIndex,
    pub tout_tvar: Box<Tvar>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EvalTypeDestructorTData {
    pub destructor_use_op: UseOp,
    pub reason: Reason,
    pub repos: Option<(Reason, bool)>,
    pub destructor: Box<Destructor>,
    pub tout: Box<Tvar>,
}

// Data structs for boxed UseTInner variants (with CX parameter - manual impls)

pub struct CallTData<CX: ContextFamily = ()> {
    pub use_op: UseOp,
    pub reason: Reason,
    pub call_action: Box<CallAction>,
    pub return_hint: LazyHintT<CX>,
}

impl<CX: ContextFamily> Clone for CallTData<CX> {
    fn clone(&self) -> Self {
        CallTData {
            use_op: self.use_op.clone(),
            reason: self.reason.clone(),
            call_action: self.call_action.clone(),
            return_hint: self.return_hint.clone(),
        }
    }
}

impl<CX: ContextFamily> PartialEq for CallTData<CX> {
    fn eq(&self, other: &Self) -> bool {
        self.use_op == other.use_op
            && self.reason == other.reason
            && self.call_action == other.call_action
            && self.return_hint == other.return_hint
    }
}

impl<CX: ContextFamily> Eq for CallTData<CX> {}

impl<CX: ContextFamily> std::hash::Hash for CallTData<CX> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.use_op.hash(state);
        self.reason.hash(state);
        self.call_action.hash(state);
        self.return_hint.hash(state);
    }
}

impl<CX: ContextFamily> PartialOrd for CallTData<CX> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<CX: ContextFamily> Ord for CallTData<CX> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.use_op
            .cmp(&other.use_op)
            .then_with(|| self.reason.cmp(&other.reason))
            .then_with(|| self.call_action.cmp(&other.call_action))
            .then_with(|| self.return_hint.cmp(&other.return_hint))
    }
}

impl<CX: ContextFamily> std::fmt::Debug for CallTData<CX> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CallTData")
            .field("use_op", &self.use_op)
            .field("reason", &self.reason)
            .field("call_action", &self.call_action)
            .field("return_hint", &self.return_hint)
            .finish()
    }
}

pub struct TestPropTData<CX: ContextFamily = ()> {
    pub use_op: UseOp,
    pub reason: Reason,
    pub id: i32,
    pub propref: Box<PropRef>,
    pub tout: Box<Tvar>,
    pub hint: LazyHintT<CX>,
}

impl<CX: ContextFamily> Clone for TestPropTData<CX> {
    fn clone(&self) -> Self {
        TestPropTData {
            use_op: self.use_op.clone(),
            reason: self.reason.clone(),
            id: self.id,
            propref: self.propref.clone(),
            tout: self.tout.clone(),
            hint: self.hint.clone(),
        }
    }
}

impl<CX: ContextFamily> PartialEq for TestPropTData<CX> {
    fn eq(&self, other: &Self) -> bool {
        self.use_op == other.use_op
            && self.reason == other.reason
            && self.id == other.id
            && self.propref == other.propref
            && self.tout == other.tout
            && self.hint == other.hint
    }
}

impl<CX: ContextFamily> Eq for TestPropTData<CX> {}

impl<CX: ContextFamily> std::hash::Hash for TestPropTData<CX> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.use_op.hash(state);
        self.reason.hash(state);
        self.id.hash(state);
        self.propref.hash(state);
        self.tout.hash(state);
        self.hint.hash(state);
    }
}

impl<CX: ContextFamily> PartialOrd for TestPropTData<CX> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<CX: ContextFamily> Ord for TestPropTData<CX> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.use_op
            .cmp(&other.use_op)
            .then_with(|| self.reason.cmp(&other.reason))
            .then_with(|| self.id.cmp(&other.id))
            .then_with(|| self.propref.cmp(&other.propref))
            .then_with(|| self.tout.cmp(&other.tout))
            .then_with(|| self.hint.cmp(&other.hint))
    }
}

impl<CX: ContextFamily> std::fmt::Debug for TestPropTData<CX> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TestPropTData")
            .field("use_op", &self.use_op)
            .field("reason", &self.reason)
            .field("id", &self.id)
            .field("propref", &self.propref)
            .field("tout", &self.tout)
            .field("hint", &self.hint)
            .finish()
    }
}

pub struct MethodTData<CX: ContextFamily = ()> {
    pub use_op: UseOp,
    pub reason: Reason,
    pub prop_reason: Reason,
    pub propref: Box<PropRef>,
    pub method_action: Box<MethodAction<CX>>,
}

impl<CX: ContextFamily> Clone for MethodTData<CX> {
    fn clone(&self) -> Self {
        MethodTData {
            use_op: self.use_op.clone(),
            reason: self.reason.clone(),
            prop_reason: self.prop_reason.clone(),
            propref: self.propref.clone(),
            method_action: self.method_action.clone(),
        }
    }
}

impl<CX: ContextFamily> PartialEq for MethodTData<CX> {
    fn eq(&self, other: &Self) -> bool {
        self.use_op == other.use_op
            && self.reason == other.reason
            && self.prop_reason == other.prop_reason
            && self.propref == other.propref
            && self.method_action == other.method_action
    }
}

impl<CX: ContextFamily> Eq for MethodTData<CX> {}

impl<CX: ContextFamily> std::hash::Hash for MethodTData<CX> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.use_op.hash(state);
        self.reason.hash(state);
        self.prop_reason.hash(state);
        self.propref.hash(state);
        self.method_action.hash(state);
    }
}

impl<CX: ContextFamily> PartialOrd for MethodTData<CX> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<CX: ContextFamily> Ord for MethodTData<CX> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.use_op
            .cmp(&other.use_op)
            .then_with(|| self.reason.cmp(&other.reason))
            .then_with(|| self.prop_reason.cmp(&other.prop_reason))
            .then_with(|| self.propref.cmp(&other.propref))
            .then_with(|| self.method_action.cmp(&other.method_action))
    }
}

impl<CX: ContextFamily> std::fmt::Debug for MethodTData<CX> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MethodTData")
            .field("use_op", &self.use_op)
            .field("reason", &self.reason)
            .field("prop_reason", &self.prop_reason)
            .field("propref", &self.propref)
            .field("method_action", &self.method_action)
            .finish()
    }
}

pub struct CallElemTData<CX: ContextFamily = ()> {
    pub use_op: UseOp,
    pub reason: Reason,
    pub prop_reason: Reason,
    pub key_t: Type,
    pub method_action: Box<MethodAction<CX>>,
}

impl<CX: ContextFamily> Clone for CallElemTData<CX> {
    fn clone(&self) -> Self {
        CallElemTData {
            use_op: self.use_op.clone(),
            reason: self.reason.clone(),
            prop_reason: self.prop_reason.clone(),
            key_t: self.key_t.clone(),
            method_action: self.method_action.clone(),
        }
    }
}

impl<CX: ContextFamily> PartialEq for CallElemTData<CX> {
    fn eq(&self, other: &Self) -> bool {
        self.use_op == other.use_op
            && self.reason == other.reason
            && self.prop_reason == other.prop_reason
            && self.key_t == other.key_t
            && self.method_action == other.method_action
    }
}

impl<CX: ContextFamily> Eq for CallElemTData<CX> {}

impl<CX: ContextFamily> std::hash::Hash for CallElemTData<CX> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.use_op.hash(state);
        self.reason.hash(state);
        self.prop_reason.hash(state);
        self.key_t.hash(state);
        self.method_action.hash(state);
    }
}

impl<CX: ContextFamily> PartialOrd for CallElemTData<CX> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<CX: ContextFamily> Ord for CallElemTData<CX> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.use_op
            .cmp(&other.use_op)
            .then_with(|| self.reason.cmp(&other.reason))
            .then_with(|| self.prop_reason.cmp(&other.prop_reason))
            .then_with(|| self.key_t.cmp(&other.key_t))
            .then_with(|| self.method_action.cmp(&other.method_action))
    }
}

impl<CX: ContextFamily> std::fmt::Debug for CallElemTData<CX> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CallElemTData")
            .field("use_op", &self.use_op)
            .field("reason", &self.reason)
            .field("prop_reason", &self.prop_reason)
            .field("key_t", &self.key_t)
            .field("method_action", &self.method_action)
            .finish()
    }
}

pub struct ElemTData<CX: ContextFamily = ()> {
    pub use_op: UseOp,
    pub reason: Reason,
    pub obj: Type,
    pub action: Box<ElemAction<CX>>,
}

impl<CX: ContextFamily> Clone for ElemTData<CX> {
    fn clone(&self) -> Self {
        ElemTData {
            use_op: self.use_op.clone(),
            reason: self.reason.clone(),
            obj: self.obj.clone(),
            action: self.action.clone(),
        }
    }
}

impl<CX: ContextFamily> PartialEq for ElemTData<CX> {
    fn eq(&self, other: &Self) -> bool {
        self.use_op == other.use_op
            && self.reason == other.reason
            && self.obj == other.obj
            && self.action == other.action
    }
}

impl<CX: ContextFamily> Eq for ElemTData<CX> {}

impl<CX: ContextFamily> std::hash::Hash for ElemTData<CX> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.use_op.hash(state);
        self.reason.hash(state);
        self.obj.hash(state);
        self.action.hash(state);
    }
}

impl<CX: ContextFamily> PartialOrd for ElemTData<CX> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<CX: ContextFamily> Ord for ElemTData<CX> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.use_op
            .cmp(&other.use_op)
            .then_with(|| self.reason.cmp(&other.reason))
            .then_with(|| self.obj.cmp(&other.obj))
            .then_with(|| self.action.cmp(&other.action))
    }
}

impl<CX: ContextFamily> std::fmt::Debug for ElemTData<CX> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ElemTData")
            .field("use_op", &self.use_op)
            .field("reason", &self.reason)
            .field("obj", &self.obj)
            .field("action", &self.action)
            .finish()
    }
}

pub struct ReactKitTData<CX: ContextFamily = ()> {
    pub use_op: UseOp,
    pub reason: Reason,
    pub tool: Box<react::Tool<CX>>,
}

impl<CX: ContextFamily> Clone for ReactKitTData<CX> {
    fn clone(&self) -> Self {
        ReactKitTData {
            use_op: self.use_op.clone(),
            reason: self.reason.clone(),
            tool: self.tool.clone(),
        }
    }
}

impl<CX: ContextFamily> PartialEq for ReactKitTData<CX> {
    fn eq(&self, other: &Self) -> bool {
        self.use_op == other.use_op && self.reason == other.reason && self.tool == other.tool
    }
}

impl<CX: ContextFamily> Eq for ReactKitTData<CX> {}

impl<CX: ContextFamily> std::hash::Hash for ReactKitTData<CX> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.use_op.hash(state);
        self.reason.hash(state);
        self.tool.hash(state);
    }
}

impl<CX: ContextFamily> PartialOrd for ReactKitTData<CX> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<CX: ContextFamily> Ord for ReactKitTData<CX> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.use_op
            .cmp(&other.use_op)
            .then_with(|| self.reason.cmp(&other.reason))
            .then_with(|| self.tool.cmp(&other.tool))
    }
}

impl<CX: ContextFamily> std::fmt::Debug for ReactKitTData<CX> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReactKitTData")
            .field("use_op", &self.use_op)
            .field("reason", &self.reason)
            .field("tool", &self.tool)
            .finish()
    }
}

pub struct SealGenericTData<CX: ContextFamily = ()> {
    pub loc: ALoc,
    pub id: flow_typing_generics::GenericId,
    pub name: SubstName,
    pub no_infer: bool,
    pub cont: Cont<CX>,
}

impl<CX: ContextFamily> Clone for SealGenericTData<CX> {
    fn clone(&self) -> Self {
        SealGenericTData {
            loc: self.loc.clone(),
            id: self.id.clone(),
            name: self.name.clone(),
            no_infer: self.no_infer,
            cont: self.cont.clone(),
        }
    }
}

impl<CX: ContextFamily> PartialEq for SealGenericTData<CX> {
    fn eq(&self, other: &Self) -> bool {
        self.loc == other.loc
            && self.id == other.id
            && self.name == other.name
            && self.no_infer == other.no_infer
            && self.cont == other.cont
    }
}

impl<CX: ContextFamily> Eq for SealGenericTData<CX> {}

impl<CX: ContextFamily> std::hash::Hash for SealGenericTData<CX> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.loc.hash(state);
        self.id.hash(state);
        self.name.hash(state);
        self.no_infer.hash(state);
        self.cont.hash(state);
    }
}

impl<CX: ContextFamily> PartialOrd for SealGenericTData<CX> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<CX: ContextFamily> Ord for SealGenericTData<CX> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.loc
            .cmp(&other.loc)
            .then_with(|| self.id.cmp(&other.id))
            .then_with(|| self.name.cmp(&other.name))
            .then_with(|| self.no_infer.cmp(&other.no_infer))
            .then_with(|| self.cont.cmp(&other.cont))
    }
}

impl<CX: ContextFamily> std::fmt::Debug for SealGenericTData<CX> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SealGenericTData")
            .field("loc", &self.loc)
            .field("id", &self.id)
            .field("name", &self.name)
            .field("no_infer", &self.no_infer)
            .field("cont", &self.cont)
            .finish()
    }
}

pub struct ResolveUnionTData<CX: ContextFamily = ()> {
    pub reason: Reason,
    pub unresolved: flow_data_structure_wrapper::list::FlowOcamlList<Type>,
    pub resolved: flow_data_structure_wrapper::list::FlowOcamlList<Type>,
    pub upper: Box<UseT<CX>>,
    pub id: i32,
}

impl<CX: ContextFamily> Clone for ResolveUnionTData<CX> {
    fn clone(&self) -> Self {
        ResolveUnionTData {
            reason: self.reason.clone(),
            unresolved: self.unresolved.clone(),
            resolved: self.resolved.clone(),
            upper: self.upper.clone(),
            id: self.id,
        }
    }
}

impl<CX: ContextFamily> PartialEq for ResolveUnionTData<CX> {
    fn eq(&self, other: &Self) -> bool {
        self.reason == other.reason
            && self.unresolved == other.unresolved
            && self.resolved == other.resolved
            && self.upper == other.upper
            && self.id == other.id
    }
}

impl<CX: ContextFamily> Eq for ResolveUnionTData<CX> {}

impl<CX: ContextFamily> std::hash::Hash for ResolveUnionTData<CX> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.reason.hash(state);
        self.unresolved.hash(state);
        self.resolved.hash(state);
        self.upper.hash(state);
        self.id.hash(state);
    }
}

impl<CX: ContextFamily> PartialOrd for ResolveUnionTData<CX> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<CX: ContextFamily> Ord for ResolveUnionTData<CX> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.reason
            .cmp(&other.reason)
            .then_with(|| self.unresolved.cmp(&other.unresolved))
            .then_with(|| self.resolved.cmp(&other.resolved))
            .then_with(|| self.upper.cmp(&other.upper))
            .then_with(|| self.id.cmp(&other.id))
    }
}

impl<CX: ContextFamily> std::fmt::Debug for ResolveUnionTData<CX> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResolveUnionTData")
            .field("reason", &self.reason)
            .field("unresolved", &self.unresolved)
            .field("resolved", &self.resolved)
            .field("upper", &self.upper)
            .field("id", &self.id)
            .finish()
    }
}

pub enum UseTInner<CX: ContextFamily = ()> {
    /// Use a definition type as an upper bound
    UseT(UseOp, Type),

    // Operations on runtime values
    CallT(Box<CallTData<CX>>),
    ConditionalT(Box<ConditionalTData>),
    MethodT(Box<MethodTData<CX>>),
    PrivateMethodT(Box<PrivateMethodTData<CX>>),
    SetPropT(
        UseOp,
        Reason,
        Box<PropRef>,
        SetMode,
        WriteCtx,
        Type,
        Option<Type>,
    ),
    SetPrivatePropT(Box<SetPrivatePropTData>),
    GetPropT(Box<GetPropTData<CX>>),
    GetPrivatePropT(Box<GetPrivatePropTData>),
    TestPropT(Box<TestPropTData<CX>>),
    SetElemT(Box<SetElemTData>),
    GetElemT(Box<GetElemTData>),
    CallElemT(Box<CallElemTData<CX>>),
    GetStaticsT(Reason, type_collector::TypeCollector),
    GetProtoT(Reason, type_collector::TypeCollector),

    // Repositioning
    ReposLowerT {
        reason: Reason,
        use_desc: bool,
        use_t: Box<UseT<CX>>,
    },
    ReposUseT(Box<ReposUseTData>),

    // Operations on runtime types
    ConstructorT(Box<ConstructorTData<CX>>),
    SuperT(Box<SuperTData>),
    ImplementsT(UseOp, Type),
    MixinT(Reason, type_collector::TypeCollector),
    ToStringT {
        orig_t: Option<Type>,
        reason: Reason,
        t_out: Box<UseT<CX>>,
    },

    // Operation on polymorphic types
    SpecializeT(Box<SpecializeTData<CX>>),
    ThisSpecializeT(Reason, Type, Box<Cont<CX>>),
    ValueToTypeReferenceT(Box<ValueToTypeReferenceTData>),
    ConcretizeTypeAppsT(
        UseOp,
        Box<(Rc<[Type]>, bool, UseOp, Reason)>,
        Box<(Type, Rc<[Type]>, bool, UseOp, Reason)>,
        bool,
    ),

    // Operation on prototypes
    LookupT(Box<LookupTData>),

    // Operations on objects
    // The bool is whether the rest object is a literal.
    ObjRestT(Reason, Rc<[String]>, Type, i32, bool),
    ObjTestProtoT(Reason, type_collector::TypeCollector),
    ArrRestT(Box<ArrRestTData>),
    GetKeysT {
        reason: Reason,
        t_out: Box<UseT<CX>>,
        // Whether symbol keys are included: type-level `keyof` includes
        // them, runtime enumeration APIs do not.
        include_symbols: bool,
    },
    // The indexer half of `GetKeysT`: contributes an index signature's key to
    // the key set. A symbol-valued key contributes as that same symbol type and
    // is left out of runtime enumeration; every other key is stringified.
    GetKeysDictKeyT {
        reason: Reason,
        t_out: Box<UseT<CX>>,
        include_symbols: bool,
    },
    HasOwnPropT(Box<HasOwnPropTData>),
    GetValuesT(Reason, Type),
    GetDictValuesT(Reason, Box<UseT<CX>>),
    ElemT(Box<ElemTData<CX>>),
    MapTypeT(Box<MapTypeTData>),
    ObjKitT(
        UseOp,
        Reason,
        Box<object::ResolveTool>,
        Box<object::Tool>,
        Type,
    ),
    ReactKitT(Box<ReactKitTData<CX>>),

    // Tools for preprocessing types
    ConcretizeT(Box<ConcretizeTData>),
    ResolveSpreadT(Box<ResolveSpreadTData>),
    CondT(Box<CondTData>),
    ExtendsUseT(Box<ExtendsUseTData>),
    ResolveUnionT(Box<ResolveUnionTData<CX>>),
    GetEnumT(Box<GetEnumTData>),
    FilterOptionalT(UseOp, Type),
    FilterMaybeT(UseOp, Type),
    DeepReadOnlyT(Box<Tvar>, ReactDro),
    HooklikeT(Box<Tvar>),
    SealGenericT(Box<SealGenericTData<CX>>),
    OptionalIndexedAccessT(Box<OptionalIndexedAccessTData>),
    ConvertEmptyPropsToMixedT(ALoc, type_collector::TypeCollector),
    ExitRendersT {
        renders_reason: Reason,
        u: Box<UseT<CX>>,
    },
    EvalTypeDestructorT(Box<EvalTypeDestructorTData>),
}

impl<CX: ContextFamily> Clone for UseTInner<CX> {
    fn clone(&self) -> Self {
        match self {
            UseTInner::UseT(a, b) => UseTInner::UseT(a.clone(), b.clone()),
            UseTInner::CallT(a) => UseTInner::CallT(a.clone()),
            UseTInner::ConditionalT(a) => UseTInner::ConditionalT(a.clone()),
            UseTInner::MethodT(a) => UseTInner::MethodT(a.clone()),
            UseTInner::PrivateMethodT(a) => UseTInner::PrivateMethodT(a.clone()),
            UseTInner::SetPropT(a, b, c, d, e, f, g) => UseTInner::SetPropT(
                a.clone(),
                b.clone(),
                c.clone(),
                d.clone(),
                e.clone(),
                f.clone(),
                g.clone(),
            ),
            UseTInner::SetPrivatePropT(a) => UseTInner::SetPrivatePropT(a.clone()),
            UseTInner::GetPropT(a) => UseTInner::GetPropT(a.clone()),
            UseTInner::GetPrivatePropT(a) => UseTInner::GetPrivatePropT(a.clone()),
            UseTInner::TestPropT(a) => UseTInner::TestPropT(a.clone()),
            UseTInner::SetElemT(a) => UseTInner::SetElemT(a.clone()),
            UseTInner::GetElemT(a) => UseTInner::GetElemT(a.clone()),
            UseTInner::CallElemT(a) => UseTInner::CallElemT(a.clone()),
            UseTInner::GetStaticsT(a, b) => UseTInner::GetStaticsT(a.dupe(), b.dupe()),
            UseTInner::GetProtoT(a, b) => UseTInner::GetProtoT(a.clone(), b.clone()),
            UseTInner::ReposLowerT {
                reason,
                use_desc,
                use_t,
            } => UseTInner::ReposLowerT {
                reason: reason.clone(),
                use_desc: *use_desc,
                use_t: use_t.clone(),
            },
            UseTInner::ReposUseT(a) => UseTInner::ReposUseT(a.clone()),
            UseTInner::ConstructorT(a) => UseTInner::ConstructorT(a.clone()),
            UseTInner::SuperT(a) => UseTInner::SuperT(a.clone()),
            UseTInner::ImplementsT(a, b) => UseTInner::ImplementsT(a.clone(), b.clone()),
            UseTInner::MixinT(a, b) => UseTInner::MixinT(a.clone(), b.clone()),
            UseTInner::ToStringT {
                orig_t,
                reason,
                t_out,
            } => UseTInner::ToStringT {
                orig_t: orig_t.clone(),
                reason: reason.clone(),
                t_out: t_out.clone(),
            },
            UseTInner::SpecializeT(a) => UseTInner::SpecializeT(a.clone()),
            UseTInner::ThisSpecializeT(a, b, c) => {
                UseTInner::ThisSpecializeT(a.clone(), b.clone(), c.clone())
            }
            UseTInner::ValueToTypeReferenceT(a) => UseTInner::ValueToTypeReferenceT(a.clone()),
            UseTInner::ConcretizeTypeAppsT(a, b, c, d) => {
                UseTInner::ConcretizeTypeAppsT(a.clone(), b.clone(), c.clone(), *d)
            }
            UseTInner::LookupT(a) => UseTInner::LookupT(a.clone()),
            UseTInner::ObjRestT(a, b, c, d, e) => {
                UseTInner::ObjRestT(a.clone(), b.clone(), c.clone(), *d, *e)
            }
            UseTInner::ObjTestProtoT(a, b) => UseTInner::ObjTestProtoT(a.clone(), b.clone()),
            UseTInner::ArrRestT(a) => UseTInner::ArrRestT(a.clone()),
            UseTInner::GetKeysT {
                reason,
                t_out,
                include_symbols,
            } => UseTInner::GetKeysT {
                reason: reason.clone(),
                t_out: t_out.clone(),
                include_symbols: *include_symbols,
            },
            UseTInner::GetKeysDictKeyT {
                reason,
                t_out,
                include_symbols,
            } => UseTInner::GetKeysDictKeyT {
                reason: reason.clone(),
                t_out: t_out.clone(),
                include_symbols: *include_symbols,
            },
            UseTInner::HasOwnPropT(a) => UseTInner::HasOwnPropT(a.clone()),
            UseTInner::GetValuesT(a, b) => UseTInner::GetValuesT(a.clone(), b.clone()),
            UseTInner::GetDictValuesT(a, b) => UseTInner::GetDictValuesT(a.clone(), b.clone()),
            UseTInner::ElemT(a) => UseTInner::ElemT(a.clone()),
            UseTInner::MapTypeT(a) => UseTInner::MapTypeT(a.clone()),
            UseTInner::ObjKitT(a, b, c, d, e) => {
                UseTInner::ObjKitT(a.clone(), b.clone(), c.clone(), d.clone(), e.clone())
            }
            UseTInner::ReactKitT(a) => UseTInner::ReactKitT(a.clone()),
            UseTInner::ConcretizeT(a) => UseTInner::ConcretizeT(a.clone()),
            UseTInner::ResolveSpreadT(a) => UseTInner::ResolveSpreadT(a.clone()),
            UseTInner::CondT(a) => UseTInner::CondT(a.clone()),
            UseTInner::ExtendsUseT(a) => UseTInner::ExtendsUseT(a.clone()),
            UseTInner::ResolveUnionT(a) => UseTInner::ResolveUnionT(a.clone()),
            UseTInner::GetEnumT(a) => UseTInner::GetEnumT(a.clone()),
            UseTInner::FilterOptionalT(a, b) => UseTInner::FilterOptionalT(a.clone(), b.clone()),
            UseTInner::FilterMaybeT(a, b) => UseTInner::FilterMaybeT(a.clone(), b.clone()),
            UseTInner::DeepReadOnlyT(a, b) => UseTInner::DeepReadOnlyT(a.clone(), b.clone()),
            UseTInner::HooklikeT(a) => UseTInner::HooklikeT(a.clone()),
            UseTInner::SealGenericT(a) => UseTInner::SealGenericT(a.clone()),
            UseTInner::OptionalIndexedAccessT(a) => UseTInner::OptionalIndexedAccessT(a.clone()),
            UseTInner::ConvertEmptyPropsToMixedT(a, b) => {
                UseTInner::ConvertEmptyPropsToMixedT(a.clone(), b.clone())
            }
            UseTInner::ExitRendersT { renders_reason, u } => UseTInner::ExitRendersT {
                renders_reason: renders_reason.clone(),
                u: u.clone(),
            },
            UseTInner::EvalTypeDestructorT(a) => UseTInner::EvalTypeDestructorT(a.clone()),
        }
    }
}

impl<CX: ContextFamily> PartialEq for UseTInner<CX> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (UseTInner::UseT(a1, b1), UseTInner::UseT(a2, b2)) => a1 == a2 && b1 == b2,
            (UseTInner::CallT(a1), UseTInner::CallT(a2)) => a1 == a2,
            (UseTInner::ConditionalT(a1), UseTInner::ConditionalT(a2)) => a1 == a2,
            (UseTInner::MethodT(a1), UseTInner::MethodT(a2)) => a1 == a2,
            (UseTInner::PrivateMethodT(a1), UseTInner::PrivateMethodT(a2)) => a1 == a2,
            (
                UseTInner::SetPropT(a1, b1, c1, d1, e1, f1, g1),
                UseTInner::SetPropT(a2, b2, c2, d2, e2, f2, g2),
            ) => a1 == a2 && b1 == b2 && c1 == c2 && d1 == d2 && e1 == e2 && f1 == f2 && g1 == g2,
            (UseTInner::SetPrivatePropT(a1), UseTInner::SetPrivatePropT(a2)) => a1 == a2,
            (UseTInner::GetPropT(a1), UseTInner::GetPropT(a2)) => a1 == a2,
            (UseTInner::GetPrivatePropT(a1), UseTInner::GetPrivatePropT(a2)) => a1 == a2,
            (UseTInner::TestPropT(a1), UseTInner::TestPropT(a2)) => a1 == a2,
            (UseTInner::SetElemT(a1), UseTInner::SetElemT(a2)) => a1 == a2,
            (UseTInner::GetElemT(a1), UseTInner::GetElemT(a2)) => a1 == a2,
            (UseTInner::CallElemT(a1), UseTInner::CallElemT(a2)) => a1 == a2,
            (UseTInner::GetStaticsT(a1, b1), UseTInner::GetStaticsT(a2, b2)) => {
                a1 == a2 && b1 == b2
            }
            (UseTInner::GetProtoT(a1, b1), UseTInner::GetProtoT(a2, b2)) => a1 == a2 && b1 == b2,
            (
                UseTInner::ReposLowerT {
                    reason: a1,
                    use_desc: b1,
                    use_t: c1,
                },
                UseTInner::ReposLowerT {
                    reason: a2,
                    use_desc: b2,
                    use_t: c2,
                },
            ) => a1 == a2 && b1 == b2 && c1 == c2,
            (UseTInner::ReposUseT(a1), UseTInner::ReposUseT(a2)) => a1 == a2,
            (UseTInner::ConstructorT(a1), UseTInner::ConstructorT(a2)) => a1 == a2,
            (UseTInner::SuperT(a1), UseTInner::SuperT(a2)) => a1 == a2,
            (UseTInner::ImplementsT(a1, b1), UseTInner::ImplementsT(a2, b2)) => {
                a1 == a2 && b1 == b2
            }
            (UseTInner::MixinT(a1, b1), UseTInner::MixinT(a2, b2)) => a1 == a2 && b1 == b2,
            (
                UseTInner::ToStringT {
                    orig_t: a1,
                    reason: b1,
                    t_out: c1,
                },
                UseTInner::ToStringT {
                    orig_t: a2,
                    reason: b2,
                    t_out: c2,
                },
            ) => a1 == a2 && b1 == b2 && c1 == c2,
            (UseTInner::SpecializeT(a1), UseTInner::SpecializeT(a2)) => a1 == a2,
            (UseTInner::ThisSpecializeT(a1, b1, c1), UseTInner::ThisSpecializeT(a2, b2, c2)) => {
                a1 == a2 && b1 == b2 && c1 == c2
            }
            (UseTInner::ValueToTypeReferenceT(a1), UseTInner::ValueToTypeReferenceT(a2)) => {
                a1 == a2
            }
            (
                UseTInner::ConcretizeTypeAppsT(a1, b1, c1, d1),
                UseTInner::ConcretizeTypeAppsT(a2, b2, c2, d2),
            ) => a1 == a2 && b1 == b2 && c1 == c2 && d1 == d2,
            (UseTInner::LookupT(a1), UseTInner::LookupT(a2)) => a1 == a2,
            (UseTInner::ObjRestT(a1, b1, c1, d1, e1), UseTInner::ObjRestT(a2, b2, c2, d2, e2)) => {
                a1 == a2 && b1 == b2 && c1 == c2 && d1 == d2 && e1 == e2
            }
            (UseTInner::ObjTestProtoT(a1, b1), UseTInner::ObjTestProtoT(a2, b2)) => {
                a1 == a2 && b1 == b2
            }
            (UseTInner::ArrRestT(a1), UseTInner::ArrRestT(a2)) => a1 == a2,
            (
                UseTInner::GetKeysT {
                    reason: r1,
                    t_out: t1,
                    include_symbols: s1,
                },
                UseTInner::GetKeysT {
                    reason: r2,
                    t_out: t2,
                    include_symbols: s2,
                },
            ) => r1 == r2 && t1 == t2 && s1 == s2,
            (
                UseTInner::GetKeysDictKeyT {
                    reason: r1,
                    t_out: t1,
                    include_symbols: s1,
                },
                UseTInner::GetKeysDictKeyT {
                    reason: r2,
                    t_out: t2,
                    include_symbols: s2,
                },
            ) => r1 == r2 && t1 == t2 && s1 == s2,
            (UseTInner::HasOwnPropT(a1), UseTInner::HasOwnPropT(a2)) => a1 == a2,
            (UseTInner::GetValuesT(a1, b1), UseTInner::GetValuesT(a2, b2)) => a1 == a2 && b1 == b2,
            (UseTInner::GetDictValuesT(a1, b1), UseTInner::GetDictValuesT(a2, b2)) => {
                a1 == a2 && b1 == b2
            }
            (UseTInner::ElemT(a1), UseTInner::ElemT(a2)) => a1 == a2,
            (UseTInner::MapTypeT(a1), UseTInner::MapTypeT(a2)) => a1 == a2,
            (UseTInner::ObjKitT(a1, b1, c1, d1, e1), UseTInner::ObjKitT(a2, b2, c2, d2, e2)) => {
                a1 == a2 && b1 == b2 && c1 == c2 && d1 == d2 && e1 == e2
            }
            (UseTInner::ReactKitT(a1), UseTInner::ReactKitT(a2)) => a1 == a2,
            (UseTInner::ConcretizeT(a1), UseTInner::ConcretizeT(a2)) => a1 == a2,
            (UseTInner::ResolveSpreadT(a1), UseTInner::ResolveSpreadT(a2)) => a1 == a2,
            (UseTInner::CondT(a1), UseTInner::CondT(a2)) => a1 == a2,
            (UseTInner::ExtendsUseT(a1), UseTInner::ExtendsUseT(a2)) => a1 == a2,
            (UseTInner::ResolveUnionT(a1), UseTInner::ResolveUnionT(a2)) => a1 == a2,
            (UseTInner::GetEnumT(a1), UseTInner::GetEnumT(a2)) => a1 == a2,
            (UseTInner::FilterOptionalT(a1, b1), UseTInner::FilterOptionalT(a2, b2)) => {
                a1 == a2 && b1 == b2
            }
            (UseTInner::FilterMaybeT(a1, b1), UseTInner::FilterMaybeT(a2, b2)) => {
                a1 == a2 && b1 == b2
            }
            (UseTInner::DeepReadOnlyT(a1, b1), UseTInner::DeepReadOnlyT(a2, b2)) => {
                a1 == a2 && b1 == b2
            }
            (UseTInner::HooklikeT(a1), UseTInner::HooklikeT(a2)) => a1 == a2,
            (UseTInner::SealGenericT(a1), UseTInner::SealGenericT(a2)) => a1 == a2,
            (UseTInner::OptionalIndexedAccessT(a1), UseTInner::OptionalIndexedAccessT(a2)) => {
                a1 == a2
            }
            (
                UseTInner::ConvertEmptyPropsToMixedT(a1, b1),
                UseTInner::ConvertEmptyPropsToMixedT(a2, b2),
            ) => a1 == a2 && b1 == b2,
            (
                UseTInner::ExitRendersT {
                    renders_reason: a1,
                    u: b1,
                },
                UseTInner::ExitRendersT {
                    renders_reason: a2,
                    u: b2,
                },
            ) => a1 == a2 && b1 == b2,
            (UseTInner::EvalTypeDestructorT(a1), UseTInner::EvalTypeDestructorT(a2)) => a1 == a2,
            _ => false,
        }
    }
}

impl<CX: ContextFamily> Eq for UseTInner<CX> {}

impl<CX: ContextFamily> std::hash::Hash for UseTInner<CX> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self).hash(state);
        match self {
            UseTInner::UseT(a, b) => {
                a.hash(state);
                b.hash(state);
            }
            UseTInner::CallT(a) => a.hash(state),
            UseTInner::ConditionalT(a) => a.hash(state),
            UseTInner::MethodT(a) => a.hash(state),
            UseTInner::PrivateMethodT(a) => a.hash(state),
            UseTInner::SetPropT(a, b, c, d, e, f, g) => {
                a.hash(state);
                b.hash(state);
                c.hash(state);
                d.hash(state);
                e.hash(state);
                f.hash(state);
                g.hash(state);
            }
            UseTInner::SetPrivatePropT(a) => a.hash(state),
            UseTInner::GetPropT(a) => a.hash(state),
            UseTInner::GetPrivatePropT(a) => a.hash(state),
            UseTInner::TestPropT(a) => a.hash(state),
            UseTInner::SetElemT(a) => a.hash(state),
            UseTInner::GetElemT(a) => a.hash(state),
            UseTInner::CallElemT(a) => a.hash(state),
            UseTInner::GetStaticsT(a, b) => {
                a.hash(state);
                b.hash(state);
            }
            UseTInner::GetProtoT(a, b) => {
                a.hash(state);
                b.hash(state);
            }
            UseTInner::ReposLowerT {
                reason,
                use_desc,
                use_t,
            } => {
                reason.hash(state);
                use_desc.hash(state);
                use_t.hash(state);
            }
            UseTInner::ReposUseT(a) => a.hash(state),
            UseTInner::ConstructorT(a) => a.hash(state),
            UseTInner::SuperT(a) => a.hash(state),
            UseTInner::ImplementsT(a, b) => {
                a.hash(state);
                b.hash(state);
            }
            UseTInner::MixinT(a, b) => {
                a.hash(state);
                b.hash(state);
            }
            UseTInner::ToStringT {
                orig_t,
                reason,
                t_out,
            } => {
                orig_t.hash(state);
                reason.hash(state);
                t_out.hash(state);
            }
            UseTInner::SpecializeT(a) => a.hash(state),
            UseTInner::ThisSpecializeT(a, b, c) => {
                a.hash(state);
                b.hash(state);
                c.hash(state);
            }
            UseTInner::ValueToTypeReferenceT(a) => a.hash(state),
            UseTInner::ConcretizeTypeAppsT(a, b, c, d) => {
                a.hash(state);
                b.hash(state);
                c.hash(state);
                d.hash(state);
            }
            UseTInner::LookupT(a) => a.hash(state),
            UseTInner::ObjRestT(a, b, c, d, e) => {
                a.hash(state);
                b.hash(state);
                c.hash(state);
                d.hash(state);
                e.hash(state);
            }
            UseTInner::ObjTestProtoT(a, b) => {
                a.hash(state);
                b.hash(state);
            }
            UseTInner::ArrRestT(a) => a.hash(state),
            UseTInner::GetKeysT {
                reason,
                t_out,
                include_symbols,
            } => {
                reason.hash(state);
                t_out.hash(state);
                include_symbols.hash(state);
            }
            UseTInner::GetKeysDictKeyT {
                reason,
                t_out,
                include_symbols,
            } => {
                reason.hash(state);
                t_out.hash(state);
                include_symbols.hash(state);
            }
            UseTInner::HasOwnPropT(a) => a.hash(state),
            UseTInner::GetValuesT(a, b) => {
                a.hash(state);
                b.hash(state);
            }
            UseTInner::GetDictValuesT(a, b) => {
                a.hash(state);
                b.hash(state);
            }
            UseTInner::ElemT(a) => a.hash(state),
            UseTInner::MapTypeT(a) => a.hash(state),
            UseTInner::ObjKitT(a, b, c, d, e) => {
                a.hash(state);
                b.hash(state);
                c.hash(state);
                d.hash(state);
                e.hash(state);
            }
            UseTInner::ReactKitT(a) => a.hash(state),
            UseTInner::ConcretizeT(a) => a.hash(state),
            UseTInner::ResolveSpreadT(a) => a.hash(state),
            UseTInner::CondT(a) => a.hash(state),
            UseTInner::ExtendsUseT(a) => a.hash(state),
            UseTInner::ResolveUnionT(a) => a.hash(state),
            UseTInner::GetEnumT(a) => a.hash(state),
            UseTInner::FilterOptionalT(a, b) => {
                a.hash(state);
                b.hash(state);
            }
            UseTInner::FilterMaybeT(a, b) => {
                a.hash(state);
                b.hash(state);
            }
            UseTInner::DeepReadOnlyT(a, b) => {
                a.hash(state);
                b.hash(state);
            }
            UseTInner::HooklikeT(a) => a.hash(state),
            UseTInner::SealGenericT(a) => a.hash(state),
            UseTInner::OptionalIndexedAccessT(a) => a.hash(state),
            UseTInner::ConvertEmptyPropsToMixedT(a, b) => {
                a.hash(state);
                b.hash(state);
            }
            UseTInner::ExitRendersT { renders_reason, u } => {
                renders_reason.hash(state);
                u.hash(state);
            }
            UseTInner::EvalTypeDestructorT(a) => a.hash(state),
        }
    }
}

impl<CX: ContextFamily> PartialOrd for UseTInner<CX> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<CX: ContextFamily> Ord for UseTInner<CX> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        fn variant_index<CX: ContextFamily>(v: &UseTInner<CX>) -> u32 {
            match v {
                UseTInner::UseT(..) => 0,
                UseTInner::CallT(..) => 2,
                UseTInner::ConditionalT(..) => 3,
                UseTInner::MethodT(..) => 4,
                UseTInner::PrivateMethodT(..) => 5,
                UseTInner::SetPropT(..) => 6,
                UseTInner::SetPrivatePropT(..) => 7,
                UseTInner::GetPropT(..) => 9,
                UseTInner::GetPrivatePropT(..) => 10,
                UseTInner::TestPropT(..) => 11,
                UseTInner::SetElemT(..) => 12,
                UseTInner::GetElemT(..) => 13,
                UseTInner::CallElemT(..) => 14,
                UseTInner::GetStaticsT(..) => 15,
                UseTInner::GetProtoT(..) => 16,
                UseTInner::ReposLowerT { .. } => 18,
                UseTInner::ReposUseT(..) => 19,
                UseTInner::ConstructorT(..) => 20,
                UseTInner::SuperT(..) => 21,
                UseTInner::ImplementsT(..) => 22,
                UseTInner::MixinT(..) => 23,
                UseTInner::ToStringT { .. } => 24,
                UseTInner::SpecializeT(..) => 25,
                UseTInner::ThisSpecializeT(..) => 26,
                UseTInner::ValueToTypeReferenceT(..) => 27,
                UseTInner::ConcretizeTypeAppsT(..) => 28,
                UseTInner::LookupT(..) => 29,
                UseTInner::ObjRestT(..) => 30,
                UseTInner::ObjTestProtoT(..) => 31,
                UseTInner::ArrRestT(..) => 33,
                UseTInner::GetKeysT { .. } => 34,
                UseTInner::HasOwnPropT(..) => 35,
                UseTInner::GetValuesT(..) => 36,
                UseTInner::GetDictValuesT(..) => 37,
                UseTInner::ElemT(..) => 38,
                UseTInner::MapTypeT(..) => 39,
                UseTInner::ObjKitT(..) => 40,
                UseTInner::ReactKitT(..) => 41,
                UseTInner::ConcretizeT(..) => 42,
                UseTInner::ResolveSpreadT(..) => 43,
                UseTInner::CondT(..) => 44,
                UseTInner::ExtendsUseT(..) => 45,
                UseTInner::ResolveUnionT(..) => 47,
                UseTInner::GetEnumT(..) => 51,
                UseTInner::FilterOptionalT(..) => 52,
                UseTInner::FilterMaybeT(..) => 53,
                UseTInner::DeepReadOnlyT(..) => 54,
                UseTInner::HooklikeT(..) => 55,
                UseTInner::SealGenericT(..) => 56,
                UseTInner::OptionalIndexedAccessT(..) => 57,
                UseTInner::ConvertEmptyPropsToMixedT(..) => 59,
                UseTInner::ExitRendersT { .. } => 60,
                UseTInner::EvalTypeDestructorT(..) => 61,
                UseTInner::GetKeysDictKeyT { .. } => 62,
            }
        }
        let disc = variant_index(self).cmp(&variant_index(other));
        if disc != std::cmp::Ordering::Equal {
            return disc;
        }
        match (self, other) {
            (UseTInner::UseT(a1, b1), UseTInner::UseT(a2, b2)) => {
                a1.cmp(a2).then_with(|| b1.cmp(b2))
            }
            (UseTInner::CallT(a1), UseTInner::CallT(a2)) => a1.cmp(a2),
            (UseTInner::ConditionalT(a1), UseTInner::ConditionalT(a2)) => a1.cmp(a2),
            (UseTInner::MethodT(a1), UseTInner::MethodT(a2)) => a1.cmp(a2),
            (UseTInner::PrivateMethodT(a1), UseTInner::PrivateMethodT(a2)) => a1.cmp(a2),
            (
                UseTInner::SetPropT(a1, b1, c1, d1, e1, f1, g1),
                UseTInner::SetPropT(a2, b2, c2, d2, e2, f2, g2),
            ) => a1
                .cmp(a2)
                .then_with(|| b1.cmp(b2))
                .then_with(|| c1.cmp(c2))
                .then_with(|| d1.cmp(d2))
                .then_with(|| e1.cmp(e2))
                .then_with(|| f1.cmp(f2))
                .then_with(|| g1.cmp(g2)),
            (UseTInner::SetPrivatePropT(a1), UseTInner::SetPrivatePropT(a2)) => a1.cmp(a2),
            (UseTInner::GetPropT(a1), UseTInner::GetPropT(a2)) => a1.cmp(a2),
            (UseTInner::GetPrivatePropT(a1), UseTInner::GetPrivatePropT(a2)) => a1.cmp(a2),
            (UseTInner::TestPropT(a1), UseTInner::TestPropT(a2)) => a1.cmp(a2),
            (UseTInner::SetElemT(a1), UseTInner::SetElemT(a2)) => a1.cmp(a2),
            (UseTInner::GetElemT(a1), UseTInner::GetElemT(a2)) => a1.cmp(a2),
            (UseTInner::CallElemT(a1), UseTInner::CallElemT(a2)) => a1.cmp(a2),
            (UseTInner::GetStaticsT(a1, b1), UseTInner::GetStaticsT(a2, b2)) => {
                a1.cmp(a2).then_with(|| b1.cmp(b2))
            }
            (UseTInner::GetProtoT(a1, b1), UseTInner::GetProtoT(a2, b2)) => {
                a1.cmp(a2).then_with(|| b1.cmp(b2))
            }
            (
                UseTInner::ReposLowerT {
                    reason: a1,
                    use_desc: b1,
                    use_t: c1,
                },
                UseTInner::ReposLowerT {
                    reason: a2,
                    use_desc: b2,
                    use_t: c2,
                },
            ) => a1.cmp(a2).then_with(|| b1.cmp(b2)).then_with(|| c1.cmp(c2)),
            (UseTInner::ReposUseT(a1), UseTInner::ReposUseT(a2)) => a1.cmp(a2),
            (UseTInner::ConstructorT(a1), UseTInner::ConstructorT(a2)) => a1.cmp(a2),
            (UseTInner::SuperT(a1), UseTInner::SuperT(a2)) => a1.cmp(a2),
            (UseTInner::ImplementsT(a1, b1), UseTInner::ImplementsT(a2, b2)) => {
                a1.cmp(a2).then_with(|| b1.cmp(b2))
            }
            (UseTInner::MixinT(a1, b1), UseTInner::MixinT(a2, b2)) => {
                a1.cmp(a2).then_with(|| b1.cmp(b2))
            }
            (
                UseTInner::ToStringT {
                    orig_t: a1,
                    reason: b1,
                    t_out: c1,
                },
                UseTInner::ToStringT {
                    orig_t: a2,
                    reason: b2,
                    t_out: c2,
                },
            ) => a1.cmp(a2).then_with(|| b1.cmp(b2)).then_with(|| c1.cmp(c2)),
            (UseTInner::SpecializeT(a1), UseTInner::SpecializeT(a2)) => a1.cmp(a2),
            (UseTInner::ThisSpecializeT(a1, b1, c1), UseTInner::ThisSpecializeT(a2, b2, c2)) => {
                a1.cmp(a2).then_with(|| b1.cmp(b2)).then_with(|| c1.cmp(c2))
            }
            (UseTInner::ValueToTypeReferenceT(a1), UseTInner::ValueToTypeReferenceT(a2)) => {
                a1.cmp(a2)
            }
            (
                UseTInner::ConcretizeTypeAppsT(a1, b1, c1, d1),
                UseTInner::ConcretizeTypeAppsT(a2, b2, c2, d2),
            ) => a1
                .cmp(a2)
                .then_with(|| b1.cmp(b2))
                .then_with(|| c1.cmp(c2))
                .then_with(|| d1.cmp(d2)),
            (UseTInner::LookupT(a1), UseTInner::LookupT(a2)) => a1.cmp(a2),
            (UseTInner::ObjRestT(a1, b1, c1, d1, e1), UseTInner::ObjRestT(a2, b2, c2, d2, e2)) => {
                a1.cmp(a2)
                    .then_with(|| b1.cmp(b2))
                    .then_with(|| c1.cmp(c2))
                    .then_with(|| d1.cmp(d2))
                    .then_with(|| e1.cmp(e2))
            }
            (UseTInner::ObjTestProtoT(a1, b1), UseTInner::ObjTestProtoT(a2, b2)) => {
                a1.cmp(a2).then_with(|| b1.cmp(b2))
            }
            (UseTInner::ArrRestT(a1), UseTInner::ArrRestT(a2)) => a1.cmp(a2),
            (
                UseTInner::GetKeysT {
                    reason: r1,
                    t_out: t1,
                    include_symbols: s1,
                },
                UseTInner::GetKeysT {
                    reason: r2,
                    t_out: t2,
                    include_symbols: s2,
                },
            ) => r1.cmp(r2).then_with(|| t1.cmp(t2)).then_with(|| s1.cmp(s2)),
            (
                UseTInner::GetKeysDictKeyT {
                    reason: r1,
                    t_out: t1,
                    include_symbols: s1,
                },
                UseTInner::GetKeysDictKeyT {
                    reason: r2,
                    t_out: t2,
                    include_symbols: s2,
                },
            ) => r1.cmp(r2).then_with(|| t1.cmp(t2)).then_with(|| s1.cmp(s2)),
            (UseTInner::HasOwnPropT(a1), UseTInner::HasOwnPropT(a2)) => a1.cmp(a2),
            (UseTInner::GetValuesT(a1, b1), UseTInner::GetValuesT(a2, b2)) => {
                a1.cmp(a2).then_with(|| b1.cmp(b2))
            }
            (UseTInner::GetDictValuesT(a1, b1), UseTInner::GetDictValuesT(a2, b2)) => {
                a1.cmp(a2).then_with(|| b1.cmp(b2))
            }
            (UseTInner::ElemT(a1), UseTInner::ElemT(a2)) => a1.cmp(a2),
            (UseTInner::MapTypeT(a1), UseTInner::MapTypeT(a2)) => a1.cmp(a2),
            (UseTInner::ObjKitT(a1, b1, c1, d1, e1), UseTInner::ObjKitT(a2, b2, c2, d2, e2)) => a1
                .cmp(a2)
                .then_with(|| b1.cmp(b2))
                .then_with(|| c1.cmp(c2))
                .then_with(|| d1.cmp(d2))
                .then_with(|| e1.cmp(e2)),
            (UseTInner::ReactKitT(a1), UseTInner::ReactKitT(a2)) => a1.cmp(a2),
            (UseTInner::ConcretizeT(a1), UseTInner::ConcretizeT(a2)) => a1.cmp(a2),
            (UseTInner::ResolveSpreadT(a1), UseTInner::ResolveSpreadT(a2)) => a1.cmp(a2),
            (UseTInner::CondT(a1), UseTInner::CondT(a2)) => a1.cmp(a2),
            (UseTInner::ExtendsUseT(a1), UseTInner::ExtendsUseT(a2)) => a1.cmp(a2),
            (UseTInner::ResolveUnionT(a1), UseTInner::ResolveUnionT(a2)) => a1.cmp(a2),
            (UseTInner::GetEnumT(a1), UseTInner::GetEnumT(a2)) => a1.cmp(a2),
            (UseTInner::FilterOptionalT(a1, b1), UseTInner::FilterOptionalT(a2, b2)) => {
                a1.cmp(a2).then_with(|| b1.cmp(b2))
            }
            (UseTInner::FilterMaybeT(a1, b1), UseTInner::FilterMaybeT(a2, b2)) => {
                a1.cmp(a2).then_with(|| b1.cmp(b2))
            }
            (UseTInner::DeepReadOnlyT(a1, b1), UseTInner::DeepReadOnlyT(a2, b2)) => {
                a1.cmp(a2).then_with(|| b1.cmp(b2))
            }
            (UseTInner::HooklikeT(a1), UseTInner::HooklikeT(a2)) => a1.cmp(a2),
            (UseTInner::SealGenericT(a1), UseTInner::SealGenericT(a2)) => a1.cmp(a2),
            (UseTInner::OptionalIndexedAccessT(a1), UseTInner::OptionalIndexedAccessT(a2)) => {
                a1.cmp(a2)
            }
            (
                UseTInner::ConvertEmptyPropsToMixedT(a1, b1),
                UseTInner::ConvertEmptyPropsToMixedT(a2, b2),
            ) => a1.cmp(a2).then_with(|| b1.cmp(b2)),
            (
                UseTInner::ExitRendersT {
                    renders_reason: a1,
                    u: b1,
                },
                UseTInner::ExitRendersT {
                    renders_reason: a2,
                    u: b2,
                },
            ) => a1.cmp(a2).then_with(|| b1.cmp(b2)),
            (UseTInner::EvalTypeDestructorT(a1), UseTInner::EvalTypeDestructorT(a2)) => a1.cmp(a2),
            _ => std::cmp::Ordering::Equal,
        }
    }
}

impl<CX: ContextFamily> std::fmt::Debug for UseTInner<CX> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UseTInner::UseT(a, b) => f.debug_tuple("UseT").field(a).field(b).finish(),
            UseTInner::CallT(a) => f.debug_tuple("CallT").field(a).finish(),
            UseTInner::ConditionalT(a) => f.debug_tuple("ConditionalT").field(a).finish(),
            UseTInner::MethodT(a) => f.debug_tuple("MethodT").field(a).finish(),
            UseTInner::PrivateMethodT(a) => f.debug_tuple("PrivateMethodT").field(a).finish(),
            UseTInner::SetPropT(a, b, c, d, e, ff, g) => f
                .debug_tuple("SetPropT")
                .field(a)
                .field(b)
                .field(c)
                .field(d)
                .field(e)
                .field(ff)
                .field(g)
                .finish(),
            UseTInner::SetPrivatePropT(a) => f.debug_tuple("SetPrivatePropT").field(a).finish(),
            UseTInner::GetPropT(a) => f.debug_tuple("GetPropT").field(a).finish(),
            UseTInner::GetPrivatePropT(a) => f.debug_tuple("GetPrivatePropT").field(a).finish(),
            UseTInner::TestPropT(a) => f.debug_tuple("TestPropT").field(a).finish(),
            UseTInner::SetElemT(a) => f.debug_tuple("SetElemT").field(a).finish(),
            UseTInner::GetElemT(a) => f.debug_tuple("GetElemT").field(a).finish(),
            UseTInner::CallElemT(a) => f.debug_tuple("CallElemT").field(a).finish(),
            UseTInner::GetStaticsT(a, b) => f.debug_tuple("GetStaticsT").field(a).field(b).finish(),
            UseTInner::GetProtoT(a, b) => f.debug_tuple("GetProtoT").field(a).field(b).finish(),
            UseTInner::ReposLowerT {
                reason,
                use_desc,
                use_t,
            } => f
                .debug_struct("ReposLowerT")
                .field("reason", reason)
                .field("use_desc", use_desc)
                .field("use_t", use_t)
                .finish(),
            UseTInner::ReposUseT(a) => f.debug_tuple("ReposUseT").field(a).finish(),
            UseTInner::ConstructorT(a) => f.debug_tuple("ConstructorT").field(a).finish(),
            UseTInner::SuperT(a) => f.debug_tuple("SuperT").field(a).finish(),
            UseTInner::ImplementsT(a, b) => f.debug_tuple("ImplementsT").field(a).field(b).finish(),
            UseTInner::MixinT(a, b) => f.debug_tuple("MixinT").field(a).field(b).finish(),
            UseTInner::ToStringT {
                orig_t,
                reason,
                t_out,
            } => f
                .debug_struct("ToStringT")
                .field("orig_t", orig_t)
                .field("reason", reason)
                .field("t_out", t_out)
                .finish(),
            UseTInner::SpecializeT(a) => f.debug_tuple("SpecializeT").field(a).finish(),
            UseTInner::ThisSpecializeT(a, b, c) => f
                .debug_tuple("ThisSpecializeT")
                .field(a)
                .field(b)
                .field(c)
                .finish(),
            UseTInner::ValueToTypeReferenceT(a) => {
                f.debug_tuple("ValueToTypeReferenceT").field(a).finish()
            }
            UseTInner::ConcretizeTypeAppsT(a, b, c, d) => f
                .debug_tuple("ConcretizeTypeAppsT")
                .field(a)
                .field(b)
                .field(c)
                .field(d)
                .finish(),
            UseTInner::LookupT(a) => f.debug_tuple("LookupT").field(a).finish(),
            UseTInner::ObjRestT(a, b, c, d, e) => f
                .debug_tuple("ObjRestT")
                .field(a)
                .field(b)
                .field(c)
                .field(d)
                .field(e)
                .finish(),
            UseTInner::ObjTestProtoT(a, b) => {
                f.debug_tuple("ObjTestProtoT").field(a).field(b).finish()
            }
            UseTInner::ArrRestT(a) => f.debug_tuple("ArrRestT").field(a).finish(),
            UseTInner::GetKeysT {
                reason,
                t_out,
                include_symbols,
            } => f
                .debug_struct("GetKeysT")
                .field("reason", reason)
                .field("t_out", t_out)
                .field("include_symbols", include_symbols)
                .finish(),
            UseTInner::GetKeysDictKeyT {
                reason,
                t_out,
                include_symbols,
            } => f
                .debug_struct("GetKeysDictKeyT")
                .field("reason", reason)
                .field("t_out", t_out)
                .field("include_symbols", include_symbols)
                .finish(),
            UseTInner::HasOwnPropT(a) => f.debug_tuple("HasOwnPropT").field(a).finish(),
            UseTInner::GetValuesT(a, b) => f.debug_tuple("GetValuesT").field(a).field(b).finish(),
            UseTInner::GetDictValuesT(a, b) => {
                f.debug_tuple("GetDictValuesT").field(a).field(b).finish()
            }
            UseTInner::ElemT(a) => f.debug_tuple("ElemT").field(a).finish(),
            UseTInner::MapTypeT(a) => f.debug_tuple("MapTypeT").field(a).finish(),
            UseTInner::ObjKitT(a, b, c, d, e) => f
                .debug_tuple("ObjKitT")
                .field(a)
                .field(b)
                .field(c)
                .field(d)
                .field(e)
                .finish(),
            UseTInner::ReactKitT(a) => f.debug_tuple("ReactKitT").field(a).finish(),
            UseTInner::ConcretizeT(a) => f.debug_tuple("ConcretizeT").field(a).finish(),
            UseTInner::ResolveSpreadT(a) => f.debug_tuple("ResolveSpreadT").field(a).finish(),
            UseTInner::CondT(a) => f.debug_tuple("CondT").field(a).finish(),
            UseTInner::ExtendsUseT(a) => f.debug_tuple("ExtendsUseT").field(a).finish(),
            UseTInner::ResolveUnionT(a) => f.debug_tuple("ResolveUnionT").field(a).finish(),
            UseTInner::GetEnumT(a) => f.debug_tuple("GetEnumT").field(a).finish(),
            UseTInner::FilterOptionalT(a, b) => {
                f.debug_tuple("FilterOptionalT").field(a).field(b).finish()
            }
            UseTInner::FilterMaybeT(a, b) => {
                f.debug_tuple("FilterMaybeT").field(a).field(b).finish()
            }
            UseTInner::DeepReadOnlyT(a, b) => {
                f.debug_tuple("DeepReadOnlyT").field(a).field(b).finish()
            }
            UseTInner::HooklikeT(a) => f.debug_tuple("HooklikeT").field(a).finish(),
            UseTInner::SealGenericT(a) => f.debug_tuple("SealGenericT").field(a).finish(),
            UseTInner::OptionalIndexedAccessT(a) => {
                f.debug_tuple("OptionalIndexedAccessT").field(a).finish()
            }
            UseTInner::ConvertEmptyPropsToMixedT(a, b) => f
                .debug_tuple("ConvertEmptyPropsToMixedT")
                .field(a)
                .field(b)
                .finish(),
            UseTInner::ExitRendersT { renders_reason, u } => f
                .debug_struct("ExitRendersT")
                .field("renders_reason", renders_reason)
                .field("u", u)
                .finish(),
            UseTInner::EvalTypeDestructorT(a) => {
                f.debug_tuple("EvalTypeDestructorT").field(a).finish()
            }
        }
    }
}

pub struct UseT<CX: ContextFamily = ()>(Rc<UseTInner<CX>>);

impl<CX: ContextFamily> Clone for UseT<CX> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<CX: ContextFamily> Dupe for UseT<CX> {}

impl<CX: ContextFamily> Hash for UseT<CX> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

impl<CX: ContextFamily> PartialEq for UseT<CX> {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0) || self.0 == other.0
    }
}

impl<CX: ContextFamily> Eq for UseT<CX> {}

impl<CX: ContextFamily> PartialOrd for UseT<CX> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<CX: ContextFamily> Ord for UseT<CX> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        if Rc::ptr_eq(&self.0, &other.0) {
            std::cmp::Ordering::Equal
        } else {
            self.0.cmp(&other.0)
        }
    }
}

impl<CX: ContextFamily> std::fmt::Debug for UseT<CX> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl<CX: ContextFamily> Deref for UseT<CX> {
    type Target = UseTInner<CX>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<CX: ContextFamily> UseT<CX> {
    pub fn new(inner: UseTInner<CX>) -> Self {
        Self(Rc::new(inner))
    }

    pub fn ptr_eq(&self, other: &UseT<CX>) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl<CX: ContextFamily> From<UseTInner<CX>> for UseT<CX> {
    fn from(inner: UseTInner<CX>) -> Self {
        UseT::new(inner)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EnumCheck {
    pub case_test_loc: ALoc,
    pub member_name: FlowSmolStr,
}
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EnumExhaustiveCheckPossiblyValidData {
    // We only convert a "possible check" into a "check" if it has the same
    // enum type as the discriminant.
    pub possible_checks: VecDeque<(Type, EnumCheck)>,
    pub checks: Rc<[EnumCheck]>,
    pub default_case_loc: Option<ALoc>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EnumPossibleExhaustiveCheckT {
    EnumExhaustiveCheckPossiblyValid(Box<EnumExhaustiveCheckPossiblyValidData>),
    EnumExhaustiveCheckInvalid(Rc<[ALoc]>),
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CallAction {
    Funcalltype(Box<FuncallType>),
    ConcretizeCallee(type_collector::TypeCollector),
}

// opt_use_t: use_ts which can be part of an optional chain
pub enum OptUseT<CX: ContextFamily = ()> {
    OptCallT {
        use_op: UseOp,
        reason: Reason,
        opt_funcalltype: OptFuncallType,
        return_hint: LazyHintT<CX>,
    },
    OptMethodT(UseOp, Reason, Reason, PropRef, OptMethodAction<CX>),
    OptPrivateMethodT(
        UseOp,
        Reason,
        Reason,
        FlowSmolStr,
        Rc<[ClassBinding]>,
        bool,
        OptMethodAction<CX>,
    ),
    OptGetPropT {
        use_op: UseOp,
        reason: Reason,
        id: Option<i32>,
        propref: PropRef,
        hint: LazyHintT<CX>,
    },
    OptGetPrivatePropT(UseOp, Reason, FlowSmolStr, Rc<[ClassBinding]>, bool),
    OptTestPropT(UseOp, Reason, i32, PropRef, LazyHintT<CX>),
    OptGetElemT(UseOp, Reason, Option<i32>, bool, Type),
    OptCallElemT(UseOp, Reason, Reason, Type, OptMethodAction<CX>),
}

impl<CX: ContextFamily> Clone for OptUseT<CX> {
    fn clone(&self) -> Self {
        match self {
            OptUseT::OptCallT {
                use_op,
                reason,
                opt_funcalltype,
                return_hint,
            } => OptUseT::OptCallT {
                use_op: use_op.clone(),
                reason: reason.clone(),
                opt_funcalltype: opt_funcalltype.clone(),
                return_hint: return_hint.clone(),
            },
            OptUseT::OptMethodT(a, b, c, d, e) => {
                OptUseT::OptMethodT(a.clone(), b.clone(), c.clone(), d.clone(), e.clone())
            }
            OptUseT::OptPrivateMethodT(a, b, c, d, e, f, g) => OptUseT::OptPrivateMethodT(
                a.clone(),
                b.clone(),
                c.clone(),
                d.clone(),
                e.clone(),
                *f,
                g.clone(),
            ),
            OptUseT::OptGetPropT {
                use_op,
                reason,
                id,
                propref,
                hint,
            } => OptUseT::OptGetPropT {
                use_op: use_op.clone(),
                reason: reason.clone(),
                id: *id,
                propref: propref.clone(),
                hint: hint.clone(),
            },
            OptUseT::OptGetPrivatePropT(a, b, c, d, e) => {
                OptUseT::OptGetPrivatePropT(a.clone(), b.clone(), c.clone(), d.clone(), *e)
            }
            OptUseT::OptTestPropT(a, b, c, d, e) => {
                OptUseT::OptTestPropT(a.clone(), b.clone(), *c, d.clone(), e.clone())
            }
            OptUseT::OptGetElemT(a, b, c, d, e) => {
                OptUseT::OptGetElemT(a.clone(), b.clone(), *c, *d, e.clone())
            }
            OptUseT::OptCallElemT(a, b, c, d, e) => {
                OptUseT::OptCallElemT(a.clone(), b.clone(), c.clone(), d.clone(), e.clone())
            }
        }
    }
}

impl<CX: ContextFamily> PartialEq for OptUseT<CX> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (
                OptUseT::OptCallT {
                    use_op: a1,
                    reason: b1,
                    opt_funcalltype: c1,
                    return_hint: d1,
                },
                OptUseT::OptCallT {
                    use_op: a2,
                    reason: b2,
                    opt_funcalltype: c2,
                    return_hint: d2,
                },
            ) => a1 == a2 && b1 == b2 && c1 == c2 && d1 == d2,
            (OptUseT::OptMethodT(a1, b1, c1, d1, e1), OptUseT::OptMethodT(a2, b2, c2, d2, e2)) => {
                a1 == a2 && b1 == b2 && c1 == c2 && d1 == d2 && e1 == e2
            }
            (
                OptUseT::OptPrivateMethodT(a1, b1, c1, d1, e1, f1, g1),
                OptUseT::OptPrivateMethodT(a2, b2, c2, d2, e2, f2, g2),
            ) => a1 == a2 && b1 == b2 && c1 == c2 && d1 == d2 && e1 == e2 && f1 == f2 && g1 == g2,
            (
                OptUseT::OptGetPropT {
                    use_op: a1,
                    reason: b1,
                    id: c1,
                    propref: d1,
                    hint: e1,
                },
                OptUseT::OptGetPropT {
                    use_op: a2,
                    reason: b2,
                    id: c2,
                    propref: d2,
                    hint: e2,
                },
            ) => a1 == a2 && b1 == b2 && c1 == c2 && d1 == d2 && e1 == e2,
            (
                OptUseT::OptGetPrivatePropT(a1, b1, c1, d1, e1),
                OptUseT::OptGetPrivatePropT(a2, b2, c2, d2, e2),
            ) => a1 == a2 && b1 == b2 && c1 == c2 && d1 == d2 && e1 == e2,
            (
                OptUseT::OptTestPropT(a1, b1, c1, d1, e1),
                OptUseT::OptTestPropT(a2, b2, c2, d2, e2),
            ) => a1 == a2 && b1 == b2 && c1 == c2 && d1 == d2 && e1 == e2,
            (
                OptUseT::OptGetElemT(a1, b1, c1, d1, e1),
                OptUseT::OptGetElemT(a2, b2, c2, d2, e2),
            ) => a1 == a2 && b1 == b2 && c1 == c2 && d1 == d2 && e1 == e2,
            (
                OptUseT::OptCallElemT(a1, b1, c1, d1, e1),
                OptUseT::OptCallElemT(a2, b2, c2, d2, e2),
            ) => a1 == a2 && b1 == b2 && c1 == c2 && d1 == d2 && e1 == e2,
            _ => false,
        }
    }
}

impl<CX: ContextFamily> Eq for OptUseT<CX> {}

impl<CX: ContextFamily> std::hash::Hash for OptUseT<CX> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self).hash(state);
        match self {
            OptUseT::OptCallT {
                use_op,
                reason,
                opt_funcalltype,
                return_hint,
            } => {
                use_op.hash(state);
                reason.hash(state);
                opt_funcalltype.hash(state);
                return_hint.hash(state);
            }
            OptUseT::OptMethodT(a, b, c, d, e) => {
                a.hash(state);
                b.hash(state);
                c.hash(state);
                d.hash(state);
                e.hash(state);
            }
            OptUseT::OptPrivateMethodT(a, b, c, d, e, f, g) => {
                a.hash(state);
                b.hash(state);
                c.hash(state);
                d.hash(state);
                e.hash(state);
                f.hash(state);
                g.hash(state);
            }
            OptUseT::OptGetPropT {
                use_op,
                reason,
                id,
                propref,
                hint,
            } => {
                use_op.hash(state);
                reason.hash(state);
                id.hash(state);
                propref.hash(state);
                hint.hash(state);
            }
            OptUseT::OptGetPrivatePropT(a, b, c, d, e) => {
                a.hash(state);
                b.hash(state);
                c.hash(state);
                d.hash(state);
                e.hash(state);
            }
            OptUseT::OptTestPropT(a, b, c, d, e) => {
                a.hash(state);
                b.hash(state);
                c.hash(state);
                d.hash(state);
                e.hash(state);
            }
            OptUseT::OptGetElemT(a, b, c, d, e) => {
                a.hash(state);
                b.hash(state);
                c.hash(state);
                d.hash(state);
                e.hash(state);
            }
            OptUseT::OptCallElemT(a, b, c, d, e) => {
                a.hash(state);
                b.hash(state);
                c.hash(state);
                d.hash(state);
                e.hash(state);
            }
        }
    }
}

impl<CX: ContextFamily> PartialOrd for OptUseT<CX> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<CX: ContextFamily> Ord for OptUseT<CX> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        fn variant_index<CX: ContextFamily>(v: &OptUseT<CX>) -> u32 {
            match v {
                OptUseT::OptCallT { .. } => 0,
                OptUseT::OptMethodT(..) => 1,
                OptUseT::OptPrivateMethodT(..) => 2,
                OptUseT::OptGetPropT { .. } => 3,
                OptUseT::OptGetPrivatePropT(..) => 4,
                OptUseT::OptTestPropT(..) => 5,
                OptUseT::OptGetElemT(..) => 6,
                OptUseT::OptCallElemT(..) => 7,
            }
        }
        let disc = variant_index(self).cmp(&variant_index(other));
        if disc != std::cmp::Ordering::Equal {
            return disc;
        }
        match (self, other) {
            (
                OptUseT::OptCallT {
                    use_op: a1,
                    reason: b1,
                    opt_funcalltype: c1,
                    return_hint: d1,
                },
                OptUseT::OptCallT {
                    use_op: a2,
                    reason: b2,
                    opt_funcalltype: c2,
                    return_hint: d2,
                },
            ) => a1
                .cmp(a2)
                .then_with(|| b1.cmp(b2))
                .then_with(|| c1.cmp(c2))
                .then_with(|| d1.cmp(d2)),
            (OptUseT::OptMethodT(a1, b1, c1, d1, e1), OptUseT::OptMethodT(a2, b2, c2, d2, e2)) => {
                a1.cmp(a2)
                    .then_with(|| b1.cmp(b2))
                    .then_with(|| c1.cmp(c2))
                    .then_with(|| d1.cmp(d2))
                    .then_with(|| e1.cmp(e2))
            }
            (
                OptUseT::OptPrivateMethodT(a1, b1, c1, d1, e1, f1, g1),
                OptUseT::OptPrivateMethodT(a2, b2, c2, d2, e2, f2, g2),
            ) => a1
                .cmp(a2)
                .then_with(|| b1.cmp(b2))
                .then_with(|| c1.cmp(c2))
                .then_with(|| d1.cmp(d2))
                .then_with(|| e1.cmp(e2))
                .then_with(|| f1.cmp(f2))
                .then_with(|| g1.cmp(g2)),
            (
                OptUseT::OptGetPropT {
                    use_op: a1,
                    reason: b1,
                    id: c1,
                    propref: d1,
                    hint: e1,
                },
                OptUseT::OptGetPropT {
                    use_op: a2,
                    reason: b2,
                    id: c2,
                    propref: d2,
                    hint: e2,
                },
            ) => a1
                .cmp(a2)
                .then_with(|| b1.cmp(b2))
                .then_with(|| c1.cmp(c2))
                .then_with(|| d1.cmp(d2))
                .then_with(|| e1.cmp(e2)),
            (
                OptUseT::OptGetPrivatePropT(a1, b1, c1, d1, e1),
                OptUseT::OptGetPrivatePropT(a2, b2, c2, d2, e2),
            ) => a1
                .cmp(a2)
                .then_with(|| b1.cmp(b2))
                .then_with(|| c1.cmp(c2))
                .then_with(|| d1.cmp(d2))
                .then_with(|| e1.cmp(e2)),
            (
                OptUseT::OptTestPropT(a1, b1, c1, d1, e1),
                OptUseT::OptTestPropT(a2, b2, c2, d2, e2),
            ) => a1
                .cmp(a2)
                .then_with(|| b1.cmp(b2))
                .then_with(|| c1.cmp(c2))
                .then_with(|| d1.cmp(d2))
                .then_with(|| e1.cmp(e2)),
            (
                OptUseT::OptGetElemT(a1, b1, c1, d1, e1),
                OptUseT::OptGetElemT(a2, b2, c2, d2, e2),
            ) => a1
                .cmp(a2)
                .then_with(|| b1.cmp(b2))
                .then_with(|| c1.cmp(c2))
                .then_with(|| d1.cmp(d2))
                .then_with(|| e1.cmp(e2)),
            (
                OptUseT::OptCallElemT(a1, b1, c1, d1, e1),
                OptUseT::OptCallElemT(a2, b2, c2, d2, e2),
            ) => a1
                .cmp(a2)
                .then_with(|| b1.cmp(b2))
                .then_with(|| c1.cmp(c2))
                .then_with(|| d1.cmp(d2))
                .then_with(|| e1.cmp(e2)),
            _ => std::cmp::Ordering::Equal,
        }
    }
}

impl<CX: ContextFamily> std::fmt::Debug for OptUseT<CX> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OptUseT::OptCallT {
                use_op,
                reason,
                opt_funcalltype,
                return_hint,
            } => f
                .debug_struct("OptCallT")
                .field("use_op", use_op)
                .field("reason", reason)
                .field("opt_funcalltype", opt_funcalltype)
                .field("return_hint", return_hint)
                .finish(),
            OptUseT::OptMethodT(a, b, c, d, e) => f
                .debug_tuple("OptMethodT")
                .field(a)
                .field(b)
                .field(c)
                .field(d)
                .field(e)
                .finish(),
            OptUseT::OptPrivateMethodT(a, b, c, d, e, ff, g) => f
                .debug_tuple("OptPrivateMethodT")
                .field(a)
                .field(b)
                .field(c)
                .field(d)
                .field(e)
                .field(ff)
                .field(g)
                .finish(),
            OptUseT::OptGetPropT {
                use_op,
                reason,
                id,
                propref,
                hint,
            } => f
                .debug_struct("OptGetPropT")
                .field("use_op", use_op)
                .field("reason", reason)
                .field("id", id)
                .field("propref", propref)
                .field("hint", hint)
                .finish(),
            OptUseT::OptGetPrivatePropT(a, b, c, d, e) => f
                .debug_tuple("OptGetPrivatePropT")
                .field(a)
                .field(b)
                .field(c)
                .field(d)
                .field(e)
                .finish(),
            OptUseT::OptTestPropT(a, b, c, d, e) => f
                .debug_tuple("OptTestPropT")
                .field(a)
                .field(b)
                .field(c)
                .field(d)
                .field(e)
                .finish(),
            OptUseT::OptGetElemT(a, b, c, d, e) => f
                .debug_tuple("OptGetElemT")
                .field(a)
                .field(b)
                .field(c)
                .field(d)
                .field(e)
                .finish(),
            OptUseT::OptCallElemT(a, b, c, d, e) => f
                .debug_tuple("OptCallElemT")
                .field(a)
                .field(b)
                .field(c)
                .field(d)
                .field(e)
                .finish(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum OptState {
    NonOptional,
    NewChain,
    ContinueChain,
    AssertChain,
}

pub struct CallMData<CX: ContextFamily = ()> {
    pub methodcalltype: MethodCallType,
    pub return_hint: LazyHintT<CX>,
    pub specialized_callee: Option<SpecializedCallee>,
}

pub struct ChainMData<CX: ContextFamily = ()> {
    pub exp_reason: Reason,
    pub lhs_reason: Reason,
    pub lhs_expression: ExpressionReferenceData<ALoc>,
    pub methodcalltype: MethodCallType,
    pub voided_out_collector: Option<type_collector::TypeCollector>,
    pub return_hint: LazyHintT<CX>,
    pub specialized_callee: Option<SpecializedCallee>,
}

pub enum MethodAction<CX: ContextFamily = ()> {
    CallM(Box<CallMData<CX>>),
    ChainM(Box<ChainMData<CX>>),
    NoMethodAction(type_collector::TypeCollector),
}

impl<CX: ContextFamily> Clone for MethodAction<CX> {
    fn clone(&self) -> Self {
        match self {
            MethodAction::CallM(box CallMData {
                methodcalltype,
                return_hint,
                specialized_callee,
            }) => MethodAction::CallM(Box::new(CallMData {
                methodcalltype: methodcalltype.clone(),
                return_hint: return_hint.clone(),
                specialized_callee: specialized_callee.clone(),
            })),
            MethodAction::ChainM(box ChainMData {
                exp_reason,
                lhs_reason,
                lhs_expression,
                methodcalltype,
                voided_out_collector,
                return_hint,
                specialized_callee,
            }) => MethodAction::ChainM(Box::new(ChainMData {
                exp_reason: exp_reason.clone(),
                lhs_reason: lhs_reason.clone(),
                lhs_expression: lhs_expression.dupe(),
                methodcalltype: methodcalltype.clone(),
                voided_out_collector: voided_out_collector.clone(),
                return_hint: return_hint.clone(),
                specialized_callee: specialized_callee.clone(),
            })),
            MethodAction::NoMethodAction(a) => MethodAction::NoMethodAction(a.clone()),
        }
    }
}

impl<CX: ContextFamily> PartialEq for MethodAction<CX> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (
                MethodAction::CallM(box CallMData {
                    methodcalltype: a1,
                    return_hint: b1,
                    specialized_callee: c1,
                }),
                MethodAction::CallM(box CallMData {
                    methodcalltype: a2,
                    return_hint: b2,
                    specialized_callee: c2,
                }),
            ) => a1 == a2 && b1 == b2 && c1 == c2,
            (
                MethodAction::ChainM(box ChainMData {
                    exp_reason: a1,
                    lhs_reason: b1,
                    lhs_expression: g1,
                    methodcalltype: c1,
                    voided_out_collector: d1,
                    return_hint: e1,
                    specialized_callee: f1,
                }),
                MethodAction::ChainM(box ChainMData {
                    exp_reason: a2,
                    lhs_reason: b2,
                    lhs_expression: g2,
                    methodcalltype: c2,
                    voided_out_collector: d2,
                    return_hint: e2,
                    specialized_callee: f2,
                }),
            ) => a1 == a2 && b1 == b2 && g1 == g2 && c1 == c2 && d1 == d2 && e1 == e2 && f1 == f2,
            (MethodAction::NoMethodAction(a1), MethodAction::NoMethodAction(a2)) => a1 == a2,
            _ => false,
        }
    }
}

impl<CX: ContextFamily> Eq for MethodAction<CX> {}

impl<CX: ContextFamily> std::hash::Hash for MethodAction<CX> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self).hash(state);
        match self {
            MethodAction::CallM(box CallMData {
                methodcalltype,
                return_hint,
                specialized_callee,
            }) => {
                methodcalltype.hash(state);
                return_hint.hash(state);
                specialized_callee.hash(state);
            }
            MethodAction::ChainM(box ChainMData {
                exp_reason,
                lhs_reason,
                lhs_expression,
                methodcalltype,
                voided_out_collector,
                return_hint,
                specialized_callee,
            }) => {
                exp_reason.hash(state);
                lhs_reason.hash(state);
                lhs_expression.hash(state);
                methodcalltype.hash(state);
                voided_out_collector.hash(state);
                return_hint.hash(state);
                specialized_callee.hash(state);
            }
            MethodAction::NoMethodAction(a) => {
                a.hash(state);
            }
        }
    }
}

impl<CX: ContextFamily> PartialOrd for MethodAction<CX> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<CX: ContextFamily> Ord for MethodAction<CX> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        fn variant_index<CX: ContextFamily>(v: &MethodAction<CX>) -> u32 {
            match v {
                MethodAction::CallM(..) => 0,
                MethodAction::ChainM(..) => 1,
                MethodAction::NoMethodAction(..) => 2,
            }
        }
        let disc = variant_index(self).cmp(&variant_index(other));
        if disc != std::cmp::Ordering::Equal {
            return disc;
        }
        match (self, other) {
            (
                MethodAction::CallM(box CallMData {
                    methodcalltype: a1,
                    return_hint: b1,
                    specialized_callee: c1,
                }),
                MethodAction::CallM(box CallMData {
                    methodcalltype: a2,
                    return_hint: b2,
                    specialized_callee: c2,
                }),
            ) => a1.cmp(a2).then_with(|| b1.cmp(b2)).then_with(|| c1.cmp(c2)),
            (
                MethodAction::ChainM(box ChainMData {
                    exp_reason: a1,
                    lhs_reason: b1,
                    lhs_expression: g1,
                    methodcalltype: c1,
                    voided_out_collector: d1,
                    return_hint: e1,
                    specialized_callee: f1,
                }),
                MethodAction::ChainM(box ChainMData {
                    exp_reason: a2,
                    lhs_reason: b2,
                    lhs_expression: g2,
                    methodcalltype: c2,
                    voided_out_collector: d2,
                    return_hint: e2,
                    specialized_callee: f2,
                }),
            ) => a1
                .cmp(a2)
                .then_with(|| b1.cmp(b2))
                .then_with(|| g1.cmp(g2))
                .then_with(|| c1.cmp(c2))
                .then_with(|| d1.cmp(d2))
                .then_with(|| e1.cmp(e2))
                .then_with(|| f1.cmp(f2)),
            (MethodAction::NoMethodAction(a1), MethodAction::NoMethodAction(a2)) => a1.cmp(a2),
            _ => std::cmp::Ordering::Equal,
        }
    }
}

impl<CX: ContextFamily> std::fmt::Debug for MethodAction<CX> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MethodAction::CallM(box CallMData {
                methodcalltype,
                return_hint,
                specialized_callee,
            }) => f
                .debug_struct("CallM")
                .field("methodcalltype", methodcalltype)
                .field("return_hint", return_hint)
                .field("specialized_callee", specialized_callee)
                .finish(),
            MethodAction::ChainM(box ChainMData {
                exp_reason,
                lhs_reason,
                lhs_expression,
                methodcalltype,
                voided_out_collector,
                return_hint,
                specialized_callee,
            }) => f
                .debug_struct("ChainM")
                .field("exp_reason", exp_reason)
                .field("lhs_reason", lhs_reason)
                .field("lhs_expression", lhs_expression)
                .field("methodcalltype", methodcalltype)
                .field("voided_out_collector", voided_out_collector)
                .field("return_hint", return_hint)
                .field("specialized_callee", specialized_callee)
                .finish(),
            MethodAction::NoMethodAction(a) => f.debug_tuple("NoMethodAction").field(a).finish(),
        }
    }
}

pub struct OptCallMData<CX: ContextFamily = ()> {
    pub opt_methodcalltype: OptMethodCallType,
    pub return_hint: LazyHintT<CX>,
    pub specialized_callee: Option<SpecializedCallee>,
}

pub struct OptChainMData<CX: ContextFamily = ()> {
    pub exp_reason: Reason,
    pub lhs_reason: Reason,
    pub lhs_expression: ExpressionReferenceData<ALoc>,
    pub opt_methodcalltype: OptMethodCallType,
    pub voided_out_collector: Option<type_collector::TypeCollector>,
    pub return_hint: LazyHintT<CX>,
    pub specialized_callee: Option<SpecializedCallee>,
}

pub enum OptMethodAction<CX: ContextFamily = ()> {
    OptCallM(Box<OptCallMData<CX>>),
    OptChainM(Box<OptChainMData<CX>>),
    OptNoMethodAction(type_collector::TypeCollector),
}

impl<CX: ContextFamily> Clone for OptMethodAction<CX> {
    fn clone(&self) -> Self {
        match self {
            OptMethodAction::OptCallM(box OptCallMData {
                opt_methodcalltype,
                return_hint,
                specialized_callee,
            }) => OptMethodAction::OptCallM(Box::new(OptCallMData {
                opt_methodcalltype: opt_methodcalltype.clone(),
                return_hint: return_hint.clone(),
                specialized_callee: specialized_callee.clone(),
            })),
            OptMethodAction::OptChainM(box OptChainMData {
                exp_reason,
                lhs_reason,
                lhs_expression,
                opt_methodcalltype,
                voided_out_collector,
                return_hint,
                specialized_callee,
            }) => OptMethodAction::OptChainM(Box::new(OptChainMData {
                exp_reason: exp_reason.clone(),
                lhs_reason: lhs_reason.clone(),
                lhs_expression: lhs_expression.dupe(),
                opt_methodcalltype: opt_methodcalltype.clone(),
                voided_out_collector: voided_out_collector.clone(),
                return_hint: return_hint.clone(),
                specialized_callee: specialized_callee.clone(),
            })),
            OptMethodAction::OptNoMethodAction(a) => OptMethodAction::OptNoMethodAction(a.clone()),
        }
    }
}

impl<CX: ContextFamily> PartialEq for OptMethodAction<CX> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (
                OptMethodAction::OptCallM(box OptCallMData {
                    opt_methodcalltype: a1,
                    return_hint: b1,
                    specialized_callee: c1,
                }),
                OptMethodAction::OptCallM(box OptCallMData {
                    opt_methodcalltype: a2,
                    return_hint: b2,
                    specialized_callee: c2,
                }),
            ) => a1 == a2 && b1 == b2 && c1 == c2,
            (
                OptMethodAction::OptChainM(box OptChainMData {
                    exp_reason: a1,
                    lhs_reason: b1,
                    lhs_expression: g1,
                    opt_methodcalltype: c1,
                    voided_out_collector: d1,
                    return_hint: e1,
                    specialized_callee: f1,
                }),
                OptMethodAction::OptChainM(box OptChainMData {
                    exp_reason: a2,
                    lhs_reason: b2,
                    lhs_expression: g2,
                    opt_methodcalltype: c2,
                    voided_out_collector: d2,
                    return_hint: e2,
                    specialized_callee: f2,
                }),
            ) => a1 == a2 && b1 == b2 && g1 == g2 && c1 == c2 && d1 == d2 && e1 == e2 && f1 == f2,
            (OptMethodAction::OptNoMethodAction(a1), OptMethodAction::OptNoMethodAction(a2)) => {
                a1 == a2
            }
            _ => false,
        }
    }
}

impl<CX: ContextFamily> Eq for OptMethodAction<CX> {}

impl<CX: ContextFamily> std::hash::Hash for OptMethodAction<CX> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self).hash(state);
        match self {
            OptMethodAction::OptCallM(box OptCallMData {
                opt_methodcalltype,
                return_hint,
                specialized_callee,
            }) => {
                opt_methodcalltype.hash(state);
                return_hint.hash(state);
                specialized_callee.hash(state);
            }
            OptMethodAction::OptChainM(box OptChainMData {
                exp_reason,
                lhs_reason,
                lhs_expression,
                opt_methodcalltype,
                voided_out_collector,
                return_hint,
                specialized_callee,
            }) => {
                exp_reason.hash(state);
                lhs_reason.hash(state);
                lhs_expression.hash(state);
                opt_methodcalltype.hash(state);
                voided_out_collector.hash(state);
                return_hint.hash(state);
                specialized_callee.hash(state);
            }
            OptMethodAction::OptNoMethodAction(a) => {
                a.hash(state);
            }
        }
    }
}

impl<CX: ContextFamily> PartialOrd for OptMethodAction<CX> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<CX: ContextFamily> Ord for OptMethodAction<CX> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        fn variant_index<CX: ContextFamily>(v: &OptMethodAction<CX>) -> u32 {
            match v {
                OptMethodAction::OptCallM(..) => 0,
                OptMethodAction::OptChainM(..) => 1,
                OptMethodAction::OptNoMethodAction(..) => 2,
            }
        }
        let disc = variant_index(self).cmp(&variant_index(other));
        if disc != std::cmp::Ordering::Equal {
            return disc;
        }
        match (self, other) {
            (
                OptMethodAction::OptCallM(box OptCallMData {
                    opt_methodcalltype: a1,
                    return_hint: b1,
                    specialized_callee: c1,
                }),
                OptMethodAction::OptCallM(box OptCallMData {
                    opt_methodcalltype: a2,
                    return_hint: b2,
                    specialized_callee: c2,
                }),
            ) => a1.cmp(a2).then_with(|| b1.cmp(b2)).then_with(|| c1.cmp(c2)),
            (
                OptMethodAction::OptChainM(box OptChainMData {
                    exp_reason: a1,
                    lhs_reason: b1,
                    lhs_expression: g1,
                    opt_methodcalltype: c1,
                    voided_out_collector: d1,
                    return_hint: e1,
                    specialized_callee: f1,
                }),
                OptMethodAction::OptChainM(box OptChainMData {
                    exp_reason: a2,
                    lhs_reason: b2,
                    lhs_expression: g2,
                    opt_methodcalltype: c2,
                    voided_out_collector: d2,
                    return_hint: e2,
                    specialized_callee: f2,
                }),
            ) => a1
                .cmp(a2)
                .then_with(|| b1.cmp(b2))
                .then_with(|| g1.cmp(g2))
                .then_with(|| c1.cmp(c2))
                .then_with(|| d1.cmp(d2))
                .then_with(|| e1.cmp(e2))
                .then_with(|| f1.cmp(f2)),
            (OptMethodAction::OptNoMethodAction(a1), OptMethodAction::OptNoMethodAction(a2)) => {
                a1.cmp(a2)
            }
            _ => std::cmp::Ordering::Equal,
        }
    }
}

impl<CX: ContextFamily> std::fmt::Debug for OptMethodAction<CX> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OptMethodAction::OptCallM(box OptCallMData {
                opt_methodcalltype,
                return_hint,
                specialized_callee,
            }) => f
                .debug_struct("OptCallM")
                .field("opt_methodcalltype", opt_methodcalltype)
                .field("return_hint", return_hint)
                .field("specialized_callee", specialized_callee)
                .finish(),
            OptMethodAction::OptChainM(box OptChainMData {
                exp_reason,
                lhs_reason,
                lhs_expression,
                opt_methodcalltype,
                voided_out_collector,
                return_hint,
                specialized_callee,
            }) => f
                .debug_struct("OptChainM")
                .field("exp_reason", exp_reason)
                .field("lhs_reason", lhs_reason)
                .field("lhs_expression", lhs_expression)
                .field("opt_methodcalltype", opt_methodcalltype)
                .field("voided_out_collector", voided_out_collector)
                .field("return_hint", return_hint)
                .field("specialized_callee", specialized_callee)
                .finish(),
            OptMethodAction::OptNoMethodAction(a) => {
                f.debug_tuple("OptNoMethodAction").field(a).finish()
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PredicateInner {
    AndP(Predicate, Predicate),
    OrP(Predicate, Predicate),
    NotP(Predicate),
    BinaryP(BinaryTest, Type),
    TruthyP,
    NullP,
    MaybeP,
    SingletonBoolP(Box<(ALoc, bool)>),
    SingletonStrP(Box<(ALoc, bool, String)>),
    SingletonNumP(Box<(ALoc, bool, NumberLiteral)>),
    SingletonBigIntP(Box<(ALoc, bool, BigIntLiteral)>),
    BoolP(Box<ALoc>),
    FunP,
    NumP(Box<ALoc>),
    BigIntP(Box<ALoc>),
    ObjP,
    StrP(Box<ALoc>),
    SymbolP(Box<ALoc>),
    VoidP,
    ArrP,
    ArrLenP {
        op: ArrayLengthOp,
        n: i32,
    },
    PropExistsP {
        propname: FlowSmolStr,
        reason: Reason,
    },
    // `if (a.b)` flows `a` to `PredicateT(PropTruthyP("b"), tout)`
    PropTruthyP(FlowSmolStr, Reason),
    // `if (a?.b === null)` flows `a` to `PredicateT(PropIsExactlyNullP("b"), tout)`
    PropIsExactlyNullP(FlowSmolStr, Reason),
    // `if (a?.b !== undefined)` flows `a` to `PredicateT(PropNonVoidP("b"), tout)`
    PropNonVoidP(FlowSmolStr, Reason),
    // `if (a.b?.c)` flows `a` to `PredicateT(PropNonMaybeP("b"), tout)`
    PropNonMaybeP(FlowSmolStr, Reason),
    // Encondes the latent predicate associated with the [index]-th parameter
    // of the function in type [t]. We also include information for all type arguments
    // and argument types of the call, to enable polymorphic calls.
    LatentP(Box<PredFuncallInfo>, Rc<[i32]>),
    LatentThisP(Box<PredFuncallInfo>),
    ImpossibleP,
}

#[derive(Debug, Clone, Dupe, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Predicate(Rc<PredicateInner>);

impl std::ops::Deref for Predicate {
    type Target = PredicateInner;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Predicate {
    pub fn new(inner: PredicateInner) -> Self {
        Self(Rc::new(inner))
    }

    pub fn ptr_eq(&self, other: &Predicate) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PredicateConcretetizerVariant {
    ConcretizeForGeneralPredicateTest,
    ConcretizeKeepOptimizedUnions,
    ConcretizeRHSForInstanceOfPredicateTest,
    ConcretizeRHSForLiteralPredicateTest,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BinaryTest {
    // e1 instanceof e2
    InstanceofTest,
    // e1.key === e2
    SentinelProp(FlowSmolStr),
    // e1 === e2
    EqTest,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ArrayLengthOp {
    ArrLenEqual,
    ArrLenGreaterThanEqual,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Literal {
    Truthy,
    AnyLiteral,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MixedFlavor {
    MixedEverything,
    MixedTruthy,
    MixedNonMaybe,
    MixedNonNull,
    MixedNonVoid,
    MixedFunction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AnySource {
    CatchAny,
    AnnotatedAny,
    AnyError(Option<AnyErrorKind>),
    Unsound(UnsoundnessKind),
    Untyped,
    Placeholder,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AnyErrorKind {
    UnresolvedName,
    MissingAnnotation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
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

#[derive(Debug, Clone, Dupe, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FunParam(pub Option<FlowSmolStr>, pub Type);

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FunRestParam(pub Option<FlowSmolStr>, pub ALoc, pub Type);

// used by FunT
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FunType {
    pub this_t: (Type, ThisStatus),
    pub params: Rc<[FunParam]>,
    pub rest_param: Option<FunRestParam>,
    pub return_t: Type,
    pub type_guard: Option<TypeGuard>,
    pub def_reason: Reason,
    pub function_reference_kind: FunctionReferenceKind,
    pub effect_: ReactEffectType,
    pub strictness_kind: TypeStrictnessKind,
    pub literal: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ReactEffectType {
    HookDecl(ALocId),
    HookAnnot,
    ArbitraryEffect,
    AnyEffect,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TypeGuardInner {
    pub inferred: bool,
    pub reason: Reason,
    pub param_name: (ALoc, FlowSmolStr),
    pub type_guard: Option<Type>,
    pub kind: TypeGuardKind,
}

impl TypeGuardInner {
    pub fn one_sided(&self) -> bool {
        !matches!(self.kind, TypeGuardKind::Default)
    }

    pub fn is_asserts(&self) -> bool {
        matches!(self.kind, TypeGuardKind::Asserts)
    }
}

#[derive(Debug, Clone, Dupe, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TypeGuard(Rc<TypeGuardInner>);

impl Deref for TypeGuard {
    type Target = TypeGuardInner;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl TypeGuard {
    pub fn new(inner: TypeGuardInner) -> Self {
        Self(Rc::new(inner))
    }

    pub fn ptr_eq(&self, other: &TypeGuard) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

/// FunTs distinguish methods from function values so method-to-method subtyping
/// can be lenient while calls still check the concrete `this` parameter.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ThisStatus {
    ThisMethod,
    ThisMethodWithExplicitThis,
    ThisFunction,
}

impl ThisStatus {
    pub fn is_method(&self) -> bool {
        matches!(self, Self::ThisMethod | Self::ThisMethodWithExplicitThis)
    }
}

/// A CallT constructor can be used to compute hints in calls to IntersectionTs.
/// (See `synthesis_speculation_call` in type_hint.rs.) We use speculation_hint_state
/// to keep track of the various states of hint computation during speculation.
/// The state is initialized to the "unset" phase. If an overload succeeds, it is
/// recorded along with the speculation path (list of speculation_ids that led
/// to this choice) in the "set" constructor. For each subsequent success there
/// are two cases:
/// (i) The successful speculation id belongs to the "set" speculation path (we
/// are basically popping off a long speculation path). In this case, the state
/// remains the same.
/// (ii) The successful speculation id has not been recorded in "set". This choice
/// is a sibling of the currently "set" choice. This is possible thanks to the
/// special behavior of ConcretizeT with union-like types. In this case, we
/// will only accept the overload if all sibling branches agree on the result.
/// Otherwise the result is deemed "invalid".
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SpeculationHintSetData(pub Rc<[i32]>, pub Type);

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SpeculationHintState {
    SpeculationHintUnset,
    SpeculationHintInvalid,
    SpeculationHintSet(Box<SpeculationHintSetData>),
}

#[derive(Debug, Clone, Copy, Dupe, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SpecState {
    pub speculation_id: i32,
    pub case_id: i32,
}

/// This mutable structure will collect information on the specialized form of the
/// callee type of a function call. For example, when calling a function with type
/// `<X>(x: Array<X>) => X`, with `[""]`, it will collect `(x: Array<string>) => string`.
/// This type will be used to populate the TAST for the callee expression.
///
/// When recording types not under speculation, then the results will be appended
/// to the [finalized] list.
///
/// Under speculation we can't really commit to the specialized function we are
/// calling during a speculative branch. We record the type as a "speculative
/// candidate", and include the speculative state under which this addition was
/// made. After speculation is done (at the "fire action" part of Speculation_kit)
/// we determine if the branch was successful, and if so, we promote the type to
/// the finalized list.
///
/// Note that for signature-help results we do not record types under speculation
/// so we only need a simple list to record signatures.
#[derive(Debug, Clone)]
pub struct SpecializedCallee {
    pub finalized: Rc<RefCell<std::collections::VecDeque<Type>>>,
    pub speculative_candidates: Rc<RefCell<std::collections::VecDeque<(Type, SpecState)>>>,
    pub init_speculation_state: Option<SpecState>,
    pub sig_help: Rc<RefCell<std::collections::VecDeque<Type>>>,
}

impl PartialEq for SpecializedCallee {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == std::cmp::Ordering::Equal
    }
}
impl Eq for SpecializedCallee {}

impl PartialOrd for SpecializedCallee {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SpecializedCallee {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.finalized
            .borrow()
            .cmp(&*other.finalized.borrow())
            .then_with(|| {
                self.speculative_candidates
                    .borrow()
                    .cmp(&*other.speculative_candidates.borrow())
            })
            .then_with(|| {
                self.init_speculation_state
                    .cmp(&other.init_speculation_state)
            })
            .then_with(|| self.sig_help.borrow().cmp(&*other.sig_help.borrow()))
    }
}

impl std::hash::Hash for SpecializedCallee {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.finalized.borrow().hash(state);
        self.speculative_candidates.borrow().hash(state);
        self.init_speculation_state.hash(state);
        self.sig_help.borrow().hash(state);
    }
}

// Used by CallT and similar constructors
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FuncallType {
    pub call_this_t: Type,
    pub call_targs: Option<Rc<[Targ]>>,
    pub call_args_tlist: Rc<[CallArg]>,
    pub call_tout: Tvar,
    pub call_strict_arity: bool,
    pub call_speculation_hint_state: Option<Rc<RefCell<SpeculationHintState>>>,
    pub call_specialized_callee: Option<SpecializedCallee>,
}

impl PartialOrd for FuncallType {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for FuncallType {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.call_this_t
            .cmp(&other.call_this_t)
            .then_with(|| self.call_targs.cmp(&other.call_targs))
            .then_with(|| self.call_args_tlist.cmp(&other.call_args_tlist))
            .then_with(|| self.call_tout.cmp(&other.call_tout))
            .then_with(|| self.call_strict_arity.cmp(&other.call_strict_arity))
            .then_with(|| {
                match (
                    &self.call_speculation_hint_state,
                    &other.call_speculation_hint_state,
                ) {
                    (None, None) => std::cmp::Ordering::Equal,
                    (None, Some(_)) => std::cmp::Ordering::Less,
                    (Some(_), None) => std::cmp::Ordering::Greater,
                    (Some(a), Some(b)) => a.borrow().cmp(&*b.borrow()),
                }
            })
            .then_with(|| {
                self.call_specialized_callee
                    .cmp(&other.call_specialized_callee)
            })
    }
}

impl std::hash::Hash for FuncallType {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.call_this_t.hash(state);
        self.call_targs.hash(state);
        self.call_args_tlist.hash(state);
        self.call_tout.hash(state);
        self.call_strict_arity.hash(state);
        self.call_speculation_hint_state
            .as_ref()
            .map(|r| r.borrow().clone())
            .hash(state);
        self.call_specialized_callee.hash(state);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MethodCallType {
    pub meth_generic_this: Option<Type>,
    pub meth_targs: Option<Rc<[Targ]>>,
    pub meth_args_tlist: Rc<[CallArg]>,
    pub meth_tout: Tvar,
    pub meth_strict_arity: bool,
}

#[derive(Debug, Clone, Dupe, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Targ {
    /// This tvar gets lower bounds from the instantiations of _. It is used to power type-services
    /// like type-at-pos and should not be used for type checking
    ImplicitArg(Tvar),
    ExplicitArg(Type),
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OptFuncallType(
    pub Type,
    pub Option<Rc<[Targ]>>,
    pub Rc<[CallArg]>,
    pub bool,
    pub Option<SpecializedCallee>,
);

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OptMethodCallType {
    pub opt_meth_generic_this: Option<Type>,
    pub opt_meth_targs: Option<Rc<[Targ]>>,
    pub opt_meth_args_tlist: Rc<[CallArg]>,
    pub opt_meth_strict_arity: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CallArgInner {
    Arg(Type),
    SpreadArg(Type),
}

#[derive(Debug, Clone, Dupe, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CallArg(Rc<CallArgInner>);

impl Deref for CallArg {
    type Target = CallArgInner;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl CallArg {
    pub fn new(inner: CallArgInner) -> Self {
        Self(Rc::new(inner))
    }

    pub fn arg(t: Type) -> Self {
        Self::new(CallArgInner::Arg(t))
    }

    pub fn spread_arg(t: Type) -> Self {
        Self::new(CallArgInner::SpreadArg(t))
    }

    pub fn ptr_eq(&self, other: &CallArg) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PredFuncallInfo(
    pub UseOp,
    pub ALoc,
    pub Type,
    pub Option<Rc<[Targ]>>,
    pub Rc<[CallArg]>,
);

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
pub enum DroType {
    HookReturn,
    HookArg,
    Props,
    DebugAnnot,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ReactDro(pub ALoc, pub DroType);

#[derive(Debug, Clone, Copy, Dupe, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TupleViewKind {
    ArrayLiteral,
    RestArguments,
}

#[derive(Debug, Clone, Copy, Dupe, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ArrayLiteralKind {
    Mutable,
    Const,
    RestArguments,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TupleView {
    pub kind: TupleViewKind,
    pub elements: Rc<[TupleElement]>,
    pub arity: (i32, i32),
    pub inexact: bool,
    /// A trailing rest element, as in `TupleATData`.
    pub rest: Option<TupleElement>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ArrayATData {
    ///Should elements of this array be treated as propagating read-only, and if so, what location is responsible
    pub react_dro: Option<ReactDro>,
    pub elem_t: Type,
    pub tuple_view: Option<TupleView>,
    pub strictness_kind: TypeStrictnessKind,
    /// Literal elements can be generalized by natural inference.
    pub literal: bool,
    /// Fresh mutable arrays can be checked covariantly.
    pub fresh: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TupleATData {
    pub react_dro: Option<ReactDro>,
    /// The union of the types of `elements` and `rest`.
    pub elem_t: Type,
    pub elements: Rc<[TupleElement]>,
    /// Arity represents the range of valid arities of `elements`, considering optional elements.
    /// It ranges from the number of required elements, to the total number of elements.
    pub arity: (i32, i32),
    pub inexact: bool,
    /// A trailing rest element, as `...Array<T>` in `[string, ...Array<T>]`. Its `t` is the
    /// element type `T`, and it is never `optional`. A tuple with a rest element is not `inexact`.
    pub rest: Option<TupleElement>,
    pub strictness_kind: TypeStrictnessKind,
    pub literal: bool,
    pub fresh: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ArrType {
    ArrayAT(Box<ArrayATData>),
    /// TupleAT carries `elem_t` alongside `elements`. Why do tuples carry around `elem_t`? Well, so
    /// that they don't need to recompute their general type when you do
    /// `myTuple[expr]`
    TupleAT(Box<TupleATData>),
    /// ROArrayAT(elemt) is the super type for all tuples and arrays for which
    /// elemt is a supertype of every element type
    ROArrayAT(Box<ROArrayATData>),
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ROArrayATData {
    pub elem_t: Type,
    pub react_dro: Option<ReactDro>,
    pub literal: bool,
}

impl ArrType {
    pub fn literal(&self) -> bool {
        match self {
            Self::ArrayAT(a) => a.literal,
            Self::TupleAT(a) => a.literal,
            Self::ROArrayAT(a) => a.literal,
        }
    }
    pub fn fresh(&self) -> bool {
        match self {
            Self::ArrayAT(a) => a.fresh,
            Self::TupleAT(a) => a.fresh,
            Self::ROArrayAT(_) => false,
        }
    }
    pub fn with_literal(mut self, literal: bool, fresh: bool) -> Self {
        match &mut self {
            Self::ArrayAT(a) => {
                a.literal = literal;
                a.fresh = fresh;
            }
            Self::TupleAT(a) => {
                a.literal = literal;
                a.fresh = fresh;
            }
            Self::ROArrayAT(a) => a.literal = literal,
        }
        self
    }
}

#[derive(Debug, Clone, Dupe, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TupleElement {
    pub reason: Reason,
    pub name: Option<FlowSmolStr>,
    pub t: Type,
    pub polarity: Polarity,
    pub optional: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ObjType {
    pub flags: Flags,
    pub props_tmap: properties::Id,
    pub proto_t: Type,
    pub call_t: Option<i32>,
    /// reachable_targs is populated during substitution. We use those reachable
    /// targs to avoid traversing the full objtype structure during any-propagation
    /// and instead pollute the reachable_targs directly.
    pub reachable_targs: Rc<[(Type, Polarity)]>,
    pub strictness_kind: TypeStrictnessKind,
    pub literal: bool,
}

/// Object.assign(target, source1, ...source2) first resolves target then the sources.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ObjAssignKind {
    /// Obj.assign(target, source) with flag indicating whether source must be exact
    ObjAssign { assert_exact: bool },
    /// Obj.assign(target, ...source)
    ObjSpreadAssign,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NamespaceType {
    pub namespace_symbol: Symbol,
    pub values_type: Type,
    pub types_tmap: properties::Id,
}

pub enum Cont<CX: ContextFamily = ()> {
    Lower(UseOp, Type),
    Upper(Box<UseT<CX>>),
    /// Collects the computed result without adding a subtyping constraint.
    Collect(type_collector::TypeCollector),
}

impl<CX: ContextFamily> Clone for Cont<CX> {
    fn clone(&self) -> Self {
        match self {
            Cont::Lower(a, b) => Cont::Lower(a.clone(), b.clone()),
            Cont::Upper(a) => Cont::Upper(a.clone()),
            Cont::Collect(a) => Cont::Collect(a.dupe()),
        }
    }
}

impl<CX: ContextFamily> PartialEq for Cont<CX> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Cont::Lower(a1, b1), Cont::Lower(a2, b2)) => a1 == a2 && b1 == b2,
            (Cont::Upper(a1), Cont::Upper(a2)) => a1 == a2,
            (Cont::Collect(a1), Cont::Collect(a2)) => a1 == a2,
            _ => false,
        }
    }
}

impl<CX: ContextFamily> Eq for Cont<CX> {}

impl<CX: ContextFamily> std::hash::Hash for Cont<CX> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self).hash(state);
        match self {
            Cont::Lower(a, b) => {
                a.hash(state);
                b.hash(state);
            }
            Cont::Upper(a) => {
                a.hash(state);
            }
            Cont::Collect(a) => a.hash(state),
        }
    }
}

impl<CX: ContextFamily> PartialOrd for Cont<CX> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<CX: ContextFamily> Ord for Cont<CX> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        fn variant_index<CX: ContextFamily>(v: &Cont<CX>) -> u32 {
            match v {
                Cont::Lower(..) => 0,
                Cont::Upper(..) => 1,
                Cont::Collect(..) => 2,
            }
        }
        let disc = variant_index(self).cmp(&variant_index(other));
        if disc != std::cmp::Ordering::Equal {
            return disc;
        }
        match (self, other) {
            (Cont::Lower(a1, b1), Cont::Lower(a2, b2)) => a1.cmp(a2).then_with(|| b1.cmp(b2)),
            (Cont::Upper(a1), Cont::Upper(a2)) => a1.cmp(a2),
            (Cont::Collect(a1), Cont::Collect(a2)) => a1.cmp(a2),
            _ => std::cmp::Ordering::Equal,
        }
    }
}

impl<CX: ContextFamily> std::fmt::Debug for Cont<CX> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Cont::Lower(a, b) => f.debug_tuple("Lower").field(a).field(b).finish(),
            Cont::Upper(a) => f.debug_tuple("Upper").field(a).finish(),
            Cont::Collect(a) => f.debug_tuple("Collect").field(a).finish(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DerivedType {
    pub own: FlowOrdMap<Name, Property>,
    pub proto: FlowOrdMap<Name, Property>,
    pub static_: FlowOrdMap<Name, Property>,
    pub strictness_kind: TypeStrictnessKind,
    pub reference: DerivedReference,
    /// The instance type of the class or interface whose members are checked.
    pub instance: Type,
}

/// How errors refer to the class or interface whose members are checked.
#[derive(Debug, Clone, Dupe, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DerivedReference {
    Named(FlowSmolStr),
    /// An anonymous class or an inline interface, by its instance type.
    Unnamed(Type),
}

/// LookupT is a general-purpose tool for traversing prototype chains in search
/// of properties. In all cases, if the property is found somewhere along the
/// prototype chain, the property type will unify with the output tvar. Lookup
/// kinds control what happens when a property is not found.
///
/// Strict
///   If the property is not found, emit an error against the stored type, the
///   object the lookup started from.
///
/// NonstrictReturning(NonstrictReturningData(None, _))
///   If the property is not found, do nothing. Note that lookups of this kind
///   will not add any constraints to the output tvar.
///
/// NonstrictReturning(NonstrictReturningData(Some((default, result)), _))
///   If the property is not found, deliver the default type to `result`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NonstrictReturningData(
    pub Option<(Type, LookupDefaultResult)>,
    pub Option<(i32, (Reason, Type))>,
);

/// How a lookup uses its default type when no property is found.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LookupDefaultResult {
    Unify(Type),
    Flow { use_op: UseOp, tout: Type },
    Lookup(Box<LookupTData>),
}

/// Controls how a property lookup terminates after traversing the prototype chain.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LookupKind {
    /// Reports a missing-property error against the stored object type if the lookup fails.
    Strict(Type),
    /// Does not report a missing-property error. On failure, it may return a default
    /// type and record a property-test miss.
    NonstrictReturning(Box<NonstrictReturningData>),
}

/// An indexer property that matched a named access, deferred until the lookup reaches
/// the end of the prototype chain. See [`LookupTData::indexer_fallback`].
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct IndexerFallbackData {
    pub property: Property,
    /// The instance the indexer was declared on.
    pub obj: Type,
    /// TypeScript resolves named access through a string indexer silently, so a
    /// declared property is genuinely not required whenever either end of the access is
    /// TypeScript. Pure Flow code still reports the property as missing and only reuses
    /// the indexer's value type for downstream checking.
    pub suppress_missing_error: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ReadPropData {
    pub use_op: UseOp,
    pub obj_t: Type,
    pub tout: Tvar,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WritePropData {
    pub use_op: UseOp,
    pub obj_t: Type,
    pub prop_tout: Option<Type>,
    pub tin: Type,
    pub write_ctx: WriteCtx,
    pub mode: SetMode,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LookupPropsForSubtypingData {
    pub use_op: UseOp,
    /// The upper properties the lower type must provide, each paired with the
    /// reference it is looked up under. Non-empty, and the enclosing lookup's
    /// own `propref` is the reference of the first entry. Properties looked up
    /// together stay together as the lookup walks the prototype chain, so the
    /// ones that are never found are reported as a single error.
    pub props: Rc<[(PropRef, Property)]>,
    pub strictness_kind: TypeStrictnessKind,
    pub lower: Type,
    pub upper: Type,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LookupActionMatchPropData {
    pub use_op: UseOp,
    pub drop_generic: bool,
    pub prop_t: Type,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LookupAction {
    ReadProp(Box<ReadPropData>),
    WriteProp(Box<WritePropData>),
    LookupPropForTvarPopulation { polarity: Polarity, tout: Type },
    LookupPropsForSubtyping(Box<LookupPropsForSubtypingData>),
    SuperProp(Box<(UseOp, Property, TypeStrictnessKind, DerivedReference)>),
    MatchProp(Box<LookupActionMatchPropData>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum WriteCtx {
    ThisInCtor,
    Normal,
}

/// Property writes can either be assignments (from `a.x = e`) or deletions
/// (from `delete a.x`). For the most part, we can treat these the same
/// (flowing void into `a.x` in a deletion), but if the property being deleted
/// originates in an indexer, we need to know not to flow `void` into the
/// indexer's type, which would cause an error. The `set_mode` type records
/// whether a property write is an assignment or a deletion, to help handle
/// this special case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SetMode {
    Delete,
    Assign,
}

/// See the above comment on `set_mode`--this type is the other half, which
/// records whether a property originates in an indexer or from a property
/// map. This is relevant when the property is being deleted--we should flow
/// `void` to the property's type if it originates in a property map (to ensure
/// that its type is nullable and raise an error if it's not) but not if it's
/// from an indexer, where our current semantics are intentionally unsound with
/// respect to undefined anyways.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PropertySource {
    DynamicProperty,
    PropertyMapProperty,
    IndexerProperty,
}

/// WriteElem has a `tout` parameter to serve as a trigger for ordering
/// operations. We only need this in one place: object literal initialization.
/// In particular, a computed property in the object initializer users SetElemT
/// to initialize the property value, but in order to avoid race conditions we
/// need to ensure that reads happen after writes.
pub struct ReadElemData {
    pub id: Option<i32>,
    pub from_annot: bool,
    pub skip_optional: bool,
    pub access_iterables: bool,
    pub tout: Tvar,
}

pub struct WriteElemData {
    pub tin: Type,
    pub tout: Option<Type>,
    pub mode: SetMode,
}

pub enum ElemAction<CX: ContextFamily = ()> {
    ReadElem(Box<ReadElemData>),
    WriteElem(Box<WriteElemData>),
    CallElem(Reason, Box<MethodAction<CX>>),
}

impl<CX: ContextFamily> Clone for ElemAction<CX> {
    fn clone(&self) -> Self {
        match self {
            ElemAction::ReadElem(box ReadElemData {
                id,
                from_annot,
                skip_optional,
                access_iterables,
                tout,
            }) => ElemAction::ReadElem(Box::new(ReadElemData {
                id: *id,
                from_annot: *from_annot,
                skip_optional: *skip_optional,
                access_iterables: *access_iterables,
                tout: tout.clone(),
            })),
            ElemAction::WriteElem(box WriteElemData { tin, tout, mode }) => {
                ElemAction::WriteElem(Box::new(WriteElemData {
                    tin: tin.clone(),
                    tout: tout.clone(),
                    mode: mode.clone(),
                }))
            }
            ElemAction::CallElem(a, b) => ElemAction::CallElem(a.clone(), b.clone()),
        }
    }
}

impl<CX: ContextFamily> PartialEq for ElemAction<CX> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (
                ElemAction::ReadElem(box ReadElemData {
                    id: a1,
                    from_annot: b1,
                    skip_optional: c1,
                    access_iterables: d1,
                    tout: e1,
                }),
                ElemAction::ReadElem(box ReadElemData {
                    id: a2,
                    from_annot: b2,
                    skip_optional: c2,
                    access_iterables: d2,
                    tout: e2,
                }),
            ) => a1 == a2 && b1 == b2 && c1 == c2 && d1 == d2 && e1 == e2,
            (
                ElemAction::WriteElem(box WriteElemData {
                    tin: a1,
                    tout: b1,
                    mode: c1,
                }),
                ElemAction::WriteElem(box WriteElemData {
                    tin: a2,
                    tout: b2,
                    mode: c2,
                }),
            ) => a1 == a2 && b1 == b2 && c1 == c2,
            (ElemAction::CallElem(a1, b1), ElemAction::CallElem(a2, b2)) => a1 == a2 && b1 == b2,
            _ => false,
        }
    }
}

impl<CX: ContextFamily> Eq for ElemAction<CX> {}

impl<CX: ContextFamily> std::hash::Hash for ElemAction<CX> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self).hash(state);
        match self {
            ElemAction::ReadElem(box ReadElemData {
                id,
                from_annot,
                skip_optional,
                access_iterables,
                tout,
            }) => {
                id.hash(state);
                from_annot.hash(state);
                skip_optional.hash(state);
                access_iterables.hash(state);
                tout.hash(state);
            }
            ElemAction::WriteElem(box WriteElemData { tin, tout, mode }) => {
                tin.hash(state);
                tout.hash(state);
                mode.hash(state);
            }
            ElemAction::CallElem(a, b) => {
                a.hash(state);
                b.hash(state);
            }
        }
    }
}

impl<CX: ContextFamily> PartialOrd for ElemAction<CX> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<CX: ContextFamily> Ord for ElemAction<CX> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        fn variant_index<CX: ContextFamily>(v: &ElemAction<CX>) -> u32 {
            match v {
                ElemAction::ReadElem(..) => 0,
                ElemAction::WriteElem(..) => 1,
                ElemAction::CallElem(..) => 2,
            }
        }
        let disc = variant_index(self).cmp(&variant_index(other));
        if disc != std::cmp::Ordering::Equal {
            return disc;
        }
        match (self, other) {
            (
                ElemAction::ReadElem(box ReadElemData {
                    id: a1,
                    from_annot: b1,
                    skip_optional: c1,
                    access_iterables: d1,
                    tout: e1,
                }),
                ElemAction::ReadElem(box ReadElemData {
                    id: a2,
                    from_annot: b2,
                    skip_optional: c2,
                    access_iterables: d2,
                    tout: e2,
                }),
            ) => a1
                .cmp(a2)
                .then_with(|| b1.cmp(b2))
                .then_with(|| c1.cmp(c2))
                .then_with(|| d1.cmp(d2))
                .then_with(|| e1.cmp(e2)),
            (
                ElemAction::WriteElem(box WriteElemData {
                    tin: a1,
                    tout: b1,
                    mode: c1,
                }),
                ElemAction::WriteElem(box WriteElemData {
                    tin: a2,
                    tout: b2,
                    mode: c2,
                }),
            ) => a1.cmp(a2).then_with(|| b1.cmp(b2)).then_with(|| c1.cmp(c2)),
            (ElemAction::CallElem(a1, b1), ElemAction::CallElem(a2, b2)) => {
                a1.cmp(a2).then_with(|| b1.cmp(b2))
            }
            _ => std::cmp::Ordering::Equal,
        }
    }
}

impl<CX: ContextFamily> std::fmt::Debug for ElemAction<CX> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ElemAction::ReadElem(box ReadElemData {
                id,
                from_annot,
                skip_optional,
                access_iterables,
                tout,
            }) => f
                .debug_struct("ReadElem")
                .field("id", id)
                .field("from_annot", from_annot)
                .field("skip_optional", skip_optional)
                .field("access_iterables", access_iterables)
                .field("tout", tout)
                .finish(),
            ElemAction::WriteElem(box WriteElemData { tin, tout, mode }) => f
                .debug_struct("WriteElem")
                .field("tin", tin)
                .field("tout", tout)
                .field("mode", mode)
                .finish(),
            ElemAction::CallElem(a, b) => f.debug_tuple("CallElem").field(a).field(b).finish(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PropRef {
    Named {
        reason: Reason,
        name: Name,
        from_indexed_access: bool,
    },
    Computed(Type),
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ObjKind {
    Exact,
    Inexact,
    Indexed(DictType),
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Flags {
    pub react_dro: Option<ReactDro>,
    pub obj_kind: ObjKind,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DictType {
    pub dict_name: Option<FlowSmolStr>,
    pub key: Type,
    pub value: Type,
    pub dict_polarity: Polarity,
}

/// key_loc refer to the location of the identifier, if one exists,
/// preferred_def_locs refer to the (potentially multiple) def_loc that go-to-definition should
/// jump to. If the field is None, go-to-definition will jump to key_loc. The field should only
/// be populated if multiple def_locs make sense, or if we want to track fields where we want to
/// make an extra go-to-def jump for better UX.
#[derive(Clone, Dupe)]
pub struct Property(Rc<PropertyInner>);

impl std::hash::Hash for Property {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

impl PartialEq for Property {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0) || self.0 == other.0
    }
}

impl Eq for Property {}

impl PartialOrd for Property {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Property {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        if Rc::ptr_eq(&self.0, &other.0) {
            std::cmp::Ordering::Equal
        } else {
            self.0.cmp(&other.0)
        }
    }
}

impl Deref for Property {
    type Target = PropertyInner;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::fmt::Debug for Property {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl Property {
    pub fn new(inner: PropertyInner) -> Self {
        Self(Rc::new(inner))
    }

    pub fn ptr_eq(&self, other: &Property) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FieldData {
    pub preferred_def_locs: Option<Vec1<ALoc>>,
    pub key_loc: Option<ALoc>,
    pub type_: Type,
    pub polarity: Polarity,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GetSetData {
    pub get_key_loc: Option<ALoc>,
    pub get_type: Type,
    pub set_key_loc: Option<ALoc>,
    pub set_type: Type,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PropertyInner {
    Field(Box<FieldData>),
    Get { key_loc: Option<ALoc>, type_: Type },
    Set { key_loc: Option<ALoc>, type_: Type },
    GetSet(Box<GetSetData>),
    Method { key_loc: Option<ALoc>, type_: Type },
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PropertyType {
    /// A field that is defined in the source file as a field.
    OrdinaryField { type_: Type, polarity: Polarity },
    /// Some properties that are normalized to a field.
    SyntheticField {
        get_type: Option<Type>,
        set_type: Option<Type>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NamedSymbolInner {
    pub name_loc: Option<ALoc>,
    pub preferred_def_locs: Option<Vec1<ALoc>>,
    pub type_: Type,
    /// Optional [_for_extends] variant of [type_], populated only for re-export
    /// chains ([export type {X} from ...] and [export {X} from ...]) so that
    /// barrel-style re-exports preserve polymorphic [this]. When [Some], it
    /// carries the un-canonicalized exporter-side type (a
    /// [ClassT(ThisInstanceT(..))], possibly under a [PolyT]) suitable for
    /// [this_typeapp] wrapping in [extends]/[implements] consumers. When
    /// [None], consumers fall back to [type_]. The regular import path always
    /// reads [type_]; only the [_for_extends] read paths consult this field.
    pub type_for_extends: Option<Type>,
}

/// Rc-wrapped NamedSymbol for smaller B-tree entries (8 bytes vs 64 bytes).
/// Implements Deref to NamedSymbolInner so field access works transparently.
#[derive(Debug, Clone)]
pub struct NamedSymbol(Rc<NamedSymbolInner>);

impl NamedSymbol {
    pub fn new(
        name_loc: Option<ALoc>,
        preferred_def_locs: Option<Vec1<ALoc>>,
        type_: Type,
        type_for_extends: Option<Type>,
    ) -> Self {
        NamedSymbol(Rc::new(NamedSymbolInner {
            name_loc,
            preferred_def_locs,
            type_,
            type_for_extends,
        }))
    }

    pub fn ptr_eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl Dupe for NamedSymbol {}

impl std::ops::Deref for NamedSymbol {
    type Target = NamedSymbolInner;
    fn deref(&self) -> &NamedSymbolInner {
        &self.0
    }
}

impl PartialEq for NamedSymbol {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0) || self.0 == other.0
    }
}

impl Eq for NamedSymbol {}

impl PartialOrd for NamedSymbol {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for NamedSymbol {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        if Rc::ptr_eq(&self.0, &other.0) {
            std::cmp::Ordering::Equal
        } else {
            self.0.cmp(&other.0)
        }
    }
}

impl std::hash::Hash for NamedSymbol {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

// This has to go here so that Type doesn't depend on Scope
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ClassBinding {
    pub class_binding_id: ALocId,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InstTypeInner {
    pub class_name: Option<FlowSmolStr>,
    pub class_id: ALocId,
    pub type_args: Rc<[(SubstName, Reason, Type, Polarity)]>,
    pub own_props: properties::Id,
    pub proto_props: properties::Id,
    pub inst_call_t: Option<i32>,
    pub inst_construct_t: Option<i32>,
    pub initialized_fields: FlowOrdSet<Name>,
    pub initialized_static_fields: FlowOrdSet<Name>,
    pub inst_kind: InstanceKind,
    pub inst_dict: object::Dict,
    pub class_private_fields: properties::Id,
    pub class_private_static_fields: properties::Id,
    pub class_private_methods: properties::Id,
    pub class_private_static_methods: properties::Id,
    pub inst_react_dro: Option<ReactDro>,
    pub inst_abstract: bool,
    pub inst_abstract_props: FlowOrdSet<Name>,
    pub strictness_kind: TypeStrictnessKind,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InstType(Rc<InstTypeInner>);

impl Deref for InstType {
    type Target = InstTypeInner;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl InstType {
    pub fn new(inner: InstTypeInner) -> Self {
        Self(Rc::new(inner))
    }

    pub fn ptr_eq(&self, other: &InstType) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum InstanceKind {
    ClassKind,
    InterfaceKind {
        inline: bool,
        // When true, primitives flowing into this interface are rejected rather
        // than promoted to their boxed wrappers (the .ts-only consumer-keyed
        // promotion in subtyping_kit.rs). Set for the `object` builtin, which
        // models TS's `object` type and must exclude every primitive value
        // regardless of the consuming file.
        reject_primitives: bool,
    },
    RecordKind {
        defaulted_props: FlowOrdSet<FlowSmolStr>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InstanceTInner {
    pub inst: InstType,
    pub static_: Type,
    pub super_: Type,
    pub implements: Rc<[Type]>,
}

#[derive(Debug, Clone, Dupe, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InstanceT(Rc<InstanceTInner>);

impl Deref for InstanceT {
    type Target = InstanceTInner;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl InstanceT {
    pub fn new(inner: InstanceTInner) -> Self {
        Self(Rc::new(inner))
    }

    pub fn ptr_eq(&self, other: &InstanceT) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NominalTypeInner {
    pub nominal_id: nominal::Id,
    pub underlying_t: nominal::UnderlyingT,
    pub lower_t: Option<Type>,
    pub upper_t: Option<Type>,
    pub nominal_type_args: Rc<[(SubstName, Reason, Type, Polarity)]>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NominalType(Rc<NominalTypeInner>);

impl Deref for NominalType {
    type Target = NominalTypeInner;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl NominalType {
    pub fn new(inner: NominalTypeInner) -> Self {
        Self(Rc::new(inner))
    }

    pub fn ptr_eq(&self, other: &NominalType) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ExportTypes {
    /// tmap used to store individual, named ES exports as generated by `export`
    /// statements in a module.
    pub value_exports_tmap: exports::Id,
    /// Note that CommonJS modules may also populate this tmap if their export
    /// type is an object (that object's properties become named exports) or if
    /// it has any "type" exports via `export type ...`
    pub type_exports_tmap: exports::Id,
    /// This stores the CommonJS export type when applicable and is used as the
    /// exact return type for calls to require(). This slot doesn't apply to pure
    /// ES modules.
    pub cjs_export: Option<(Option<ALoc>, Type)>,
    /// Sometimes we claim the module exports any or Object, implying that it
    ///has every named export
    pub has_every_named_export: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ImportKind {
    ImportType,
    ImportTypeof,
    ImportValue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ExportKind {
    DirectExport,
    ReExport,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TypeParamInner {
    pub reason: Reason,
    pub name: SubstName,
    pub bound: Type,
    pub polarity: Polarity,
    pub default: Option<Type>,
    pub is_this: bool,
    pub is_const: bool,
}

#[derive(Debug, Clone, Dupe, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TypeParam(Rc<TypeParamInner>);

impl Deref for TypeParam {
    type Target = TypeParamInner;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl TypeParam {
    pub fn new(inner: TypeParamInner) -> Self {
        Self(Rc::new(inner))
    }

    pub fn ptr_eq(&self, other: &TypeParam) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

pub type TypeParams = Option<(ALoc, Vec1<TypeParam>)>;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ModuleTypeInner {
    pub module_reason: Reason,
    pub module_export_types: ExportTypes,
    pub module_is_strict: bool,
    pub module_available_platforms: Option<PlatformSet>,
}

#[derive(Debug, Clone, Dupe, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ModuleType(Rc<ModuleTypeInner>);

impl Deref for ModuleType {
    type Target = ModuleTypeInner;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl ModuleType {
    pub fn new(inner: ModuleTypeInner) -> Self {
        Self(Rc::new(inner))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Selector {
    Prop(FlowSmolStr, bool),
    Elem(Type),
    ObjRest(Rc<[FlowSmolStr]>),
    ArrRest(i32),
    Default,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DestructorSpreadTypeData(
    pub object::spread::Target,
    pub flow_data_structure_wrapper::list::FlowOcamlList<object::spread::Operand>,
    pub Option<object::spread::OperandSlice>,
);

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DestructorSpreadTupleTypeData {
    pub reason_tuple: Reason,
    pub reason_spread: Reason,
    pub inexact: bool,
    pub resolved: flow_data_structure_wrapper::list::FlowOcamlList<ResolvedParam>,
    pub unresolved: flow_data_structure_wrapper::list::FlowOcamlList<UnresolvedParam>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DestructorConditionalTypeData {
    pub distributive_tparam_name: Option<SubstName>,
    pub infer_tparams: Rc<[TypeParam]>,
    pub extends_t: Type,
    pub true_t: Type,
    pub false_t: Type,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DestructorMappedTypeData {
    /// Homomorphic mapped types use an inline keyof: {[key in keyof O]: T} or a type parameter
    /// bound by $Keys/keyof: type Homomorphic<Keys: $Keys<O>> = {[key in O]: T}
    pub homomorphic: MappedTypeHomomorphicFlag,
    pub distributive_tparam_name: Option<SubstName>,
    pub property_type: Type,
    /// Optional poly type over the same key tparam used by [property_type]. When [Some], each
    /// source key is substituted to produce the destination key (TS `as` clause).
    pub name_type: Option<Type>,
    pub mapped_type_flags: MappedTypeFlags,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Destructor {
    // Merges the members of an intersection into one type, or evaluates to the
    // intersection itself when they cannot be merged. `check_exact` reports
    // intersections that exact Flow objects make impossible. Only created under
    // `experimental.normalize_intersections`; see
    // `flow_typing_flow_js::intersection_kit`.
    NormalizeIntersection { check_exact: bool },
    NonMaybeType,
    PropertyType { name: Name },
    ElementType { index_type: Type },
    OptionalIndexedAccessNonMaybeType { index: OptionalIndexedAccessIndex },
    OptionalIndexedAccessResultType { void_reason: Reason },
    ExactType,
    ReadOnlyType,
    PartialType,
    RequiredType,
    SpreadType(Box<DestructorSpreadTypeData>),
    SpreadTupleType(Box<DestructorSpreadTupleTypeData>),
    RestType(object::rest::MergeMode, Type),
    ValuesType,
    ConditionalType(Box<DestructorConditionalTypeData>),
    TypeMap(TypeMap),
    ReactElementConfigType,
    ReactCheckComponentConfig { props: properties::PropertiesMap },
    ReactDRO(Box<ReactDro>),
    MappedType(Box<DestructorMappedTypeData>),
    EnumType,
}

#[derive(Debug, Clone, Dupe, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MappedTypeHomomorphicFlag {
    Homomorphic,
    SemiHomomorphic(Type),
    Unspecialized,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MappedTypeOptionality {
    MakeOptional,
    RemoveOptional,
    KeepOptionality,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MappedTypeVariance {
    OverrideVariance(Polarity),
    RemoveVariance(Polarity),
    KeepVariance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MappedTypeFlags {
    pub variance: MappedTypeVariance,
    pub optional: MappedTypeOptionality,
}

#[derive(Debug, Clone, Dupe, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum OptionalIndexedAccessIndex {
    OptionalIndexedAccessStrLitIndex(Name),
    OptionalIndexedAccessTypeIndex(Type),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TypeMap {
    ObjectKeyMirror,
}

/// Concretizers of resolved types: simplify types like EvalT, OpenT, TypeAppT,
/// etc. The order of the constructors below denotes the order in which the
/// respective catch-all cases appears in flow_js.rs.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ConcretizationKind {
    /// Capture results before unwrapping their constraint nodes.
    ConcretizeForLowerBounds,
    ConcretizeForCJSExtractNamedExportsAndTypeExports,
    ConcretizeForOptionalChain,
    ConcretizeForImportsExports,
    ConcretizeForInspection,
    ConcretizeForClassExtends(Type),
    ConcretizeForEnumExhaustiveCheck,
    ConcretizeForPredicate(PredicateConcretetizerVariant),
    ConcretizeForOperatorsChecking,
    ConcretizeForComputedObjectKeys,
    ConcretizeForObjectAssign,
    ConcretizeForDestructuring,
    ConcretizeForSentinelPropTest,
    ConcretizeForMatchArg {
        keep_unions: bool,
    },
    ConcretizeForUnusedPromiseCheck,
    ConcretizeForConstructorReturn,
    ConcretizeAll,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ResolveSpreadType {
    pub rrt_resolved: flow_data_structure_wrapper::list::FlowOcamlList<ResolvedParam>,
    pub rrt_unresolved: flow_data_structure_wrapper::list::FlowOcamlList<UnresolvedParam>,
    /// Once all the elements have been resolved, this tells us what type to construct
    pub rrt_resolve_to: SpreadResolve,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UnresolvedArgData(
    pub TupleElement,
    pub Option<flow_typing_generics::GenericId>,
);

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum UnresolvedParam {
    UnresolvedArg(Box<UnresolvedArgData>),
    UnresolvedSpreadArg(Type),
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ResolvedArgData(
    pub TupleElement,
    pub Option<flow_typing_generics::GenericId>,
);

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ResolvedSpreadArgData(
    pub Reason,
    pub ArrType,
    pub Option<flow_typing_generics::GenericId>,
);

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ResolvedParam {
    ResolvedArg(Box<ResolvedArgData>),
    ResolvedSpreadArg(Box<ResolvedSpreadArgData>),
    ResolvedAnySpreadArg(Reason, AnySource),
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SpreadResolve {
    ResolveSpreadsToTupleType {
        id: i32,
        elem_t: Type,
        inexact: bool,
        tout: Type,
    },
    /// Once we've finished resolving spreads, try to construct an array with known element types
    ResolveSpreadsToArrayLiteral {
        id: i32,
        kind: ArrayLiteralKind,
        literal: bool,
        fresh: bool,
        elem_t: Type,
        tout: Type,
    },
    /// Once we've finished resolving spreads, try to construct a non-tuple array
    ResolveSpreadsToArray {
        elem_t: Type,
        tout: Type,
        literal: bool,
        fresh: bool,
    },
    /// Once we've finished resolving spreads for a function's arguments, call the
    /// function with those arguments
    ResolveSpreadsToMultiflowCallFull(i32, Box<FunType>),
    ResolveSpreadsToMultiflowSubtypeFull(i32, Box<FunType>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SpreadArrayResolveTo {
    ResolveToArrayLiteral {
        kind: ArrayLiteralKind,
        literal: bool,
        fresh: bool,
    },
    ResolveToArray {
        literal: bool,
        fresh: bool,
    },
    ResolveToTupleType {
        inexact: bool,
    },
}

/// Add some flavor to the TypeT constructor. For now this information is only
/// used by the type normalizer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TypeTKind {
    TypeAliasKind,
    TypeParamKind,
    OpaqueKind,
    ImportTypeofKind,
    ImportClassKind,
    ImportEnumKind,
    InstanceKind,
    // T/U in renders T/renders (T | U). Render types do not require type arguments for polymorphic components
    RenderTypeKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FrozenKind {
    NotFrozen,
    FrozenProp,
    FrozenDirect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum GetEnumKind {
    GetEnumObject,
    GetEnumValue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DepthTrace(u32);

impl DepthTrace {
    pub fn depth(&self) -> u32 {
        self.0
    }

    pub fn dummy_trace() -> Self {
        Self(0)
    }

    pub fn unit_trace() -> Self {
        Self(1)
    }

    pub fn rec_trace(parent_depth: Self) -> Self {
        Self(parent_depth.0 + 1)
    }

    pub fn concat_trace(traces: &[Self]) -> Self {
        Self(traces.iter().map(|t| t.0).max().unwrap_or(0))
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum UnionEnum {
    Str(FlowSmolStr),
    Num(NumberLiteral),
    Bool(bool),
    BigInt(BigIntLiteral),
    Void,
    Null,
}

impl PartialEq for UnionEnum {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Str(a), Self::Str(b)) => a == b,
            (Self::Num(a), Self::Num(b)) => a == b,
            (Self::Bool(a), Self::Bool(b)) => a == b,
            (Self::BigInt(a), Self::BigInt(b)) => a == b,
            (Self::Void, Self::Void) | (Self::Null, Self::Null) => true,
            _ => false,
        }
    }
}

impl Eq for UnionEnum {}

impl PartialOrd for UnionEnum {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for UnionEnum {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        fn discriminant(v: &UnionEnum) -> u8 {
            match v {
                UnionEnum::Str(_) => 0,
                UnionEnum::Num(_) => 1,
                UnionEnum::Bool(_) => 2,
                UnionEnum::BigInt(_) => 3,
                UnionEnum::Void => 4,
                UnionEnum::Null => 5,
            }
        }
        match (self, other) {
            (Self::Str(a), Self::Str(b)) => a.cmp(b),
            (Self::Num(a), Self::Num(b)) => a.cmp(b),
            (Self::Bool(a), Self::Bool(b)) => a.cmp(b),
            (Self::BigInt(a), Self::BigInt(b)) => a.cmp(b),
            _ => discriminant(self).cmp(&discriminant(other)),
        }
    }
}

impl std::hash::Hash for UnionEnum {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self).hash(state);
        match self {
            Self::Str(s) => s.hash(state),
            Self::Num(n) => n.hash(state),
            Self::Bool(b) => b.hash(state),
            Self::BigInt(b) => b.hash(state),
            Self::Void | Self::Null => {}
        }
    }
}

impl std::fmt::Display for UnionEnum {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Str(s) => write!(f, "{}", s),
            Self::Num(NumberLiteral(_, s)) => write!(f, "{}", s),
            Self::Bool(b) => write!(f, "{}", b),
            Self::BigInt(BigIntLiteral(_, s)) => write!(f, "{}", s),
            Self::Void => write!(f, "undefined"),
            Self::Null => write!(f, "null"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum UnionEnumStar {
    One(UnionEnum),
    Many(UnionEnumSet),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum UnionEnumTag {
    SingletonStr,
    Str,
    NumericStrKey,
    SingletonNum,
    Num,
    SingletonBigInt,
    BigInt,
    SingletonBool,
    Bool,
    Void,
    Null,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UnionEnumSet(Rc<[UnionEnum]>);

impl UnionEnumSet {
    pub fn new() -> Self {
        Self(Rc::from([]))
    }

    pub fn from_vec(mut values: Vec<UnionEnum>) -> Self {
        values.sort();
        values.dedup();
        Self(Rc::from(values))
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn iter(&self) -> impl Iterator<Item = &UnionEnum> {
        self.0.iter()
    }

    pub fn contains(&self, value: &UnionEnum) -> bool {
        self.0.binary_search(value).is_ok()
    }

    pub fn is_subset(&self, other: &Self) -> bool {
        self.iter().all(|value| other.contains(value))
    }

    pub fn difference<'a>(&'a self, other: &'a Self) -> impl Iterator<Item = &'a UnionEnum> {
        self.iter().filter(move |value| !other.contains(value))
    }
}

impl Default for UnionEnumSet {
    fn default() -> Self {
        Self::new()
    }
}

impl FromIterator<UnionEnum> for UnionEnumSet {
    fn from_iter<T: IntoIterator<Item = UnionEnum>>(iter: T) -> Self {
        Self::from_vec(iter.into_iter().collect())
    }
}

pub mod property {
    use super::*;

    pub fn property_type(p: &Property) -> PropertyType {
        match p.deref() {
            PropertyInner::Field(f) => PropertyType::OrdinaryField {
                type_: f.type_.dupe(),
                polarity: f.polarity,
            },
            PropertyInner::Get { type_, .. } | PropertyInner::Method { type_, .. } => {
                PropertyType::SyntheticField {
                    get_type: Some(type_.dupe()),
                    set_type: None,
                }
            }
            PropertyInner::Set { type_, .. } => PropertyType::SyntheticField {
                get_type: None,
                set_type: Some(type_.dupe()),
            },
            PropertyInner::GetSet(gs) => PropertyType::SyntheticField {
                get_type: Some(gs.get_type.dupe()),
                set_type: Some(gs.set_type.dupe()),
            },
        }
    }

    pub fn polarity(p: &Property) -> Polarity {
        match p.deref() {
            PropertyInner::Field(f) => f.polarity,
            PropertyInner::Get { .. } => Polarity::Positive,
            PropertyInner::Set { .. } => Polarity::Negative,
            PropertyInner::GetSet(_) => Polarity::Neutral,
            PropertyInner::Method { .. } => Polarity::Positive,
        }
    }

    pub fn polarity_of_property_type(pt: &PropertyType) -> Polarity {
        match pt {
            PropertyType::OrdinaryField { polarity, .. } => *polarity,
            PropertyType::SyntheticField {
                get_type: None,
                set_type: None,
            } => {
                panic!("Illegal property_type: both get_type and set_type are None")
            }
            PropertyType::SyntheticField {
                get_type: Some(_),
                set_type: None,
            } => Polarity::Positive,
            PropertyType::SyntheticField {
                get_type: None,
                set_type: Some(_),
            } => Polarity::Negative,
            PropertyType::SyntheticField {
                get_type: Some(_),
                set_type: Some(_),
            } => Polarity::Neutral,
        }
    }

    pub fn read_t_of_property_type(pt: &PropertyType) -> Option<Type> {
        match pt {
            PropertyType::OrdinaryField { type_, polarity } => {
                if Polarity::compat(*polarity, Polarity::Positive) {
                    Some(type_.dupe())
                } else {
                    None
                }
            }
            PropertyType::SyntheticField { get_type, .. } => get_type.as_ref().map(|t| t.dupe()),
        }
    }

    pub fn read_t(p: &Property) -> Option<Type> {
        read_t_of_property_type(&property_type(p))
    }

    pub fn write_t_of_property_type(pt: &PropertyType, ctx: Option<WriteCtx>) -> Option<Type> {
        let ctx = ctx.unwrap_or(WriteCtx::Normal);
        match pt {
            PropertyType::OrdinaryField { type_, polarity } => {
                if matches!(ctx, WriteCtx::ThisInCtor)
                    || Polarity::compat(*polarity, Polarity::Negative)
                {
                    Some(type_.dupe())
                } else {
                    None
                }
            }
            PropertyType::SyntheticField { set_type, .. } => set_type.as_ref().map(|t| t.dupe()),
        }
    }

    pub fn write_t(p: &Property) -> Option<Type> {
        write_t_of_property_type(&property_type(p), None)
    }

    pub fn read_loc(p: &Property) -> Option<ALoc> {
        match p.deref() {
            PropertyInner::Field(f) => f.key_loc.dupe(),
            PropertyInner::Get { key_loc, .. } | PropertyInner::Method { key_loc, .. } => {
                key_loc.dupe()
            }
            PropertyInner::GetSet(gs) => gs.get_key_loc.dupe(),
            PropertyInner::Set { .. } => None,
        }
    }

    pub fn write_loc(p: &Property) -> Option<ALoc> {
        match p.deref() {
            PropertyInner::Field(f) => f.key_loc.dupe(),
            PropertyInner::Set { key_loc, .. } => key_loc.dupe(),
            PropertyInner::GetSet(gs) => gs.set_key_loc.dupe(),
            PropertyInner::Method { .. } | PropertyInner::Get { .. } => None,
        }
    }

    pub fn first_loc(p: &Property) -> Option<ALoc> {
        match p.deref() {
            PropertyInner::Field(f) => f.key_loc.dupe(),
            PropertyInner::Get { key_loc, .. }
            | PropertyInner::Set { key_loc, .. }
            | PropertyInner::Method { key_loc, .. } => key_loc.dupe(),
            PropertyInner::GetSet(gs) => match (&gs.get_key_loc, &gs.set_key_loc) {
                (None, None) => None,
                (Some(loc), None) | (None, Some(loc)) => Some(loc.dupe()),
                (Some(loc1), Some(loc2)) => {
                    if loc1 <= loc2 {
                        Some(loc1.dupe())
                    } else {
                        Some(loc2.dupe())
                    }
                }
            },
        }
    }

    pub fn def_locs(p: &Property) -> Option<Vec1<ALoc>> {
        match p.deref() {
            PropertyInner::Field(f) if f.preferred_def_locs.is_some() => {
                f.preferred_def_locs.clone()
            }
            PropertyInner::Field(f) => f.key_loc.as_ref().map(|loc| Vec1::new(loc.dupe())),
            PropertyInner::Get { key_loc, .. }
            | PropertyInner::Set { key_loc, .. }
            | PropertyInner::Method { key_loc, .. } => {
                key_loc.as_ref().map(|loc| Vec1::new(loc.dupe()))
            }
            PropertyInner::GetSet(gs) => match (&gs.get_key_loc, &gs.set_key_loc) {
                (None, None) => None,
                (Some(loc), None) | (None, Some(loc)) => Some(Vec1::new(loc.dupe())),
                (Some(loc1), Some(loc2)) => {
                    let mut nel = Vec1::new(loc1.dupe());
                    nel.push(loc2.dupe());
                    Some(nel)
                }
            },
        }
    }

    pub fn iter_t<F>(mut f: F, p: &Property)
    where
        F: FnMut(&Type),
    {
        match p.deref() {
            PropertyInner::Field(fd) => f(&fd.type_),
            PropertyInner::Get { type_, .. }
            | PropertyInner::Set { type_, .. }
            | PropertyInner::Method { type_, .. } => f(type_),
            PropertyInner::GetSet(gs) => {
                f(&gs.get_type);
                f(&gs.set_type);
            }
        }
    }

    pub fn fold_t<A, F>(f: F, acc: A, p: &Property) -> A
    where
        F: Fn(A, &Type) -> A,
    {
        match p.deref() {
            PropertyInner::Field(fd) => f(acc, &fd.type_),
            PropertyInner::Get { type_, .. }
            | PropertyInner::Set { type_, .. }
            | PropertyInner::Method { type_, .. } => f(acc, type_),
            PropertyInner::GetSet(gs) => f(f(acc, &gs.get_type), &gs.set_type),
        }
    }

    pub fn map_t<F>(mut f: F, p: &Property) -> Property
    where
        F: FnMut(&Type) -> Type,
    {
        match p.deref() {
            PropertyInner::Field(fd) => {
                let type_prime = f(&fd.type_);
                if fd.type_.ptr_eq(&type_prime) {
                    p.dupe()
                } else {
                    Property::new(PropertyInner::Field(Box::new(FieldData {
                        preferred_def_locs: fd.preferred_def_locs.clone(),
                        key_loc: fd.key_loc.dupe(),
                        type_: type_prime,
                        polarity: fd.polarity,
                    })))
                }
            }
            PropertyInner::Get { key_loc, type_ } => {
                let type_prime = f(type_);
                if type_.ptr_eq(&type_prime) {
                    p.dupe()
                } else {
                    Property::new(PropertyInner::Get {
                        key_loc: key_loc.dupe(),
                        type_: type_prime,
                    })
                }
            }
            PropertyInner::Set { key_loc, type_ } => {
                let type_prime = f(type_);
                if type_.ptr_eq(&type_prime) {
                    p.dupe()
                } else {
                    Property::new(PropertyInner::Set {
                        key_loc: key_loc.dupe(),
                        type_: type_prime,
                    })
                }
            }
            PropertyInner::GetSet(gs) => {
                let get_type_prime = f(&gs.get_type);
                let set_type_prime = f(&gs.set_type);
                if gs.get_type.ptr_eq(&get_type_prime) && gs.set_type.ptr_eq(&set_type_prime) {
                    p.dupe()
                } else {
                    Property::new(PropertyInner::GetSet(Box::new(GetSetData {
                        get_key_loc: gs.get_key_loc.dupe(),
                        get_type: get_type_prime,
                        set_key_loc: gs.set_key_loc.dupe(),
                        set_type: set_type_prime,
                    })))
                }
            }
            PropertyInner::Method { key_loc, type_ } => {
                let type_prime = f(type_);
                if type_.ptr_eq(&type_prime) {
                    p.dupe()
                } else {
                    Property::new(PropertyInner::Method {
                        key_loc: key_loc.dupe(),
                        type_: type_prime,
                    })
                }
            }
        }
    }

    pub fn ident_map_t<F>(mut f: F, p: &Property) -> Cow<'_, Property>
    where
        F: FnMut(&Type) -> Type,
    {
        match p.deref() {
            PropertyInner::Field(fd) => {
                let type_prime = f(&fd.type_);
                if Rc::ptr_eq(&type_prime.0, &fd.type_.0) {
                    Cow::Borrowed(p)
                } else {
                    Cow::Owned(Property::new(PropertyInner::Field(Box::new(FieldData {
                        preferred_def_locs: fd.preferred_def_locs.clone(),
                        key_loc: fd.key_loc.dupe(),
                        type_: type_prime,
                        polarity: fd.polarity,
                    }))))
                }
            }
            PropertyInner::Get { key_loc, type_ } => {
                let type_prime = f(type_);
                if Rc::ptr_eq(&type_prime.0, &type_.0) {
                    Cow::Borrowed(p)
                } else {
                    Cow::Owned(Property::new(PropertyInner::Get {
                        key_loc: key_loc.dupe(),
                        type_: type_prime,
                    }))
                }
            }
            PropertyInner::Set { key_loc, type_ } => {
                let type_prime = f(type_);
                if Rc::ptr_eq(&type_prime.0, &type_.0) {
                    Cow::Borrowed(p)
                } else {
                    Cow::Owned(Property::new(PropertyInner::Set {
                        key_loc: key_loc.dupe(),
                        type_: type_prime,
                    }))
                }
            }
            PropertyInner::GetSet(gs) => {
                let get_type_prime = f(&gs.get_type);
                let set_type_prime = f(&gs.set_type);
                if Rc::ptr_eq(&get_type_prime.0, &gs.get_type.0)
                    && Rc::ptr_eq(&set_type_prime.0, &gs.set_type.0)
                {
                    Cow::Borrowed(p)
                } else {
                    Cow::Owned(Property::new(PropertyInner::GetSet(Box::new(GetSetData {
                        get_key_loc: gs.get_key_loc.dupe(),
                        get_type: get_type_prime,
                        set_key_loc: gs.set_key_loc.dupe(),
                        set_type: set_type_prime,
                    }))))
                }
            }
            PropertyInner::Method { key_loc, type_ } => {
                let type_prime = f(type_);
                if Rc::ptr_eq(&type_prime.0, &type_.0) {
                    Cow::Borrowed(p)
                } else {
                    Cow::Owned(Property::new(PropertyInner::Method {
                        key_loc: key_loc.dupe(),
                        type_: type_prime,
                    }))
                }
            }
        }
    }

    pub fn forall_t<F>(f: F, p: &Property) -> bool
    where
        F: Fn(&Type) -> bool,
    {
        fold_t(|acc, t| acc && f(t), true, p)
    }

    pub fn assert_field(p: &Property) -> Type {
        match p.deref() {
            PropertyInner::Field(fd) => fd.type_.dupe(),
            _ => panic!("Unexpected field type"),
        }
    }

    pub fn is_method(p: &Property) -> bool {
        matches!(p.deref(), PropertyInner::Method { .. })
    }

    /// Underlying type field of a Property, regardless of polarity (unlike `read_t`,
    /// which filters by polarity). For GetSet — which carries both a get and a set
    /// type — this returns the get type.
    pub fn type_(p: &Property) -> &Type {
        match p.deref() {
            PropertyInner::Field(fd) => &fd.type_,
            PropertyInner::Method { type_, .. }
            | PropertyInner::Get { type_, .. }
            | PropertyInner::Set { type_, .. } => type_,
            PropertyInner::GetSet(gs) => &gs.get_type,
        }
    }

    /// Return a Property with the field polarity overridden. Method/Get/Set/GetSet
    /// have no field polarity and are returned unchanged.
    pub fn with_polarity(p: &Property, polarity: Polarity) -> Property {
        match p.deref() {
            PropertyInner::Field(fd) if fd.polarity == polarity => p.dupe(),
            PropertyInner::Field(fd) => Property::new(PropertyInner::Field(Box::new(FieldData {
                polarity,
                ..(**fd).clone()
            }))),
            _ => p.dupe(),
        }
    }

    /// Map the underlying type of a Field. Method/Get/Set/GetSet are returned unchanged.
    pub fn map_field_t<F>(p: &Property, f: F) -> Property
    where
        F: FnOnce(&Type) -> Type,
    {
        match p.deref() {
            PropertyInner::Field(fd) => {
                let type_prime = f(&fd.type_);
                if fd.type_.ptr_eq(&type_prime) {
                    p.dupe()
                } else {
                    Property::new(PropertyInner::Field(Box::new(FieldData {
                        type_: type_prime,
                        ..(**fd).clone()
                    })))
                }
            }
            _ => p.dupe(),
        }
    }

    /// Map the underlying type and override field polarity in one pass.
    /// Method/Get/Set/GetSet have no field polarity, so only their types are mapped.
    pub fn map_t_with_polarity<F>(p: &Property, polarity: Polarity, mut f: F) -> Property
    where
        F: FnMut(&Type) -> Type,
    {
        match p.deref() {
            PropertyInner::Field(fd) => {
                let type_prime = f(&fd.type_);
                if fd.type_.ptr_eq(&type_prime) && fd.polarity == polarity {
                    p.dupe()
                } else {
                    Property::new(PropertyInner::Field(Box::new(FieldData {
                        preferred_def_locs: fd.preferred_def_locs.clone(),
                        key_loc: fd.key_loc.dupe(),
                        type_: type_prime,
                        polarity,
                    })))
                }
            }
            PropertyInner::Get { key_loc, type_ } => {
                let type_prime = f(type_);
                if type_.ptr_eq(&type_prime) {
                    p.dupe()
                } else {
                    Property::new(PropertyInner::Get {
                        key_loc: key_loc.dupe(),
                        type_: type_prime,
                    })
                }
            }
            PropertyInner::Set { key_loc, type_ } => {
                let type_prime = f(type_);
                if type_.ptr_eq(&type_prime) {
                    p.dupe()
                } else {
                    Property::new(PropertyInner::Set {
                        key_loc: key_loc.dupe(),
                        type_: type_prime,
                    })
                }
            }
            PropertyInner::GetSet(gs) => {
                let get_type_prime = f(&gs.get_type);
                let set_type_prime = f(&gs.set_type);
                if gs.get_type.ptr_eq(&get_type_prime) && gs.set_type.ptr_eq(&set_type_prime) {
                    p.dupe()
                } else {
                    Property::new(PropertyInner::GetSet(Box::new(GetSetData {
                        get_key_loc: gs.get_key_loc.dupe(),
                        get_type: get_type_prime,
                        set_key_loc: gs.set_key_loc.dupe(),
                        set_type: set_type_prime,
                    })))
                }
            }
            PropertyInner::Method { key_loc, type_ } => {
                let type_prime = f(type_);
                if type_.ptr_eq(&type_prime) {
                    p.dupe()
                } else {
                    Property::new(PropertyInner::Method {
                        key_loc: key_loc.dupe(),
                        type_: type_prime,
                    })
                }
            }
        }
    }

    /// Map the underlying type of a Field and override its polarity in one pass.
    /// Method/Get/Set/GetSet are returned unchanged.
    pub fn map_field_t_with_polarity<F>(p: &Property, polarity: Polarity, f: F) -> Property
    where
        F: FnOnce(&Type) -> Type,
    {
        match p.deref() {
            PropertyInner::Field(fd) => {
                let type_prime = f(&fd.type_);
                if fd.type_.ptr_eq(&type_prime) && fd.polarity == polarity {
                    p.dupe()
                } else {
                    Property::new(PropertyInner::Field(Box::new(FieldData {
                        preferred_def_locs: fd.preferred_def_locs.clone(),
                        key_loc: fd.key_loc.dupe(),
                        type_: type_prime,
                        polarity,
                    })))
                }
            }
            _ => p.dupe(),
        }
    }
}

pub mod properties {
    use super::*;
    use crate::source_or_generated_id;

    #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Dupe)]
    pub struct Id(source_or_generated_id::Id);

    impl Id {
        pub fn generate_id() -> Self {
            Self(source_or_generated_id::Id::generate_id())
        }

        pub fn of_aloc_id(type_sig: bool, aloc_id: flow_aloc::ALocId) -> Self {
            Self(source_or_generated_id::Id::of_aloc_id(type_sig, aloc_id))
        }

        pub fn from_type_sig(&self) -> bool {
            self.0.from_type_sig()
        }

        pub fn debug_string(&self) -> String {
            self.0.debug_string()
        }

        pub fn stable_string(&self) -> String {
            self.0.stable_string()
        }
    }

    #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Dupe)]
    pub struct PropertiesMap(Rc<Vec<(Name, Property)>>);

    impl std::hash::Hash for PropertiesMap {
        fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
            for entry in self.0.iter() {
                entry.hash(state);
            }
        }
    }

    impl Default for PropertiesMap {
        fn default() -> Self {
            Self::new()
        }
    }

    impl PropertiesMap {
        pub fn new() -> Self {
            Self(Rc::new(Vec::new()))
        }

        pub fn from_btree_map(map: BTreeMap<Name, Property>) -> Self {
            Self(Rc::new(map.into_iter().collect()))
        }

        pub fn is_empty(&self) -> bool {
            self.0.is_empty()
        }

        pub fn len(&self) -> usize {
            self.0.len()
        }

        pub fn ptr_eq(&self, other: &Self) -> bool {
            Rc::ptr_eq(&self.0, &other.0)
        }

        pub fn get(&self, name: &Name) -> Option<&Property> {
            self.0
                .binary_search_by(|(key, _)| key.cmp(name))
                .ok()
                .map(|index| &self.0[index].1)
        }

        pub fn contains_key(&self, name: &Name) -> bool {
            self.0.binary_search_by(|(key, _)| key.cmp(name)).is_ok()
        }

        pub fn insert(&mut self, name: Name, prop: Property) -> Option<Property> {
            let map = Rc::make_mut(&mut self.0);
            match map.binary_search_by(|(key, _)| key.cmp(&name)) {
                Ok(index) => Some(std::mem::replace(&mut map[index].1, prop)),
                Err(index) => {
                    map.insert(index, (name, prop));
                    None
                }
            }
        }

        pub fn remove(&mut self, name: &Name) -> Option<Property> {
            let index = self.0.binary_search_by(|(key, _)| key.cmp(name)).ok()?;
            Some(Rc::make_mut(&mut self.0).remove(index).1)
        }

        pub fn iter(&self) -> impl DoubleEndedIterator<Item = (&Name, &Property)> {
            self.0.iter().map(|(name, prop)| (name, prop))
        }

        pub fn iter_union<'a>(
            &'a self,
            other: &'a Self,
        ) -> impl Iterator<Item = (&'a Name, Option<&'a Property>, Option<&'a Property>)> {
            let mut iter1 = self.0.iter().peekable();
            let mut iter2 = other.0.iter().peekable();
            std::iter::from_fn(move || match (iter1.peek(), iter2.peek()) {
                (None, None) => None,
                (Some(_), None) => {
                    let (name, prop) = iter1.next().unwrap();
                    Some((name, Some(prop), None))
                }
                (None, Some(_)) => {
                    let (name, prop) = iter2.next().unwrap();
                    Some((name, None, Some(prop)))
                }
                (Some((name1, _)), Some((name2, _))) => match name1.cmp(name2) {
                    std::cmp::Ordering::Less => {
                        let (name, prop) = iter1.next().unwrap();
                        Some((name, Some(prop), None))
                    }
                    std::cmp::Ordering::Greater => {
                        let (name, prop) = iter2.next().unwrap();
                        Some((name, None, Some(prop)))
                    }
                    std::cmp::Ordering::Equal => {
                        let (name, prop1) = iter1.next().unwrap();
                        let (_, prop2) = iter2.next().unwrap();
                        Some((name, Some(prop1), Some(prop2)))
                    }
                },
            })
        }

        pub fn iter_union_rev<'a>(
            &'a self,
            other: &'a Self,
        ) -> impl Iterator<Item = (&'a Name, Option<&'a Property>, Option<&'a Property>)> {
            let mut iter1 = self.0.iter().rev().peekable();
            let mut iter2 = other.0.iter().rev().peekable();
            std::iter::from_fn(move || match (iter1.peek(), iter2.peek()) {
                (None, None) => None,
                (Some(_), None) => {
                    let (name, prop) = iter1.next().unwrap();
                    Some((name, Some(prop), None))
                }
                (None, Some(_)) => {
                    let (name, prop) = iter2.next().unwrap();
                    Some((name, None, Some(prop)))
                }
                (Some((name1, _)), Some((name2, _))) => match name1.cmp(name2) {
                    std::cmp::Ordering::Greater => {
                        let (name, prop) = iter1.next().unwrap();
                        Some((name, Some(prop), None))
                    }
                    std::cmp::Ordering::Less => {
                        let (name, prop) = iter2.next().unwrap();
                        Some((name, None, Some(prop)))
                    }
                    std::cmp::Ordering::Equal => {
                        let (name, prop1) = iter1.next().unwrap();
                        let (_, prop2) = iter2.next().unwrap();
                        Some((name, Some(prop1), Some(prop2)))
                    }
                },
            })
        }

        pub fn ident_map<F>(&self, mut f: F) -> Self
        where
            F: FnMut(&Property) -> Property,
        {
            let mut map_prime: Option<Vec<(Name, Property)>> = None;
            for (index, (name, prop)) in self.0.iter().enumerate() {
                let prop_prime = f(prop);
                if let Some(map_prime) = map_prime.as_mut() {
                    map_prime.push((name.dupe(), prop_prime));
                } else if !prop.ptr_eq(&prop_prime) {
                    let mut new_map = Vec::with_capacity(self.len());
                    new_map.extend(self.0[..index].iter().cloned());
                    new_map.push((name.dupe(), prop_prime));
                    map_prime = Some(new_map);
                }
            }
            match map_prime {
                Some(map) => Self(Rc::new(map)),
                None => self.dupe(),
            }
        }

        pub fn values(&self) -> impl Iterator<Item = &Property> {
            self.0.iter().map(|(_, prop)| prop)
        }

        pub fn keys(&self) -> impl Iterator<Item = &Name> {
            self.0.iter().map(|(name, _)| name)
        }

        pub fn extract_named_exports(&self) -> BTreeMap<Name, NamedSymbol> {
            let mut tmap = BTreeMap::new();
            for (name, prop) in self.iter() {
                if let Some(mut type_) = property::read_t(prop) {
                    let preferred_def_locs = match prop.deref() {
                        PropertyInner::Field(fd) => fd.preferred_def_locs.clone(),
                        _ => None,
                    };

                    if matches!(prop.deref(), PropertyInner::Method { .. }) {
                        type_ = method_to_function(&type_, false);
                    }

                    tmap.insert(
                        name.dupe(),
                        NamedSymbol::new(property::read_loc(prop), preferred_def_locs, type_, None),
                    );
                }
            }
            tmap
        }

        pub fn iter_t<F>(&self, mut f: F)
        where
            F: FnMut(&Type),
        {
            for prop in self.values() {
                property::iter_t(&mut f, prop);
            }
        }
    }

    impl From<BTreeMap<Name, Property>> for PropertiesMap {
        fn from(map: BTreeMap<Name, Property>) -> Self {
            Self::from_btree_map(map)
        }
    }

    impl std::iter::FromIterator<(Name, Property)> for PropertiesMap {
        fn from_iter<I: IntoIterator<Item = (Name, Property)>>(iter: I) -> Self {
            Self::from_btree_map(iter.into_iter().collect())
        }
    }

    impl<'a> IntoIterator for &'a PropertiesMap {
        type Item = &'a (Name, Property);
        type IntoIter = std::slice::Iter<'a, (Name, Property)>;

        fn into_iter(self) -> Self::IntoIter {
            self.0.iter()
        }
    }

    pub type Set = FlowOrdSet<Id>;

    pub type Map = FlowOrdMap<Id, PropertiesMap>;

    pub fn method_to_function(t: &Type, use_mixed_this: bool) -> Type {
        method_to_function_inner(t, use_mixed_this, false)
    }

    pub fn method_to_function_for_subtyping(t: &Type) -> Type {
        method_to_function_inner(t, true, true)
    }

    fn method_to_function_inner(
        t: &Type,
        use_mixed_this: bool,
        preserve_explicit_method: bool,
    ) -> Type {
        match t.deref() {
            TypeInner::DefT(r, deref!(DefTInner::FunT(static_, ft)))
                if ft.this_t.1.is_method()
                    && !(preserve_explicit_method
                        && ft.this_t.1 == ThisStatus::ThisMethodWithExplicitThis) =>
            {
                let mut new_ft = (**ft).clone();
                new_ft.this_t = (
                    if use_mixed_this && ft.this_t.1 == ThisStatus::ThisMethod {
                        mixed_t::make(r.dupe())
                    } else {
                        new_ft.this_t.0.dupe()
                    },
                    ThisStatus::ThisFunction,
                );
                Type::new(TypeInner::DefT(
                    r.dupe(),
                    DefT::new(DefTInner::FunT(static_.dupe(), Rc::new(new_ft))),
                ))
            }
            #[rustfmt::skip]
            TypeInner::DefT(r, deref!(DefTInner::PolyT(box PolyTData {
                kind: poly_kind,
                tparams_loc,
                tparams,
                t_out,
                id,
                strictness_kind,
            }))) => {
                let new_t_out =
                    method_to_function_inner(t_out, use_mixed_this, preserve_explicit_method);
                if Rc::ptr_eq(&new_t_out.0, &t_out.0) {
                    t.dupe()
                } else {
                    Type::new(TypeInner::DefT(
                        r.dupe(),
                        DefT::new(DefTInner::PolyT(Box::new(PolyTData {
                            kind: *poly_kind,
                            tparams_loc: tparams_loc.dupe(),
                            tparams: tparams.dupe(),
                            t_out: new_t_out,
                            id: id.dupe(),
                            strictness_kind: *strictness_kind,
                        }))),
                    ))
                }
            }
            TypeInner::IntersectionT(r, rep) => {
                let new_rep = rep
                    .map(|t| method_to_function_inner(t, use_mixed_this, preserve_explicit_method));
                if new_rep
                    .members_iter()
                    .zip(rep.members_iter())
                    .all(|(t1, t2)| Rc::ptr_eq(&t1.0, &t2.0))
                {
                    t.dupe()
                } else {
                    Type::new(TypeInner::IntersectionT(r.dupe(), new_rep))
                }
            }
            _ => t.dupe(),
        }
    }
}

pub mod nominal {
    use super::*;

    #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum StuckEvalKind {
        StuckEvalForNonMaybeType,
        StuckEvalForElementType,
        StuckEvalForOptionalIndexedAccessWithTypeIndexNonMaybeType,
        StuckEvalForOptionalIndexedAccessResultType,
        StuckEvalForExactType,
        StuckEvalForReadOnlyType,
        StuckEvalForPartialType,
        StuckEvalForRequiredType,
        StuckEvalForValuesType,
        StuckEvalForConditionalType { infer_tparam_ids: Rc<Vec<ALocId>> },
        StuckEvalForKeyMirrorType,
        StuckEvalForEnumType,
        StuckEvalForPropertyType { name: Box<Name> },
        StuckEvalForOptionalIndexedAccessWithStrLitIndexNonMaybeType { name: Box<Name> },
        StuckEvalForGenericallyMappedObject(SubstName),
        StuckEvalForReactDRO(DroType),
    }

    #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct UserDefinedOpaqueTypeIdData(pub ALocId, pub FlowSmolStr);

    #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum Id {
        /// Special nominal type to test t ~> union optimization
        InternalEnforceUnionOptimized,
        /// The intrinsic `ExactObjectType`: the supertype of every exact object type, which no
        /// lower bound type can express, and a subtype of `{...}`.
        ExactObjectType,
        /// We also track the name so that we can do common interface conformance check, where we check
        /// the structural subtyping for nominal constructs defined in impl and interface files.
        UserDefinedOpaqueTypeId(Box<UserDefinedOpaqueTypeIdData>),
        /// Stuck evaluation with kind
        StuckEval(StuckEvalKind),
    }

    impl std::fmt::Display for Id {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                Id::UserDefinedOpaqueTypeId(box UserDefinedOpaqueTypeIdData(_aloc_id, name)) => {
                    write!(f, "user-defined {} (<opaque>)", name)
                }
                Id::InternalEnforceUnionOptimized => write!(f, "InternalEnforceUnionOptimized"),
                Id::ExactObjectType => write!(f, "ExactObjectType"),
                Id::StuckEval(kind) => match kind {
                    StuckEvalKind::StuckEvalForNonMaybeType => write!(f, "StuckEvalForNonMaybeType"),
                    StuckEvalKind::StuckEvalForPropertyType { name } => {
                        write!(f, "StuckEvalForPropertyType {}", name)
                    }
                    StuckEvalKind::StuckEvalForElementType => write!(f, "StuckEvalForElementType"),
                    StuckEvalKind::StuckEvalForOptionalIndexedAccessWithStrLitIndexNonMaybeType {
                        name,
                    } => write!(
                        f,
                        "StuckEvalForOptionalIndexedAccessWithStrLitIndexNonMaybeType {}",
                        name
                    ),
                    StuckEvalKind::StuckEvalForOptionalIndexedAccessWithTypeIndexNonMaybeType => {
                        write!(f, "StuckEvalForOptionalIndexedAccessWithTypeIndexNonMaybeType")
                    }
                    StuckEvalKind::StuckEvalForOptionalIndexedAccessResultType => {
                        write!(f, "StuckEvalForOptionalIndexedAccessResultType")
                    }
                    StuckEvalKind::StuckEvalForExactType => write!(f, "StuckEvalForExactType"),
                    StuckEvalKind::StuckEvalForReadOnlyType => write!(f, "StuckEvalForReadOnlyType"),
                    StuckEvalKind::StuckEvalForPartialType => write!(f, "StuckEvalForPartialType"),
                    StuckEvalKind::StuckEvalForRequiredType => write!(f, "StuckEvalForRequiredType"),
                    StuckEvalKind::StuckEvalForValuesType => write!(f, "StuckEvalForValuesType"),
                    StuckEvalKind::StuckEvalForConditionalType { .. } => {
                        write!(f, "StuckEvalForConditionalType")
                    }
                    StuckEvalKind::StuckEvalForGenericallyMappedObject(n) => {
                        write!(f, "StuckEvalForGenericMappedType {}", n)
                    }
                    StuckEvalKind::StuckEvalForKeyMirrorType => write!(f, "StuckEvalForKeyMirrorType"),
                    StuckEvalKind::StuckEvalForReactDRO(_) => write!(f, "StuckEvalForReactDRO"),
                    StuckEvalKind::StuckEvalForEnumType => write!(f, "StuckEvalForEnumType"),
                },
            }
        }
    }

    #[derive(Debug, Clone, Dupe, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct CustomErrorData {
        pub custom_error_loc: ALoc,
        pub t: Type,
    }

    #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum UnderlyingT {
        /// Fully opaque type with no underlying representation
        FullyOpaque,
        /// Opaque with local underlying type
        OpaqueWithLocal { t: Type },
        /// Custom error with location and underlying type
        CustomError(Box<CustomErrorData>),
    }
}

pub mod eval {
    use std::collections::BTreeSet;

    use dupe::Dupe;
    use flow_data_structure_wrapper::ord_map::FlowOrdMap;

    use crate::source_or_generated_id;

    #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Dupe)]
    pub struct Id(source_or_generated_id::Id);

    impl Id {
        pub fn generate_id() -> Self {
            Self(source_or_generated_id::Id::generate_id())
        }

        pub fn of_aloc_id(type_sig: bool, aloc_id: flow_aloc::ALocId) -> Self {
            Self(source_or_generated_id::Id::of_aloc_id(type_sig, aloc_id))
        }

        pub fn from_type_sig(&self) -> bool {
            self.0.from_type_sig()
        }

        pub fn debug_string(&self) -> String {
            self.0.debug_string()
        }

        pub fn stable_string(&self) -> String {
            self.0.stable_string()
        }
    }

    pub type Map<T> = FlowOrdMap<Id, T>;

    pub type Set = BTreeSet<Id>;
}

pub mod poly {
    use std::collections::BTreeSet;

    use dupe::Dupe;

    use crate::source_or_generated_id;

    #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Dupe)]
    pub struct Id(source_or_generated_id::Id);

    impl Id {
        pub fn generate_id() -> Self {
            Self(source_or_generated_id::Id::generate_id())
        }

        pub fn of_aloc_id(type_sig: bool, aloc_id: flow_aloc::ALocId) -> Self {
            Self(source_or_generated_id::Id::of_aloc_id(type_sig, aloc_id))
        }

        pub fn from_type_sig(&self) -> bool {
            self.0.from_type_sig()
        }

        pub fn debug_string(&self) -> String {
            self.0.debug_string()
        }

        pub fn stable_string(&self) -> String {
            self.0.stable_string()
        }
    }

    pub type Set = BTreeSet<Id>;
}

pub mod exports {
    use dupe::Dupe;
    use flow_common::reason;
    use flow_data_structure_wrapper::ord_map::FlowOrdMap;

    use super::*;

    /// Sorted-Vec-backed export map. O(n) construction from sorted data
    /// (Rust's sort detects pre-sorted input in linear time), O(log n) lookup
    /// via binary search, O(1) dupe via Rc sharing.
    #[derive(Debug, Clone)]
    pub struct T(Rc<Vec<(Name, NamedSymbol)>>);

    impl Dupe for T {}

    impl Default for T {
        fn default() -> Self {
            T::new()
        }
    }

    impl PartialEq for T {
        fn eq(&self, other: &Self) -> bool {
            Rc::ptr_eq(&self.0, &other.0) || self.0 == other.0
        }
    }

    impl Eq for T {}

    impl PartialOrd for T {
        fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
            Some(self.cmp(other))
        }
    }

    impl Ord for T {
        fn cmp(&self, other: &Self) -> std::cmp::Ordering {
            if Rc::ptr_eq(&self.0, &other.0) {
                std::cmp::Ordering::Equal
            } else {
                self.0.cmp(&other.0)
            }
        }
    }

    impl std::hash::Hash for T {
        fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
            self.0.hash(state);
        }
    }

    impl T {
        pub fn new() -> Self {
            T(Rc::new(Vec::new()))
        }

        pub fn get(&self, key: &Name) -> Option<&NamedSymbol> {
            self.0
                .binary_search_by(|(k, _)| k.cmp(key))
                .ok()
                .map(|i| &self.0[i].1)
        }

        pub fn contains_key(&self, key: &Name) -> bool {
            self.0.binary_search_by(|(k, _)| k.cmp(key)).is_ok()
        }

        pub fn is_empty(&self) -> bool {
            self.0.is_empty()
        }

        pub fn ptr_eq(&self, other: &Self) -> bool {
            Rc::ptr_eq(&self.0, &other.0)
        }

        pub fn len(&self) -> usize {
            self.0.len()
        }

        pub fn insert(&mut self, key: Name, value: NamedSymbol) {
            let inner = Rc::make_mut(&mut self.0);
            match inner.binary_search_by(|(k, _)| k.cmp(&key)) {
                Ok(i) => inner[i].1 = value,
                Err(i) => inner.insert(i, (key, value)),
            }
        }

        /// Insert if key not present, return mutable ref to value
        pub fn entry_or_insert(&mut self, key: Name, value: NamedSymbol) {
            let inner = Rc::make_mut(&mut self.0);
            match inner.binary_search_by(|(k, _)| k.cmp(&key)) {
                Ok(_) => {} // already present
                Err(i) => inner.insert(i, (key, value)),
            }
        }

        pub fn iter(&self) -> std::slice::Iter<'_, (Name, NamedSymbol)> {
            self.0.iter()
        }

        pub fn ident_map<F>(&self, mut f: F) -> Self
        where
            F: FnMut(&NamedSymbol) -> NamedSymbol,
        {
            let mut map_prime = None;
            for (name, symbol) in self.iter() {
                let symbol_prime = f(symbol);
                if !symbol.ptr_eq(&symbol_prime) {
                    map_prime
                        .get_or_insert_with(|| self.dupe())
                        .insert(name.dupe(), symbol_prime);
                }
            }
            map_prime.unwrap_or_else(|| self.dupe())
        }

        pub fn keys(&self) -> impl Iterator<Item = &Name> {
            self.0.iter().map(|(k, _)| k)
        }

        pub fn values(&self) -> impl Iterator<Item = &NamedSymbol> {
            self.0.iter().map(|(_, v)| v)
        }

        pub fn into_ord_map(self) -> FlowOrdMap<Name, NamedSymbol> {
            self.0.iter().map(|(k, v)| (k.dupe(), v.clone())).collect()
        }

        /// Returns this export map with the `default` entry removed. When there
        /// is no `default` export (the common case) `self` is returned
        /// unchanged -- a zero-cost move; otherwise a filtered copy is built in
        /// a single O(n) pass (the source is already sorted, so order is
        /// preserved). Used to implement spec-compliant `export *`, which
        /// re-exports named exports but never the source module's default export
        /// (matching `GetExportedNames` in the ES spec and Node.js/ESM).
        pub fn without_default(self) -> Self {
            let default_name = Name::new("default");
            if !self.contains_key(&default_name) {
                return self;
            }
            T(Rc::new(
                self.0
                    .iter()
                    .filter(|(k, _)| *k != default_name)
                    .cloned()
                    .collect(),
            ))
        }

        /// Merge two sorted export maps, preferring entries from `other` on key collision.
        /// Both inputs are already sorted, so this is O(n+m).
        pub fn union(self, other: T) -> T {
            if self.is_empty() {
                return other;
            }
            if other.is_empty() {
                return self;
            }
            let a = &*self.0;
            let b = &*other.0;
            let mut result = Vec::with_capacity(a.len() + b.len());
            let mut i = 0;
            let mut j = 0;
            while i < a.len() && j < b.len() {
                match a[i].0.cmp(&b[j].0) {
                    std::cmp::Ordering::Less => {
                        result.push(a[i].clone());
                        i += 1;
                    }
                    std::cmp::Ordering::Greater => {
                        result.push(b[j].clone());
                        j += 1;
                    }
                    std::cmp::Ordering::Equal => {
                        // other wins on collision (like im::OrdMap::union)
                        result.push(b[j].clone());
                        i += 1;
                        j += 1;
                    }
                }
            }
            while i < a.len() {
                result.push(a[i].clone());
                i += 1;
            }
            while j < b.len() {
                result.push(b[j].clone());
                j += 1;
            }
            T(Rc::new(result))
        }
    }

    impl FromIterator<(Name, NamedSymbol)> for T {
        fn from_iter<I: IntoIterator<Item = (Name, NamedSymbol)>>(iter: I) -> Self {
            let mut pairs: Vec<(Name, NamedSymbol)> = iter.into_iter().collect();
            pairs.sort_by(|(a, _), (b, _)| a.cmp(b));
            pairs.dedup_by(|(a, _), (b, _)| a == b);
            T(Rc::new(pairs))
        }
    }

    impl IntoIterator for T {
        type Item = (Name, NamedSymbol);
        type IntoIter = std::vec::IntoIter<(Name, NamedSymbol)>;

        fn into_iter(self) -> Self::IntoIter {
            match Rc::try_unwrap(self.0) {
                Ok(vec) => vec.into_iter(),
                Err(rc) => rc.as_ref().clone().into_iter(),
            }
        }
    }

    impl<'a> IntoIterator for &'a T {
        type Item = &'a (Name, NamedSymbol);
        type IntoIter = std::slice::Iter<'a, (Name, NamedSymbol)>;

        fn into_iter(self) -> Self::IntoIter {
            self.0.iter()
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Dupe)]
    pub struct Id(usize);

    impl Id {
        pub fn new(id: usize) -> Self {
            Self(id)
        }

        pub fn mk_id() -> Self {
            Self(reason::mk_id())
        }
    }

    impl std::fmt::Display for Id {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{}", self.0)
        }
    }

    pub type Map = FlowOrdMap<Id, T>;

    pub type ExportsMap = BTreeMap<Name, Type>;
    pub type ExportsIdMap = std::collections::HashMap<Id, ExportsMap>;
}

// check here

/// We encapsulate UnionT's internal structure
/// so we can use specialized representations for
/// unions with exploitable regularity and/or
/// simplicity properties, e.g. enums.
///
/// Representations are opaque. `make` chooses a
/// representation internally, and client code which
/// needs to interact with member types directly
/// can do so via `members`, which provides access
/// via the standard list representation.
pub mod union_rep {
    use std::cell::RefCell;
    use std::collections::BTreeMap;

    use properties::PropertiesMap;

    use super::*;

    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum UnionKind {
        ProvidersKind,
        ConditionalKind,
        ImplicitInstantiationKind,
        ResolvedKind,
        LogicalKind,
        UnknownKind,
    }

    #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
    pub enum FinallyOptimizedRep {
        /// [None] in the position of tag means that the values used to create this
        /// enum union were not of the same tag.
        EnumUnion(UnionEnumSet, Option<UnionEnumTag>),
        PartiallyOptimizedUnionEnum(UnionEnumSet),
        AlmostDisjointUnionWithPossiblyNonUniqueKeys(
            Box<BTreeMap<Name, BTreeMap<UnionEnum, Vec1<Type>>>>,
        ),
        PartiallyOptimizedAlmostDisjointUnionWithPossiblyNonUniqueKeys(
            Box<BTreeMap<Name, BTreeMap<UnionEnum, Vec1<Type>>>>,
        ),
        Empty,
        Singleton(Type),
    }

    // Manual Hash impl because BTreeMap does not implement Hash in std.
    // OCaml's generic compare/hash traverses all fields structurally,
    // including through ref cells. We match that behavior here.
    impl std::hash::Hash for FinallyOptimizedRep {
        fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
            std::mem::discriminant(self).hash(state);
            match self {
                FinallyOptimizedRep::EnumUnion(set, tag) => {
                    set.hash(state);
                    tag.hash(state);
                }
                FinallyOptimizedRep::PartiallyOptimizedUnionEnum(set) => {
                    set.hash(state);
                }
                FinallyOptimizedRep::AlmostDisjointUnionWithPossiblyNonUniqueKeys(map) => {
                    for (k, v) in map.iter() {
                        k.hash(state);
                        for (k2, v2) in v {
                            k2.hash(state);
                            v2.hash(state);
                        }
                    }
                }
                FinallyOptimizedRep::PartiallyOptimizedAlmostDisjointUnionWithPossiblyNonUniqueKeys(map) => {
                    for (k, v) in map.iter() {
                        k.hash(state);
                        for (k2, v2) in v {
                            k2.hash(state);
                            v2.hash(state);
                        }
                    }
                }
                FinallyOptimizedRep::Empty => {}
                FinallyOptimizedRep::Singleton(t) => {
                    t.hash(state);
                }
            }
        }
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
    pub enum OptimizedError<L: Dupe> {
        ContainsUnresolved {
            loc: L,
            desc: type_or_type_desc::TypeOrTypeDescT<L>,
        },
        NoCandidateMembers,
        NoCommonKeys,
    }

    /// quick membership tests for enums and disjoint unions
    #[derive(Debug, Clone)]
    pub enum QuickMemResult {
        Yes,
        No,
        Conditional(Type),
        Unknown,
    }

    #[derive(Debug)]
    struct Specialization(RefCell<Option<FinallyOptimizedRep>>);

    impl Specialization {
        fn new(value: Option<FinallyOptimizedRep>) -> Self {
            Self(RefCell::new(value))
        }

        fn borrow(&self) -> std::cell::Ref<'_, Option<FinallyOptimizedRep>> {
            self.0.borrow()
        }

        fn borrow_mut(&self) -> std::cell::RefMut<'_, Option<FinallyOptimizedRep>> {
            self.0.borrow_mut()
        }
    }

    impl PartialEq for Specialization {
        fn eq(&self, other: &Self) -> bool {
            std::ptr::eq(self.0.as_ptr(), other.0.as_ptr()) || *self.0.borrow() == *other.0.borrow()
        }
    }

    impl Eq for Specialization {}

    impl PartialOrd for Specialization {
        fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
            Some(self.cmp(other))
        }
    }

    impl Ord for Specialization {
        fn cmp(&self, other: &Self) -> std::cmp::Ordering {
            if std::ptr::eq(self.0.as_ptr(), other.0.as_ptr()) {
                std::cmp::Ordering::Equal
            } else {
                self.0.borrow().cmp(&other.0.borrow())
            }
        }
    }

    impl std::hash::Hash for Specialization {
        fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
            self.0.borrow().hash(state)
        }
    }

    #[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
    pub struct UnionRepInner {
        t0: Type,
        t1: Type,
        ts: Rc<[Type]>,
        /// optional source location of the union,
        /// used as identity of the union for fast-path check.
        source_aloc: Option<ALocId>,
        /// if union is an enum (set of singletons over a common base) then Some (base, set)
        /// (additional specializations probably to come)
        specialization: Specialization,
        /// A union is synthetic roughly when it does not emerge from an annotation,
        /// e.g. when it emerges as the collection of lower bounds during implicit
        /// instantiation.
        kind: UnionKind,
    }

    // Mirror OCaml's bounded `Hashtbl.hash`: cap the recursion through
    // `ts` to keep `UnionRepInner` hashing O(1) instead of O(union
    // membership). Equality is unchanged so HashMap correctness is
    // preserved; we just trade hash uniqueness on huge unions for
    // bounded hash cost. Length is hashed so unions of different sizes
    // still differ.
    const UNION_HASH_BOUND: usize = 8;

    impl std::hash::Hash for UnionRepInner {
        fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
            self.t0.hash(state);
            self.t1.hash(state);
            self.ts.len().hash(state);
            for t in self.ts.iter().take(UNION_HASH_BOUND) {
                t.hash(state);
            }
            self.source_aloc.hash(state);
            self.specialization.hash(state);
            self.kind.hash(state);
        }
    }

    #[derive(Clone, Dupe, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct UnionRep(Rc<UnionRepInner>);

    impl std::fmt::Debug for UnionRep {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            self.0.fmt(f)
        }
    }

    impl UnionRep {
        pub fn ptr_eq(&self, other: &UnionRep) -> bool {
            Rc::ptr_eq(&self.0, &other.0)
        }

        pub fn same_source(&self, other: &UnionRep) -> bool {
            match (&self.0.source_aloc, &other.0.source_aloc) {
                (Some(id1), Some(id2)) => id1 == id2,
                _ => false,
            }
        }

        pub fn same_structure(&self, other: &UnionRep) -> bool {
            self.0.t0 == other.0.t0 && self.0.t1 == other.0.t1 && self.0.ts == other.0.ts
        }

        pub fn is_synthetic(&self) -> bool {
            !matches!(self.0.kind, UnionKind::UnknownKind)
        }

        pub fn union_kind(&self) -> UnionKind {
            self.0.kind
        }

        pub fn disjoint_object_union_props(&self) -> Option<BTreeSet<Name>> {
            match self.0.specialization.borrow().as_ref() {
                Some(
                    FinallyOptimizedRep::AlmostDisjointUnionWithPossiblyNonUniqueKeys(prop_map)
                    | FinallyOptimizedRep::PartiallyOptimizedAlmostDisjointUnionWithPossiblyNonUniqueKeys(
                        prop_map,
                    ),
                ) => Some(prop_map.keys().cloned().collect()),
                _ => None,
            }
        }

        pub fn is_optimized_finally(&self) -> bool {
            self.0.specialization.borrow().is_some()
        }

        pub fn optimize_enum_only<C, F>(&self, cx: &mut C, flatten: F)
        where
            F: FnOnce(&mut C, &mut dyn Iterator<Item = &Type>) -> Vec<Type>,
        {
            let mut members = self.members_iter();
            let ts = flatten(cx, &mut members);
            if has_unflattened_types(&ts).is_none() {
                if let Ok(opt @ FinallyOptimizedRep::EnumUnion(_, _)) = enum_optimize(&ts) {
                    *self.0.specialization.borrow_mut() = Some(opt);
                }
            }
        }

        /// Optimizes this union with the operations given, which run with `cx`.
        pub fn optimize_<C, F2, F3, F4, F5>(
            &self,
            cx: &mut C,
            reasonless_eq: F2,
            flatten: F3,
            find_resolved: F4,
            find_props: F5,
        ) -> Result<FinallyOptimizedRep, OptimizedError<ALoc>>
        where
            F2: Fn(&mut C, &Type, &Type) -> bool,
            F3: FnOnce(&mut C, &mut dyn Iterator<Item = &Type>) -> Vec<Type>,
            F4: Fn(&mut C, &Type) -> Option<Type>,
            F5: Fn(&mut C, properties::Id) -> PropertiesMap,
        {
            let mut members = self.members_iter();
            let ts = flatten(cx, &mut members);
            match has_unflattened_types(&ts) {
                None => {
                    let opt = enum_optimize(&ts);
                    match opt {
                        Err(_) => disjoint_union_optimize(
                            cx,
                            &reasonless_eq,
                            &find_resolved,
                            &find_props,
                            &ts,
                        ),
                        _ => opt,
                    }
                }
                Some(t) => Err(OptimizedError::ContainsUnresolved {
                    loc: crate::type_util::ref_loc_of_t(&t).dupe(),
                    desc: type_or_type_desc::TypeOrTypeDescT::Type(t),
                }),
            }
        }

        pub fn set_optimize(&self, opt: Result<FinallyOptimizedRep, OptimizedError<ALoc>>) {
            if let Ok(opt) = opt {
                *self.0.specialization.borrow_mut() = Some(opt);
            }
        }

        pub fn optimize<C, F2, F3, F4, F5>(
            &self,
            cx: &mut C,
            reasonless_eq: F2,
            flatten: F3,
            find_resolved: F4,
            find_props: F5,
        ) where
            F2: Fn(&mut C, &Type, &Type) -> bool,
            F3: FnOnce(&mut C, &mut dyn Iterator<Item = &Type>) -> Vec<Type>,
            F4: Fn(&mut C, &Type) -> Option<Type>,
            F5: Fn(&mut C, properties::Id) -> PropertiesMap,
        {
            let opt = self.optimize_(cx, reasonless_eq, flatten, find_resolved, find_props);
            self.set_optimize(opt);
        }

        pub fn check_enum(&self) -> Option<UnionEnumSet> {
            match self.0.specialization.borrow().as_ref() {
                Some(FinallyOptimizedRep::EnumUnion(enums, _)) => Some(enums.clone()),
                _ => None,
            }
        }

        pub fn check_enum_with_tag(&self) -> Option<(UnionEnumSet, Option<UnionEnumTag>)> {
            match self.0.specialization.borrow().as_ref() {
                Some(FinallyOptimizedRep::EnumUnion(enums, tag)) => Some((enums.clone(), *tag)),
                _ => None,
            }
        }

        pub fn string_of_specialization(&self) -> &'static str {
            string_of_specialization_(self.0.specialization.borrow().as_ref())
        }

        pub fn members_iter(&self) -> impl Iterator<Item = &Type> {
            std::iter::once(&self.0.t0)
                .chain(std::iter::once(&self.0.t1))
                .chain(self.0.ts.iter())
        }

        /// map rep r to rep r' along type mapping f. if nothing would be changed,
        /// returns the physically-identical rep.
        pub fn ident_map<F>(&self, always_keep_source: bool, mut f: F) -> UnionRep
        where
            F: FnMut(&Type) -> Type,
        {
            let t0_prime = f(&self.0.t0);
            let t1_prime = f(&self.0.t1);
            let ts_prime = list_utils::ident_map(&mut f, Type::ptr_eq, self.0.ts.dupe());
            let changed = !Rc::ptr_eq(&t0_prime.0, &self.0.t0.0)
                || !Rc::ptr_eq(&t1_prime.0, &self.0.t1.0)
                || !Rc::ptr_eq(&self.0.ts, &ts_prime);
            if changed {
                let source_aloc = if always_keep_source {
                    self.0.source_aloc.dupe()
                } else {
                    None
                };
                make(source_aloc, self.0.kind, t0_prime, t1_prime, ts_prime)
            } else {
                self.dupe()
            }
        }

        pub fn try_ident_map<F, E>(&self, always_keep_source: bool, mut f: F) -> Result<UnionRep, E>
        where
            F: FnMut(&Type) -> Result<Type, E>,
        {
            let t0_prime = f(&self.0.t0)?;
            let t1_prime = f(&self.0.t1)?;
            let ts_prime = list_utils::try_ident_map(&mut f, Type::ptr_eq, self.0.ts.dupe())?;
            let changed = !Rc::ptr_eq(&t0_prime.0, &self.0.t0.0)
                || !Rc::ptr_eq(&t1_prime.0, &self.0.t1.0)
                || !Rc::ptr_eq(&self.0.ts, &ts_prime);
            Ok(if changed {
                let source_aloc = if always_keep_source {
                    self.0.source_aloc.dupe()
                } else {
                    None
                };
                make(source_aloc, self.0.kind, t0_prime, t1_prime, ts_prime)
            } else {
                self.dupe()
            })
        }
    }

    // canonicalize a type w.r.t. enum membership
    pub fn canon(t: &Type) -> Option<UnionEnum> {
        match t.0.as_ref() {
            TypeInner::DefT(_, def_t) => match &**def_t {
                DefTInner::SingletonStrT { value, .. } => Some(UnionEnum::Str(value.dupe())),
                DefTInner::NumericStrKeyT(num_lit) => {
                    Some(UnionEnum::Str(num_lit.1.to_string().into()))
                }
                DefTInner::SingletonNumT { value, .. } => Some(UnionEnum::Num(value.clone())),
                DefTInner::SingletonBigIntT { value, .. } => Some(UnionEnum::BigInt(value.clone())),
                DefTInner::SingletonBoolT { value, .. } => Some(UnionEnum::Bool(*value)),
                DefTInner::VoidT => Some(UnionEnum::Void),
                DefTInner::NullT => Some(UnionEnum::Null),
                _ => None,
            },
            _ => None,
        }
    }

    fn is_base(t: &Type) -> bool {
        match t.0.as_ref() {
            TypeInner::DefT(_, def_t) => matches!(
                &**def_t,
                DefTInner::NumericStrKeyT(_)
                    | DefTInner::SingletonStrT {
                        from_annot: true,
                        ..
                    }
                    | DefTInner::SingletonNumT {
                        from_annot: true,
                        ..
                    }
                    | DefTInner::SingletonBigIntT {
                        from_annot: true,
                        ..
                    }
                    | DefTInner::SingletonBoolT {
                        from_annot: true,
                        ..
                    }
                    | DefTInner::VoidT
                    | DefTInner::NullT
            ),
            _ => false,
        }
    }

    // disjoint unions are stored as singleton type maps
    pub type UnionEnumMap = BTreeMap<UnionEnum, Vec1<Type>>;

    pub fn tag_of_member(t: &Type) -> Option<UnionEnumTag> {
        match t.0.as_ref() {
            TypeInner::DefT(_, def_t) => match &**def_t {
                DefTInner::SingletonStrT { .. } => Some(UnionEnumTag::SingletonStr),
                DefTInner::StrGeneralT { .. } => Some(UnionEnumTag::Str),
                DefTInner::NumericStrKeyT(_) => Some(UnionEnumTag::NumericStrKey),
                DefTInner::SingletonNumT { .. } => Some(UnionEnumTag::SingletonNum),
                DefTInner::NumGeneralT(_) => Some(UnionEnumTag::Num),
                DefTInner::SingletonBigIntT { .. } => Some(UnionEnumTag::SingletonBigInt),
                DefTInner::BigIntGeneralT(_) => Some(UnionEnumTag::BigInt),
                DefTInner::SingletonBoolT { .. } => Some(UnionEnumTag::SingletonBool),
                DefTInner::BoolGeneralT => Some(UnionEnumTag::Bool),
                DefTInner::VoidT => Some(UnionEnumTag::Void),
                DefTInner::NullT => Some(UnionEnumTag::Null),
                _ => None,
            },
            _ => None,
        }
    }

    /// given a list of members, build a rep.
    /// specialized reps are used on compatible type lists
    pub fn make(
        source_aloc: Option<ALocId>,
        kind: UnionKind,
        t0: Type,
        t1: Type,
        ts: Rc<[Type]>,
    ) -> UnionRep {
        fn mk_enum<'a>(
            mut tset: Vec<UnionEnum>,
            mut tag: Option<UnionEnumTag>,
            types: impl IntoIterator<Item = &'a Type>,
        ) -> Option<(UnionEnumSet, Option<UnionEnumTag>)> {
            for t in types {
                match canon(t) {
                    Some(tcanon) if is_base(t) => {
                        let t_tag = tag_of_member(t);
                        tag = if tag == t_tag { tag } else { None };
                        tset.push(tcanon);
                    }
                    _ => return None,
                }
            }
            Some((UnionEnumSet::from_vec(tset), tag))
        }

        let enum_opt = mk_enum(
            Vec::new(),
            tag_of_member(&t0),
            std::iter::once(&t0)
                .chain(std::iter::once(&t1))
                .chain(ts.iter()),
        )
        .map(|(tset, tag)| FinallyOptimizedRep::EnumUnion(tset, tag));

        UnionRep(Rc::new(UnionRepInner {
            t0,
            t1,
            ts,
            source_aloc,
            specialization: Specialization::new(enum_opt),
            kind,
        }))
    }

    pub fn cons(t0: Type, rep: &UnionRep) -> UnionRep {
        let t1 = rep.0.t0.dupe();
        let t2 = rep.0.t1.dupe();
        let ts: Vec<_> = std::iter::once(t2)
            .chain(rep.0.ts.iter().map(|t| t.dupe()))
            .collect();
        make(None, UnionKind::UnknownKind, t0, t1, ts.into())
    }

    /********** Optimizations **********/

    fn has_unflattened_types(ts: &[Type]) -> Option<Type> {
        for t in ts {
            match &**t {
                TypeInner::OpenT(_)
                | TypeInner::ImplicitInstantiationTvar(_)
                | TypeInner::EvalT { .. }
                | TypeInner::TypeAppT(..)
                | TypeInner::KeysT(_, _)
                | TypeInner::IntersectionT(_, _)
                | TypeInner::NominalT { .. } => {
                    return Some(t.dupe());
                }
                TypeInner::DefT(_, def_t)
                    if matches!(&**def_t, DefTInner::InstanceT(_) | DefTInner::PolyT(_)) =>
                {
                    return Some(t.dupe());
                }
                _ => {}
            }
        }
        None
    }

    fn enum_optimize(ts: &[Type]) -> Result<FinallyOptimizedRep, OptimizedError<ALoc>> {
        fn split_enum(ts: &[Type]) -> (UnionEnumSet, Option<UnionEnumTag>, bool) {
            if ts.is_empty() {
                return (UnionEnumSet::new(), None, false);
            }

            let mut tset = Vec::new();
            let mut acc_tag = tag_of_member(&ts[0]);
            let mut partial = false;

            for t in ts {
                match canon(t) {
                    Some(tcanon) if is_base(t) => {
                        let t_tag = tag_of_member(t);
                        acc_tag = if acc_tag == t_tag { acc_tag } else { None };
                        tset.push(tcanon);
                    }
                    _ => {
                        acc_tag = None;
                        partial = true;
                    }
                }
            }

            (UnionEnumSet::from_vec(tset), acc_tag, partial)
        }
        match ts.len() {
            0 => Ok(FinallyOptimizedRep::Empty),
            1 => Ok(FinallyOptimizedRep::Singleton(ts[0].dupe())),
            _ => {
                let (tset, tag, partial) = split_enum(ts);
                if partial {
                    if tset.is_empty() {
                        Err(OptimizedError::NoCandidateMembers)
                    } else {
                        Ok(FinallyOptimizedRep::PartiallyOptimizedUnionEnum(tset))
                    }
                } else {
                    Ok(FinallyOptimizedRep::EnumUnion(tset, tag))
                }
            }
        }
    }

    fn canon_prop<C, F>(cx: &mut C, find_resolved: &F, p: &Property) -> Option<UnionEnum>
    where
        F: Fn(&mut C, &Type) -> Option<Type>,
    {
        property::read_t(p)
            .and_then(|t| find_resolved(cx, &t))
            .and_then(|t| canon(&t))
    }

    fn base_prop<C, F>(cx: &mut C, find_resolved: &F, p: &Property) -> Option<UnionEnum>
    where
        F: Fn(&mut C, &Type) -> Option<Type>,
    {
        property::read_t(p)
            .and_then(|t| find_resolved(cx, &t))
            .and_then(|t| if is_base(&t) { canon(&t) } else { None })
    }

    fn props_of<C, F>(cx: &mut C, find_props: &F, t: &Type) -> Option<PropertiesMap>
    where
        F: Fn(&mut C, properties::Id) -> PropertiesMap,
    {
        match &**t {
            TypeInner::DefT(_, def_t) => match &**def_t {
                DefTInner::ObjT(obj) => Some(find_props(cx, obj.props_tmap.dupe())),
                _ => None,
            },
            _ => None,
        }
    }

    fn disjoint_union_optimize<C, ReasonlessEq, FindResolved, FindProps>(
        cx: &mut C,
        reasonless_eq: &ReasonlessEq,
        find_resolved: &FindResolved,
        find_props: &FindProps,
        ts: &[Type],
    ) -> Result<FinallyOptimizedRep, OptimizedError<ALoc>>
    where
        ReasonlessEq: Fn(&mut C, &Type, &Type) -> bool,
        FindResolved: Fn(&mut C, &Type) -> Option<Type>,
        FindProps: Fn(&mut C, properties::Id) -> PropertiesMap,
    {
        fn base_props_of<C, F2, F3>(
            cx: &mut C,
            find_resolved: &F2,
            find_props: &F3,
            t: &Type,
        ) -> Option<BTreeMap<Name, (UnionEnum, Type)>>
        where
            F2: Fn(&mut C, &Type) -> Option<Type>,
            F3: Fn(&mut C, properties::Id) -> PropertiesMap,
        {
            props_of(cx, find_props, t).map(|prop_map: PropertiesMap| {
                let mut result = BTreeMap::new();
                for (key, p) in prop_map.iter() {
                    if let Some(enum_val) = base_prop(cx, find_resolved, p) {
                        result.insert(key.dupe(), (enum_val, t.dupe()));
                    }
                }
                result
            })
        }

        // Returns a tuple of (candidates, partial):
        //  - [candidates] is a list that contains a candidate for each union member.
        //    A candidate is a mapping from keys in that object to a UnionEnum value.
        //    Properties that do not have a UnionEnum representation are ignored.
        //  - [partial] is true iff there exist non-object-like members in the union
        fn split_disjoint_union<C, F2, F3>(
            cx: &mut C,
            find_resolved: &F2,
            find_props: &F3,
            ts: &[Type],
        ) -> (Vec<BTreeMap<Name, (UnionEnum, Type)>>, bool)
        where
            F2: Fn(&mut C, &Type) -> Option<Type>,
            F3: Fn(&mut C, properties::Id) -> PropertiesMap,
        {
            let mut candidates = Vec::new();
            let mut partial = false;

            for t in ts {
                match base_props_of(cx, find_resolved, find_props, t) {
                    None => partial = true,
                    Some(base_props) => candidates.push(base_props),
                }
            }

            (candidates, partial)
        }

        fn almost_unique_values<C, F1>(
            cx: &mut C,
            reasonless_eq: &F1,
            mut hybrid_idx: UnionEnumMap,
            values: Vec<(UnionEnum, Type)>,
        ) -> UnionEnumMap
        where
            F1: Fn(&mut C, &Type, &Type) -> bool,
        {
            for (enum_val, t) in values {
                match hybrid_idx.entry(enum_val) {
                    std::collections::btree_map::Entry::Occupied(mut entry) => {
                        let existing = entry.get_mut();
                        let exists = existing
                            .iter()
                            .any(|existing_t| reasonless_eq(cx, &t, existing_t));
                        if exists {
                            // This corresponds to the case
                            // type T = { f: "a" };
                            // type Union = T | T;
                            // Don't add duplicate
                        } else {
                            existing.push(t);
                        }
                    }
                    std::collections::btree_map::Entry::Vacant(entry) => {
                        entry.insert(Vec1::new(t));
                    }
                }
            }
            hybrid_idx
        }

        fn almost_unique<C, F1>(
            cx: &mut C,
            reasonless_eq: &F1,
            idx: BTreeMap<Name, Vec<(UnionEnum, Type)>>,
        ) -> BTreeMap<Name, UnionEnumMap>
        where
            F1: Fn(&mut C, &Type, &Type) -> bool,
        {
            let mut result = BTreeMap::new();
            for (key, values) in idx.into_iter() {
                let hybrid_idx = almost_unique_values(cx, reasonless_eq, BTreeMap::new(), values);
                result.insert(key, hybrid_idx);
            }
            result
        }

        // Compute the intersection of properties of objects that have singleton types
        fn intersect_props(
            base_props: BTreeMap<Name, (UnionEnum, Type)>,
            candidates: &[BTreeMap<Name, (UnionEnum, Type)>],
        ) -> BTreeMap<Name, Vec<(UnionEnum, Type)>> {
            // Initialize with first candidate
            let mut init = BTreeMap::new();
            for (key, enum_t) in base_props.iter() {
                init.insert(key.dupe(), vec![enum_t.clone()]);
            }

            // Merge with remaining candidates
            for candidate in candidates {
                init.retain(|key, values| {
                    if let Some(enum_t) = candidate.get(key) {
                        values.push(enum_t.clone());
                        true
                    } else {
                        false
                    }
                });
            }

            init
        }

        match ts.len() {
            0 => Ok(FinallyOptimizedRep::Empty),
            1 => Ok(FinallyOptimizedRep::Singleton(ts[0].dupe())),
            _ => {
                let (candidates, partial) = split_disjoint_union(cx, find_resolved, find_props, ts);

                match candidates.split_first() {
                    None => Err(OptimizedError::NoCandidateMembers),
                    Some((first, rest)) => {
                        let idx = intersect_props(first.clone(), rest);
                        if idx.is_empty() {
                            Err(OptimizedError::NoCommonKeys)
                        } else {
                            let hybrid_map = almost_unique(cx, reasonless_eq, idx);
                            if partial {
                                Ok(FinallyOptimizedRep::PartiallyOptimizedAlmostDisjointUnionWithPossiblyNonUniqueKeys(Box::new(hybrid_map)))
                            } else {
                                Ok(FinallyOptimizedRep::AlmostDisjointUnionWithPossiblyNonUniqueKeys(Box::new(hybrid_map)))
                            }
                        }
                    }
                }
            }
        }
    }

    /********** Quick matching **********/

    pub fn join_quick_mem_results(results: (QuickMemResult, QuickMemResult)) -> QuickMemResult {
        match results {
            (QuickMemResult::Yes, _) | (_, QuickMemResult::Yes) => QuickMemResult::Yes,
            (QuickMemResult::Unknown, _) | (_, QuickMemResult::Unknown) => QuickMemResult::Unknown,
            (QuickMemResult::Conditional(_), _) | (_, QuickMemResult::Conditional(_)) => {
                QuickMemResult::Unknown
            }
            (QuickMemResult::No, QuickMemResult::No) => QuickMemResult::No,
        }
    }

    // assume we know that l is a canonizable type
    pub fn quick_mem_enum<C, F>(
        cx: &mut C,
        quick_subtype: F,
        l: &Type,
        rep: &UnionRep,
    ) -> QuickMemResult
    where
        F: FnOnce(&mut C, &Type, &Type) -> bool,
    {
        match canon(l) {
            Some(tcanon) => {
                match &*rep.0.specialization.borrow() {
                    None => QuickMemResult::Unknown,
                    Some(FinallyOptimizedRep::Empty) => QuickMemResult::No,
                    Some(FinallyOptimizedRep::Singleton(t)) => {
                        if quick_subtype(cx, l, t) {
                            QuickMemResult::Yes
                        } else {
                            QuickMemResult::Conditional(t.dupe())
                        }
                    }
                    Some(FinallyOptimizedRep::AlmostDisjointUnionWithPossiblyNonUniqueKeys(_)) => {
                        QuickMemResult::No
                    }
                    Some(FinallyOptimizedRep::PartiallyOptimizedAlmostDisjointUnionWithPossiblyNonUniqueKeys(_)) => {
                        QuickMemResult::Unknown
                    }
                    Some(FinallyOptimizedRep::EnumUnion(tset, _)) => {
                        if tset.contains(&tcanon) {
                            QuickMemResult::Yes
                        } else {
                            QuickMemResult::No
                        }
                    }
                    Some(FinallyOptimizedRep::PartiallyOptimizedUnionEnum(tset)) => {
                        if tset.contains(&tcanon) {
                            QuickMemResult::Yes
                        } else {
                            QuickMemResult::Unknown
                        }
                    }
                }
            }
            None => {
                QuickMemResult::Unknown
            }
        }
    }

    fn lookup_almost_disjoint_union<C, F>(
        cx: &mut C,
        find_resolved: &F,
        prop_map: &PropertiesMap,
        partial: bool,
        map: &BTreeMap<Name, UnionEnumMap>,
    ) -> QuickMemResult
    where
        F: Fn(&mut C, &Type) -> Option<Type>,
    {
        let mut acc = QuickMemResult::Unknown;
        for (key, idx) in map.iter() {
            if !matches!(acc, QuickMemResult::Unknown) {
                break;
            }
            acc = match prop_map.get(key) {
                Some(p) => match canon_prop(cx, find_resolved, p) {
                    Some(enum_val) => match idx.get(&enum_val) {
                        Some(types) if types.len() == 1 => {
                            QuickMemResult::Conditional(types[0].dupe())
                        }
                        Some(_) => {
                            // Multiple types for this enum value
                            QuickMemResult::Unknown
                        }
                        None => {
                            if partial {
                                QuickMemResult::Unknown
                            } else {
                                QuickMemResult::No
                            }
                        }
                    },
                    None => QuickMemResult::Unknown,
                },
                None => {
                    if partial {
                        QuickMemResult::Unknown
                    } else {
                        QuickMemResult::No
                    }
                }
            };
        }

        acc
    }

    // we know that l is an object type or exact object type
    pub fn quick_mem_disjoint_union<C>(
        cx: &mut C,
        find_resolved: impl Fn(&mut C, &Type) -> Option<Type>,
        find_props: impl Fn(&mut C, properties::Id) -> PropertiesMap,
        quick_subtype: impl FnOnce(&mut C, &Type, &Type) -> bool,
        l: &Type,
        rep: &UnionRep,
    ) -> QuickMemResult {
        match props_of(cx, &find_props, l) {
            Some(prop_map) => {
                match rep.0.specialization.borrow().as_ref() {
                    None => QuickMemResult::Unknown,
                    Some(FinallyOptimizedRep::Empty) => QuickMemResult::No,
                    Some(FinallyOptimizedRep::Singleton(t)) => {
                        if quick_subtype(cx, l, t) {
                            QuickMemResult::Yes
                        } else {
                            QuickMemResult::Conditional(t.dupe())
                        }
                    }
                    Some(FinallyOptimizedRep::AlmostDisjointUnionWithPossiblyNonUniqueKeys(map)) => {
                        lookup_almost_disjoint_union(cx, &find_resolved, &prop_map, false, map)
                    }
                    Some(FinallyOptimizedRep::PartiallyOptimizedAlmostDisjointUnionWithPossiblyNonUniqueKeys(map)) => {
                        lookup_almost_disjoint_union(cx, &find_resolved, &prop_map, true, map)
                    }
                    Some(FinallyOptimizedRep::EnumUnion(_, _)) => QuickMemResult::No,
                    Some(FinallyOptimizedRep::PartiallyOptimizedUnionEnum(_)) => QuickMemResult::Unknown,
                }
            }
            None => panic!("quick_mem_disjoint_union is defined only on object / exact object types"),
        }
    }

    pub fn string_of_specialization_(opt: Option<&FinallyOptimizedRep>) -> &'static str {
        match opt {
            Some(FinallyOptimizedRep::EnumUnion(_, _)) => "Enum",
            Some(FinallyOptimizedRep::Empty) => "Empty",
            Some(FinallyOptimizedRep::Singleton(_)) => "Singleton",
            Some(
                FinallyOptimizedRep::PartiallyOptimizedAlmostDisjointUnionWithPossiblyNonUniqueKeys(
                    _,
                ),
            ) => "Partially Optimized Almost Disjoint Union with possibly non-unique keys",
            Some(FinallyOptimizedRep::AlmostDisjointUnionWithPossiblyNonUniqueKeys(_)) => {
                "Almost Disjoint Union with possibly non-unique keys"
            }
            Some(FinallyOptimizedRep::PartiallyOptimizedUnionEnum(_)) => "Partially Optimized Enum",
            None => "No Specialization",
        }
    }
}

pub mod inter_rep {
    use super::*;

    /// intersection rep is:
    /// - member list in declaration order
    #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
    struct InterRepInner {
        t0: Type,
        t1: Type,
        ts: Rc<[Type]>,
    }

    // Mirror OCaml's bounded `Hashtbl.hash`: cap recursion through `ts`.
    // Same rationale as `UnionRepInner`'s manual Hash above.
    const INTER_HASH_BOUND: usize = 8;

    impl std::hash::Hash for InterRepInner {
        fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
            self.t0.hash(state);
            self.t1.hash(state);
            self.ts.len().hash(state);
            for t in self.ts.iter().take(INTER_HASH_BOUND) {
                t.hash(state);
            }
        }
    }

    #[derive(Clone, Dupe, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct InterRep(Rc<InterRepInner>);

    impl std::fmt::Debug for InterRep {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            self.0.fmt(f)
        }
    }

    /// build rep from list of members
    pub fn make(t0: Type, t1: Type, ts: Rc<[Type]>) -> InterRep {
        InterRep(Rc::new(InterRepInner { t0, t1, ts }))
    }

    impl InterRep {
        pub fn ptr_eq(&self, other: &InterRep) -> bool {
            Rc::ptr_eq(&self.0, &other.0)
        }

        /// members in declaration order (as iterator)
        pub fn members_iter(&self) -> impl Iterator<Item = &Type> {
            std::iter::once(&self.0.t0)
                .chain(std::iter::once(&self.0.t1))
                .chain(self.0.ts.iter())
        }

        /// map rep r to rep r' along type mapping f. drops history
        pub fn map<F>(&self, f: F) -> InterRep
        where
            F: Fn(&Type) -> Type,
        {
            make(
                f(&self.0.t0),
                f(&self.0.t1),
                self.0.ts.iter().map(f).collect(),
            )
        }

        /// map rep r to rep r' along type mapping f. drops history. if nothing would
        /// be changed, returns the physically-identical rep.
        pub fn ident_map<F>(&self, mut f: F) -> InterRep
        where
            F: FnMut(&Type) -> Type,
        {
            let t0_ = f(&self.0.t0);
            let t1_ = f(&self.0.t1);
            let mut changed =
                !Rc::ptr_eq(&t0_.0, &self.0.t0.0) || !Rc::ptr_eq(&t1_.0, &self.0.t1.0);

            let mut ts_prime = Vec::new();
            for member in self.0.ts.iter() {
                let member_ = f(member);
                changed = changed || !Rc::ptr_eq(&member_.0, &member.0);
                ts_prime.push(member_);
            }

            if changed {
                make(t0_, t1_, ts_prime.into())
            } else {
                self.dupe()
            }
        }
    }
}

pub mod object {
    use super::*;

    #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum ResolveTool {
        // Each part of a spread must be resolved in order to compute the result
        Resolve(Resolve),
        // In order to resolve an InstanceT, all supers must also be resolved to
        // collect class properties, which are own.
        Super(Slice, Vec<Type>, Rc<Resolve>),
    }

    #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum Resolve {
        Next,
        // Resolve each element of a union or intersection
        List0(Vec1<Type>, Join),
        List(Rc<[Type]>, Vec1<Resolved>, Join),
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum JoinOp {
        And,
        Or,
    }

    // This location is that of the entire intersection/union, not just the location of the &/| symbol
    #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct Join(pub ALoc, pub JoinOp);
    // A union type resolves to a resolved spread with more than one element
    #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct Resolved(pub Vec1<Slice>);

    #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct Slice {
        pub reason: Reason,
        pub strictness_kind: TypeStrictnessKind,
        pub props: properties::PropertiesMap,
        pub flags: Flags,
        pub frozen: bool,
        pub generics: GenericSpreadId,
        pub interface: Option<(Type, InstType)>,
        pub reachable_targs: Rc<[(Type, Polarity)]>,
        pub literal: bool,
        /// The props beyond `props` of an inexact slice belong to an exact object, an
        /// `ExactObjectType`.
        pub exact_object_type: bool,
    }

    pub type Dict = Option<DictType>;

    pub mod spread {
        use super::*;

        // This is the type we feed into SpreadType to be processed by object_kit. It's different
        // than slice because object_kit processes the properties in ways that do not need to
        // be exposed to other files.
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct OperandSliceInner {
            pub reason: Reason,
            pub prop_map: properties::PropertiesMap,
            pub generics: GenericSpreadId,
            pub dict: Dict,
            pub reachable_targs: Rc<[(Type, Polarity)]>,
        }

        #[derive(Debug, Clone, Dupe, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct OperandSlice(Rc<OperandSliceInner>);

        impl std::ops::Deref for OperandSlice {
            type Target = OperandSliceInner;

            fn deref(&self) -> &Self::Target {
                &self.0
            }
        }

        impl OperandSlice {
            pub fn new(inner: OperandSliceInner) -> Self {
                Self(Rc::new(inner))
            }

            pub fn ptr_eq(&self, other: &OperandSlice) -> bool {
                Rc::ptr_eq(&self.0, &other.0)
            }
        }

        #[derive(Debug, Clone, Dupe, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub enum Operand {
            Slice(OperandSlice),
            Type(Type),
        }

        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub enum AccElement {
            ResolvedSlice(Resolved),
            InlineSlice(OperandSlice),
        }

        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct State {
            pub todo_rev: flow_data_structure_wrapper::list::FlowOcamlList<Operand>,
            pub acc: flow_data_structure_wrapper::list::FlowOcamlList<AccElement>,
            pub spread_id: i32,
            pub union_t: Option<Type>,
            pub curr_resolve_idx: i32,
        }

        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub enum SealType {
            Sealed,
            Frozen,
            AsConst,
        }

        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub enum Target {
            // When spreading values, the result is exact if all of the input types are
            // also exact. If any input type is inexact, the output is inexact.
            Value {
                make_seal: SealType,
                strictness_kind: TypeStrictnessKind,
                literal: bool,
            },
            // Type spreads use the declared shape of inexact inputs. The annotation
            // determines whether the resulting object is exact.
            Annot {
                make_exact: bool,
                strictness_kind: TypeStrictnessKind,
            },
        }

        impl Target {
            pub fn strictness_kind(&self) -> TypeStrictnessKind {
                match self {
                    Target::Value {
                        strictness_kind, ..
                    }
                    | Target::Annot {
                        strictness_kind, ..
                    } => *strictness_kind,
                }
            }

            pub fn literal(&self) -> bool {
                match self {
                    Target::Value { literal, .. } => *literal,
                    Target::Annot { .. } => false,
                }
            }
        }
    }

    pub mod rest {
        use super::*;

        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub enum State {
            One(Type),
            Done(Resolved),
        }

        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub enum MergeMode {
            SpreadReversal,
            ReactConfigMerge(Polarity),
            Omit,
        }
    }

    pub mod react_config {
        use super::*;

        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub enum State {
            Config {
                component_default_props: Option<Type>,
            },
            Defaults {
                config: Resolved,
            },
        }

        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub enum RefManipulation {
            KeepRef,
            AddRef(Type),
        }
    }

    #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct ObjectToolReactConfigData {
        pub state: react_config::State,
        pub ref_manipulation: react_config::RefManipulation,
        pub literal: bool,
    }

    #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct ObjectToolObjectMapData {
        pub prop_type: Type,
        pub name_type: Option<Type>,
        pub mapped_type_flags: MappedTypeFlags,
        pub selected_keys_opt: Option<Type>,
    }

    #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum Tool {
        MakeExact,
        ReadOnly,
        Partial,
        Required,
        Spread(Box<(spread::Target, spread::State)>),
        Rest(Box<(rest::MergeMode, rest::State)>),
        ReactConfig(Box<ObjectToolReactConfigData>),
        ReactCheckComponentConfig { props: properties::PropertiesMap },
        ObjectRep,
        ObjectMap(Box<ObjectToolObjectMapData>),
    }

    pub type GenericSpreadId = flow_typing_generics::SpreadId;
}

pub mod react {
    use super::*;

    pub struct CreateElementData<CX: ContextFamily = ()> {
        pub component: Type,
        pub jsx_props: Type,
        pub tout: Tvar,
        pub targs: Option<Rc<[Targ]>>,
        pub return_hint: LazyHintT<CX>,
        pub inferred_targs: Option<Rc<[(Type, SubstName)]>>,
        pub specialized_component: Option<SpecializedCallee>,
        pub specialized_props: Option<SpecializedCallee>,
    }

    pub enum Tool<CX: ContextFamily = ()> {
        CreateElement(Box<CreateElementData<CX>>),
        ConfigCheck { props: Type },
        GetConfig { tout: Type },
    }

    impl<CX: ContextFamily> Clone for Tool<CX> {
        fn clone(&self) -> Self {
            match self {
                Tool::CreateElement(box CreateElementData {
                    component,
                    jsx_props,
                    tout,
                    targs,
                    return_hint,
                    inferred_targs,
                    specialized_component,
                    specialized_props,
                }) => Tool::CreateElement(Box::new(CreateElementData {
                    component: component.clone(),
                    jsx_props: jsx_props.clone(),
                    tout: tout.clone(),
                    targs: targs.clone(),
                    return_hint: return_hint.clone(),
                    inferred_targs: inferred_targs.clone(),
                    specialized_component: specialized_component.clone(),
                    specialized_props: specialized_props.clone(),
                })),
                Tool::ConfigCheck { props } => Tool::ConfigCheck {
                    props: props.clone(),
                },
                Tool::GetConfig { tout } => Tool::GetConfig { tout: tout.clone() },
            }
        }
    }

    impl<CX: ContextFamily> PartialEq for Tool<CX> {
        fn eq(&self, other: &Self) -> bool {
            match (self, other) {
                (
                    Tool::CreateElement(box CreateElementData {
                        component: a1,
                        jsx_props: b1,
                        tout: c1,
                        targs: d1,
                        return_hint: e1,
                        inferred_targs: f1,
                        specialized_component: g1,
                        specialized_props: h1,
                    }),
                    Tool::CreateElement(box CreateElementData {
                        component: a2,
                        jsx_props: b2,
                        tout: c2,
                        targs: d2,
                        return_hint: e2,
                        inferred_targs: f2,
                        specialized_component: g2,
                        specialized_props: h2,
                    }),
                ) => {
                    a1 == a2
                        && b1 == b2
                        && c1 == c2
                        && d1 == d2
                        && e1 == e2
                        && f1 == f2
                        && g1 == g2
                        && h1 == h2
                }
                (Tool::ConfigCheck { props: a1 }, Tool::ConfigCheck { props: a2 }) => a1 == a2,
                (Tool::GetConfig { tout: a1 }, Tool::GetConfig { tout: a2 }) => a1 == a2,
                _ => false,
            }
        }
    }

    impl<CX: ContextFamily> Eq for Tool<CX> {}

    impl<CX: ContextFamily> std::hash::Hash for Tool<CX> {
        fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
            std::mem::discriminant(self).hash(state);
            match self {
                Tool::CreateElement(box CreateElementData {
                    component,
                    jsx_props,
                    tout,
                    targs,
                    return_hint,
                    inferred_targs,
                    specialized_component,
                    specialized_props,
                }) => {
                    component.hash(state);
                    jsx_props.hash(state);
                    tout.hash(state);
                    targs.hash(state);
                    return_hint.hash(state);
                    inferred_targs.hash(state);
                    specialized_component.hash(state);
                    specialized_props.hash(state);
                }
                Tool::ConfigCheck { props } => {
                    props.hash(state);
                }
                Tool::GetConfig { tout } => {
                    tout.hash(state);
                }
            }
        }
    }

    impl<CX: ContextFamily> PartialOrd for Tool<CX> {
        fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
            Some(self.cmp(other))
        }
    }

    impl<CX: ContextFamily> Ord for Tool<CX> {
        fn cmp(&self, other: &Self) -> std::cmp::Ordering {
            fn variant_index<CX: ContextFamily>(v: &Tool<CX>) -> u32 {
                match v {
                    Tool::CreateElement(..) => 0,
                    Tool::ConfigCheck { .. } => 1,
                    Tool::GetConfig { .. } => 2,
                }
            }
            let disc = variant_index(self).cmp(&variant_index(other));
            if disc != std::cmp::Ordering::Equal {
                return disc;
            }
            match (self, other) {
                (
                    Tool::CreateElement(box CreateElementData {
                        component: a1,
                        jsx_props: b1,
                        tout: c1,
                        targs: d1,
                        return_hint: e1,
                        inferred_targs: f1,
                        specialized_component: g1,
                        specialized_props: h1,
                    }),
                    Tool::CreateElement(box CreateElementData {
                        component: a2,
                        jsx_props: b2,
                        tout: c2,
                        targs: d2,
                        return_hint: e2,
                        inferred_targs: f2,
                        specialized_component: g2,
                        specialized_props: h2,
                    }),
                ) => a1
                    .cmp(a2)
                    .then_with(|| b1.cmp(b2))
                    .then_with(|| c1.cmp(c2))
                    .then_with(|| d1.cmp(d2))
                    .then_with(|| e1.cmp(e2))
                    .then_with(|| f1.cmp(f2))
                    .then_with(|| g1.cmp(g2))
                    .then_with(|| h1.cmp(h2)),
                (Tool::ConfigCheck { props: a1 }, Tool::ConfigCheck { props: a2 }) => a1.cmp(a2),
                (Tool::GetConfig { tout: a1 }, Tool::GetConfig { tout: a2 }) => a1.cmp(a2),
                _ => std::cmp::Ordering::Equal,
            }
        }
    }

    impl<CX: ContextFamily> std::fmt::Debug for Tool<CX> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                Tool::CreateElement(box CreateElementData {
                    component,
                    jsx_props,
                    tout,
                    targs,
                    return_hint,
                    inferred_targs,
                    specialized_component,
                    specialized_props,
                }) => f
                    .debug_struct("CreateElement")
                    .field("component", component)
                    .field("jsx_props", jsx_props)
                    .field("tout", tout)
                    .field("targs", targs)
                    .field("return_hint", return_hint)
                    .field("inferred_targs", inferred_targs)
                    .field("specialized_component", specialized_component)
                    .field("specialized_props", specialized_props)
                    .finish(),
                Tool::ConfigCheck { props } => {
                    f.debug_struct("ConfigCheck").field("props", props).finish()
                }
                Tool::GetConfig { tout } => {
                    f.debug_struct("GetConfig").field("tout", tout).finish()
                }
            }
        }
    }
}

pub mod arith_kind {
    use flow_parser::ast::expression::AssignmentOperator;
    use flow_parser::ast::expression::BinaryOperator;

    use super::*;

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
    pub enum ArithKindInner {
        Plus,
        RShift3,
        Other,
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
    pub struct ArithKind(pub FlowSmolStr, pub ArithKindInner);

    impl ArithKind {
        pub fn of_binary_operator(op: BinaryOperator) -> ArithKind {
            let s = FlowSmolStr::from(op.as_str());
            let kind = match op {
                BinaryOperator::Plus => ArithKindInner::Plus,
                BinaryOperator::RShift3 => ArithKindInner::RShift3,
                _ => ArithKindInner::Other,
            };
            ArithKind(s, kind)
        }

        pub fn of_assignment_operator(op: AssignmentOperator) -> ArithKind {
            let s = FlowSmolStr::from(op.as_str());
            let kind = match op {
                AssignmentOperator::PlusAssign => ArithKindInner::Plus,
                AssignmentOperator::RShift3Assign => ArithKindInner::RShift3,
                _ => ArithKindInner::Other,
            };
            ArithKind(s, kind)
        }

        pub fn to_string(&self) -> &str {
            &self.0
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum UnaryArithKind {
    Plus,
    Minus,
    BitNot,
    Update,
}

pub mod type_collector {
    use std::cell::RefCell;

    use super::*;

    #[derive(Debug, Clone, Dupe)]
    pub struct TypeCollector {
        types: Rc<RefCell<BTreeSet<Type>>>,
    }

    impl PartialEq for TypeCollector {
        fn eq(&self, other: &Self) -> bool {
            std::ptr::eq(self.types.as_ptr(), other.types.as_ptr())
        }
    }

    impl Eq for TypeCollector {}

    impl PartialOrd for TypeCollector {
        fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
            Some(self.cmp(other))
        }
    }

    impl Ord for TypeCollector {
        fn cmp(&self, other: &Self) -> std::cmp::Ordering {
            (self.types.as_ptr() as usize).cmp(&(other.types.as_ptr() as usize))
        }
    }

    impl std::hash::Hash for TypeCollector {
        fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
            std::ptr::hash(self.types.as_ptr(), state)
        }
    }

    impl TypeCollector {
        pub fn create() -> Self {
            TypeCollector {
                types: Rc::new(RefCell::new(BTreeSet::new())),
            }
        }

        pub fn add(&self, t: Type) {
            let is_empty =
                matches!(&*t, TypeInner::DefT(_, def_t) if matches!(&**def_t, DefTInner::EmptyT));
            if !is_empty {
                self.add_including_empty(t);
            }
        }

        /// Capture an immediate lower bound, including an explicit empty type.
        pub fn add_including_empty(&self, t: Type) {
            self.types.borrow_mut().insert(t);
        }

        pub fn collect(&self) -> BTreeSet<Type> {
            self.types.borrow().clone()
        }

        pub fn collect_to_vec(&self) -> Vec<Type> {
            self.types.borrow().iter().map(|t| t.dupe()).collect()
        }

        pub fn union_opt(&self, reason: Reason) -> Option<Type> {
            crate::type_util::union_of_ts_opt(
                reason,
                self.collect_to_vec(),
                Some(union_rep::UnionKind::ResolvedKind),
            )
        }

        pub fn iter<F>(&self, mut f: F) -> Result<(), flow_utils_concurrency::job_error::JobError>
        where
            F: FnMut(&Type) -> Result<(), flow_utils_concurrency::job_error::JobError>,
        {
            for t in self.types.borrow().iter() {
                f(t)?;
            }
            Ok(())
        }
    }
}

pub mod concretize_seen {
    use std::cell::RefCell;
    use std::collections::BTreeSet;

    use super::Type;

    #[derive(Debug, Clone)]
    pub struct ConcretizeSeen {
        seen: RefCell<BTreeSet<i32>>,
        expanding_type_apps: RefCell<Vec<Type>>,
    }

    impl PartialEq for ConcretizeSeen {
        fn eq(&self, other: &Self) -> bool {
            std::ptr::eq(self.seen.as_ptr(), other.seen.as_ptr())
        }
    }

    impl Eq for ConcretizeSeen {}

    impl PartialOrd for ConcretizeSeen {
        fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
            Some(self.cmp(other))
        }
    }

    impl Ord for ConcretizeSeen {
        fn cmp(&self, other: &Self) -> std::cmp::Ordering {
            (self.seen.as_ptr() as usize).cmp(&(other.seen.as_ptr() as usize))
        }
    }

    impl std::hash::Hash for ConcretizeSeen {
        fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
            std::ptr::hash(self.seen.as_ptr(), state)
        }
    }

    impl ConcretizeSeen {
        pub fn new() -> Self {
            ConcretizeSeen {
                seen: RefCell::new(BTreeSet::new()),
                expanding_type_apps: RefCell::new(Vec::new()),
            }
        }

        pub fn contains(&self, tvar: &i32) -> bool {
            self.seen.borrow().contains(tvar)
        }

        pub fn insert(&self, tvar: i32) {
            self.seen.borrow_mut().insert(tvar);
        }

        pub fn is_expanding_type_app(&self, t: &Type) -> bool {
            self.expanding_type_apps.borrow().contains(t)
        }

        pub fn push_expanding_type_app(&self, t: Type) {
            self.expanding_type_apps.borrow_mut().push(t);
        }

        pub fn pop_expanding_type_app(&self) {
            self.expanding_type_apps.borrow_mut().pop();
        }
    }
}

/// We need to record type description in error messages.
/// However, we cannot generate type descriptions during inference.
/// We also cannot make it lazy because we cannot compare lazy values.
/// Therefore, we create a variant for this. During inference, we will always
/// have Type. After inference, we turn it into `TypeDesc`  
pub mod type_or_type_desc {
    use flow_common_ty::ty::ALocElt;

    use super::*;

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
    pub enum ErrorTypeDesc<L: Dupe> {
        Normalized(ALocElt),
        ReasonDesc(VirtualReasonDesc<L>),
        Unknown,
    }

    impl<L: Dupe> ErrorTypeDesc<L> {
        pub fn map_locs<M: Dupe>(self, f: impl Fn(&L) -> M) -> ErrorTypeDesc<M> {
            match self {
                Self::Normalized(ty) => ErrorTypeDesc::Normalized(ty),
                Self::ReasonDesc(desc) => ErrorTypeDesc::ReasonDesc(desc.map_locs(&f)),
                Self::Unknown => ErrorTypeDesc::Unknown,
            }
        }
    }

    #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum TypeOrTypeDescT<L: Dupe> {
        Type(Type),
        TypeDesc(ErrorTypeDesc<L>),
    }

    impl<L> serde::Serialize for TypeOrTypeDescT<L>
    where
        L: Dupe + serde::Serialize,
    {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: serde::Serializer,
        {
            #[derive(serde::Serialize)]
            enum TypeOrTypeDescSerde<'a, L: Dupe> {
                Type,
                TypeDesc(&'a ErrorTypeDesc<L>),
            }

            match self {
                TypeOrTypeDescT::Type(_) => TypeOrTypeDescSerde::<L>::Type.serialize(serializer),
                TypeOrTypeDescT::TypeDesc(desc) => {
                    TypeOrTypeDescSerde::TypeDesc(desc).serialize(serializer)
                }
            }
        }
    }

    impl<'de, L> serde::Deserialize<'de> for TypeOrTypeDescT<L>
    where
        L: Dupe + serde::Deserialize<'de>,
    {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: serde::Deserializer<'de>,
        {
            #[derive(serde::Deserialize)]
            enum TypeOrTypeDescSerde<L: Dupe> {
                Type,
                TypeDesc(ErrorTypeDesc<L>),
            }

            Ok(match TypeOrTypeDescSerde::deserialize(deserializer)? {
                TypeOrTypeDescSerde::Type => TypeOrTypeDescT::TypeDesc(ErrorTypeDesc::Unknown),
                TypeOrTypeDescSerde::TypeDesc(desc) => TypeOrTypeDescT::TypeDesc(desc),
            })
        }
    }

    // SAFETY: TypeOrTypeDesc is only accessed under a Mutex in map_reduce::call
    // (one thread at a time). The only non-Send/Sync field is Type(Rc<TypeInner>),
    // which is never concurrently accessed. This mirrors OCaml's MultiWorkerLwt
    // which uses multi-process parallelism where marshalling handles isolation.
    unsafe impl<L: Dupe + Send> Send for TypeOrTypeDescT<L> {}
    unsafe impl<L: Dupe + Sync> Sync for TypeOrTypeDescT<L> {}

    pub fn map_loc<F, A, B>(f: F, t: TypeOrTypeDescT<A>) -> TypeOrTypeDescT<B>
    where
        F: Fn(&A) -> B,
        A: Dupe,
        B: Dupe,
    {
        match t {
            TypeOrTypeDescT::Type(ty) => TypeOrTypeDescT::Type(ty),
            TypeOrTypeDescT::TypeDesc(desc) => TypeOrTypeDescT::TypeDesc(desc.map_locs(f)),
        }
    }
}

pub fn unknown_use() -> UseOp {
    thread_local! {
        static CACHED: UseOp = VirtualUseOp::Op(Arc::new(VirtualRootUseOp::UnknownUse));
    }
    CACHED.with(|c| c.dupe())
}

pub fn name_of_propref(propref: &PropRef) -> Option<Name> {
    match propref {
        PropRef::Named { name, .. } => Some(name.dupe()),
        PropRef::Computed(_) => None,
    }
}

pub mod constraint {
    use super::*;

    pub type SpeculationId = i32;
    pub type CaseId = i32;

    pub struct UseTypeKey<CX: ContextFamily = ()> {
        pub use_t: UseT<CX>,
        pub assoc: Option<(SpeculationId, CaseId)>,
    }

    impl<CX: ContextFamily> Clone for UseTypeKey<CX> {
        fn clone(&self) -> Self {
            UseTypeKey {
                use_t: self.use_t.clone(),
                assoc: self.assoc,
            }
        }
    }

    impl<CX: ContextFamily> PartialEq for UseTypeKey<CX> {
        fn eq(&self, other: &Self) -> bool {
            self.use_t == other.use_t && self.assoc == other.assoc
        }
    }

    impl<CX: ContextFamily> Eq for UseTypeKey<CX> {}

    impl<CX: ContextFamily> std::hash::Hash for UseTypeKey<CX> {
        fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
            self.use_t.hash(state);
            self.assoc.hash(state);
        }
    }

    impl<CX: ContextFamily> std::fmt::Debug for UseTypeKey<CX> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("UseTypeKey")
                .field("use_t", &self.use_t)
                .field("assoc", &self.assoc)
                .finish()
        }
    }

    impl<CX: ContextFamily> PartialOrd for UseTypeKey<CX> {
        fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
            Some(self.cmp(other))
        }
    }

    impl<CX: ContextFamily> Ord for UseTypeKey<CX> {
        fn cmp(&self, other: &Self) -> std::cmp::Ordering {
            match self.assoc.cmp(&other.assoc) {
                std::cmp::Ordering::Equal => self.use_t.cmp(&other.use_t),
                other_ord => other_ord,
            }
        }
    }

    pub mod forcing_state {
        use std::cell::UnsafeCell;

        use flow_lazy::Thunk;

        use super::*;

        enum ForcingStateInner<CX: ContextFamily, A> {
            Lazy(Thunk<CX, A>),
            LazyResult(Thunk<CX, Result<A, flow_utils_concurrency::job_error::JobError>>),
            Forcing,
            Forced(A),
            ForcedWithCyclicError(A),
            ForcedWithJobError(A, flow_utils_concurrency::job_error::JobError),
        }

        pub struct State<CX: ContextFamily, A, B> {
            inner: Rc<UnsafeCell<ForcingStateInner<CX, A>>>,
            error_reason: Option<B>,
        }

        impl<CX: ContextFamily, A: Clone + std::fmt::Debug, B: Clone + std::fmt::Debug>
            std::fmt::Debug for State<CX, A, B>
        {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                // SAFETY: single-threaded, just reading for debug
                let status_str = match unsafe { &*self.inner.get() } {
                    ForcingStateInner::Lazy(_) => "Lazy",
                    ForcingStateInner::LazyResult(_) => "LazyResult",
                    ForcingStateInner::Forcing => "Forcing",
                    ForcingStateInner::Forced(_) => "Forced",
                    ForcingStateInner::ForcedWithCyclicError(_) => "ForcedWithCyclicError",
                    ForcingStateInner::ForcedWithJobError(_, _) => "ForcedWithJobError",
                };
                f.debug_struct("State")
                    .field("status", &status_str)
                    .field("error_reason", &self.error_reason)
                    .finish()
            }
        }

        pub type ForcingState<CX = ()> = State<CX, Type, Reason>;
        pub type ModuleTypeForcingState<CX = ()> = State<CX, Result<ModuleType, Type>, Reason>;

        impl<CX: ContextFamily, A: Dupe + 'static, B: Clone> State<CX, A, B> {
            pub fn of_lazy_t<F>(error_reason: B, f: F) -> Self
            where
                F: for<'a> FnOnce(&mut CX::Context<'a>) -> A + 'static,
            {
                State {
                    inner: Rc::new(UnsafeCell::new(ForcingStateInner::Lazy(Box::new(f)))),
                    error_reason: Some(error_reason),
                }
            }

            pub fn try_of_lazy_t<F>(error_reason: B, f: F) -> Self
            where
                F: for<'a> FnOnce(
                        &mut CX::Context<'a>,
                    )
                        -> Result<A, flow_utils_concurrency::job_error::JobError>
                    + 'static,
            {
                State {
                    inner: Rc::new(UnsafeCell::new(ForcingStateInner::LazyResult(Box::new(f)))),
                    error_reason: Some(error_reason),
                }
            }

            pub fn of_non_lazy_t(t: A) -> Self {
                State {
                    inner: Rc::new(UnsafeCell::new(ForcingStateInner::Forced(t))),
                    error_reason: None,
                }
            }

            pub fn force<'c, F>(&self, cx: &mut CX::Context<'c>, on_error: F) -> A
            where
                F: FnOnce(&mut CX::Context<'c>, &B) -> A,
            {
                // SAFETY: single-threaded, no concurrent access to the same State
                let inner = unsafe { &mut *self.inner.get() };

                match inner {
                    ForcingStateInner::Lazy(_) => {
                        let old = std::mem::replace(inner, ForcingStateInner::Forcing);
                        let ForcingStateInner::Lazy(f) = old else {
                            unreachable!()
                        };

                        let t = f(cx);

                        match inner {
                            ForcingStateInner::Forcing => {
                                *inner = ForcingStateInner::Forced(t.dupe());
                                t
                            }
                            ForcingStateInner::ForcedWithCyclicError(err_val) => err_val.dupe(),
                            _ => panic!("Invalid state after forcing"),
                        }
                    }
                    ForcingStateInner::LazyResult(_) => {
                        let old = std::mem::replace(inner, ForcingStateInner::Forcing);
                        let ForcingStateInner::LazyResult(f) = old else {
                            unreachable!()
                        };

                        match f(cx) {
                            Ok(t) => match inner {
                                ForcingStateInner::Forcing => {
                                    *inner = ForcingStateInner::Forced(t.dupe());
                                    t
                                }
                                ForcingStateInner::ForcedWithCyclicError(err_val) => err_val.dupe(),
                                _ => panic!("Invalid state after forcing"),
                            },
                            Err(e) => {
                                let fallback = on_error(
                                    cx,
                                    self.error_reason.as_ref().expect("No error reason"),
                                );
                                *inner = ForcingStateInner::ForcedWithJobError(fallback.dupe(), e);
                                fallback
                            }
                        }
                    }
                    ForcingStateInner::Forcing => {
                        let t = on_error(cx, self.error_reason.as_ref().expect("No error reason"));
                        *inner = ForcingStateInner::ForcedWithCyclicError(t.dupe());
                        t
                    }
                    ForcingStateInner::Forced(t) => t.dupe(),
                    ForcingStateInner::ForcedWithCyclicError(t) => t.dupe(),
                    ForcingStateInner::ForcedWithJobError(t, _) => t.dupe(),
                }
            }

            pub fn try_force<'c, F>(
                &self,
                cx: &mut CX::Context<'c>,
                on_error: F,
            ) -> Result<A, flow_utils_concurrency::job_error::JobError>
            where
                F: FnOnce(&mut CX::Context<'c>, &B) -> A,
            {
                // SAFETY: single-threaded, no concurrent access to the same State
                let inner = unsafe { &mut *self.inner.get() };

                match inner {
                    ForcingStateInner::Lazy(_) => {
                        let old = std::mem::replace(inner, ForcingStateInner::Forcing);
                        let ForcingStateInner::Lazy(f) = old else {
                            unreachable!()
                        };

                        let t = f(cx);

                        match inner {
                            ForcingStateInner::Forcing => {
                                *inner = ForcingStateInner::Forced(t.dupe());
                                Ok(t)
                            }
                            ForcingStateInner::ForcedWithCyclicError(err_val) => Ok(err_val.dupe()),
                            _ => panic!("Invalid state after forcing"),
                        }
                    }
                    ForcingStateInner::LazyResult(_) => {
                        let old = std::mem::replace(inner, ForcingStateInner::Forcing);
                        let ForcingStateInner::LazyResult(f) = old else {
                            unreachable!()
                        };

                        match f(cx) {
                            Ok(t) => match inner {
                                ForcingStateInner::Forcing => {
                                    *inner = ForcingStateInner::Forced(t.dupe());
                                    Ok(t)
                                }
                                ForcingStateInner::ForcedWithCyclicError(err_val) => {
                                    Ok(err_val.dupe())
                                }
                                _ => panic!("Invalid state after forcing"),
                            },
                            Err(e) => {
                                let fallback = on_error(
                                    cx,
                                    self.error_reason.as_ref().expect("No error reason"),
                                );
                                *inner = ForcingStateInner::ForcedWithJobError(fallback, e.dupe());
                                Err(e)
                            }
                        }
                    }
                    ForcingStateInner::Forcing => {
                        let t = on_error(cx, self.error_reason.as_ref().expect("No error reason"));
                        *inner = ForcingStateInner::ForcedWithCyclicError(t.dupe());
                        Ok(t)
                    }
                    ForcingStateInner::Forced(t) => Ok(t.dupe()),
                    ForcingStateInner::ForcedWithCyclicError(t) => Ok(t.dupe()),
                    ForcingStateInner::ForcedWithJobError(_, e) => Err(e.dupe()),
                }
            }

            pub fn already_forced_with_cyclic_error(&self) -> bool {
                matches!(
                    unsafe { &*self.inner.get() },
                    ForcingStateInner::ForcedWithCyclicError(_)
                )
            }

            pub fn error_reason(&self) -> Option<&B> {
                self.error_reason.as_ref()
            }

            pub fn ptr_eq(&self, other: &Self) -> bool {
                Rc::ptr_eq(&self.inner, &other.inner)
            }

            /// The value if it has been forced, without forcing it.
            pub fn forced(&self) -> Option<A> {
                match unsafe { &*self.inner.get() } {
                    ForcingStateInner::Lazy(_)
                    | ForcingStateInner::LazyResult(_)
                    | ForcingStateInner::Forcing => None,
                    ForcingStateInner::Forced(t) => Some(t.dupe()),
                    ForcingStateInner::ForcedWithCyclicError(t) => Some(t.dupe()),
                    ForcingStateInner::ForcedWithJobError(t, _) => Some(t.dupe()),
                }
            }

            /// A state for another context's graph. Forcing it calls `force_copy` with the
            /// context it is forced in and this state, which `force_copy` forces in the context
            /// it belongs to.
            pub fn copy<F>(&self, force_copy: F) -> Self
            where
                B: 'static,
                F: for<'a> FnOnce(&mut CX::Context<'a>, &Self) -> A + 'static,
            {
                let src_state = self.clone();
                State {
                    inner: Rc::new(UnsafeCell::new(ForcingStateInner::Lazy(Box::new(
                        move |dst_cx: &mut CX::Context<'_>| force_copy(dst_cx, &src_state),
                    )))),
                    error_reason: self.error_reason.clone(),
                }
            }
        }

        impl<CX: ContextFamily> State<CX, Result<ModuleType, Type>, Reason> {
            pub fn of_lazy_module(
                reason: Reason,
                lazy_module: Rc<flow_lazy::Lazy<CX, ModuleType>>,
            ) -> Self {
                State {
                    inner: Rc::new(UnsafeCell::new(ForcingStateInner::Lazy(Box::new(
                        move |cx: &mut CX::Context<'_>| Ok(lazy_module.get_forced(cx).dupe()),
                    )))),
                    error_reason: Some(reason),
                }
            }

            pub fn of_error_module(t: Type) -> Self {
                State {
                    inner: Rc::new(UnsafeCell::new(ForcingStateInner::Forced(Err(t)))),
                    error_reason: None,
                }
            }
        }

        impl<CX: ContextFamily, A: Clone, B: Clone> Clone for State<CX, A, B> {
            fn clone(&self) -> Self {
                State {
                    inner: self.inner.clone(),
                    error_reason: self.error_reason.clone(),
                }
            }
        }

        #[cfg(test)]
        mod tests {
            use std::cell::RefCell;
            use std::rc::Rc;

            use flow_common::reason::locationless_reason;

            use super::*;
            use crate::type_::any_t;

            fn assert_forced_to_any(s: &ForcingState) {
                match &*s.force(&mut (), |_, r| any_t::error(r.dupe())) {
                    TypeInner::AnyT(_, _) => {}
                    _ => panic!("Invalid type"),
                }
            }

            #[test]
            fn invalid_self_recursive() {
                let s: Rc<RefCell<Option<ForcingState>>> = Rc::new(RefCell::new(None));
                let s_clone = s.clone();
                let state = ForcingState::of_lazy_t(
                    locationless_reason(VirtualReasonDesc::RNull),
                    move |_cx| {
                        let s_ref = s_clone.borrow();
                        let inner_state = s_ref.as_ref().unwrap();
                        inner_state.force(&mut (), |_, r| any_t::error(r.dupe()))
                    },
                );
                *s.borrow_mut() = Some(state);
                let s_ref = s.borrow();
                let state = s_ref.as_ref().unwrap();
                assert_forced_to_any(state);
            }

            #[test]
            fn invalid_mutually_recursive() {
                let s1: Rc<RefCell<Option<ForcingState>>> = Rc::new(RefCell::new(None));
                let s2: Rc<RefCell<Option<ForcingState>>> = Rc::new(RefCell::new(None));

                let s2_for_s1 = s2.clone();
                let state1 = ForcingState::of_lazy_t(
                    locationless_reason(VirtualReasonDesc::RNull),
                    move |_cx| {
                        let s2_ref = s2_for_s1.borrow();
                        let inner_state = s2_ref.as_ref().unwrap();
                        inner_state.force(&mut (), |_, r| any_t::error(r.dupe()))
                    },
                );

                let s1_for_s2 = s1.clone();
                let state2 = ForcingState::of_lazy_t(
                    locationless_reason(VirtualReasonDesc::RNull),
                    move |_cx| {
                        let s1_ref = s1_for_s2.borrow();
                        let inner_state = s1_ref.as_ref().unwrap();
                        inner_state.force(&mut (), |_, r| any_t::error(r.dupe()))
                    },
                );

                *s1.borrow_mut() = Some(state1);
                *s2.borrow_mut() = Some(state2);

                let s1_ref = s1.borrow();
                let s2_ref = s2.borrow();
                assert_forced_to_any(s1_ref.as_ref().unwrap());
                assert_forced_to_any(s2_ref.as_ref().unwrap());
            }

            #[test]
            fn invalid_self_recursive_mapped() {
                let s: Rc<RefCell<Option<ForcingState>>> = Rc::new(RefCell::new(None));
                let s_clone = s.clone();
                let state = ForcingState::of_lazy_t(
                    locationless_reason(VirtualReasonDesc::RNull),
                    move |_cx| {
                        let s_ref = s_clone.borrow();
                        let inner_state = s_ref.as_ref().unwrap();
                        inner_state.force(&mut (), |_, r| any_t::error(r.dupe()))
                    },
                );
                *s.borrow_mut() = Some(state);

                let s_ref = s.borrow();
                let state = s_ref.as_ref().unwrap();
                let state_copy = state.copy(|_dst_cx, src_state| {
                    src_state.force(&mut (), |_, r| any_t::error(r.dupe()))
                });
                assert_forced_to_any(&state_copy);
            }

            #[test]
            fn invalid_self_recursive_force_twice() {
                let s: Rc<RefCell<Option<ForcingState>>> = Rc::new(RefCell::new(None));
                let s_clone = s.clone();
                let state = ForcingState::of_lazy_t(
                    locationless_reason(VirtualReasonDesc::RNull),
                    move |_cx| {
                        let s_ref = s_clone.borrow();
                        let inner_state = s_ref.as_ref().unwrap();
                        let _ = inner_state.force(&mut (), |_, r| any_t::error(r.dupe()));
                        inner_state.force(&mut (), |_, r| any_t::error(r.dupe()))
                    },
                );
                *s.borrow_mut() = Some(state);

                let s_ref = s.borrow();
                let state = s_ref.as_ref().unwrap();
                let state_copy = state.copy(|_dst_cx, src_state| {
                    src_state.force(&mut (), |_, r| any_t::error(r.dupe()))
                });
                assert_forced_to_any(&state_copy);
            }

            #[test]
            fn lazy_result_ok_caches_via_try_force() {
                let state: ForcingState = ForcingState::try_of_lazy_t(
                    locationless_reason(VirtualReasonDesc::RNull),
                    move |_cx| Ok(any_t::error(locationless_reason(VirtualReasonDesc::RNull))),
                );
                let r1 = state.try_force(&mut (), |_, r| any_t::error(r.dupe()));
                let r2 = state.try_force(&mut (), |_, r| any_t::error(r.dupe()));
                assert!(r1.is_ok());
                assert!(r2.is_ok());
            }

            #[test]
            fn lazy_result_err_propagates_then_caches() {
                use flow_utils_concurrency::job_error::JobError;
                use flow_utils_concurrency::worker_cancel::WorkerCanceled;

                let state: ForcingState = ForcingState::try_of_lazy_t(
                    locationless_reason(VirtualReasonDesc::RNull),
                    move |_cx| Err(JobError::Canceled(WorkerCanceled)),
                );
                let r1 = state.try_force(&mut (), |_, r| any_t::error(r.dupe()));
                assert!(matches!(r1, Err(JobError::Canceled(_))));
                // Second call returns the cached error.
                let r2 = state.try_force(&mut (), |_, r| any_t::error(r.dupe()));
                assert!(matches!(r2, Err(JobError::Canceled(_))));
            }

            #[test]
            fn lazy_result_err_via_force_returns_fallback_then_try_force_returns_err() {
                use flow_utils_concurrency::job_error::JobError;
                use flow_utils_concurrency::worker_cancel::WorkerCanceled;

                let state: ForcingState = ForcingState::try_of_lazy_t(
                    locationless_reason(VirtualReasonDesc::RNull),
                    move |_cx| Err(JobError::Canceled(WorkerCanceled)),
                );
                // Old API: caller observes only the fallback A.
                let fallback = state.force(&mut (), |_, r| any_t::error(r.dupe()));
                match &*fallback {
                    TypeInner::AnyT(_, _) => {}
                    _ => panic!("Invalid type"),
                }
                // New API on the same state still surfaces the JobError.
                let r = state.try_force(&mut (), |_, r| any_t::error(r.dupe()));
                assert!(matches!(r, Err(JobError::Canceled(_))));
            }
        }
    }

    #[derive(Debug, Clone)]
    pub enum Constraints<CX: ContextFamily = ()> {
        Resolved(Type),
        Unresolved(BoundsRef<CX>),
        FullyResolved(forcing_state::ForcingState<CX>),
    }

    impl<CX: ContextFamily> Default for Constraints<CX> {
        fn default() -> Self {
            Constraints::Unresolved(Rc::new(std::cell::RefCell::new(Bounds::default())))
        }
    }

    pub type BoundsRef<CX = ()> = Rc<std::cell::RefCell<Bounds<CX>>>;

    #[derive(Debug)]
    pub struct Bounds<CX: ContextFamily = ()> {
        pub lower: BTreeMap<Type, (DepthTrace, UseOp)>,
        pub upper: BTreeMap<UseTypeKey<CX>, DepthTrace>,
        pub lowertvars: BTreeMap<i32, (DepthTrace, UseOp)>,
        pub uppertvars: BTreeMap<i32, (DepthTrace, UseOp)>,
    }

    impl<CX: ContextFamily> Clone for Bounds<CX> {
        fn clone(&self) -> Self {
            Bounds {
                lower: self.lower.clone(),
                upper: self.upper.clone(),
                lowertvars: self.lowertvars.clone(),
                uppertvars: self.uppertvars.clone(),
            }
        }
    }

    impl<CX: ContextFamily> Default for Bounds<CX> {
        fn default() -> Self {
            Self::empty()
        }
    }

    impl<CX: ContextFamily> Bounds<CX> {
        pub fn empty() -> Self {
            Bounds {
                lower: BTreeMap::new(),
                upper: BTreeMap::new(),
                lowertvars: BTreeMap::new(),
                uppertvars: BTreeMap::new(),
            }
        }
    }
}

pub mod aconstraint {
    use std::cell::RefCell;
    use std::ops::Deref;

    use super::*;

    #[derive(
        Debug,
        Clone,
        Dupe,
        PartialEq,
        Eq,
        PartialOrd,
        Ord,
        Hash,
        serde::Serialize,
        serde::Deserialize
    )]
    pub enum AnnotationInferenceOperation {
        ImportsAndExports,
        CommonJSExports,
        Inspection,
        ImportTypeof(FlowSmolStr),
        Export(Name),
        ClassExtends,
        ClassMixins,
        TypeApplication,
        ThisSpecialization,
        TypeReference,
        Property(Option<Name>),
        ComputedProperty,
        Enum,
        Statics,
        Exact,
        ReactCheckComponentConfig,
        ReadOnly,
        Partial,
        Required,
        Spread,
        Rest,
        ReactConfig,
        Object,
        MappedType,
        Prototype,
        Mixins,
        Arithmetic(FlowSmolStr),
        UnaryPlus,
        UnaryMinus,
        BitwiseNot,
        Update,
        UnaryNot,
        ObjectKeyMirror,
        DeepReadOnly,
        KeySet,
        ToString,
        Values,
        ArrayRest,
    }

    #[derive(Clone)]
    pub struct AnnotSpecializeTData {
        pub use_op: UseOp,
        pub reason: Reason,
        pub reason2: Reason,
        pub types: Option<Rc<[Type]>>,
        pub operation: AnnotationInferenceOperation,
    }

    #[derive(Clone)]
    pub struct AnnotGetTypeFromNamespaceTData {
        pub use_op: UseOp,
        pub reason: Reason,
        pub prop_ref: (Reason, Name),
    }

    #[derive(Clone)]
    pub struct AnnotGetPropTData {
        pub reason: Reason,
        pub use_op: UseOp,
        pub from_annot: bool,
        pub prop_ref: PropRef,
    }

    #[derive(Clone)]
    pub struct AnnotLookupTData {
        pub reason: Reason,
        pub use_op: UseOp,
        pub prop_ref: PropRef,
        pub type_: Type,
        /// See [`super::LookupTData::indexer_fallback`]. Annotation inference walks the
        /// prototype chain by recursion rather than through the constraint graph, but the
        /// candidate still has to reach the end of the chain, so it rides along here too.
        pub indexer_fallback: Option<Box<super::IndexerFallbackData>>,
    }

    #[derive(Clone)]
    pub struct AnnotObjKitTData {
        pub reason: Reason,
        pub use_op: UseOp,
        pub resolve_tool: object::ResolveTool,
        pub tool: object::Tool,
    }

    #[derive(Clone)]
    pub struct AnnotArithTData {
        pub reason: Reason,
        pub flip: bool,
        pub rhs_t: Type,
        pub kind: arith_kind::ArithKind,
    }

    #[derive(Clone)]
    pub struct AnnotDeepReadOnlyTData {
        pub reason: Reason,
        pub loc: ALoc,
        pub dro_type: DroType,
    }

    pub type AnnotConcretizeForImportsExportsFn<CX> =
        Rc<dyn for<'a> Fn(&mut <CX as ContextFamily>::Context<'a>, Type) -> Type>;

    #[derive(Clone)]
    pub enum OpInner<CX: ContextFamily> {
        AnnotConcretizeForImportsExports(Reason, AnnotConcretizeForImportsExportsFn<CX>),
        AnnotConcretizeForCJSExtractNamedExportsAndTypeExports(Reason),
        AnnotConcretizeForInspection {
            reason: Reason,
            collector: type_collector::TypeCollector,
        },
        AnnotConcretizeForClassExtends {
            reason: Reason,
            collector: type_collector::TypeCollector,
            this_t: Type,
        },
        AnnotImportTypeofT {
            reason: Reason,
            name: FlowSmolStr,
        },
        AnnotAssertExportIsTypeT {
            reason: Reason,
            name: Name,
        },
        AnnotSpecializeT(Box<AnnotSpecializeTData>),
        AnnotThisSpecializeT {
            reason: Reason,
            type_: Type,
        },
        AnnotUseTTypeT {
            reason: Reason,
            kind: TypeTKind,
        },
        AnnotGetTypeFromNamespaceT(Box<AnnotGetTypeFromNamespaceTData>),
        AnnotGetEnumT(Reason),
        AnnotGetPropT(Box<AnnotGetPropTData>),
        AnnotGetElemT {
            reason: Reason,
            use_op: UseOp,
            key: Type,
        },
        AnnotElemT {
            reason: Reason,
            use_op: UseOp,
            from_annot: bool,
            source: Type,
        },
        AnnotGetStaticsT(Reason),
        AnnotLookupT(Box<AnnotLookupTData>),
        AnnotObjKitT(Box<AnnotObjKitTData>),
        AnnotObjTestProtoT(Reason),
        AnnotMixinT(Reason),
        AnnotArithT(Box<AnnotArithTData>),
        AnnotUnaryArithT {
            reason: Reason,
            kind: UnaryArithKind,
        },
        AnnotNotT(Reason),
        AnnotObjKeyMirror(Reason),
        AnnotDeepReadOnlyT(Box<AnnotDeepReadOnlyTData>),
        AnnotGetKeysT(Reason),
        // The indexer half of `AnnotGetKeysT`, mirroring `GetKeysDictKeyT`.
        // Annotation inference always includes symbol keys, since a `keyof`
        // annotation is type-level and never runtime enumeration.
        AnnotGetKeysDictKeyT(Reason),
        AnnotToStringT {
            orig_t: Option<Type>,
            reason: Reason,
        },
        AnnotObjRestT {
            reason: Reason,
            keys: Rc<[FlowSmolStr]>,
        },
        AnnotGetValuesT(Reason),
        AnnotFilterOptionalT(Reason),
    }

    #[derive(Clone, Dupe)]
    pub struct Op<CX: ContextFamily>(Rc<OpInner<CX>>);

    impl<CX: ContextFamily> Op<CX> {
        pub fn new(inner: OpInner<CX>) -> Self {
            Op(Rc::new(inner))
        }
    }

    impl<CX: ContextFamily> Deref for Op<CX> {
        type Target = OpInner<CX>;

        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }

    /// This kind of constraint is meant to represent type annotations. Unlike the
    /// constraints described above that may gradually evolve towards the solution
    /// of an unresolved tvar, annotation constraints are resolved immediately.
    ///
    /// There are three kinds of constraints:
    ///
    /// - `Annot_unresolved` is a constraint for which we have no information yet.
    ///   This state is associated, for example, with a type variable representing
    ///   a type alias that we have just started visiting, and so do not have a
    ///   type for its body yet.
    ///
    /// - `Annot_op { op, id, .. }` expresses the fact that the current variable is the
    ///   result of applying the operation `op` on the type that another annotation
    ///   variable `id` will resolve to.
    ///
    /// An annotation variable starts off in the Annot_unresolved or Annot_op state
    /// and is resolved in one step by removing it.
    ///
    /// As inference proceeds other types can depend on the current annotation variable
    /// through an Annot_op constraint. This fact is recorded in the `dependents` set
    /// that is maintained for every unresolved annotation variable. It is important
    /// to keep this information around, so we can force these dependents into
    /// evaluation when the current variable becomes resolved (see resolve_id in
    /// annotation_inference.rs). An implied invariant is that all variables in the
    /// dependent set are of the Annot_op kind.
    ///
    /// While in one of the two initial states the respective annotation variable
    /// ids are only present in the annotation graph. Once resolved, the resolved
    /// type is immediately stored in the main type graph as FullyResolved constraint.
    ///
    /// In rare cases like
    ///
    /// ```javascript
    /// // file rec-export.js
    /// import {p} from './rec-export';
    /// export {p};
    /// ```
    ///
    /// it is possible for a variable to never become resolved. This is the equivalent
    /// of a type variable (from above) to never accumulate any lower bounds. When
    /// such constraints get detected, the result is immediately replaced with 'any',
    /// which is similar to how we handle the above constraints, when that type
    /// variable is exported.
    #[derive(Clone)]
    pub enum AConstraintInner<CX: ContextFamily> {
        AnnotUnresolved {
            reason: Reason,
            dependents: Rc<RefCell<FlowOrdSet<i32>>>,
        },
        AnnotOp {
            op: Op<CX>,
            id: i32,
            dependents: Rc<RefCell<FlowOrdSet<i32>>>,
        },
    }

    #[derive(Clone, Dupe)]
    pub struct AConstraint<CX: ContextFamily>(Rc<AConstraintInner<CX>>);

    impl<CX: ContextFamily> AConstraint<CX> {
        pub fn new(inner: AConstraintInner<CX>) -> Self {
            AConstraint(Rc::new(inner))
        }
    }

    impl<CX: ContextFamily> Deref for AConstraint<CX> {
        type Target = AConstraintInner<CX>;

        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }

    impl<CX: ContextFamily> Op<CX> {
        pub fn reason(&self) -> Reason {
            match &**self {
                OpInner::AnnotConcretizeForImportsExports(reason, _)
                | OpInner::AnnotConcretizeForCJSExtractNamedExportsAndTypeExports(reason)
                | OpInner::AnnotConcretizeForInspection { reason, .. }
                | OpInner::AnnotConcretizeForClassExtends { reason, .. }
                | OpInner::AnnotImportTypeofT { reason, .. }
                | OpInner::AnnotAssertExportIsTypeT { reason, .. }
                | OpInner::AnnotThisSpecializeT { reason, .. }
                | OpInner::AnnotUseTTypeT { reason, .. }
                | OpInner::AnnotGetEnumT(reason)
                | OpInner::AnnotGetElemT { reason, .. }
                | OpInner::AnnotElemT { reason, .. }
                | OpInner::AnnotGetStaticsT(reason)
                | OpInner::AnnotObjTestProtoT(reason)
                | OpInner::AnnotMixinT(reason)
                | OpInner::AnnotUnaryArithT { reason, .. }
                | OpInner::AnnotNotT(reason)
                | OpInner::AnnotObjKeyMirror(reason)
                | OpInner::AnnotGetKeysT(reason)
                | OpInner::AnnotGetKeysDictKeyT(reason)
                | OpInner::AnnotToStringT { reason, .. }
                | OpInner::AnnotObjRestT { reason, .. }
                | OpInner::AnnotGetValuesT(reason)
                | OpInner::AnnotFilterOptionalT(reason) => reason.dupe(),
                OpInner::AnnotSpecializeT(data) => data.reason.dupe(),
                OpInner::AnnotGetTypeFromNamespaceT(data) => data.reason.dupe(),
                OpInner::AnnotGetPropT(data) => data.reason.dupe(),
                OpInner::AnnotLookupT(data) => data.reason.dupe(),
                OpInner::AnnotObjKitT(data) => data.reason.dupe(),
                OpInner::AnnotArithT(data) => data.reason.dupe(),
                OpInner::AnnotDeepReadOnlyT(data) => data.reason.dupe(),
            }
        }

        pub fn use_op(&self) -> Option<UseOp> {
            match &**self {
                OpInner::AnnotGetElemT { use_op, .. } | OpInner::AnnotElemT { use_op, .. } => {
                    Some(use_op.dupe())
                }
                OpInner::AnnotSpecializeT(data) => Some(data.use_op.dupe()),
                OpInner::AnnotGetTypeFromNamespaceT(data) => Some(data.use_op.dupe()),
                OpInner::AnnotGetPropT(data) => Some(data.use_op.dupe()),
                OpInner::AnnotLookupT(data) => Some(data.use_op.dupe()),
                OpInner::AnnotObjKitT(data) => Some(data.use_op.dupe()),
                _ => None,
            }
        }
    }

    impl<CX: ContextFamily> AConstraint<CX> {
        pub fn deps(&self) -> &RefCell<FlowOrdSet<i32>> {
            match &**self {
                AConstraintInner::AnnotUnresolved { dependents, .. } => dependents,
                AConstraintInner::AnnotOp { dependents, .. } => dependents,
            }
        }

        pub fn to_annot_op_exn(&self) -> &Op<CX> {
            match &**self {
                AConstraintInner::AnnotUnresolved { .. } => panic!("to_annot_op_exn on unresolved"),
                AConstraintInner::AnnotOp { op, .. } => op,
            }
        }

        pub fn update_deps<F>(&self, f: F)
        where
            F: FnOnce(FlowOrdSet<i32>) -> FlowOrdSet<i32>,
        {
            let deps = self.deps();
            let old_deps = deps.borrow().dupe();
            *deps.borrow_mut() = f(old_deps);
        }
    }

    impl<CX: ContextFamily> Op<CX> {
        pub fn string_of_operation(&self) -> &'static str {
            match &**self {
                OpInner::AnnotSpecializeT(_) => "Annot_SpecializeT",
                OpInner::AnnotDeepReadOnlyT(_) => "Annot_DeepReadOnlyT",
                OpInner::AnnotThisSpecializeT { .. } => "Annot_ThisSpecializeT",
                OpInner::AnnotUseTTypeT { .. } => "Annot_UseT_TypeT",
                OpInner::AnnotConcretizeForImportsExports(_, _) => {
                    "Annot_ConcretizeForImportsExports"
                }
                OpInner::AnnotConcretizeForCJSExtractNamedExportsAndTypeExports(_) => {
                    "Annot_ConcretizeForCJSExtractNamedExportsAndTypeExports"
                }
                OpInner::AnnotConcretizeForInspection { .. } => "Annot_ConcretizeForInspection",
                OpInner::AnnotConcretizeForClassExtends { .. } => "Annot_ConcretizeForClassExtends",
                OpInner::AnnotImportTypeofT { .. } => "Annot_ImportTypeofT",
                OpInner::AnnotAssertExportIsTypeT { .. } => "Annot_AssertExportIsTypeT",
                OpInner::AnnotGetTypeFromNamespaceT(_) => "Annot_GetTypeFromNamespaceT",
                OpInner::AnnotGetPropT(_) => "Annot_GetPropT",
                OpInner::AnnotGetElemT { .. } => "Annot_GetElemT",
                OpInner::AnnotGetEnumT(_) => "Annot_GetEnumT",
                OpInner::AnnotElemT { .. } => "Annot_ElemT",
                OpInner::AnnotGetStaticsT(_) => "Annot_GetStaticsT",
                OpInner::AnnotLookupT(_) => "Annot_LookupT",
                OpInner::AnnotObjKitT(_) => "Annot_ObjKitT",
                OpInner::AnnotObjTestProtoT(_) => "Annot_ObjTestProtoT",
                OpInner::AnnotMixinT(_) => "Annot_MixinT",
                OpInner::AnnotArithT(_) => "Annot_ArithT",
                OpInner::AnnotUnaryArithT { .. } => "Annot_UnaryArithT",
                OpInner::AnnotNotT(_) => "Annot_NotT",
                OpInner::AnnotObjKeyMirror(_) => "Annot_ObjKeyMirror",
                OpInner::AnnotGetKeysT(_) => "Annot_GetKeysT",
                OpInner::AnnotGetKeysDictKeyT(_) => "Annot_GetKeysDictKeyT",
                OpInner::AnnotToStringT { .. } => "Annot_ToStringT",
                OpInner::AnnotObjRestT { .. } => "Annot_ObjRestT",
                OpInner::AnnotGetValuesT(_) => "Annot_GetValuesT",
                OpInner::AnnotFilterOptionalT(_) => "Annot_FilterOptionalT",
            }
        }

        pub fn error_operation(&self) -> AnnotationInferenceOperation {
            match &**self {
                OpInner::AnnotConcretizeForImportsExports(_, _) => {
                    AnnotationInferenceOperation::ImportsAndExports
                }
                OpInner::AnnotConcretizeForCJSExtractNamedExportsAndTypeExports(_) => {
                    AnnotationInferenceOperation::CommonJSExports
                }
                OpInner::AnnotConcretizeForInspection { .. } => {
                    AnnotationInferenceOperation::Inspection
                }
                OpInner::AnnotConcretizeForClassExtends { .. } => {
                    AnnotationInferenceOperation::ThisSpecialization
                }
                OpInner::AnnotImportTypeofT { name, .. } => {
                    AnnotationInferenceOperation::ImportTypeof(name.dupe())
                }
                OpInner::AnnotAssertExportIsTypeT { name, .. } => {
                    AnnotationInferenceOperation::Export(name.dupe())
                }
                OpInner::AnnotSpecializeT(data) => data.operation.dupe(),
                OpInner::AnnotThisSpecializeT { .. } => {
                    AnnotationInferenceOperation::ThisSpecialization
                }
                OpInner::AnnotUseTTypeT { .. } => AnnotationInferenceOperation::TypeReference,
                OpInner::AnnotGetTypeFromNamespaceT(data) => {
                    AnnotationInferenceOperation::Property(Some(data.prop_ref.1.dupe()))
                }
                OpInner::AnnotGetEnumT(_) => AnnotationInferenceOperation::Enum,
                OpInner::AnnotGetPropT(data) => {
                    AnnotationInferenceOperation::Property(name_of_propref(&data.prop_ref))
                }
                OpInner::AnnotGetElemT { .. } | OpInner::AnnotElemT { .. } => {
                    AnnotationInferenceOperation::ComputedProperty
                }
                OpInner::AnnotGetStaticsT(_) => AnnotationInferenceOperation::Statics,
                OpInner::AnnotLookupT(data) => {
                    AnnotationInferenceOperation::Property(name_of_propref(&data.prop_ref))
                }
                OpInner::AnnotObjKitT(data) => match &data.tool {
                    object::Tool::MakeExact => AnnotationInferenceOperation::Exact,
                    object::Tool::ReactCheckComponentConfig { .. } => {
                        AnnotationInferenceOperation::ReactCheckComponentConfig
                    }
                    object::Tool::ReadOnly => AnnotationInferenceOperation::ReadOnly,
                    object::Tool::Partial => AnnotationInferenceOperation::Partial,
                    object::Tool::Required => AnnotationInferenceOperation::Required,
                    object::Tool::Spread { .. } => AnnotationInferenceOperation::Spread,
                    object::Tool::Rest { .. } => AnnotationInferenceOperation::Rest,
                    object::Tool::ReactConfig { .. } => AnnotationInferenceOperation::ReactConfig,
                    object::Tool::ObjectRep => AnnotationInferenceOperation::Object,
                    object::Tool::ObjectMap { .. } => AnnotationInferenceOperation::MappedType,
                },
                OpInner::AnnotObjTestProtoT(_) => AnnotationInferenceOperation::Prototype,
                OpInner::AnnotMixinT(_) => AnnotationInferenceOperation::Mixins,
                OpInner::AnnotArithT(data) => {
                    AnnotationInferenceOperation::Arithmetic(data.kind.0.dupe())
                }
                OpInner::AnnotUnaryArithT { kind, .. } => match kind {
                    UnaryArithKind::Plus => AnnotationInferenceOperation::UnaryPlus,
                    UnaryArithKind::Minus => AnnotationInferenceOperation::UnaryMinus,
                    UnaryArithKind::BitNot => AnnotationInferenceOperation::BitwiseNot,
                    UnaryArithKind::Update => AnnotationInferenceOperation::Update,
                },
                OpInner::AnnotNotT(_) => AnnotationInferenceOperation::UnaryNot,
                OpInner::AnnotObjKeyMirror(_) => AnnotationInferenceOperation::ObjectKeyMirror,
                OpInner::AnnotDeepReadOnlyT(_) => AnnotationInferenceOperation::DeepReadOnly,
                OpInner::AnnotGetKeysT(_) | OpInner::AnnotGetKeysDictKeyT(_) => {
                    AnnotationInferenceOperation::KeySet
                }
                OpInner::AnnotToStringT { .. } => AnnotationInferenceOperation::ToString,
                OpInner::AnnotObjRestT { .. } => AnnotationInferenceOperation::Rest,
                OpInner::AnnotGetValuesT(_) => AnnotationInferenceOperation::Values,
                OpInner::AnnotFilterOptionalT(_) => AnnotationInferenceOperation::MappedType,
            }
        }
    }
}

#[derive(Debug, Clone)]
pub enum UnifyCause {
    MutableArray {
        lower_array_t: Type,
        upper_array_t: Type,
    },
    MutableProperty {
        lower_obj_t: Type,
        upper_obj_t: Type,
        property_name: Option<FlowSmolStr>,
    },
    Uncategorized,
}

#[derive(Debug, Clone)]
pub struct TypeContext<CX: ContextFamily = ()> {
    /// map from tvar ids to nodes (type info structures)
    pub graph: flow_utils_union_find::Graph<constraint::Constraints<CX>>,
    /// obj types point to mutable property maps
    pub property_maps: BTreeMap<properties::Id, properties::PropertiesMap>,
    /// indirection to support context opt
    pub call_props: flow_data_structure_wrapper::int_map::IntHashMap<i32, Type>,
    /// modules point to mutable export maps
    pub export_maps: std::collections::HashMap<exports::Id, exports::T>,
    /// map from evaluation ids to types
    pub evaluated: eval::Map<Type>,
}

#[derive(Clone)]
pub struct FlowSet<CX: ContextFamily = ()> {
    // Single canonical map for membership lookup. `add` is O(log N) instead
    // of O(L * log N) — no walking of separate BTreeMap layers per call.
    //
    // The original Rust port stacked one `BTreeMap<Type, BTreeSet<UseT>>`
    // per speculation level; every `add` had to walk all layers checking
    // for duplicates. Profiling showed this dominated `__flow_impl` for
    // workloads with deep speculation nesting and was unique to the Rust
    // port — OCaml uses a single immutable `UseTypeSet.t TypeMap.t` with
    // ref-eq snapshot/restore, paying only one lookup per `add`.
    //
    // Snapshot/restore for speculation rollback now uses a per-level log
    // of inserted (l, u) pairs: `pop_level` undoes its log entries from
    // the canonical map, restoring the pre-snapshot state. The vast
    // majority of inserts succeed (entries kept) and never pay the
    // rollback cost.
    map: BTreeMap<Type, BTreeSet<UseT<CX>>>,
    // Per-level rollback logs. Each entry is the (l, u) pair inserted at
    // that level. There is always one base level whose log is unused
    // (entries at the base never need to be rolled back).
    logs: Vec<Vec<(Type, UseT<CX>)>>,
}

impl<CX: ContextFamily> std::fmt::Debug for FlowSet<CX> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FlowSet")
            .field("map", &self.map)
            .field("levels", &self.logs.len())
            .finish()
    }
}

impl<CX: ContextFamily> Default for FlowSet<CX> {
    fn default() -> Self {
        Self {
            map: BTreeMap::new(),
            logs: vec![Vec::new()],
        }
    }
}

impl<CX: ContextFamily> FlowSet<CX> {
    /// Returns whether the pair is inserted into the set.
    pub fn add(&mut self, l: Type, u: UseT<CX>) -> bool {
        // Capture the rollback-log info up front, but only `dupe`/`clone`
        // when we're inside an active speculation level (`logs.len() > 1`).
        // For the non-speculation common case those dupes would be pure
        // waste — pre-refactor `add` did neither dupe nor a log push.
        let in_speculation = self.logs.len() > 1;
        match self.map.entry(l) {
            std::collections::btree_map::Entry::Occupied(mut entry) => {
                if entry.get().contains(&u) {
                    return false;
                }
                let log_entry = if in_speculation {
                    Some((entry.key().dupe(), u.clone()))
                } else {
                    None
                };
                entry.get_mut().insert(u);
                if let Some(e) = log_entry {
                    self.logs.last_mut().unwrap().push(e);
                }
                true
            }
            std::collections::btree_map::Entry::Vacant(entry) => {
                let log_entry = if in_speculation {
                    Some((entry.key().dupe(), u.clone()))
                } else {
                    None
                };
                let mut set = BTreeSet::new();
                set.insert(u);
                entry.insert(set);
                if let Some(e) = log_entry {
                    self.logs.last_mut().unwrap().push(e);
                }
                true
            }
        }
    }

    pub fn fold<F, R>(&self, mut f: F, init: R) -> R
    where
        F: FnMut(R, (&Type, &UseT<CX>)) -> R,
    {
        self.map.iter().fold(init, |acc, (l, us)| {
            us.iter().fold(acc, |acc, u| f(acc, (l, u)))
        })
    }

    pub fn level_count(&self) -> usize {
        self.logs.len()
    }

    pub fn push_level(&mut self) {
        self.logs.push(Vec::new());
    }

    /// Pop the top level, undoing entries that were inserted since the level
    /// was pushed. The base level (logs[0]) is never popped.
    pub fn pop_level(&mut self) {
        if self.logs.len() <= 1 {
            return;
        }
        let log = self.logs.pop().unwrap();
        self.rollback_log(log);
    }

    /// Truncate to the given number of levels, undoing entries inserted at
    /// each popped level.
    pub fn truncate_to(&mut self, n: usize) {
        let n = n.max(1);
        while self.logs.len() > n {
            let log = self.logs.pop().unwrap();
            self.rollback_log(log);
        }
    }

    /// Move entries from the top level's rollback log into a specific
    /// target level. The top level remains on the stack with an empty log.
    /// Any subsequent `pop_level`/`truncate_to` past the target will undo
    /// these entries instead.
    pub fn move_top_entries_to_level(&mut self, target: usize) {
        let top_idx = self.logs.len() - 1;
        if top_idx == target {
            return;
        }
        if target >= self.logs.len() {
            // The cache was externally reset (e.g., via clear()) during
            // speculation, destroying levels that callers still reference.
            // The target level no longer exists; the entries that were in
            // the top level's log refer to entries that may not be safe to
            // roll back, so just drop the log (entries stay in the map).
            self.logs[top_idx].clear();
            return;
        }
        let entries = std::mem::take(&mut self.logs[top_idx]);
        self.logs[target].extend(entries);
    }

    /// Clear a specific level's entries without removing it from the stack.
    /// Undoes all entries logged at that level from the canonical map.
    pub fn clear_level(&mut self, index: usize) {
        if index >= self.logs.len() {
            return;
        }
        let log = std::mem::take(&mut self.logs[index]);
        self.rollback_log(log);
    }

    pub fn clear(&mut self) {
        self.map.clear();
        for log in &mut self.logs {
            log.clear();
        }
    }

    fn rollback_log(&mut self, log: Vec<(Type, UseT<CX>)>) {
        // Process in reverse so nested adds undo in LIFO order. Sets that
        // become empty are pruned from the map.
        for (l, u) in log.into_iter().rev() {
            if let std::collections::btree_map::Entry::Occupied(mut entry) = self.map.entry(l) {
                let set = entry.get_mut();
                set.remove(&u);
                if set.is_empty() {
                    entry.remove();
                }
            }
        }
    }
}

pub type SubstCacheMap<T> = FlowOrdMap<(poly::Id, Rc<[Type]>), T>;

pub type EvalIdCacheMap = MultiLevelMap<eval::Id, Type>;

pub type EvalIdSet = BTreeSet<eval::Id>;

pub type IdCacheMap = MultiLevelMap<(Type, TypeDestructorT), eval::Id>;

pub type EvalReposCacheMap = MultiLevelMap<(Type, TypeDestructorT, eval::Id), Type>;

pub type FixCacheMap = MultiLevelMap<(bool, Type), Type>;

pub type ThisSpecializeCacheMap = MultiLevelMap<(Type, Type), Type>;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ObjRestGuardKey {
    pub reason: Reason,
    pub props: properties::PropertiesMap,
    pub reachable_targs: Rc<[(Type, Polarity)]>,
    pub obj_kind: ObjKind,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ConstFoldKey {
    Reason(Reason),
    ObjRest(Box<ObjRestGuardKey>),
}

pub type ConstFoldMap = MultiLevelMap<(i32, ConstFoldKey, i32), i32>;

pub fn open_tvar(tvar: &Type) -> &Tvar {
    match &**tvar {
        TypeInner::OpenT(tvar) => tvar,
        _ => panic!("open_tvar called on non-OpenT"),
    }
}

pub mod num_module_t {
    use super::*;

    pub fn desc() -> flow_common::reason::ReasonDesc {
        flow_common::reason::VirtualReasonDesc::RNumber
    }

    pub fn make(reason: Reason) -> Type {
        Type::new(TypeInner::DefT(
            reason,
            DefT::new(DefTInner::NumGeneralT(Literal::AnyLiteral)),
        ))
    }

    pub fn at(tok: ALoc) -> Type {
        let reason = flow_common::reason::mk_annot_reason(desc(), tok);
        make(reason)
    }

    pub fn why(reason: Reason) -> Type {
        let reason = reason.replace_desc(desc());
        make(reason)
    }

    pub fn why_with_use_desc(use_desc: bool, r: Reason) -> Type {
        let r = if use_desc { r } else { r.replace_desc(desc()) };
        make(r)
    }
}

pub mod str_module_t {
    use super::*;

    pub fn desc() -> flow_common::reason::ReasonDesc {
        flow_common::reason::VirtualReasonDesc::RString
    }

    pub fn make(reason: Reason) -> Type {
        Type::new(TypeInner::DefT(
            reason,
            DefT::new(DefTInner::StrGeneralT(Literal::AnyLiteral)),
        ))
    }

    pub fn at(tok: ALoc) -> Type {
        let reason = flow_common::reason::mk_annot_reason(desc(), tok);
        make(reason)
    }

    pub fn why(reason: Reason) -> Type {
        let reason = reason.replace_desc(desc());
        make(reason)
    }

    pub fn why_with_use_desc(use_desc: bool, r: Reason) -> Type {
        let r = if use_desc { r } else { r.replace_desc(desc()) };
        make(r)
    }
}

pub mod bool_module_t {
    use super::*;

    pub fn desc() -> flow_common::reason::ReasonDesc {
        flow_common::reason::VirtualReasonDesc::RBoolean
    }

    pub fn make(reason: Reason) -> Type {
        Type::new(TypeInner::DefT(reason, DefT::new(DefTInner::BoolGeneralT)))
    }

    pub fn at(tok: ALoc) -> Type {
        let reason = flow_common::reason::mk_annot_reason(desc(), tok);
        make(reason)
    }

    pub fn why(reason: Reason) -> Type {
        let reason = reason.replace_desc(desc());
        make(reason)
    }

    pub fn why_with_use_desc(use_desc: bool, r: Reason) -> Type {
        let r = if use_desc { r } else { r.replace_desc(desc()) };
        make(r)
    }
}

pub mod bigint_module_t {
    use super::*;

    pub fn desc() -> flow_common::reason::ReasonDesc {
        flow_common::reason::VirtualReasonDesc::RBigInt
    }

    pub fn make(reason: Reason) -> Type {
        Type::new(TypeInner::DefT(
            reason,
            DefT::new(DefTInner::BigIntGeneralT(Literal::AnyLiteral)),
        ))
    }

    pub fn at(tok: ALoc) -> Type {
        let reason = flow_common::reason::mk_annot_reason(desc(), tok);
        make(reason)
    }

    pub fn why(reason: Reason) -> Type {
        let reason = reason.replace_desc(desc());
        make(reason)
    }

    pub fn why_with_use_desc(use_desc: bool, r: Reason) -> Type {
        let r = if use_desc { r } else { r.replace_desc(desc()) };
        make(r)
    }
}

pub mod symbol_t {
    use super::*;

    pub fn desc() -> flow_common::reason::ReasonDesc {
        flow_common::reason::VirtualReasonDesc::RSymbol
    }

    pub fn make(reason: Reason) -> Type {
        Type::new(TypeInner::DefT(reason, DefT::new(DefTInner::SymbolT)))
    }

    pub fn at(tok: ALoc) -> Type {
        let reason = flow_common::reason::mk_annot_reason(desc(), tok);
        make(reason)
    }

    pub fn why(reason: Reason) -> Type {
        let reason = reason.replace_desc(desc());
        make(reason)
    }

    pub fn why_with_use_desc(use_desc: bool, r: Reason) -> Type {
        let r = if use_desc { r } else { r.replace_desc(desc()) };
        make(r)
    }
}

pub mod unique_symbol_t {
    use super::*;

    pub fn at(id: ALocId, loc: ALoc, name: Option<FlowSmolStr>) -> Type {
        make(id, loc, name, true)
    }

    pub fn inferred_at(id: ALocId, loc: ALoc, name: Option<FlowSmolStr>) -> Type {
        make(id, loc, name, false)
    }

    fn make(id: ALocId, loc: ALoc, name: Option<FlowSmolStr>, from_annot: bool) -> Type {
        Type::new(TypeInner::DefT(
            flow_common::reason::mk_reason(
                flow_common::reason::VirtualReasonDesc::RUniqueSymbol,
                loc,
            ),
            DefT::new(DefTInner::UniqueSymbolT(UniqueSymbolTData {
                symbol: SymbolID::with_name(id, name),
                from_annot,
            })),
        ))
    }
}

pub mod mixed_t {
    use super::*;

    pub fn desc() -> flow_common::reason::ReasonDesc {
        flow_common::reason::VirtualReasonDesc::RMixed
    }

    pub fn make(reason: Reason) -> Type {
        Type::new(TypeInner::DefT(
            reason,
            DefT::new(DefTInner::MixedT(MixedFlavor::MixedEverything)),
        ))
    }

    pub fn at(tok: ALoc) -> Type {
        let reason = flow_common::reason::mk_annot_reason(desc(), tok);
        make(reason)
    }

    pub fn why(reason: Reason) -> Type {
        let reason = reason.replace_desc(desc());
        make(reason)
    }

    pub fn why_with_use_desc(use_desc: bool, r: Reason) -> Type {
        let r = if use_desc { r } else { r.replace_desc(desc()) };
        make(r)
    }
}

pub mod empty_t {
    use super::*;

    pub fn make(reason: Reason) -> Type {
        Type::new(TypeInner::DefT(reason, DefT::new(DefTInner::EmptyT)))
    }

    pub fn at(tok: ALoc) -> Type {
        let reason = flow_common::reason::mk_annot_reason(None, tok);
        make(reason)
    }

    pub fn why(reason: Reason) -> Type {
        let reason = reason.replace_desc(None);
        make(reason)
    }

    pub fn why_with_use_desc(use_desc: bool, r: Reason) -> Type {
        let r = if use_desc { r } else { r.replace_desc(None) };
        make(r)
    }
}

pub mod any_t {
    use super::*;

    pub fn make(source: AnySource, r: Reason) -> Type {
        Type::new(TypeInner::AnyT(r, source))
    }

    pub fn at(source: AnySource, tok: ALoc) -> Type {
        let reason = flow_common::reason::mk_annot_reason(None, tok);
        make(source, reason)
    }

    pub fn why(source: AnySource, reason: Reason) -> Type {
        let reason = reason.replace_desc(None);
        make(source, reason)
    }

    pub fn annot(reason: Reason) -> Type {
        why(AnySource::AnnotatedAny, reason)
    }

    pub fn error(reason: Reason) -> Type {
        why(AnySource::AnyError(None), reason)
    }

    pub fn error_of_kind(kind: AnyErrorKind, reason: Reason) -> Type {
        why(AnySource::AnyError(Some(kind)), reason)
    }

    pub fn untyped(reason: Reason) -> Type {
        why(AnySource::Untyped, reason)
    }

    pub fn placeholder(reason: Reason) -> Type {
        why(AnySource::Placeholder, reason)
    }

    pub fn locationless(source: AnySource) -> Type {
        let reason = flow_common::reason::locationless_reason(None);
        make(source, reason)
    }
}

pub mod unsoundness {
    use super::*;

    pub fn constructor() -> AnySource {
        AnySource::Unsound(UnsoundnessKind::Constructor)
    }

    pub fn merged() -> AnySource {
        AnySource::Unsound(UnsoundnessKind::Merged)
    }

    pub fn instance_of_refi() -> AnySource {
        AnySource::Unsound(UnsoundnessKind::InstanceOfRefinement)
    }

    pub fn unresolved() -> AnySource {
        AnySource::Unsound(UnsoundnessKind::UnresolvedType)
    }

    pub fn resolve_spread() -> AnySource {
        AnySource::Unsound(UnsoundnessKind::ResolveSpread)
    }

    pub fn unimplemented() -> AnySource {
        AnySource::Unsound(UnsoundnessKind::Unimplemented)
    }

    pub fn inference_hooks() -> AnySource {
        AnySource::Unsound(UnsoundnessKind::InferenceHooks)
    }

    pub fn exports() -> AnySource {
        AnySource::Unsound(UnsoundnessKind::Exports)
    }

    pub fn bound_fn_this() -> AnySource {
        AnySource::Unsound(UnsoundnessKind::BoundFunctionThis)
    }

    pub fn dummy_static() -> AnySource {
        AnySource::Unsound(UnsoundnessKind::DummyStatic)
    }

    // Convenience functions that create AnyT with unsound sources
    pub fn merged_any(r: Reason) -> Type {
        any_t::make(merged(), r)
    }

    pub fn instance_of_refi_any(r: Reason) -> Type {
        any_t::make(instance_of_refi(), r)
    }

    pub fn unresolved_any(r: Reason) -> Type {
        any_t::make(unresolved(), r)
    }

    pub fn resolve_spread_any(r: Reason) -> Type {
        any_t::make(resolve_spread(), r)
    }

    pub fn constructor_any(r: Reason) -> Type {
        any_t::make(constructor(), r)
    }

    pub fn unimplemented_any(r: Reason) -> Type {
        any_t::make(unimplemented(), r)
    }

    pub fn inference_hooks_any(r: Reason) -> Type {
        any_t::make(inference_hooks(), r)
    }

    pub fn exports_any(r: Reason) -> Type {
        any_t::make(exports(), r)
    }

    pub fn bound_fn_this_any(r: Reason) -> Type {
        any_t::make(bound_fn_this(), r)
    }

    pub fn dummy_static_any(r: Reason) -> Type {
        any_t::make(dummy_static(), r)
    }

    pub fn why(kind: UnsoundnessKind, reason: Reason) -> Type {
        any_t::why(AnySource::Unsound(kind), reason)
    }

    pub fn at(kind: UnsoundnessKind, tok: ALoc) -> Type {
        any_t::at(AnySource::Unsound(kind), tok)
    }
}

pub mod void {
    use super::*;

    pub fn desc() -> flow_common::reason::ReasonDesc {
        flow_common::reason::VirtualReasonDesc::RVoid
    }

    pub fn make(reason: Reason) -> Type {
        Type::new(TypeInner::DefT(reason, DefT::new(DefTInner::VoidT)))
    }

    pub fn at(tok: ALoc) -> Type {
        let reason = flow_common::reason::mk_annot_reason(desc(), tok);
        make(reason)
    }

    pub fn why(reason: Reason) -> Type {
        let reason = reason.replace_desc(desc());
        make(reason)
    }

    pub fn why_with_use_desc(use_desc: bool, r: Reason) -> Type {
        let r = if use_desc { r } else { r.replace_desc(desc()) };
        make(r)
    }
}

pub mod null {
    use super::*;

    pub fn desc() -> flow_common::reason::ReasonDesc {
        flow_common::reason::VirtualReasonDesc::RNull
    }

    pub fn make(reason: Reason) -> Type {
        Type::new(TypeInner::DefT(reason, DefT::new(DefTInner::NullT)))
    }

    pub fn at(tok: ALoc) -> Type {
        let reason = flow_common::reason::mk_annot_reason(desc(), tok);
        make(reason)
    }

    pub fn why(reason: Reason) -> Type {
        let reason = reason.replace_desc(desc());
        make(reason)
    }

    pub fn why_with_use_desc(use_desc: bool, r: Reason) -> Type {
        let r = if use_desc { r } else { r.replace_desc(desc()) };
        make(r)
    }
}

pub mod obj_proto {
    use super::*;

    pub fn make(reason: Reason) -> Type {
        Type::new(TypeInner::ObjProtoT(reason))
    }

    pub fn at(tok: ALoc) -> Type {
        let reason = flow_common::reason::mk_annot_reason(None, tok);
        make(reason)
    }

    pub fn why(reason: Reason) -> Type {
        let reason = reason.replace_desc(None);
        make(reason)
    }

    pub fn why_with_use_desc(use_desc: bool, r: Reason) -> Type {
        let r = if use_desc { r } else { r.replace_desc(None) };
        make(r)
    }
}

pub mod null_proto {
    use super::*;

    pub fn desc() -> flow_common::reason::ReasonDesc {
        flow_common::reason::VirtualReasonDesc::RNull
    }

    pub fn make(reason: Reason) -> Type {
        Type::new(TypeInner::NullProtoT(reason))
    }

    pub fn at(tok: ALoc) -> Type {
        let reason = flow_common::reason::mk_annot_reason(desc(), tok);
        make(reason)
    }

    pub fn why(reason: Reason) -> Type {
        let reason = reason.replace_desc(desc());
        make(reason)
    }

    pub fn why_with_use_desc(use_desc: bool, r: Reason) -> Type {
        let r = if use_desc { r } else { r.replace_desc(desc()) };
        make(r)
    }
}

/// USE WITH CAUTION!!! Locationless types should not leak to errors, otherwise
/// they will cause error printing to crash.
///
/// We use locationless reasons legitimately for normalizing. Also, because `any`
/// doesn't cause errors, locationless `AnyT` is OK.
pub mod locationless {
    use super::*;

    pub mod num_module_t {
        use super::*;

        pub fn t() -> Type {
            let reason =
                flow_common::reason::locationless_reason(super::super::num_module_t::desc());
            super::super::num_module_t::make(reason)
        }
    }

    pub mod str_module_t {
        use super::*;

        pub fn t() -> Type {
            let reason =
                flow_common::reason::locationless_reason(super::super::str_module_t::desc());
            super::super::str_module_t::make(reason)
        }
    }

    pub mod bool_module_t {
        use super::*;

        pub fn t() -> Type {
            let reason =
                flow_common::reason::locationless_reason(super::super::bool_module_t::desc());
            super::super::bool_module_t::make(reason)
        }
    }

    pub mod mixed_t {
        use super::*;

        pub fn t() -> Type {
            let reason = flow_common::reason::locationless_reason(super::super::mixed_t::desc());
            super::super::mixed_t::make(reason)
        }
    }

    pub mod empty_t {
        use super::*;

        pub fn t() -> Type {
            let reason = flow_common::reason::locationless_reason(None);
            super::super::empty_t::make(reason)
        }
    }

    pub mod void_t {
        use super::*;

        pub fn t() -> Type {
            let reason = flow_common::reason::locationless_reason(super::super::void::desc());
            super::super::void::make(reason)
        }
    }

    pub mod null_t {
        use super::*;

        pub fn t() -> Type {
            let reason = flow_common::reason::locationless_reason(super::super::null::desc());
            super::super::null::make(reason)
        }
    }
}

pub fn hint_unavailable<CX: ContextFamily>() -> LazyHintT<CX> {
    LazyHintT(
        false,
        Rc::new(
            |_cx: &mut CX::Context<'_>, _expected_only, _skip_optional, _reason| {
                Ok(HintEvalResult::NoHint)
            },
        ),
    )
}

/* convenience */

pub fn drop_generic(t: Type) -> Type {
    match &*t {
        TypeInner::GenericT(box GenericTData { bound, .. }) => bound.dupe(),
        _ => t,
    }
}

// Primitives, like string, will be promoted to their wrapper object types for
// certain operations, like GetPropT, but not for others, like `UseT _`.
pub fn primitive_promoting_use_t<CX: ContextFamily>(use_t: &UseT<CX>) -> bool {
    match use_t.deref() {
        UseTInner::CallElemT(..)
        | UseTInner::GetElemT(..)
        | UseTInner::GetPropT(..)
        | UseTInner::GetPrivatePropT(..)
        | UseTInner::GetProtoT(..)
        | UseTInner::MethodT(..)
        | UseTInner::TestPropT(..) => true,
        // "internal" use types, which should not be called directly on primitives,
        // but it's OK if they are in practice. TODO: consider making this an internal
        // error
        UseTInner::LookupT(..) => true,
        // TODO: enumerate all use types
        _ => false,
    }
}

pub fn fold_virtual_use_op<L: Dupe + PartialEq + Eq + PartialOrd + Ord, F1, F2, T>(
    op: &VirtualUseOp<L>,
    f1: F1,
    f2: &F2,
) -> T
where
    F1: Fn(&VirtualRootUseOp<L>) -> T,
    F2: Fn(T, &VirtualFrameUseOp<L>) -> T,
{
    match op {
        VirtualUseOp::Op(root) => f1(root),
        VirtualUseOp::Frame(frame, inner_op) => {
            let acc = fold_virtual_use_op(inner_op, f1, f2);
            f2(acc, frame)
        }
    }
}

pub fn fold_use_op<F1, F2, T>(op: &UseOp, f1: F1, f2: &F2) -> T
where
    F1: Fn(&RootUseOp) -> T,
    F2: Fn(T, &FrameUseOp) -> T,
{
    fold_virtual_use_op(op, f1, f2)
}

pub fn root_of_use_op<L: Dupe + PartialEq + Eq + PartialOrd + Ord>(
    op: &VirtualUseOp<L>,
) -> &VirtualRootUseOp<L> {
    match op {
        VirtualUseOp::Op(root) => root,
        VirtualUseOp::Frame(_, inner_op) => root_of_use_op(inner_op),
    }
}

pub fn implicit_instantiation_reference_kind<L: Dupe + PartialEq + Eq + PartialOrd + Ord>(
    op: &VirtualUseOp<L>,
) -> Option<&ImplicitInstantiationReferenceKind> {
    match root_of_use_op(op) {
        VirtualRootUseOp::FunCall(data) => Some(&data.implicit_instantiation_reference),
        VirtualRootUseOp::FunCallMethod(data) => Some(&data.implicit_instantiation_reference),
        VirtualRootUseOp::JSXCreateElement(box JSXCreateElementData {
            implicit_instantiation_reference,
            ..
        }) => Some(implicit_instantiation_reference),
        VirtualRootUseOp::ReactCreateElementCall(data) => {
            Some(&data.implicit_instantiation_reference)
        }
        VirtualRootUseOp::Speculation(inner) => implicit_instantiation_reference_kind(inner),
        _ => None,
    }
}

/* Printing some types in parseable form relies on particular formats in
corresponding reason descriptions. The following module formalizes the
relevant conventions.

TODO: Encoding formats in strings instead of ADTs is not ideal, obviously. */
pub mod desc_format {
    use super::*;

    // InstanceT reasons have desc = name
    pub fn instance_reason(name: Name, loc: ALoc) -> Reason {
        flow_common::reason::mk_reason(
            flow_common::reason::VirtualReasonDesc::RType(
                name.into_smol_str_opt().expect("instance name is a string"),
            ),
            loc,
        )
    }

    pub fn name_of_instance_reason(r: &Reason) -> String {
        match r.desc(true) {
            Some(VirtualReasonDesc::RType(name)) => name.to_string(),
            Some(desc) => flow_common::reason::string_of_desc(desc),
            None => String::new(),
        }
    }

    // TypeT reasons have desc = type `name`
    pub fn type_reason(name: Name, loc: ALoc) -> Reason {
        flow_common::reason::mk_reason(
            flow_common::reason::VirtualReasonDesc::RType(
                name.into_smol_str_opt().expect("type name is a string"),
            ),
            loc,
        )
    }

    pub fn name_of_type_reason(r: &Reason) -> Name {
        match r.desc(true) {
            Some(VirtualReasonDesc::RType(name)) => Name::new(name.dupe()),
            _ => panic!("not a type reason"),
        }
    }
}

/* printing */

pub fn string_of_def_ctor(def: &DefT) -> &'static str {
    match &**def {
        DefTInner::ArrT(_) => "ArrT",
        DefTInner::BigIntGeneralT(_) => "BigIntT",
        DefTInner::BoolGeneralT => "BoolT",
        DefTInner::ClassT(_) => "ClassT",
        DefTInner::EmptyT => "EmptyT",
        DefTInner::EnumValueT(_) => "EnumValueT",
        DefTInner::EnumObjectT { .. } => "EnumObjectT",
        DefTInner::FunT(_, _) => "FunT",
        DefTInner::InstanceT(_) => "InstanceT",
        DefTInner::MixedT(_) => "MixedT",
        DefTInner::NullT => "NullT",
        DefTInner::NumGeneralT(_) => "NumT",
        DefTInner::ObjT(_) => "ObjT",
        DefTInner::PolyT(_) => "PolyT",
        DefTInner::ReactAbstractComponentT(_) => "ReactAbstractComponentT",
        DefTInner::RendersT(_) => "RendersT",
        DefTInner::NumericStrKeyT(_) => "NumericStrKeyT",
        DefTInner::SingletonBoolT { .. } => "SingletonBoolT",
        DefTInner::SingletonNumT { .. } => "SingletonNumT",
        DefTInner::SingletonStrT { .. } => "SingletonStrT",
        DefTInner::SingletonBigIntT { .. } => "SingletonBigIntT",
        DefTInner::StrGeneralT { .. } => "StrT",
        DefTInner::SymbolT => "SymbolT",
        DefTInner::UniqueSymbolT(_) => "UniqueSymbolT",
        DefTInner::TypeT { .. } => "TypeT",
        DefTInner::VoidT => "VoidT",
    }
}

pub fn string_of_ctor(t: &Type) -> &'static str {
    match &**t {
        TypeInner::OpenT(_) => "OpenT",
        TypeInner::AnyT(_, AnySource::CatchAny) => "AnyT (catch)",
        TypeInner::AnyT(_, AnySource::AnnotatedAny) => "AnyT (annotated)",
        TypeInner::AnyT(_, AnySource::AnyError(_)) => "AnyT (error)",
        TypeInner::AnyT(_, AnySource::Unsound(_)) => "AnyT (unsound)",
        TypeInner::AnyT(_, AnySource::Untyped) => "AnyT (untyped)",
        TypeInner::AnyT(_, AnySource::Placeholder) => "AnyT (placeholder)",
        TypeInner::AnnotT(_, _, _) => "AnnotT",
        TypeInner::DefT(_, def) => string_of_def_ctor(def),
        TypeInner::EvalT { .. } => "EvalT",
        TypeInner::FunProtoT(_) => "FunProtoT",
        TypeInner::GenericT(..) => "GenericT",
        TypeInner::ImplicitInstantiationTvar(..) => "ImplicitInstantiationTvar",
        TypeInner::KeysT(_, _) => "KeysT",
        TypeInner::TemplateLiteralT { .. } => "TemplateLiteralT",
        TypeInner::StringMappingT { .. } => "StringMappingT",
        TypeInner::NamespaceT(_) => "NamespaceT",
        TypeInner::NullProtoT(_) => "NullProtoT",
        TypeInner::ObjProtoT(_) => "ObjProtoT",
        TypeInner::NominalT { .. } => "NominalT",
        TypeInner::ThisInstanceT(..) => "ThisInstanceT",
        TypeInner::ThisTypeAppT(..) => "ThisTypeAppT",
        TypeInner::TypeAppT(..) => "TypeAppT",
        TypeInner::UnionT(_, _) => "UnionT",
        TypeInner::IntersectionT(_, _) => "IntersectionT",
        TypeInner::OptionalT { .. } => "OptionalT",
        TypeInner::MaybeT(_, _) => "MaybeT",
    }
}

pub fn string_of_root_use_op<L: Dupe + PartialEq + Eq + PartialOrd + Ord>(
    op: &VirtualRootUseOp<L>,
) -> &'static str {
    match op {
        VirtualRootUseOp::InitField { .. } => "InitField",
        VirtualRootUseOp::ObjectAddComputedProperty { .. } => "ObjectAddComputedProperty",
        VirtualRootUseOp::ObjectSpread { .. } => "ObjectSpread",
        VirtualRootUseOp::ObjectRest { .. } => "ObjectRest",
        VirtualRootUseOp::ObjectChain { .. } => "ObjectChain",
        VirtualRootUseOp::AssignVar { .. } => "AssignVar",
        VirtualRootUseOp::Cast { .. } => "Cast",
        VirtualRootUseOp::ClassExtendsCheck(..) => "ClassExtendsCheck",
        VirtualRootUseOp::ClassImplementsCheck(..) => "ClassImplementsCheck",
        VirtualRootUseOp::ClassOwnProtoCheck(..) => "ClassOwnProtoCheck",
        VirtualRootUseOp::ClassMethodDefinition { .. } => "ClassMethodDefinition",
        VirtualRootUseOp::Coercion { .. } => "Coercion",
        VirtualRootUseOp::ConformToCommonInterface(..) => "ConformToCommonInterface",
        VirtualRootUseOp::MergedDeclaration(..) => "MergedDeclaration",
        VirtualRootUseOp::DeclareComponentRef { .. } => "DeclareComponentRef",
        VirtualRootUseOp::DeleteProperty { .. } => "DeleteProperty",
        VirtualRootUseOp::DeleteVar { .. } => "DeleteVar",
        VirtualRootUseOp::FunCall(..) => "FunCall",
        VirtualRootUseOp::FunCallMethod(..) => "FunCallMethod",
        VirtualRootUseOp::FunImplicitReturn(..) => "FunImplicitReturn",
        VirtualRootUseOp::FunReturnStatement { .. } => "FunReturnStatement",
        VirtualRootUseOp::GeneratorYield { .. } => "GeneratorYield",
        VirtualRootUseOp::GetExport { .. } => "GetExport",
        VirtualRootUseOp::GetProperty { .. } => "GetProperty",
        VirtualRootUseOp::IndexedTypeAccess(..) => "IndexedTypeAccess",
        VirtualRootUseOp::InferBoundCompatibilityCheck { .. } => "InferBoundCompatibilityCheck",
        VirtualRootUseOp::JSXCreateElement(..) => "JSXCreateElement",
        VirtualRootUseOp::ReactCreateElementCall(..) => "ReactCreateElementCall",
        VirtualRootUseOp::ReactGetIntrinsic { .. } => "ReactGetIntrinsic",
        VirtualRootUseOp::RecordCreate(..) => "RecordCreate",
        VirtualRootUseOp::Speculation { .. } => "Speculation",
        VirtualRootUseOp::TypeApplication { .. } => "TypeApplication",
        VirtualRootUseOp::MakeReadOnly { .. } => "MakeReadOnly",
        VirtualRootUseOp::SetProperty(..) => "SetProperty",
        VirtualRootUseOp::UpdateProperty { .. } => "UpdateProperty",
        VirtualRootUseOp::RefinementCheck { .. } => "RefinementCheck",
        VirtualRootUseOp::SwitchRefinementCheck(..) => "SwitchRefinementCheck",
        VirtualRootUseOp::EvalMappedType(..) => "EvalMappedType",
        VirtualRootUseOp::TypeGuardIncompatibility(..) => "TypeGuardIncompatibility",
        VirtualRootUseOp::RenderTypeInstantiation { .. } => "RenderTypeInstantiation",
        VirtualRootUseOp::ComponentRestParamCompatibility { .. } => {
            "ComponentRestParamCompatibility"
        }
        VirtualRootUseOp::PositiveTypeGuardConsistency(..) => "PositiveTypeGuardConsistency",
        VirtualRootUseOp::AssertionFunctionConsistency(..) => "AssertionFunctionConsistency",
        VirtualRootUseOp::UnknownUse => "UnknownUse",
    }
}

pub fn string_of_frame_use_op<L: Dupe + PartialEq + Eq + PartialOrd + Ord>(
    op: &VirtualFrameUseOp<L>,
) -> &'static str {
    match op {
        VirtualFrameUseOp::ConstrainedAssignment(..) => "ConstrainedAssignment",
        VirtualFrameUseOp::ReactDeepReadOnly { .. } => "ReactDeepReadOnly",
        VirtualFrameUseOp::ArrayElementCompatibility(..) => "ArrayElementCompatibility",
        VirtualFrameUseOp::FunCompatibility { .. } => "FunCompatibility",
        VirtualFrameUseOp::StandaloneCallThis(..) => "StandaloneCallThis",
        VirtualFrameUseOp::FunMissingArg(..) => "FunMissingArg",
        VirtualFrameUseOp::FunParam(..) => "FunParam",
        VirtualFrameUseOp::FunRestParam { .. } => "FunRestParam",
        VirtualFrameUseOp::FunReturn(..) => "FunReturn",
        VirtualFrameUseOp::ImplicitTypeParam => "ImplicitTypeParam",
        VirtualFrameUseOp::IndexerKeyCompatibility(..) => "IndexerKeyCompatibility",
        VirtualFrameUseOp::OpaqueTypeLowerBoundCompatibility { .. } => {
            "OpaqueTypeLowerBoundCompatibility"
        }
        VirtualFrameUseOp::OpaqueTypeUpperBoundCompatibility { .. } => {
            "OpaqueTypeUpperBoundCompatibility"
        }
        VirtualFrameUseOp::OpaqueTypeCustomErrorCompatibility(..) => {
            "OpaqueTypeCustomErrorCompatibility"
        }
        VirtualFrameUseOp::MappedTypeKeyCompatibility(..) => "MappedTypeKeyCompatibility",
        VirtualFrameUseOp::PropertyCompatibility(..) => "PropertyCompatibility",
        VirtualFrameUseOp::ReactConfigCheck => "ReactConfigCheck",
        VirtualFrameUseOp::ReactGetConfig { .. } => "ReactGetConfig",
        VirtualFrameUseOp::TupleElementCompatibility(..) => "TupleElementCompatibility",
        VirtualFrameUseOp::TupleAssignment => "TupleAssignment",
        VirtualFrameUseOp::TypeArgCompatibility(..) => "TypeArgCompatibility",
        VirtualFrameUseOp::TypeParamBound { .. } => "TypeParamBound",
        VirtualFrameUseOp::OpaqueTypeLowerBound { .. } => "OpaqueTypeLowerBound",
        VirtualFrameUseOp::OpaqueTypeUpperBound { .. } => "OpaqueTypeUpperBound",
        VirtualFrameUseOp::UnifyFlip => "UnifyFlip",
        VirtualFrameUseOp::TypeGuardCompatibility => "TypeGuardCompatibility",
        VirtualFrameUseOp::RendersCompatibility => "RendersCompatibility",
        VirtualFrameUseOp::EnumRepresentationTypeCompatibility { .. } => {
            "EnumRepresentationTypeCompatibility"
        }
        VirtualFrameUseOp::UnionRepresentative(..) => "UnionRepresentative",
    }
}

pub fn string_of_use_op<L: Dupe + PartialEq + Eq + PartialOrd + Ord>(
    op: &VirtualUseOp<L>,
) -> String {
    match op {
        VirtualUseOp::Op(root) => string_of_root_use_op(root).to_string(),
        VirtualUseOp::Frame(frame, _) => string_of_frame_use_op(frame).to_string(),
    }
}

pub fn string_of_use_op_rec(op: &UseOp) -> String {
    fold_use_op(
        op,
        |root| string_of_root_use_op(root).to_string(),
        &|acc, frame| format!("{}({})", string_of_frame_use_op(frame), acc),
    )
}

pub fn string_of_predicate_concretizer_variant(
    variant: &PredicateConcretetizerVariant,
) -> &'static str {
    match variant {
        PredicateConcretetizerVariant::ConcretizeForGeneralPredicateTest => {
            "ConcretizeForGeneralPredicateTest"
        }
        PredicateConcretetizerVariant::ConcretizeKeepOptimizedUnions => {
            "ConcretizeKeepOptimizedUnions"
        }
        PredicateConcretetizerVariant::ConcretizeRHSForInstanceOfPredicateTest => {
            "ConcretizeRHSForInstanceOfPredicateTest"
        }
        PredicateConcretetizerVariant::ConcretizeRHSForLiteralPredicateTest => {
            "ConcretizeRHSForLiteralPredicateTest"
        }
    }
}

pub fn string_of_use_ctor<CX: ContextFamily>(use_t: &UseT<CX>) -> String {
    match use_t.deref() {
        UseTInner::UseT(op, t) => format!("UseT({}, {})", string_of_use_op(op), string_of_ctor(t)),
        UseTInner::ArrRestT(..) => "ArrRestT".to_string(),
        UseTInner::CallElemT(..) => "CallElemT".to_string(),
        UseTInner::CallT(..) => "CallT".to_string(),
        UseTInner::ConstructorT(..) => "ConstructorT".to_string(),
        UseTInner::ElemT(..) => "ElemT".to_string(),
        UseTInner::GetEnumT(..) => "GetEnumT".to_string(),
        UseTInner::ConditionalT(..) => "ConditionalT".to_string(),
        UseTInner::ExtendsUseT(..) => "ExtendsUseT".to_string(),
        UseTInner::GetElemT(..) => "GetElemT".to_string(),
        UseTInner::GetKeysT { .. } => "GetKeysT".to_string(),
        UseTInner::GetKeysDictKeyT { .. } => "GetKeysDictKeyT".to_string(),
        UseTInner::GetValuesT(..) => "GetValuesT".to_string(),
        UseTInner::GetDictValuesT(..) => "GetDictValuesT".to_string(),
        UseTInner::GetPropT(..) => "GetPropT".to_string(),
        UseTInner::GetPrivatePropT(..) => "GetPrivatePropT".to_string(),
        UseTInner::GetProtoT(..) => "GetProtoT".to_string(),
        UseTInner::GetStaticsT(..) => "GetStaticsT".to_string(),
        UseTInner::HasOwnPropT(..) => "HasOwnPropT".to_string(),
        UseTInner::ImplementsT(..) => "ImplementsT".to_string(),
        UseTInner::ConcretizeT(box ConcretizeTData { kind, .. }) => match kind {
            ConcretizationKind::ConcretizeForLowerBounds => {
                "ConcretizeT ConcretizeForLowerBounds".to_string()
            }
            ConcretizationKind::ConcretizeForImportsExports => {
                "ConcretizeT ConcretizeForImportsExports".to_string()
            }
            ConcretizationKind::ConcretizeForCJSExtractNamedExportsAndTypeExports => {
                "ConcretizeT ConcretizeForCJSExtractNamedExportsAndTypeExports".to_string()
            }
            ConcretizationKind::ConcretizeForOptionalChain => {
                "ConcretizeT ConcretizeForOptionalChain".to_string()
            }
            ConcretizationKind::ConcretizeForInspection => {
                "ConcretizeT ConcretizeForInspection".to_string()
            }
            ConcretizationKind::ConcretizeForClassExtends(_) => {
                "ConcretizeT ConcretizeForClassExtends".to_string()
            }
            ConcretizationKind::ConcretizeForEnumExhaustiveCheck => {
                "ConcretizeT ConcretizeForEnumExhaustiveCheck".to_string()
            }
            ConcretizationKind::ConcretizeForPredicate(v) => {
                format!(
                    "ConcretizeT ConcretizeForPredicate({})",
                    string_of_predicate_concretizer_variant(v)
                )
            }
            ConcretizationKind::ConcretizeForSentinelPropTest => {
                "ConcretizeT ConcretizeForPredicate".to_string()
            }
            ConcretizationKind::ConcretizeForUnusedPromiseCheck => {
                "ConcretizeT ConcretizeForUnusedPromiseCheck".to_string()
            }
            ConcretizationKind::ConcretizeForConstructorReturn => {
                "ConcretizeT ConcretizeForConstructorReturn".to_string()
            }
            ConcretizationKind::ConcretizeAll => "ConcretizeT ConcretizeAll".to_string(),
            ConcretizationKind::ConcretizeForOperatorsChecking => {
                "ConcretizeT ConcretizeForOperatorsChecking".to_string()
            }
            ConcretizationKind::ConcretizeForComputedObjectKeys => {
                "ConcretizeT ConcretizeForComputedObjectKeys".to_string()
            }
            ConcretizationKind::ConcretizeForObjectAssign => {
                "ConcretizeT ConcretizeForObjectAssign".to_string()
            }
            ConcretizationKind::ConcretizeForDestructuring => {
                "ConcretizeT ConcretizeForDestructuring".to_string()
            }
            ConcretizationKind::ConcretizeForMatchArg { keep_unions } => {
                format!(
                    "ConcretizeT ConcretizeForMatchArg {{keep_unions={}}}",
                    keep_unions
                )
            }
        },
        UseTInner::LookupT(..) => "LookupT".to_string(),
        UseTInner::MapTypeT(..) => "MapTypeT".to_string(),
        UseTInner::MethodT(..) => "MethodT".to_string(),
        UseTInner::PrivateMethodT(..) => "PrivateMethodT".to_string(),
        UseTInner::MixinT(..) => "MixinT".to_string(),
        UseTInner::ObjRestT(..) => "ObjRestT".to_string(),
        UseTInner::ObjTestProtoT(..) => "ObjTestProtoT".to_string(),
        UseTInner::ReactKitT(..) => "ReactKitT".to_string(),
        UseTInner::ReposLowerT { .. } => "ReposLowerT".to_string(),
        UseTInner::ReposUseT(..) => "ReposUseT".to_string(),
        UseTInner::ResolveSpreadT(box ResolveSpreadTData {
            resolve_spread_type,
            ..
        }) => match &resolve_spread_type.rrt_resolve_to {
            SpreadResolve::ResolveSpreadsToArray { .. } => {
                "ResolveSpreadT(ResolveSpreadsToArray)".to_string()
            }
            SpreadResolve::ResolveSpreadsToTupleType { id, .. } => {
                format!("ResolveSpreadT(ResolveSpreadsToTupleType ({}))", id)
            }
            SpreadResolve::ResolveSpreadsToArrayLiteral { id, .. } => {
                format!("ResolveSpreadT(ResolveSpreadsToArrayLiteral ({}))", id)
            }
            SpreadResolve::ResolveSpreadsToMultiflowCallFull(..) => {
                "ResolveSpreadT(ResolveSpreadsToMultiflowCallFull)".to_string()
            }
            SpreadResolve::ResolveSpreadsToMultiflowSubtypeFull(..) => {
                "ResolveSpreadT(ResolveSpreadsToMultiflowSubtypeFull)".to_string()
            }
        },
        UseTInner::SetElemT(..) => "SetElemT".to_string(),
        UseTInner::SetPropT(..) => "SetPropT".to_string(),
        UseTInner::SetPrivatePropT(..) => "SetPrivatePropT".to_string(),
        UseTInner::SpecializeT(..) => "SpecializeT".to_string(),
        UseTInner::ObjKitT(..) => "ObjKitT".to_string(),
        UseTInner::SuperT(..) => "SuperT".to_string(),
        UseTInner::TestPropT(..) => "TestPropT".to_string(),
        UseTInner::ThisSpecializeT(..) => "ThisSpecializeT".to_string(),
        UseTInner::ToStringT { .. } => "ToStringT".to_string(),
        UseTInner::ValueToTypeReferenceT(..) => "ValueToTypeReferenceT".to_string(),
        UseTInner::ConcretizeTypeAppsT(..) => "ConcretizeTypeAppsT".to_string(),
        UseTInner::CondT(..) => "CondT".to_string(),
        UseTInner::ResolveUnionT(..) => "ResolveUnionT".to_string(),
        UseTInner::FilterOptionalT(..) => "FilterOptionalT".to_string(),
        UseTInner::FilterMaybeT(..) => "FilterMaybeT".to_string(),
        UseTInner::DeepReadOnlyT(..) => "DeepReadOnlyT".to_string(),
        UseTInner::HooklikeT(..) => "HooklikeT".to_string(),
        UseTInner::SealGenericT(..) => "SealGenericT".to_string(),
        UseTInner::OptionalIndexedAccessT(..) => "OptionalIndexedAccessT".to_string(),
        UseTInner::ConvertEmptyPropsToMixedT(..) => "ConvertEmptyPropsToMixedT".to_string(),
        UseTInner::ExitRendersT { .. } => "ExitRendersT".to_string(),
        UseTInner::EvalTypeDestructorT(..) => "EvalTypeDestructorT".to_string(),
    }
}

pub fn string_of_binary_test(test: &BinaryTest) -> String {
    match test {
        BinaryTest::InstanceofTest => "instanceof".to_string(),
        BinaryTest::SentinelProp(key) => format!("sentinel prop {}", key),
        BinaryTest::EqTest => "===".to_string(),
    }
}

pub fn string_of_predicate(pred: &Predicate) -> String {
    match &**pred {
        PredicateInner::AndP(p1, p2) => {
            format!("{} && {}", string_of_predicate(p1), string_of_predicate(p2))
        }
        PredicateInner::OrP(p1, p2) => {
            format!("{} || {}", string_of_predicate(p1), string_of_predicate(p2))
        }
        PredicateInner::NotP(p) => format!("not {}", string_of_predicate(p)),
        PredicateInner::BinaryP(b, t) => match &**t {
            TypeInner::OpenT(tvar) => {
                format!(
                    "left operand of {} with right operand = OpenT({})",
                    string_of_binary_test(b),
                    tvar.1
                )
            }
            _ => {
                format!(
                    "left operand of {} with right operand = {}",
                    string_of_binary_test(b),
                    string_of_ctor(t)
                )
            }
        },
        PredicateInner::TruthyP => "truthy".to_string(),
        PredicateInner::NullP => "null".to_string(),
        PredicateInner::MaybeP => "null or undefined".to_string(),
        PredicateInner::SingletonBoolP(box (_, false)) => "false".to_string(),
        PredicateInner::SingletonBoolP(box (_, true)) => "true".to_string(),
        PredicateInner::SingletonStrP(box (_, _, value)) => format!("string `{}`", value),
        PredicateInner::SingletonNumP(box (_, _, NumberLiteral(_, raw))) => {
            format!("number `{}`", raw)
        }
        PredicateInner::SingletonBigIntP(box (_, _, BigIntLiteral(_, raw))) => {
            format!("bigint `{}`", raw)
        }
        PredicateInner::VoidP => "undefined".to_string(),
        PredicateInner::BoolP { .. } => "boolean".to_string(),
        PredicateInner::StrP { .. } => "string".to_string(),
        PredicateInner::NumP { .. } => "number".to_string(),
        PredicateInner::BigIntP { .. } => "bigint".to_string(),
        PredicateInner::FunP => "function".to_string(),
        PredicateInner::ObjP => "object".to_string(),
        PredicateInner::SymbolP { .. } => "symbol".to_string(),
        PredicateInner::ArrP => "array".to_string(),
        PredicateInner::ArrLenP { op, n } => {
            let op_str = match op {
                ArrayLengthOp::ArrLenEqual => "===",
                ArrayLengthOp::ArrLenGreaterThanEqual => ">=",
            };
            format!("array length {} {}", op_str, n)
        }
        PredicateInner::PropExistsP { propname, .. } => format!("prop `{}` exists", propname),
        PredicateInner::PropTruthyP(key, _) => format!("prop `{}` is truthy", key),
        PredicateInner::PropIsExactlyNullP(key, _) => format!("prop `{}` is exactly null", key),
        PredicateInner::PropNonVoidP(key, _) => format!("prop `{}` is not undefined", key),
        PredicateInner::PropNonMaybeP(key, _) => format!("prop `{}` is not null or undefined", key),
        PredicateInner::LatentP(lazy_pred, indices) => {
            // Access lazy value (assuming it's already forced)
            match &*lazy_pred.2 {
                TypeInner::OpenT(tvar) => {
                    let indices: Vec<String> = indices.iter().map(|i| i.to_string()).collect();
                    format!("LatentPred(TYPE_{}, {})", tvar.1, indices.join(", "))
                }
                _ => {
                    let indices: Vec<String> = indices.iter().map(|i| i.to_string()).collect();
                    format!(
                        "LatentPred({}, {})",
                        string_of_ctor(&lazy_pred.2),
                        indices.join(", ")
                    )
                }
            }
        }
        PredicateInner::LatentThisP(lazy_pred) => match &*lazy_pred.2 {
            TypeInner::OpenT(tvar) => {
                format!("LatentThisPred(TYPE_{})", tvar.1)
            }
            _ => {
                format!("LatentThisPred({})", string_of_ctor(&lazy_pred.2))
            }
        },
        PredicateInner::ImpossibleP => "impossible".to_string(),
    }
}

pub fn string_of_type_t_kind(kind: &TypeTKind) -> &'static str {
    match kind {
        TypeTKind::TypeAliasKind => "TypeAliasKind",
        TypeTKind::TypeParamKind => "TypeParamKind",
        TypeTKind::OpaqueKind => "OpaqueKind",
        TypeTKind::ImportTypeofKind => "ImportTypeofKind",
        TypeTKind::ImportClassKind => "ImportClassKind",
        TypeTKind::ImportEnumKind => "ImportEnumKind",
        TypeTKind::InstanceKind => "InstanceKind",
        TypeTKind::RenderTypeKind => "RenderTypeKind",
    }
}

/// A setter's type is determined by its sole parameter.
///
/// If it has more than one param, returns the type of the first param;
/// if it has no params, returns `any`. If it isn't even a function,
/// raises an exception because this is a Flow bug.
pub fn extract_setter_type(t: &Type) -> Type {
    match &**t {
        TypeInner::DefT(reason, def_t) => match &**def_t {
            DefTInner::FunT(_, ft) if !ft.params.is_empty() => ft.params[0].1.dupe(),
            DefTInner::FunT(..) => any_t::error(reason.dupe()),
            _ => panic!("Setter property with unexpected type"),
        },
        _ => panic!("Setter property with unexpected type"),
    }
}

pub fn extract_getter_type(t: &Type) -> Type {
    match &**t {
        TypeInner::DefT(_, def_t) => match &**def_t {
            DefTInner::FunT(_, ft) => ft.return_t.dupe(),
            _ => panic!("Getter property with unexpected type"),
        },
        _ => panic!("Getter property with unexpected type"),
    }
}

pub fn elemt_of_arrtype(arrtype: &ArrType) -> Type {
    match arrtype {
        ArrType::ArrayAT(box ArrayATData { elem_t, .. })
        | ArrType::TupleAT(box TupleATData { elem_t, .. }) => elem_t.dupe(),
        ArrType::ROArrayAT(box ROArrayATData { elem_t, .. }) => elem_t.dupe(),
    }
}

pub fn ro_of_arrtype(arrtype: &ArrType) -> flow_typing_generics::array_spread::RoStatus {
    match arrtype {
        ArrType::ArrayAT(..) => flow_typing_generics::array_spread::RoStatus::NonROSpread,
        _ => flow_typing_generics::array_spread::RoStatus::ROSpread,
    }
}

pub fn annot(in_implicit_instantiation: bool, use_desc: bool, t: &Type) -> Type {
    match &**t {
        TypeInner::ImplicitInstantiationTvar(data) => {
            let reason = &data.reason;
            if !(flow_common::reason::is_instantiable_reason(reason) && in_implicit_instantiation) {
                Type(Rc::new(TypeInner::AnnotT(
                    reason.dupe(),
                    t.dupe(),
                    use_desc,
                )))
            } else {
                t.dupe()
            }
        }
        TypeInner::OpenT(tvar) => {
            if !(flow_common::reason::is_instantiable_reason(&tvar.0) && in_implicit_instantiation)
            {
                let reason = tvar.0.dupe();
                Type(Rc::new(TypeInner::AnnotT(reason, t.dupe(), use_desc)))
            } else {
                t.dupe()
            }
        }
        _ => t.dupe(),
    }
}

/* The following functions are used as constructors for function types and
object types, which unfortunately have many fields, not all of which are
meaningful in all contexts. This part of the design should be revisited:
perhaps the data types can be refactored to make them more specialized. */

/* Methods may use a dummy statics object type to carry properties. We do not
want to encourage this pattern, but we also don't want to block uses of this
pattern. Thus, we compromise by not tracking the property types. */
pub fn dummy_static(reason: Reason) -> Type {
    let r = reason.replace_desc(None);
    unsoundness::dummy_static_any(r)
}

pub fn dummy_prototype() -> Type {
    use flow_common::reason::locationless_reason;
    Type::new(TypeInner::ObjProtoT(locationless_reason(None)))
}

pub fn bound_function_dummy_this(loc: ALoc) -> Type {
    use flow_common::reason::mk_reason;
    let r = mk_reason(VirtualReasonDesc::RDummyThis, loc);
    unsoundness::bound_fn_this_any(r)
}

pub fn dummy_this(loc: ALoc) -> Type {
    use flow_common::reason::mk_reason;
    let r = mk_reason(VirtualReasonDesc::RDummyThis, loc);
    mixed_t::make(r)
}

pub fn global_this(reason: Reason) -> Type {
    let reason = reason.replace_desc(None);
    Type::new(TypeInner::ObjProtoT(reason))
}

pub fn default_obj_assign_kind() -> ObjAssignKind {
    ObjAssignKind::ObjAssign {
        assert_exact: false,
    }
}

/* A method type is a function type with `this` specified. */
pub fn mk_methodtype(
    this_t: Type,
    subtyping: Option<ThisStatus>,
    effect_: Option<ReactEffectType>,
    tins: Vec<Type>,
    rest_param: Option<FunRestParam>,
    def_reason: Reason,
    function_reference_kind: FunctionReferenceKind,
    params_names: Option<Vec<Option<Name>>>,
    type_guard: Option<TypeGuard>,
    tout: Type,
) -> FunType {
    let subtyping = subtyping.unwrap_or(ThisStatus::ThisFunction);
    let effect_ = effect_.unwrap_or(ReactEffectType::ArbitraryEffect);

    let params = match params_names {
        None => tins.into_iter().map(|t| FunParam(None, t)).collect(),
        Some(names) => names
            .into_iter()
            .zip(tins)
            .map(|(name_opt, t)| FunParam(name_opt.map(|n| n.to_string().into()), t))
            .collect(),
    };

    FunType {
        this_t: (this_t, subtyping),
        params,
        rest_param,
        return_t: tout,
        type_guard,
        def_reason,
        function_reference_kind,
        effect_,
        strictness_kind: TypeStrictnessKind::Flow,
        literal: false,
    }
}

pub fn mk_methodcalltype(
    targs: Option<Rc<[Targ]>>,
    args: Rc<[CallArg]>,
    meth_generic_this: Option<Type>,
    meth_strict_arity: bool,
    tout: Tvar,
) -> MethodCallType {
    MethodCallType {
        meth_generic_this,
        meth_targs: targs,
        meth_args_tlist: args,
        meth_tout: tout,
        meth_strict_arity,
    }
}

/// A bound function type is a method type whose `this` parameter has been
/// bound to some type. Currently, if the function's `this` parameter is not
/// explicitly annotated we model this unsoundly using `any`, but if it is
/// then we create a methodtype with a specific `this` type.
pub fn mk_boundfunctiontype(
    this: Type,
    effect_: Option<ReactEffectType>,
    tins: Vec<Type>,
    rest_param: Option<FunRestParam>,
    def_reason: Reason,
    function_reference_kind: FunctionReferenceKind,
    params_names: Option<Vec<Option<Name>>>,
    type_guard: Option<TypeGuard>,
    tout: Type,
) -> FunType {
    mk_methodtype(
        this,
        Some(ThisStatus::ThisMethod),
        effect_,
        tins,
        rest_param,
        def_reason,
        function_reference_kind,
        params_names,
        type_guard,
        tout,
    )
}

/// A function type is a method type whose `this` parameter has been
/// bound to to the global object. Currently, if the function's `this` parameter is not
/// explicitly annotated we model this using `mixed`, but if it is
/// then we create a methodtype with a specific `this` type.  
pub fn mk_functiontype(
    reason: Reason,
    this: Option<Type>,
    effect_: Option<ReactEffectType>,
    tins: Vec<Type>,
    rest_param: Option<FunRestParam>,
    def_reason: Reason,
    function_reference_kind: FunctionReferenceKind,
    params_names: Option<Vec<Option<Name>>>,
    type_guard: Option<TypeGuard>,
    tout: Type,
) -> FunType {
    let this = this.unwrap_or_else(|| global_this(reason));
    mk_methodtype(
        this,
        None,
        effect_,
        tins,
        rest_param,
        def_reason,
        function_reference_kind,
        params_names,
        type_guard,
        tout,
    )
}

pub fn mk_boundfunctioncalltype(
    this: Type,
    targs: Option<Rc<[Targ]>>,
    args: Rc<[CallArg]>,
    call_strict_arity: bool,
    tout: Tvar,
) -> FuncallType {
    FuncallType {
        call_this_t: this,
        call_targs: targs,
        call_args_tlist: args,
        call_tout: tout,
        call_strict_arity,
        call_speculation_hint_state: None,
        call_specialized_callee: None,
    }
}

pub fn mk_functioncalltype(
    reason: Reason,
    targs: Option<Rc<[Targ]>>,
    args: Rc<[CallArg]>,
    call_strict_arity: bool,
    tout: Tvar,
) -> FuncallType {
    mk_boundfunctioncalltype(global_this(reason), targs, args, call_strict_arity, tout)
}

pub fn mk_opt_functioncalltype(
    reason: Reason,
    targs: Option<Rc<[Targ]>>,
    args: Rc<[CallArg]>,
    strict: bool,
    instantiation_probe: Option<SpecializedCallee>,
) -> OptFuncallType {
    OptFuncallType(
        global_this(reason),
        targs,
        args,
        strict,
        instantiation_probe,
    )
}

pub fn mk_opt_boundfunctioncalltype(
    this: Type,
    targs: Option<Rc<[Targ]>>,
    args: Rc<[CallArg]>,
    strict: bool,
) -> (Type, Option<Rc<[Targ]>>, Rc<[CallArg]>, bool) {
    (this, targs, args, strict)
}

pub fn mk_opt_methodcalltype(
    opt_meth_generic_this: Option<Type>,
    opt_meth_targs: Option<Rc<[Targ]>>,
    opt_meth_args_tlist: Rc<[CallArg]>,
    opt_meth_strict_arity: bool,
) -> OptMethodCallType {
    OptMethodCallType {
        opt_meth_generic_this,
        opt_meth_targs,
        opt_meth_args_tlist,
        opt_meth_strict_arity,
    }
}

pub fn default_flags() -> Flags {
    Flags {
        obj_kind: ObjKind::Exact,
        react_dro: None,
    }
}

pub fn mk_objecttype(
    flags: Option<Flags>,
    reachable_targs: Option<Rc<[(Type, Polarity)]>>,
    call: Option<i32>,
    pmap: properties::Id,
    proto: Type,
    literal: bool,
) -> ObjType {
    mk_objecttype_with_strictness(
        flags,
        reachable_targs,
        call,
        pmap,
        proto,
        TypeStrictnessKind::Flow,
        literal,
    )
}

pub fn mk_objecttype_with_strictness(
    flags: Option<Flags>,
    reachable_targs: Option<Rc<[(Type, Polarity)]>>,
    call: Option<i32>,
    pmap: properties::Id,
    proto: Type,
    strictness_kind: TypeStrictnessKind,
    literal: bool,
) -> ObjType {
    ObjType {
        flags: flags.unwrap_or_else(default_flags),
        proto_t: proto,
        props_tmap: pmap,
        call_t: call,
        reachable_targs: reachable_targs.unwrap_or_else(|| Rc::from([])),
        strictness_kind,
        literal,
    }
}

pub fn mk_object_def_type(
    reason: Reason,
    flags: Option<Flags>,
    call: Option<i32>,
    pmap: properties::Id,
    proto: Type,
) -> TypeInner {
    let reason = reason.update_desc(|desc| desc.map(VirtualReasonDesc::invalidate_rtype_alias));
    TypeInner::DefT(
        reason,
        DefT::new(DefTInner::ObjT(Rc::new(mk_objecttype(
            flags, None, call, pmap, proto, false,
        )))),
    )
}

pub fn apply_opt_funcalltype(
    opt: (
        Type,
        Option<Rc<[Targ]>>,
        Rc<[CallArg]>,
        bool,
        Option<SpecializedCallee>,
    ),
    t_out: Tvar,
) -> CallAction {
    let (this, targs, args, strict, t_callee) = opt;
    CallAction::Funcalltype(Box::new(FuncallType {
        call_this_t: this,
        call_targs: targs,
        call_args_tlist: args,
        call_tout: t_out,
        call_strict_arity: strict,
        call_speculation_hint_state: None,
        call_specialized_callee: t_callee,
    }))
}

pub fn apply_opt_methodcalltype(opt: OptMethodCallType, meth_tout: Tvar) -> MethodCallType {
    MethodCallType {
        meth_generic_this: opt.opt_meth_generic_this,
        meth_targs: opt.opt_meth_targs,
        meth_args_tlist: opt.opt_meth_args_tlist,
        meth_tout,
        meth_strict_arity: opt.opt_meth_strict_arity,
    }
}

pub fn create_intersection(rep: inter_rep::InterRep) -> Type {
    use flow_common::reason::locationless_reason;
    Type::new(TypeInner::IntersectionT(locationless_reason(None), rep))
}

pub fn apply_opt_action<CX: ContextFamily>(
    action: OptMethodAction<CX>,
    t_out: Tvar,
) -> MethodAction<CX> {
    match action {
        OptMethodAction::OptCallM(box OptCallMData {
            opt_methodcalltype,
            return_hint,
            specialized_callee,
        }) => MethodAction::CallM(Box::new(CallMData {
            methodcalltype: apply_opt_methodcalltype(opt_methodcalltype, t_out),
            return_hint,
            specialized_callee,
        })),
        OptMethodAction::OptChainM(box OptChainMData {
            exp_reason,
            lhs_reason,
            lhs_expression,
            opt_methodcalltype,
            voided_out_collector,
            return_hint,
            specialized_callee,
        }) => MethodAction::ChainM(Box::new(ChainMData {
            exp_reason,
            lhs_reason,
            lhs_expression,
            methodcalltype: apply_opt_methodcalltype(opt_methodcalltype, t_out),
            voided_out_collector,
            return_hint,
            specialized_callee,
        })),
        OptMethodAction::OptNoMethodAction(t) => MethodAction::NoMethodAction(t),
    }
}

pub fn apply_opt_use<CX: ContextFamily>(opt_use: OptUseT<CX>, t_out: Tvar) -> UseT<CX> {
    match opt_use {
        OptUseT::OptMethodT(op, r1, r2, propref, action) => {
            UseT::new(UseTInner::MethodT(Box::new(MethodTData {
                use_op: op,
                reason: r1,
                prop_reason: r2,
                propref: Box::new(propref),
                method_action: Box::new(apply_opt_action(action, t_out)),
            })))
        }
        OptUseT::OptPrivateMethodT(op, r1, r2, p, scopes, is_static, action) => {
            UseT::new(UseTInner::PrivateMethodT(Box::new(PrivateMethodTData {
                use_op: op,
                reason: r1,
                prop_reason: r2,
                name: p,
                class_bindings: scopes,
                static_: is_static,
                method_action: Box::new(apply_opt_action(action, t_out)),
            })))
        }
        OptUseT::OptCallT {
            use_op,
            reason,
            opt_funcalltype,
            return_hint,
        } => UseT::new(UseTInner::CallT(Box::new(CallTData {
            use_op,
            reason,
            call_action: Box::new(apply_opt_funcalltype(
                (
                    opt_funcalltype.0,
                    opt_funcalltype.1,
                    opt_funcalltype.2,
                    opt_funcalltype.3,
                    opt_funcalltype.4,
                ),
                t_out,
            )),
            return_hint,
        }))),
        OptUseT::OptGetPropT {
            use_op,
            reason,
            id,
            propref,
            hint,
        } => UseT::new(UseTInner::GetPropT(Box::new(GetPropTData {
            use_op,
            reason,
            id,
            from_annot: false,
            skip_optional: false,
            propref: Box::new(propref),
            tout: Box::new(t_out),
            hint,
        }))),
        OptUseT::OptGetPrivatePropT(u, r, s, cbs, b) => {
            UseT::new(UseTInner::GetPrivatePropT(Box::new(GetPrivatePropTData {
                use_op: u,
                reason: r,
                name: s,
                class_bindings: cbs,
                static_: b,
                tout: Box::new(t_out),
            })))
        }
        OptUseT::OptTestPropT(use_op, reason, id, propref, hint) => {
            UseT::new(UseTInner::TestPropT(Box::new(TestPropTData {
                use_op,
                reason,
                id,
                propref: Box::new(propref),
                tout: Box::new(t_out),
                hint,
            })))
        }
        OptUseT::OptGetElemT(use_op, reason, id, from_annot, key_t) => {
            UseT::new(UseTInner::GetElemT(Box::new(GetElemTData {
                use_op,
                reason,
                id,
                from_annot,
                skip_optional: false,
                access_iterables: false,
                key_t,
                tout: Box::new(t_out),
            })))
        }
        OptUseT::OptCallElemT(u, r1, r2, elt, call) => {
            UseT::new(UseTInner::CallElemT(Box::new(CallElemTData {
                use_op: u,
                reason: r1,
                prop_reason: r2,
                key_t: elt,
                method_action: Box::new(apply_opt_action(call, t_out)),
            })))
        }
    }
}

pub fn mk_enum_type(reason: Reason, enum_info: Rc<EnumInfo>) -> Type {
    let reason = reason.update_desc(|desc| {
        desc.map(|desc| match &desc {
            VirtualReasonDesc::REnum { name } => {
                if let Some(name_str) = name {
                    VirtualReasonDesc::RType(name_str.dupe())
                } else {
                    desc.clone()
                }
            }
            _ => desc.clone(),
        })
    });
    Type::new(TypeInner::DefT(
        reason,
        DefT::new(DefTInner::EnumValueT(enum_info)),
    ))
}

pub fn mk_enum_object_type(reason: Reason, enum_info: Rc<EnumInfo>) -> Type {
    let enum_value_t = mk_enum_type(reason.dupe(), enum_info.dupe());
    Type::new(TypeInner::DefT(
        reason,
        DefT::new(DefTInner::EnumObjectT {
            enum_value_t,
            enum_info,
        }),
    ))
}

pub fn call_of_method_app(
    call_this_t: Type,
    call_specialized_callee: Option<SpecializedCallee>,
    methodcalltype: MethodCallType,
) -> FuncallType {
    FuncallType {
        call_this_t: methodcalltype.meth_generic_this.unwrap_or(call_this_t),
        call_targs: methodcalltype.meth_targs,
        call_args_tlist: methodcalltype.meth_args_tlist,
        call_tout: methodcalltype.meth_tout,
        call_strict_arity: methodcalltype.meth_strict_arity,
        call_speculation_hint_state: None,
        call_specialized_callee,
    }
}

pub mod type_params {
    use super::*;

    pub fn to_list(tparams: &Option<(ALoc, Vec1<TypeParam>)>) -> Vec<TypeParam> {
        match tparams {
            None => Vec::new(),
            Some((_loc, params)) => params.clone().into_vec(),
        }
    }

    pub fn of_list(tparams_loc: ALoc, tparams: Vec<TypeParam>) -> Option<(ALoc, Vec1<TypeParam>)> {
        Vec1::try_from_vec(tparams)
            .ok()
            .map(|params| (tparams_loc, params))
    }

    pub fn map<F>(
        f: F,
        tparams: &Option<(ALoc, Vec1<TypeParam>)>,
    ) -> Option<(ALoc, Vec1<TypeParam>)>
    where
        F: Fn(&TypeParam) -> TypeParam,
    {
        tparams.as_ref().map(|(loc, params)| {
            let mapped = params.iter().map(f).collect::<Vec<_>>();
            (loc.dupe(), Vec1::try_from_vec(mapped).unwrap())
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AnnotatedOrInferred {
    Annotated(Type),
    Inferred(Type),
}

pub fn empty_tuple_view() -> TupleView {
    TupleView {
        kind: TupleViewKind::ArrayLiteral,
        elements: Rc::from([]),
        arity: (0, 0),
        inexact: false,
        rest: None,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use flow_data_structure_wrapper::smol_str::FlowSmolStr;

    use super::NumberLiteral;
    use super::UnionEnum;

    #[test]
    fn number_literal_neg_zero_equals_pos_zero() {
        // IEEE 754: -0.0 and 0.0 have different bit representations but OCaml
        // compare treats them as equal. The fix normalizes zeros before comparing.
        let neg_zero = NumberLiteral(-0.0, FlowSmolStr::new_inline("-0"));
        let pos_zero = NumberLiteral(0.0, FlowSmolStr::new_inline("0"));
        assert_eq!(neg_zero, pos_zero, "-0.0 and 0.0 should be equal");
    }

    #[test]
    fn number_literal_nan_equality() {
        // Different NaN bit patterns should compare as equal after normalization.
        let nan1 = NumberLiteral(f64::NAN, FlowSmolStr::new_inline("NaN"));
        let nan2 = NumberLiteral(
            f64::from_bits(0x7FF8_0000_0000_0001),
            FlowSmolStr::new_inline("NaN"),
        );
        assert_ne!(
            f64::NAN.to_bits(),
            f64::from_bits(0x7FF8_0000_0000_0001).to_bits(),
            "precondition: the two NaN bit patterns should differ"
        );
        assert_eq!(nan1, nan2, "different NaN bit patterns should be equal");
    }

    #[test]
    fn number_literal_string_excluded_from_equality() {
        // The string representation should not affect equality.
        // NumberLiteral(0.0, "0") should equal NumberLiteral(0.0, "0.0").
        let a = NumberLiteral(0.0, FlowSmolStr::new_inline("0"));
        let b = NumberLiteral(0.0, FlowSmolStr::new_inline("0.0"));
        assert_eq!(a, b, "string representation should not affect equality");
    }

    #[test]
    fn number_literal_hash_consistency_for_equal_values() {
        // Equal values must produce equal hashes.
        let mut set = HashSet::new();

        // -0.0 and 0.0 are equal, so inserting both should result in one entry
        let neg_zero = NumberLiteral(-0.0, FlowSmolStr::new_inline("-0"));
        let pos_zero = NumberLiteral(0.0, FlowSmolStr::new_inline("0"));
        set.insert(neg_zero);
        set.insert(pos_zero);
        assert_eq!(
            set.len(),
            1,
            "-0.0 and 0.0 should hash to same bucket and deduplicate"
        );

        // Same float with different string should also deduplicate
        let mut set2 = HashSet::new();
        let a = NumberLiteral(42.0, FlowSmolStr::new_inline("42"));
        let b = NumberLiteral(42.0, FlowSmolStr::new_inline("42.0"));
        set2.insert(a);
        set2.insert(b);
        assert_eq!(
            set2.len(),
            1,
            "same float with different string repr should deduplicate"
        );
    }

    #[test]
    fn number_literal_ordering_neg_zero_pos_zero() {
        // -0.0 and 0.0 should have Equal ordering after normalization.
        let neg_zero = NumberLiteral(-0.0, FlowSmolStr::new_inline("-0"));
        let pos_zero = NumberLiteral(0.0, FlowSmolStr::new_inline("0"));
        assert_eq!(
            neg_zero.cmp(&pos_zero),
            std::cmp::Ordering::Equal,
            "-0.0 and 0.0 should have Equal ordering"
        );
    }

    #[test]
    fn number_literal_ordering_string_excluded() {
        // Ordering should not consider the string representation.
        let a = NumberLiteral(1.0, FlowSmolStr::new_inline("1"));
        let b = NumberLiteral(1.0, FlowSmolStr::new_inline("1.0"));
        assert_eq!(
            a.cmp(&b),
            std::cmp::Ordering::Equal,
            "ordering should not consider string representation"
        );
    }

    #[test]
    fn number_literal_normal_values_still_compare_correctly() {
        // Sanity check: normal distinct values should still be unequal.
        let one = NumberLiteral(1.0, FlowSmolStr::new_inline("1"));
        let two = NumberLiteral(2.0, FlowSmolStr::new_inline("2"));
        assert_ne!(one, two);
        assert!(one < two);
    }

    #[test]
    fn union_enum_num_neg_zero_equals_pos_zero() {
        let a = UnionEnum::Num(NumberLiteral(-0.0, FlowSmolStr::new_inline("-0")));
        let b = UnionEnum::Num(NumberLiteral(0.0, FlowSmolStr::new_inline("0")));
        assert_eq!(a, b, "UnionEnum::Num should delegate to NumberLiteral eq");
    }

    #[test]
    fn union_enum_num_different_string_repr() {
        let a = UnionEnum::Num(NumberLiteral(5.0, FlowSmolStr::new_inline("5")));
        let b = UnionEnum::Num(NumberLiteral(5.0, FlowSmolStr::new_inline("5.0")));
        assert_eq!(a, b, "UnionEnum::Num should ignore string in NumberLiteral");
    }

    #[test]
    fn union_enum_num_ordering_delegates_to_number_literal() {
        let a = UnionEnum::Num(NumberLiteral(-0.0, FlowSmolStr::new_inline("-0")));
        let b = UnionEnum::Num(NumberLiteral(0.0, FlowSmolStr::new_inline("0")));
        assert_eq!(
            a.cmp(&b),
            std::cmp::Ordering::Equal,
            "UnionEnum::Num ordering should delegate to NumberLiteral"
        );
    }

    #[test]
    fn check_enum_sizes() {
        use std::mem::size_of;
        assert_eq!(size_of::<super::Name>(), 16);
        assert_eq!(size_of::<super::SymbolID>(), 8);
        assert_eq!(size_of::<super::DefTInner>(), 32);
        assert_eq!(size_of::<super::CanonicalRendersForm>(), 48);
        assert_eq!(size_of::<super::ComponentKind>(), 48);
        assert_eq!(size_of::<super::ReactEffectType>(), 32);
        assert_eq!(size_of::<super::VirtualRootUseOp<flow_aloc::ALoc>>(), 24);
        assert_eq!(size_of::<super::VirtualFrameUseOp<flow_aloc::ALoc>>(), 24);
        assert_eq!(size_of::<super::PredicateInner>(), 32);
        assert_eq!(size_of::<super::ArrType>(), 16);
        assert_eq!(size_of::<super::ObjKind>(), 32);
        assert_eq!(size_of::<super::Destructor>(), 24);
        assert_eq!(size_of::<super::UnresolvedParam>(), 16);
        assert_eq!(size_of::<super::ResolvedParam>(), 16);
        assert_eq!(size_of::<super::SpreadResolve>(), 24);
        assert_eq!(size_of::<super::LookupKind>(), 16);
        assert_eq!(size_of::<super::ElemAction>(), 24);
        assert_eq!(size_of::<super::SpeculationHintState>(), 16);
        assert_eq!(size_of::<super::EnumPossibleExhaustiveCheckT>(), 16);
        assert_eq!(size_of::<super::union_rep::FinallyOptimizedRep>(), 24);
        assert_eq!(size_of::<super::nominal::Id>(), 16);
        assert_eq!(size_of::<super::nominal::UnderlyingT>(), 16);
        assert_eq!(size_of::<super::object::Tool>(), 16);
        assert_eq!(size_of::<super::react::Tool>(), 16);
        assert_eq!(size_of::<super::ObjAssignKind>(), 1);
        assert_eq!(size_of::<super::LookupAction>(), 16);
    }
}
