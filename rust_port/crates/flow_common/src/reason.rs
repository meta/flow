/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

//! This module defines a general notion of trace, which is used in modules
//! Type_inference_js and Flow_js to record how the typechecker reasons about
//! code, systematically collecting, simplifying, and solving constraints. This
//! is extremely useful, not only for debugging the typechecker but also to
//! really understand why an error is reported.
//!
//! Eventually, trace information should be printed out only in verbose mode,
//! since Flow reports all errors it finds and the trace for every error can get
//! quite detailed dependening on how far apart the "source" and "sink" are and
//! how convoluted the flow between them is.
//!
//! Reasons are included in types mainly for error reporting, but sometimes we
//! also use reasons in types to recover information on the source code that
//! caused those reasons to be created. Two examples of such secondary uses of
//! reasons are:
//!
//! - strictness analysis: we use reasons to locate the origin of an object and
//!   the origin of an operation on the object, and use such origins to determine
//!   whether certain errors should be suppressed.
//!
//! - termination analysis: we use reasons to limit instantiation of type
//!   parameters in polymorphic types at particular locations, to prevent the type
//!   checker from generating an unbounded number of constraints.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::fmt;
use std::hash::Hash;
use std::hash::Hasher;
use std::ops::Deref;
use std::sync::Arc;

use dupe::Dupe;
use flow_aloc::ALoc;
use flow_aloc::ALocId;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_parser::ast;
use flow_parser::ast::expression::BinaryOperator;
use flow_parser::ast::expression::ExpressionInner;
use flow_parser::ast::expression::LogicalOperator;
use flow_parser::file_key::FileKey;
use flow_parser::file_key::FileKeyInner;
use flow_parser::loc::Loc;
use flow_parser::loc_sig::LocSig;
use flow_parser::offset_utils::OffsetTable;

use crate::error_ref::ErrorReference;
use crate::flow_import_specifier::Userland;
use crate::string_utils::code_span;
use crate::subst_name::SubstName;

static NEXT_ID: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

pub fn mk_id() -> usize {
    NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct SymbolIDInner {
    id: ALocId,
    /// Best-effort display name: the binding or property whose type is the
    /// `unique symbol`. Used only for user-facing rendering. It is deliberately
    /// excluded from equality, ordering, and hashing, so two symbol keys are the
    /// same property iff their `id`s match, regardless of the name recovered at
    /// any particular use site.
    name: Option<FlowSmolStr>,
}

/// The nominal identity of a `unique symbol`, derived from the `ALoc.id` of the
/// location where the symbol is introduced (see `DefTInner::UniqueSymbolT`). Two
/// symbol keys refer to the same property iff their `SymbolID`s are equal.
#[derive(Debug, Clone, Dupe, serde::Serialize, serde::Deserialize)]
pub struct SymbolID(Arc<SymbolIDInner>);

impl PartialEq for SymbolID {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0) || self.0.id == other.0.id
    }
}

impl Eq for SymbolID {}

impl PartialOrd for SymbolID {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SymbolID {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        if Arc::ptr_eq(&self.0, &other.0) {
            std::cmp::Ordering::Equal
        } else {
            self.0.id.cmp(&other.0.id)
        }
    }
}

impl std::hash::Hash for SymbolID {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.id.hash(state);
    }
}

impl SymbolID {
    pub fn with_name(id: ALocId, name: Option<FlowSmolStr>) -> Self {
        Self(Arc::new(SymbolIDInner { id, name }))
    }

    pub fn aloc_id(&self) -> &ALocId {
        &self.0.id
    }

    pub fn name(&self) -> Option<&FlowSmolStr> {
        self.0.name.as_ref()
    }
}

/// A property key. Object properties are keyed either by an ordinary string name
/// or, for `unique symbol` computed keys, by the symbol's nominal identity.
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
pub enum Name {
    Str(FlowSmolStr),
    Symbol(SymbolID),
}

impl Dupe for Name {}

impl Name {
    pub fn new(s: impl Into<FlowSmolStr>) -> Self {
        Name::Str(s.into())
    }

    pub fn symbol(id: SymbolID) -> Self {
        Name::Symbol(id)
    }

    pub fn is_symbol(&self) -> bool {
        matches!(self, Name::Symbol(_))
    }

    pub fn symbol_id(&self) -> Option<&SymbolID> {
        match self {
            Name::Str(_) => None,
            Name::Symbol(id) => Some(id),
        }
    }

    /// The backing string of a string key, or `None` for a symbol key. Use this
    /// for lookups, comparisons, and string-keyed collections, where a symbol
    /// key must be handled explicitly (usually skipped) rather than aliased onto
    /// a shared placeholder.
    pub fn as_str_opt(&self) -> Option<&str> {
        match self {
            Name::Str(s) => Some(s),
            Name::Symbol(_) => None,
        }
    }

    /// The backing `FlowSmolStr` of a string key, or `None` for a symbol key.
    pub fn as_smol_str_opt(&self) -> Option<&FlowSmolStr> {
        match self {
            Name::Str(s) => Some(s),
            Name::Symbol(_) => None,
        }
    }

    /// The backing `FlowSmolStr` of a string key, or `None` for a symbol key,
    /// consuming `self`.
    pub fn into_smol_str_opt(self) -> Option<FlowSmolStr> {
        match self {
            Name::Str(s) => Some(s),
            Name::Symbol(_) => None,
        }
    }

    /// Whether this key is the string `s`. A symbol key never matches a string,
    /// so this is the safe way to test a key against a fixed property name.
    pub fn matches_str(&self, s: &str) -> bool {
        matches!(self, Name::Str(n) if n.as_str() == s)
    }

    /// User-facing spelling for error messages and printed types: the ordinary
    /// name for a string key, and the TypeScript-style bracketed form (`[sym]`,
    /// or `[symbol]` when unknown) for a symbol key. This renders a symbol key
    /// rather than yielding its backing string, so it must not be used for
    /// property lookup or any comparison against real property names.
    pub fn display_smol_str(&self) -> FlowSmolStr {
        match self {
            Name::Str(s) => s.dupe(),
            Name::Symbol(_) => FlowSmolStr::from(self.to_string()),
        }
    }
}

impl fmt::Display for Name {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Name::Str(s) => write!(f, "{}", s),
            // A symbol key is rendered TypeScript-style in brackets, using the name
            // of the binding or property it comes from when known (e.g. `[sym]`),
            // and a generic `[symbol]` otherwise. The `SymbolID`'s `ALocId` is
            // non-deterministic and must never leak into user-facing output.
            Name::Symbol(sym) => match sym.name() {
                Some(name) => write!(f, "[{}]", name),
                None => write!(f, "[symbol]"),
            },
        }
    }
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
pub enum ReasonDescFunction {
    RAsync,
    RGenerator,
    RAsyncGenerator,
    RNormal,
    RUnknown,
}

impl ReasonDescFunction {
    pub fn prefix(&self) -> &'static str {
        match self {
            ReasonDescFunction::RAsync => "async ",
            ReasonDescFunction::RGenerator => "generator ",
            ReasonDescFunction::RAsyncGenerator => "async generator ",
            ReasonDescFunction::RNormal => "",
            ReasonDescFunction::RUnknown => "unknown ",
        }
    }
}

// TODO type names should not be able to be internal names
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
pub enum VirtualReasonDesc<L: Dupe> {
    RNumber,
    RBigInt,
    RString,
    RBoolean,
    RMixed,
    RVoid,
    RNull,
    RVoidedNull,
    RSymbol,
    RUniqueSymbol,
    RExports,
    RNullOrVoid,
    RObjectLit,
    RObjectType,
    RArrayLit,
    RArrayType,
    RROArrayType,
    RTupleType,
    RTupleUnknownElementFromInexact,
    RFunctionType,
    RThis,
    RThisType,
    RImplicitInstantiation,
    RDummyThis,
    RObjectPatternRestProp,
    RArrayPatternRestProp,
    RReactChildren,
    RUninitialized,
    RPossiblyUninitialized,
    RRendersNothing,
    RStringLit(FlowSmolStr),
    RNumberLit(FlowSmolStr),
    RBigIntLit(FlowSmolStr),
    RBooleanLit(bool),
    RInferredUnionElemArray {
        instantiable: bool,
        is_empty: bool,
    },
    RTupleOutOfBoundsAccess(i32),
    RFunction(ReasonDescFunction),
    REnum {
        name: Option<FlowSmolStr>,
    },
    REnumMember {
        enum_desc: Arc<VirtualReasonDesc<L>>,
        member_name: FlowSmolStr,
    },
    RRecordType(FlowSmolStr),
    RType(FlowSmolStr),
    RTypeAlias(Box<(FlowSmolStr, Option<L>, Option<Arc<VirtualReasonDesc<L>>>)>),
    ROpaqueType(FlowSmolStr),
    RTypeParam(Box<(SubstName, L, L)>),
    RTypeof(FlowSmolStr),
    RParameter(Option<FlowSmolStr>),
    RIdentifier(FlowSmolStr),
    RProperty(Option<Name>),
    RPrivateProperty(FlowSmolStr),
    RMember {
        object_: FlowSmolStr,
        property: FlowSmolStr,
    },
    RUnknownUnspecifiedProperty(Arc<VirtualReasonDesc<L>>),
    RNamedImportedType(Userland, FlowSmolStr),
    RImportStarType(FlowSmolStr),
    RImportStarTypeOf(FlowSmolStr),
    RImportStar(FlowSmolStr),
    RDefaultImportedType(FlowSmolStr, Userland),
    RCode(FlowSmolStr),
    RPartialOf(Arc<VirtualReasonDesc<L>>),
    RRequiredOf(Arc<VirtualReasonDesc<L>>),
    RModule(Userland),
    // TODO React element names should not allow internal names
    RReactElement {
        name_opt: Option<Name>,
        from_component_syntax: bool,
    },
    RComponent(FlowSmolStr),
}

impl<A: Dupe> VirtualReasonDesc<A> {
    pub fn map_locs<B: Dupe, F>(&self, f: &F) -> VirtualReasonDesc<B>
    where
        F: Fn(&A) -> B,
    {
        use VirtualReasonDesc::*;

        match self {
            // Simple variants with no locations - need to reconstruct for type conversion
            RNumber => RNumber,
            RBigInt => RBigInt,
            RString => RString,
            RBoolean => RBoolean,
            RMixed => RMixed,
            RVoid => RVoid,
            RNull => RNull,
            RVoidedNull => RVoidedNull,
            RSymbol => RSymbol,
            RUniqueSymbol => RUniqueSymbol,
            RExports => RExports,
            RNullOrVoid => RNullOrVoid,
            RStringLit(n) => RStringLit(n.dupe()),
            RNumberLit(s) => RNumberLit(s.dupe()),
            RBigIntLit(s) => RBigIntLit(s.dupe()),
            RBooleanLit(b) => RBooleanLit(*b),
            RObjectLit => RObjectLit,
            RObjectType => RObjectType,
            RArrayLit => RArrayLit,
            RArrayType => RArrayType,
            RROArrayType => RROArrayType,
            RTupleType => RTupleType,
            RTupleOutOfBoundsAccess(i) => RTupleOutOfBoundsAccess(*i),
            RTupleUnknownElementFromInexact => RTupleUnknownElementFromInexact,
            RFunction(func) => RFunction(*func),
            RFunctionType => RFunctionType,
            RUninitialized => RUninitialized,
            RPossiblyUninitialized => RPossiblyUninitialized,
            RComponent(n) => RComponent(n.dupe()),
            RInferredUnionElemArray {
                instantiable,
                is_empty,
            } => RInferredUnionElemArray {
                instantiable: *instantiable,
                is_empty: *is_empty,
            },
            REnum { name } => REnum { name: name.dupe() },
            RThis => RThis,
            RThisType => RThisType,
            RImplicitInstantiation => RImplicitInstantiation,
            RRecordType(s) => RRecordType(s.dupe()),
            RDummyThis => RDummyThis,
            RType(n) => RType(n.dupe()),
            RTypeof(s) => RTypeof(s.dupe()),
            RParameter(opt) => RParameter(opt.dupe()),
            RIdentifier(n) => RIdentifier(n.dupe()),
            RProperty(opt) => RProperty(opt.dupe()),
            RPrivateProperty(s) => RPrivateProperty(s.dupe()),
            RMember { object_, property } => RMember {
                object_: object_.dupe(),
                property: property.dupe(),
            },
            RNamedImportedType(u, s) => RNamedImportedType(u.clone(), s.dupe()),
            RImportStarType(s) => RImportStarType(s.dupe()),
            RImportStarTypeOf(s) => RImportStarTypeOf(s.dupe()),
            RImportStar(s) => RImportStar(s.dupe()),
            RDefaultImportedType(s, u) => RDefaultImportedType(s.dupe(), u.clone()),
            RCode(s) => RCode(s.dupe()),
            ROpaqueType(s) => ROpaqueType(s.dupe()),
            RRendersNothing => RRendersNothing,
            RObjectPatternRestProp => RObjectPatternRestProp,
            RArrayPatternRestProp => RArrayPatternRestProp,
            RModule(u) => RModule(u.clone()),
            RReactElement {
                name_opt,
                from_component_syntax,
            } => RReactElement {
                name_opt: name_opt.dupe(),
                from_component_syntax: *from_component_syntax,
            },
            RReactChildren => RReactChildren,

            // Recursive variants
            RUnknownUnspecifiedProperty(d) => RUnknownUnspecifiedProperty(Arc::new(d.map_locs(f))),
            RTypeAlias(box (s, loc_opt, d)) => RTypeAlias(Box::new((
                s.dupe(),
                loc_opt.as_ref().map(f),
                d.as_ref().map(|d| Arc::new(d.map_locs(f))),
            ))),
            RTypeParam(box (s, l1, l2)) => RTypeParam(Box::new((s.clone(), f(l1), f(l2)))),
            RPartialOf(d) => RPartialOf(Arc::new(d.map_locs(f))),
            RRequiredOf(d) => RRequiredOf(Arc::new(d.map_locs(f))),
            REnumMember {
                enum_desc,
                member_name,
            } => REnumMember {
                enum_desc: Arc::new(enum_desc.map_locs(f)),
                member_name: member_name.dupe(),
            },
        }
    }
}

impl<L: Dupe> VirtualReasonDesc<L> {
    /// Unwrap type aliases
    pub fn unwrap(&self) -> Option<&VirtualReasonDesc<L>> {
        match self {
            VirtualReasonDesc::RTypeAlias(box (_, _, inner)) => inner.as_deref()?.unwrap(),
            _ => Some(self),
        }
    }

    /// Classify this reason description
    pub fn classify(&self) -> ReasonClassification {
        use ReasonClassification::*;
        use VirtualReasonDesc::*;

        match self.unwrap() {
            Some(
                RNumber | RBigInt | RString | RSymbol | RUniqueSymbol | RBoolean | RStringLit(_)
                | RNumberLit(_) | RBigIntLit(_) | RBooleanLit(_),
            ) => Scalar,

            Some(
                RVoid | RNull | RVoidedNull | RUninitialized | RPossiblyUninitialized | RNullOrVoid,
            ) => Nullish,

            Some(RArrayLit | RArrayType | RROArrayType | RTupleType | RArrayPatternRestProp) => {
                Array
            }

            _ => Unclassified,
        }
    }

    /// Check if this reason description is scalar
    pub fn is_scalar(&self) -> bool {
        let c = self.classify();
        c == ReasonClassification::Scalar || c == ReasonClassification::Nullish
    }

    /// Whether this description explains provenance that the normalized type cannot express.
    /// Add things here judiciously. Only primitive-like types belong here.
    pub fn is_explanatory(&self) -> bool {
        use VirtualReasonDesc::*;

        match self.unwrap() {
            Some(
                RUninitialized
                | RPossiblyUninitialized
                | RTupleOutOfBoundsAccess(_)
                | RTupleUnknownElementFromInexact
                | RUnknownUnspecifiedProperty(_)
                | RReactElement { .. }
                | RRendersNothing,
            ) => true,
            _ => false,
        }
    }

    /// Invalidate RTypeAlias by removing the reliable def_loc
    pub fn invalidate_rtype_alias(self) -> VirtualReasonDesc<L> {
        match &self {
            VirtualReasonDesc::RTypeAlias(box (name, Some(_), inner)) => {
                VirtualReasonDesc::RTypeAlias(Box::new((name.dupe(), None, inner.clone())))
            }
            _ => self,
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
pub struct VirtualReasonInner<L: Dupe> {
    pub desc: Option<VirtualReasonDesc<L>>,
    pub loc: L,
    pub def_loc_opt: Option<L>,
    pub annot_loc_opt: Option<L>,
}

/// Wrapper struct for VirtualReason that uses Arc for cheap cloning
#[derive(Clone, Dupe)]
pub struct VirtualReason<L: Dupe>(Arc<VirtualReasonInner<L>>);

impl<L: Dupe + Hash> Hash for VirtualReason<L> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

impl<L: Dupe + PartialEq> PartialEq for VirtualReason<L> {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0) || self.0 == other.0
    }
}

impl<L: Dupe + Eq> Eq for VirtualReason<L> {}

impl<L: Dupe + PartialOrd> PartialOrd for VirtualReason<L> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        if Arc::ptr_eq(&self.0, &other.0) {
            Some(std::cmp::Ordering::Equal)
        } else {
            self.0.partial_cmp(&other.0)
        }
    }
}

impl<L: Dupe + Ord> Ord for VirtualReason<L> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        if Arc::ptr_eq(&self.0, &other.0) {
            std::cmp::Ordering::Equal
        } else {
            self.0.cmp(&other.0)
        }
    }
}

impl<L: Dupe> Deref for VirtualReason<L> {
    type Target = VirtualReasonInner<L>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<L: std::fmt::Debug + Dupe> std::fmt::Debug for VirtualReason<L> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl<L: Dupe + serde::Serialize> serde::Serialize for VirtualReason<L> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.0.serialize(serializer)
    }
}

impl<'de, L: Dupe + serde::Deserialize<'de>> serde::Deserialize<'de> for VirtualReason<L> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Ok(VirtualReason(Arc::new(VirtualReasonInner::deserialize(
            deserializer,
        )?)))
    }
}

pub type ConcreteReason = VirtualReason<Loc>;

pub type Reason = VirtualReason<ALoc>;

pub type ReasonDesc = VirtualReasonDesc<ALoc>;

impl Reason {
    /// Check if reason is from lib file
    pub fn is_lib(&self) -> bool {
        self.loc
            .source()
            .map(|file| file.is_lib_file())
            .unwrap_or(false)
    }

    /// Check if reason definition location is from lib file
    pub fn is_lib_def(&self) -> bool {
        self.def_loc()
            .source()
            .map(|file| file.is_lib_file())
            .unwrap_or(false)
    }

    /// Compare reasons with concretization
    pub fn concretize_equal(
        &self,
        other: &Reason,
        aloc_tables: &HashMap<FileKey, flow_aloc::LazyALocTable>,
    ) -> bool {
        self.loc.concretize_equal(&other.loc, aloc_tables)
            && self.desc == other.desc
            && match (&self.def_loc_opt, &other.def_loc_opt) {
                (None, None) => true,
                (Some(l1), Some(l2)) => l1.concretize_equal(l2, aloc_tables),
                _ => false,
            }
            && match (&self.annot_loc_opt, &other.annot_loc_opt) {
                (None, None) => true,
                (Some(l1), Some(l2)) => l1.concretize_equal(l2, aloc_tables),
                _ => false,
            }
    }
}

impl<A: Dupe> VirtualReason<A> {
    pub fn map_locs<B: Dupe, F>(self, f: F) -> VirtualReason<B>
    where
        F: Fn(&A) -> B,
    {
        VirtualReason::new_with(
            self.desc.as_ref().map(|desc| desc.map_locs(&f)),
            f(&self.loc),
            self.def_loc_opt.as_ref().map(&f),
            self.annot_loc_opt.as_ref().map(&f),
        )
    }
}

impl<L: Dupe> VirtualReason<L> {
    /// Projects this reason to the location and description used by an error.
    pub fn to_error_reference(&self) -> ErrorReference<L> {
        ErrorReference::new(self.loc().dupe(), self.desc.clone())
    }

    pub fn new(desc: impl Into<Option<VirtualReasonDesc<L>>>, loc: L) -> Self {
        VirtualReason(Arc::new(VirtualReasonInner {
            desc: desc.into(),
            loc,
            def_loc_opt: None,
            annot_loc_opt: None,
        }))
    }

    pub fn new_with(
        desc: Option<VirtualReasonDesc<L>>,
        loc: L,
        def_loc_opt: Option<L>,
        annot_loc_opt: Option<L>,
    ) -> Self {
        VirtualReason(Arc::new(VirtualReasonInner {
            desc,
            loc,
            def_loc_opt,
            annot_loc_opt,
        }))
    }

    // Accessors

    /// Get the location of this reason
    pub fn loc(&self) -> &L {
        &self.loc
    }

    /// Get the definition location (or fallback to loc)
    pub fn def_loc(&self) -> &L {
        match &self.def_loc_opt {
            Some(loc) => loc,
            None => &self.loc,
        }
    }

    /// Get the optional definition location
    pub fn def_loc_opt(&self) -> Option<&L> {
        self.def_loc_opt.as_ref()
    }

    /// Get the annotation location
    pub fn annot_loc(&self) -> Option<&L> {
        self.annot_loc_opt.as_ref()
    }

    /// Get the description, optionally unwrapping type aliases
    pub fn desc(&self, unwrap: bool) -> Option<&VirtualReasonDesc<L>> {
        let desc = self.0.desc.as_ref()?;
        if unwrap { desc.unwrap() } else { Some(desc) }
    }

    // Simple way to get derived reasons whose descriptions are
    // simple replacements of the original

    /// Replace desc, but keep loc, def_loc, annot_loc
    pub fn update_desc<F>(self, f: F) -> VirtualReason<L>
    where
        F: FnOnce(Option<VirtualReasonDesc<L>>) -> Option<VirtualReasonDesc<L>>,
    {
        VirtualReason::new_with(
            f(self.desc.clone()),
            self.loc.dupe(),
            self.def_loc_opt.as_ref().map(|l| l.dupe()),
            self.annot_loc_opt.as_ref().map(|l| l.dupe()),
        )
    }

    /// Replace desc, keep loc, but clobber def_loc, annot_loc as in new reason
    pub fn update_desc_new<F>(self, f: F) -> VirtualReason<L>
    where
        F: FnOnce(Option<VirtualReasonDesc<L>>) -> Option<VirtualReasonDesc<L>>,
    {
        VirtualReason::new(f(self.desc.clone()), self.loc.dupe())
    }

    /// Replace desc, but keep loc, def_loc, annot_loc
    pub fn replace_desc(self, desc: impl Into<Option<VirtualReasonDesc<L>>>) -> VirtualReason<L> {
        VirtualReason::new_with(
            desc.into(),
            self.loc.dupe(),
            self.def_loc_opt.as_ref().map(|l| l.dupe()),
            self.annot_loc_opt.as_ref().map(|l| l.dupe()),
        )
    }

    /// Replace desc, keep loc, but clobber def_loc, annot_loc as in new reason
    pub fn replace_desc_new(
        self,
        desc: impl Into<Option<VirtualReasonDesc<L>>>,
    ) -> VirtualReason<L> {
        VirtualReason::new(desc, self.loc.dupe())
    }
}

impl<L: Dupe + PartialEq> VirtualReason<L> {
    /// Replace loc, but keep def_loc
    pub fn reposition(self, loc: L) -> VirtualReason<L> {
        if loc == self.loc {
            return self;
        }
        let def_loc_opt = {
            let def_loc = self.def_loc();
            if loc == *def_loc {
                None
            } else {
                Some(def_loc.dupe())
            }
        };
        VirtualReason::new_with(
            self.desc.clone(),
            loc,
            def_loc_opt,
            self.annot_loc_opt.as_ref().map(|l| l.dupe()),
        )
    }

    pub fn reposition_and_annotate(self, loc: L) -> VirtualReason<L> {
        let same_loc = loc == self.loc;
        let same_annot = self.annot_loc_opt.as_ref() == Some(&loc);
        if same_loc && same_annot {
            return self;
        }
        let def_loc_opt = if same_loc {
            self.def_loc_opt.as_ref().map(|l| l.dupe())
        } else {
            let def_loc = self.def_loc();
            if loc == *def_loc {
                None
            } else {
                Some(def_loc.dupe())
            }
        };
        let reason_loc = if same_loc {
            self.loc.dupe()
        } else {
            loc.dupe()
        };
        VirtualReason::new_with(self.desc.clone(), reason_loc, def_loc_opt, Some(loc))
    }

    pub fn reposition_and_opt_annotate(self, loc: L, annot_loc: Option<L>) -> VirtualReason<L> {
        let same_loc = loc == self.loc;
        let same_annot = match &annot_loc {
            Some(annot_loc) => self.annot_loc_opt.as_ref() == Some(annot_loc),
            None => true,
        };
        if same_loc && same_annot {
            return self;
        }
        let def_loc_opt = if same_loc {
            self.def_loc_opt.as_ref().map(|l| l.dupe())
        } else {
            let def_loc = self.def_loc();
            if loc == *def_loc {
                None
            } else {
                Some(def_loc.dupe())
            }
        };
        let reason_loc = if same_loc { self.loc.dupe() } else { loc };
        let annot_loc = match annot_loc {
            Some(annot_loc) => Some(annot_loc),
            None => self.annot_loc_opt.as_ref().map(|l| l.dupe()),
        };
        VirtualReason::new_with(self.desc.clone(), reason_loc, def_loc_opt, annot_loc)
    }
}

impl<L: Dupe + PartialEq> VirtualReason<L> {
    /// Add / replace annot_loc, but keep loc and def_loc
    pub fn annotate(self, annot_loc: L) -> VirtualReason<L> {
        if self.annot_loc_opt.as_ref() == Some(&annot_loc) {
            return self;
        }
        VirtualReason::new_with(
            self.desc.clone(),
            self.loc.dupe(),
            self.def_loc_opt.as_ref().map(|l| l.dupe()),
            Some(annot_loc),
        )
    }

    /// When annot_loc is given, same as annot_reason; otherwise, identity
    pub fn opt_annotate(self, annot_loc: Option<L>) -> VirtualReason<L> {
        match annot_loc {
            None => self,
            Some(annot_loc) => self.annotate(annot_loc),
        }
    }
}

/// Pass in any available aloc tables to be used when comparing abstract and concrete locations from
/// the same file. Usually `Context.aloc_tables` is a good choice, but if the context is not
/// available, the empty map may be appropriate.
pub fn concretize_equal(
    aloc_tables: &HashMap<FileKey, flow_aloc::LazyALocTable>,
    r1: &Reason,
    r2: &Reason,
) -> bool {
    r1.concretize_equal(r2, aloc_tables)
}

pub fn in_range(loc: &Loc, range: &Loc) -> bool {
    let (line, line1, line2) = (loc.start.line, range.start.line, range.end.line);
    (line1 < line || (line == line1 && range.start.column <= loc.start.column))
        && (line < line2 || (line == line2 && loc.end.column <= range.end.column))
}

pub fn string_of_source(strip_root: Option<&str>, source: &FileKey) -> String {
    use flow_parser::file_key::FileKeyInner;
    match source.inner() {
        FileKeyInner::LibFile(_) => {
            if let Some(root) = strip_root {
                let file = source.to_absolute();
                let root = std::path::Path::new(root);
                let root_str = crate::files::normalized_root_prefix(root);
                let normalized_file = crate::sys_utils::normalize_filename_dir_sep(&file);
                if normalized_file.starts_with(&root_str) {
                    format!(
                        "[LIB] {}",
                        crate::files::relative_path(std::path::Path::new(&root_str), &file)
                    )
                } else {
                    format!(
                        "[LIB] {}",
                        std::path::Path::new(&file)
                            .file_name()
                            .and_then(|s| s.to_str())
                            .unwrap_or(&file)
                    )
                }
            } else {
                source.to_absolute()
            }
        }
        FileKeyInner::SourceFile(_) | FileKeyInner::JsonFile(_) | FileKeyInner::ResourceFile(_) => {
            let file = source.to_absolute();
            if let Some(root) = strip_root {
                if let Some(relative) = flow_parser::file_key::relative_flowlib_path(&file) {
                    return format!("[LIB] {}", relative);
                }
                let root = std::path::Path::new(root);
                let root_str = crate::files::normalized_root_prefix(root);
                crate::files::relative_path(std::path::Path::new(&root_str), &file)
            } else {
                file
            }
        }
    }
}

pub fn string_of_loc(strip_root: Option<&str>, loc: &Loc) -> String {
    match &loc.source {
        None => String::new(),
        Some(file) => {
            let source_str = string_of_source(strip_root, file);
            let loc_str = loc.to_string_no_source();
            format!("{}:{}", source_str, loc_str)
        }
    }
}

pub fn string_of_aloc(strip_root: Option<&str>, aloc: &ALoc) -> String {
    match aloc.source() {
        None => String::new(),
        Some(file) => {
            let source_str = string_of_source(strip_root, file);
            let loc_str = aloc.to_string_no_source();
            format!("{}:{}", source_str, loc_str)
        }
    }
}

pub fn json_of_source(strip_root: Option<&str>, source: Option<&FileKey>) -> serde_json::Value {
    match source {
        Some(x) => serde_json::Value::String(string_of_source(strip_root, x)),
        None => serde_json::Value::Null,
    }
}

pub fn json_source_type_of_source(source: Option<&FileKey>) -> serde_json::Value {
    match source.map(|s| s.inner()) {
        Some(FileKeyInner::LibFile(_)) => serde_json::Value::String("LibFile".to_string()),
        Some(FileKeyInner::SourceFile(_)) => serde_json::Value::String("SourceFile".to_string()),
        Some(FileKeyInner::JsonFile(_)) => serde_json::Value::String("JsonFile".to_string()),
        Some(FileKeyInner::ResourceFile(_)) => {
            serde_json::Value::String("ResourceFile".to_string())
        }
        None => serde_json::Value::Null,
    }
}

pub fn json_of_loc_props(
    strip_root: Option<&str>,
    catch_offset_errors: bool,
    offset_table: Option<&OffsetTable>,
    loc: &Loc,
) -> Vec<(String, serde_json::Value)> {
    // Rust-port-specific: the lexer (lex_env::pos_at_offset) emits Positions with
    // BYTE columns, while OCaml's Sedlexing-based lexer emits CODEPOINT columns
    // and OCaml's `Reason.json_of_loc` consumes those directly. The OffsetTable is
    // codepoint-indexed (matching OCaml `Offset_utils`). To produce JSON output
    // that matches the OCaml format (codepoint columns and Utf8 byte offsets) we
    // must convert byte->codepoint columns whenever an offset_table is available.
    let (start_pos, end_pos) = match offset_table {
        Some(table) => {
            let convert = |pos: flow_parser::loc::Position| match table
                .convert_flow_position_to_codepoint_position(pos)
            {
                Ok(p) => p,
                Err(exn) => {
                    if catch_offset_errors {
                        pos
                    } else {
                        panic!("{}", exn.debug_to_string())
                    }
                }
            };
            (convert(loc.start), convert(loc.end))
        }
        None => (loc.start, loc.end),
    };
    let offset_entry = |table: &OffsetTable,
                        pos: &flow_parser::loc::Position|
     -> Vec<(String, serde_json::Value)> {
        let offset = match table.offset(*pos) {
            Ok(o) => serde_json::json!(o),
            Err(exn) => {
                if catch_offset_errors {
                    serde_json::Value::Null
                } else {
                    panic!("{}", exn.debug_to_string())
                }
            }
        };
        vec![("offset".to_string(), offset)]
    };
    let mut start: Vec<(String, serde_json::Value)> = vec![
        ("line".to_string(), serde_json::json!(start_pos.line)),
        (
            "column".to_string(),
            serde_json::json!(start_pos.column + 1),
        ),
    ];
    match offset_table {
        None => {}
        Some(table) => start.extend(offset_entry(table, &start_pos)),
    }
    let mut end_: Vec<(String, serde_json::Value)> = vec![
        ("line".to_string(), serde_json::json!(end_pos.line)),
        ("column".to_string(), serde_json::json!(end_pos.column)),
    ];
    match offset_table {
        None => {}
        Some(table) => end_.extend(offset_entry(table, &end_pos)),
    }
    vec![
        (
            "source".to_string(),
            json_of_source(strip_root, loc.source.as_ref()),
        ),
        (
            "type".to_string(),
            json_source_type_of_source(loc.source.as_ref()),
        ),
        (
            "start".to_string(),
            serde_json::Value::Object(start.into_iter().collect()),
        ),
        (
            "end".to_string(),
            serde_json::Value::Object(end_.into_iter().collect()),
        ),
    ]
}

pub fn json_of_loc(
    strip_root: Option<&str>,
    catch_offset_errors: bool,
    offset_table: Option<&OffsetTable>,
    loc: &Loc,
) -> serde_json::Value {
    serde_json::Value::Object(
        json_of_loc_props(strip_root, catch_offset_errors, offset_table, loc)
            .into_iter()
            .collect(),
    )
}

pub fn mk_reason<L: Dupe>(
    desc: impl Into<Option<VirtualReasonDesc<L>>>,
    loc: L,
) -> VirtualReason<L> {
    VirtualReason::new(desc, loc)
}

/// Lift a string to a reason. Usually used as a dummy reason.
pub fn locationless_reason(desc: impl Into<Option<ReasonDesc>>) -> Reason {
    mk_reason(desc, ALoc::none())
}

pub fn func_reason(async_: bool, generator: bool, loc: ALoc) -> Reason {
    let desc = match (async_, generator) {
        (true, true) => ReasonDescFunction::RAsyncGenerator,
        (true, false) => ReasonDescFunction::RAsync,
        (false, true) => ReasonDescFunction::RGenerator,
        (false, false) => ReasonDescFunction::RNormal,
    };
    mk_reason(VirtualReasonDesc::RFunction(desc), loc)
}

pub fn mk_obj_lit_reason(as_const: bool, frozen: bool, loc: ALoc) -> Reason {
    let desc = if frozen || as_const {
        None
    } else {
        Some(VirtualReasonDesc::RObjectLit)
    };
    mk_reason(desc, loc)
}

/// Prettify React utility names (React$ -> React.)
fn prettify_react_util(s: &str) -> String {
    if s.len() >= 6 && s.starts_with("React$") {
        format!("React.{}", &s[6..])
    } else {
        s.to_string()
    }
}

pub fn string_of_desc<L: Dupe>(desc: &VirtualReasonDesc<L>) -> String {
    use VirtualReasonDesc::*;

    match desc {
        // Primitives
        RNumber => "number".to_string(),
        RBigInt => "bigint".to_string(),
        RString => "string".to_string(),
        RBoolean => "boolean".to_string(),
        RMixed => "`unknown`".to_string(),
        RVoid => "undefined".to_string(),
        RNull => "null".to_string(),
        RVoidedNull => "undefined (result of null short-circuiting an optional chain)".to_string(),
        RNullOrVoid => "null or undefined".to_string(),
        RSymbol => "symbol".to_string(),
        RUniqueSymbol => "unique symbol".to_string(),
        RExports => "exports".to_string(),

        // String literals
        RStringLit(name) if name.as_str().is_empty() => "empty string".to_string(),
        RStringLit(name) => format!("string literal `{}`", name.as_str()),

        // Number and boolean literals
        RNumberLit(x) => format!("number literal `{}`", x),
        RBigIntLit(x) => format!("bigint literal `{}`", x),
        RBooleanLit(b) => format!("boolean literal `{}`", b),

        // Objects
        RObjectLit => "object literal".to_string(),
        RObjectType => "object type".to_string(),

        // Arrays
        RArrayLit => "array literal".to_string(),
        RArrayType => "array type".to_string(),
        RInferredUnionElemArray { .. } => "inferred union of array element types \
(alternatively, provide an annotation to summarize the array element type)"
            .to_string(),
        RROArrayType => "read-only array type".to_string(),

        // Tuples
        RTupleType => "tuple type".to_string(),
        RTupleOutOfBoundsAccess(i) => {
            format!("undefined (out of bounds tuple access at index {})", i)
        }
        RTupleUnknownElementFromInexact => {
            "an unknown element of an inexact tuple type".to_string()
        }

        // Functions
        RFunction(func) => format!("{}function", func.prefix()),
        RFunctionType => "function type".to_string(),

        // Enums
        REnum { name } => match name {
            Some(n) => format!("enum `{}`", n),
            None => "enum".to_string(),
        },
        REnumMember {
            enum_desc,
            member_name,
        } => {
            format!(
                "member {} of enum {}",
                member_name,
                string_of_desc(enum_desc)
            )
        }

        // This and types
        RThis => "this".to_string(),
        RThisType => "`this` type".to_string(),
        RImplicitInstantiation => "implicit instantiation".to_string(),

        // Records
        RRecordType(name) => format!("record `{}`", name),

        // Misc
        RDummyThis => "bound `this` in method".to_string(),

        // Type aliases and params
        RType(x) => format!("`{}`", prettify_react_util(x.as_str())),
        RTypeAlias(box (x, _, _)) => format!("`{}`", prettify_react_util(x)),
        ROpaqueType(x) => format!("`{}`", prettify_react_util(x)),
        RTypeParam(box (x, _, _)) => x.formatted_string_of_subst_name(),
        RTypeof(x) => format!("`typeof {}`", x),

        // Parameters
        RParameter(Some(x)) => format!("`{}`", x),
        RParameter(None) => "parameter".to_string(),
        RIdentifier(x) => format!("`{}`", prettify_react_util(x.as_str())),

        // Properties
        RProperty(Some(x)) => format!("property `{}`", x),
        RProperty(None) => "computed property".to_string(),
        RPrivateProperty(x) => format!("property `#{}`", x),
        RMember { object_, property } => code_span(&format!("{}{}", object_, property)),
        RUnknownUnspecifiedProperty(d) => {
            format!(
                "an unknown property that may exist on the inexact {}",
                string_of_desc(d)
            )
        }

        // Imports
        RNamedImportedType(module_name, _) => {
            format!("Named import from module `{}`", module_name.display())
        }
        RImportStarType(n) => format!("import type * as {}", n),
        RImportStarTypeOf(n) => format!("import typeof * as {}", n),
        RImportStar(n) => format!("import * as {}", n),
        RDefaultImportedType(_, module_name) => {
            format!("Default import from `{}`", module_name.display())
        }
        RCode(x) => code_span(x),

        // Type wrappers
        RPartialOf(d) => format!("partial {}", string_of_desc(d)),
        RRequiredOf(d) => format!("required of {}", string_of_desc(d)),

        // Pattern rest props
        RObjectPatternRestProp => "rest of object pattern".to_string(),
        RArrayPatternRestProp => "rest of array pattern".to_string(),

        // Modules and namespaces
        RModule(module_name) => format!("module `{}`", module_name.display()),

        // React
        RReactElement { name_opt, .. } => match name_opt {
            Some(name) => format!("`{}` element", name),
            None => "React element".to_string(),
        },
        RReactChildren => "children array".to_string(),

        // Uninitialized
        RUninitialized => "uninitialized variable".to_string(),
        RPossiblyUninitialized => "possibly uninitialized variable".to_string(),

        // Type guards and components
        RComponent(name) => format!("component {}", name.as_str()),

        // Renders
        RRendersNothing => "a value that renders nothing".to_string(),
    }
}

pub fn string_of_reason(strip_root: Option<&str>, r: &Reason) -> String {
    let spos = string_of_aloc(strip_root, &r.loc);
    let desc = r.desc.as_ref().map(string_of_desc).unwrap_or_default();
    if spos.is_empty() {
        desc
    } else if desc.is_empty() {
        spos
    } else {
        format!("{}: {}", spos, desc)
    }
}

pub fn dump_reason(strip_root: Option<&str>, r: &Reason) -> String {
    format!(
        "{}: {:?}",
        string_of_aloc(strip_root, &r.loc),
        r.desc.as_ref().map(string_of_desc).unwrap_or_default()
    )
}

/// Instantiable reasons identify tvars that are created for the purpose of
/// instantiation: they are fresh rather than shared, and should become types
/// that flow to them. We assume these characteristics when performing
/// speculative matching (even though we don't yet enforce them).
pub fn is_instantiable_reason<L: Dupe>(r: &VirtualReason<L>) -> bool {
    match r.desc(true) {
        Some(
            VirtualReasonDesc::RTypeParam(..)
            | VirtualReasonDesc::RThisType
            | VirtualReasonDesc::RImplicitInstantiation,
        ) => true,
        Some(VirtualReasonDesc::RInferredUnionElemArray {
            instantiable,
            is_empty: _,
        }) => *instantiable,
        _ => false,
    }
}

pub fn is_record_reason<L: Dupe>(r: &VirtualReason<L>) -> bool {
    matches!(r.desc(true), Some(VirtualReasonDesc::RRecordType(_)))
}

pub fn is_lib_reason(r: &Reason) -> bool {
    r.loc
        .source()
        .map(|file_key| file_key.is_lib_file())
        .unwrap_or(false)
}

pub fn is_lib_reason_def(r: &Reason) -> bool {
    r.def_loc()
        .source()
        .map(|file_key| file_key.is_lib_file())
        .unwrap_or(false)
}

pub fn is_blamable_reason(r: &Reason) -> bool {
    *r.loc() != ALoc::none() && !is_lib_reason(r)
}

pub fn mk_annot_reason(desc: impl Into<Option<ReasonDesc>>, annot_loc: ALoc) -> Reason {
    VirtualReason::new_with(desc.into(), annot_loc.dupe(), None, Some(annot_loc))
}

/// Creates a description string for an arbitrary expression. This description
/// will be used to describe some code in error messages which are designed to be
/// human readable.
///
/// We want to keep these descriptions *short* so we omit a lot of information in
/// places where expressions may often go recursive. For instance, object and
/// array literals are abbreviated as [...] and {...} respectively.
///
/// The wrap argument provides a rough heuristic for when wrapping is necessary.
/// We set wrap to true when we need to append something to the final expression.
/// Then expressions which need to be wrapped will call do_wrap. e.g.
///
/// - `(1 + 2).p`
/// - `o.p`
/// - `o[1 + 2]`
///
/// In the first example we need to wrap 1 + 2 to correctly print the property
/// access. However, we don't need to wrap o in o.p. In o[1 + 2] we don't need to
/// wrap 1 + 2 since it is already wrapped in a sense.
pub fn code_desc_of_expression<M: Dupe, T: Dupe>(
    wrap: bool,
    expr: &ast::expression::Expression<M, T>,
) -> String {
    let do_wrap = |s: String| if wrap { format!("({})", s) } else { s };

    match &**expr {
        ExpressionInner::Array { inner, .. } if inner.elements.is_empty() => "[]".to_string(),
        ExpressionInner::Array { .. } => "[...]".to_string(),
        ExpressionInner::ArrowFunction { inner, .. } => match &inner.body {
            ast::function::Body::BodyExpression(e)
                if matches!(e.deref(), ExpressionInner::Object { .. }) =>
            {
                do_wrap(format!("(...) => ({})", code_desc_of_expression(false, e)))
            }
            ast::function::Body::BodyExpression(e) => {
                do_wrap(format!("(...) => {}", code_desc_of_expression(false, e)))
            }
            _ => do_wrap("(...) => { ... }".to_string()),
        },
        ExpressionInner::AsConstExpression { inner, .. } => {
            code_desc_of_expression(wrap, &inner.expression)
        }
        ExpressionInner::AsExpression { inner, .. } => {
            code_desc_of_expression(wrap, &inner.expression)
        }
        ExpressionInner::Assignment { inner, .. } => {
            let left = code_desc_of_pattern(&inner.left);
            let right = code_desc_of_expression(false, &inner.right);
            let operator = match &inner.operator {
                Some(ast::expression::AssignmentOperator::PlusAssign) => "+=",
                Some(ast::expression::AssignmentOperator::MinusAssign) => "-=",
                Some(ast::expression::AssignmentOperator::MultAssign) => "*=",
                Some(ast::expression::AssignmentOperator::ExpAssign) => "**=",
                Some(ast::expression::AssignmentOperator::DivAssign) => "/=",
                Some(ast::expression::AssignmentOperator::ModAssign) => "%=",
                Some(ast::expression::AssignmentOperator::LShiftAssign) => "<<=",
                Some(ast::expression::AssignmentOperator::RShiftAssign) => ">>=",
                Some(ast::expression::AssignmentOperator::RShift3Assign) => ">>>=",
                Some(ast::expression::AssignmentOperator::BitOrAssign) => "|=",
                Some(ast::expression::AssignmentOperator::BitXorAssign) => "^=",
                Some(ast::expression::AssignmentOperator::BitAndAssign) => "&=",
                Some(ast::expression::AssignmentOperator::AndAssign) => "&&=",
                Some(ast::expression::AssignmentOperator::OrAssign) => "||=",
                Some(ast::expression::AssignmentOperator::NullishAssign) => "??=",
                None => "=",
            };
            do_wrap(format!("{} {} {}", left, operator, right))
        }
        ExpressionInner::Binary { inner, .. } => do_wrap(code_desc_of_operation(
            &inner.left,
            inner.operator,
            &inner.right,
        )),
        ExpressionInner::Call { inner, .. } => {
            let targs = match &inner.targs {
                None => "",
                Some(args) if args.arguments.is_empty() => "<>",
                Some(_) => "<...>",
            };
            let args = if inner.arguments.arguments.is_empty() {
                "()"
            } else {
                "(...)"
            };
            format!(
                "{}{}{}",
                code_desc_of_expression(true, &inner.callee),
                targs,
                args
            )
        }
        ExpressionInner::Class { .. } => "class { ... }".to_string(),
        ExpressionInner::Conditional { inner, .. } => {
            let wrap_test = matches!(&*inner.test, ExpressionInner::Conditional { .. });
            do_wrap(format!(
                "{} ? {} : {}",
                code_desc_of_expression(wrap_test, &inner.test),
                code_desc_of_expression(false, &inner.consequent),
                code_desc_of_expression(false, &inner.alternate)
            ))
        }
        ExpressionInner::Function { .. } => "function () { ... }".to_string(),
        ExpressionInner::Identifier { inner, .. } => inner.name.to_string(),
        ExpressionInner::Import { inner, .. } => {
            format!(
                "import({})",
                code_desc_of_expression(false, &inner.argument)
            )
        }
        ExpressionInner::JSXElement { inner, .. } => code_desc_of_jsx_element(inner),
        ExpressionInner::JSXFragment { .. } => "<>...</>".to_string(),
        ExpressionInner::StringLiteral { inner, .. } if inner.value.chars().count() > 16 => {
            let truncated: String = inner.value.chars().take(10).collect();
            format!("'{}...'", truncated)
        }
        ExpressionInner::StringLiteral { inner, .. } => inner.raw.to_string(),
        ExpressionInner::NumberLiteral { inner, .. } => inner.raw.to_string(),
        ExpressionInner::BooleanLiteral { inner, .. } => {
            if inner.value {
                "true".to_string()
            } else {
                "false".to_string()
            }
        }
        ExpressionInner::NullLiteral { .. } => "null".to_string(),
        ExpressionInner::BigIntLiteral { inner, .. } => inner.raw.to_string(),
        ExpressionInner::RegExpLiteral { inner, .. } => inner.raw.clone(),
        ExpressionInner::ModuleRefLiteral { inner, .. } => inner.raw.to_string(),
        ExpressionInner::Logical { inner, .. } => do_wrap(code_desc_of_logical(
            &inner.left,
            &inner.operator,
            &inner.right,
        )),
        ExpressionInner::Match { .. } => "match".to_string(),
        ExpressionInner::Member { inner, .. } => {
            let o = code_desc_of_expression(true, &inner.object);
            let p = code_desc_of_property(
                ast::expression::OptionalMemberKind::NonOptional,
                &inner.property,
            );
            format!("{}{}", o, p)
        }
        ExpressionInner::MetaProperty { inner, .. } => {
            format!("{}.{}", inner.meta.name, inner.property.name)
        }
        ExpressionInner::New { inner, .. } => {
            let targs = match &inner.targs {
                None => "",
                Some(args) if args.arguments.is_empty() => "<>",
                Some(_) => "<...>",
            };
            let args = match &inner.arguments {
                None => "",
                Some(args) if args.arguments.is_empty() => "()",
                Some(_) => "(...)",
            };
            do_wrap(format!(
                "new {}{}{}",
                code_desc_of_expression(true, &inner.callee),
                targs,
                args
            ))
        }
        ExpressionInner::Object { .. } => "{...}".to_string(),
        ExpressionInner::Record { inner, .. } => {
            let targs = match &inner.targs {
                None => "",
                Some(args) if args.arguments.is_empty() => "<>",
                Some(_) => "<...>",
            };
            do_wrap(format!(
                "{}{} {{...}}",
                code_desc_of_expression(true, &inner.constructor),
                targs
            ))
        }
        ExpressionInner::OptionalCall { inner, .. } => {
            let targ_string = match &inner.call.targs {
                None => "",
                Some(args) if args.arguments.is_empty() => "<>",
                Some(_) => "<...>",
            };
            let arg_string = if inner.call.arguments.arguments.is_empty() {
                "()"
            } else {
                "(...)"
            };
            let optional_str = match inner.optional {
                ast::expression::OptionalCallKind::NonOptional => "",
                ast::expression::OptionalCallKind::Optional => "?.",
                ast::expression::OptionalCallKind::AssertNonnull { .. } => "!",
            };
            format!(
                "{}{}{}{}",
                code_desc_of_expression(true, &inner.call.callee),
                optional_str,
                targ_string,
                arg_string
            )
        }
        ExpressionInner::OptionalMember { inner, .. } => {
            let o = code_desc_of_expression(true, &inner.member.object);
            let p = code_desc_of_property(inner.optional.dupe(), &inner.member.property);
            format!("{}{}", o, p)
        }
        ExpressionInner::Sequence { inner, .. } => {
            code_desc_of_expression(wrap, inner.expressions.last().unwrap())
        }
        ExpressionInner::Super { .. } => "super".to_string(),
        ExpressionInner::TaggedTemplate { inner, .. } => {
            format!("{}`...`", code_desc_of_expression(true, &inner.tag))
        }
        ExpressionInner::TemplateLiteral { .. } => "`...`".to_string(),
        ExpressionInner::This { .. } => "this".to_string(),
        ExpressionInner::TSSatisfies { inner, .. } => {
            code_desc_of_expression(wrap, &inner.expression)
        }
        ExpressionInner::TypeCast { inner, .. } => code_desc_of_expression(wrap, &inner.expression),
        ExpressionInner::Unary { inner, .. } => {
            let x = code_desc_of_expression(true, &inner.argument);
            let (l, r) = match inner.operator {
                ast::expression::UnaryOperator::Minus => ("-", x.as_str()),
                ast::expression::UnaryOperator::Plus => ("+", x.as_str()),
                ast::expression::UnaryOperator::Not => ("!", x.as_str()),
                ast::expression::UnaryOperator::BitNot => ("~", x.as_str()),
                ast::expression::UnaryOperator::Typeof => ("typeof ", x.as_str()),
                ast::expression::UnaryOperator::Void => ("void ", x.as_str()),
                ast::expression::UnaryOperator::Delete => ("delete ", x.as_str()),
                ast::expression::UnaryOperator::Await => ("await ", x.as_str()),
                ast::expression::UnaryOperator::Nonnull => (x.as_str(), "!"),
            };
            do_wrap(format!("{}{}", l, r))
        }
        ExpressionInner::Update { inner, .. } => {
            let x = code_desc_of_expression(true, &inner.argument);
            let op = match inner.operator {
                ast::expression::UpdateOperator::Increment => "++",
                ast::expression::UpdateOperator::Decrement => "--",
            };
            do_wrap(if inner.prefix {
                format!("{}{}", op, x)
            } else {
                format!("{}{}", x, op)
            })
        }
        ExpressionInner::Yield { inner, .. } => match (&inner.argument, inner.delegate) {
            (Some(x), false) => do_wrap(format!("yield {}", code_desc_of_expression(false, x))),
            (Some(x), true) => do_wrap(format!("yield* {}", code_desc_of_expression(false, x))),
            (None, false) => "yield".to_string(),
            (None, true) => "yield*".to_string(),
        },
    }
}

pub fn code_desc_of_pattern<M: Dupe, T: Dupe>(patt: &ast::pattern::Pattern<M, T>) -> String {
    use ast::pattern::Pattern;
    match patt {
        Pattern::Object { .. } => "{...}".to_string(),
        Pattern::Array { .. } => "[...]".to_string(),
        Pattern::Identifier { inner, .. } => inner.name.name.to_string(),
        Pattern::Expression { inner, .. } => code_desc_of_expression(false, inner),
    }
}

/// Implementation of operator flattening logic lifted from Prettier:
/// https://github.com/prettier/prettier/blob/dd78f31aaf5b4522b780f13194d57308e5fdf53b/src/common/util.js#L328-L399
fn code_desc_of_operation<M: Dupe, T: Dupe>(
    left: &ast::expression::Expression<M, T>,
    op: ast::expression::BinaryOperator,
    right: &ast::expression::Expression<M, T>,
) -> String {
    let should_flatten = |a: ast::expression::BinaryOperator,
                          b: ast::expression::BinaryOperator| {
        use ast::expression::BinaryOperator::*;
        let precedence = |op: BinaryOperator| match op {
            BitOr => 2,
            Xor => 3,
            BitAnd => 4,
            Equal | NotEqual | StrictEqual | StrictNotEqual => 5,
            LessThan | LessThanEqual | GreaterThan | GreaterThanEqual | In | Instanceof => 6,
            LShift | RShift | RShift3 => 7,
            Plus | Minus => 8,
            Mult | Div | Mod => 9,
            Exp => 10,
        };

        if precedence(a) != precedence(b) {
            return false;
        }
        if matches!(a, Exp) {
            return false;
        }
        if matches!(
            (a, b),
            (
                Equal | NotEqual | StrictEqual | StrictNotEqual,
                Equal | NotEqual | StrictEqual | StrictNotEqual
            )
        ) {
            return false;
        }
        if (matches!(a, Mod) && matches!(b, Mult | Div | Mod))
            || (matches!(b, Mod) && matches!(a, Mult | Div | Mod))
        {
            return false;
        }
        if matches!(
            (a, b),
            (LShift | RShift | RShift3, LShift | RShift | RShift3)
        ) {
            return false;
        }
        true
    };

    let wrap_left = match left.deref() {
        ast::expression::ExpressionInner::Binary { inner, .. } => {
            !should_flatten(op, inner.operator)
        }
        _ => true,
    };

    let left_str = code_desc_of_expression(wrap_left, left);
    let right_str = code_desc_of_expression(true, right);
    let op_str = match op {
        ast::expression::BinaryOperator::Equal => "==",
        ast::expression::BinaryOperator::NotEqual => "!=",
        ast::expression::BinaryOperator::StrictEqual => "===",
        ast::expression::BinaryOperator::StrictNotEqual => "!==",
        ast::expression::BinaryOperator::LessThan => "<",
        ast::expression::BinaryOperator::LessThanEqual => "<=",
        ast::expression::BinaryOperator::GreaterThan => ">",
        ast::expression::BinaryOperator::GreaterThanEqual => ">=",
        ast::expression::BinaryOperator::LShift => "<<",
        ast::expression::BinaryOperator::RShift => ">>",
        ast::expression::BinaryOperator::RShift3 => ">>>",
        ast::expression::BinaryOperator::Plus => "+",
        ast::expression::BinaryOperator::Minus => "-",
        ast::expression::BinaryOperator::Mult => "*",
        ast::expression::BinaryOperator::Exp => "**",
        ast::expression::BinaryOperator::Div => "/",
        ast::expression::BinaryOperator::Mod => "%",
        ast::expression::BinaryOperator::BitOr => "|",
        ast::expression::BinaryOperator::Xor => "^",
        ast::expression::BinaryOperator::BitAnd => "&",
        ast::expression::BinaryOperator::In => "in",
        ast::expression::BinaryOperator::Instanceof => "instanceof",
    };

    format!("{} {} {}", left_str, op_str, right_str)
}

fn code_desc_of_logical<M: Dupe, T: Dupe>(
    left: &ast::expression::Expression<M, T>,
    op: &ast::expression::LogicalOperator,
    right: &ast::expression::Expression<M, T>,
) -> String {
    let should_flatten = |a: &ast::expression::LogicalOperator,
                          b: &ast::expression::LogicalOperator| {
        use ast::expression::LogicalOperator::*;
        let precedence = |op: &LogicalOperator| match op {
            Or | NullishCoalesce => 0,
            And => 1,
        };
        precedence(a) == precedence(b)
    };

    let wrap_left = match left.deref() {
        ast::expression::ExpressionInner::Logical { inner, .. } => {
            !should_flatten(op, &inner.operator)
        }
        _ => true,
    };

    let left_str = code_desc_of_expression(wrap_left, left);
    let right_str = code_desc_of_expression(true, right);
    let op_str = match op {
        ast::expression::LogicalOperator::Or => "||",
        ast::expression::LogicalOperator::And => "&&",
        ast::expression::LogicalOperator::NullishCoalesce => "??",
    };

    format!("{} {} {}", left_str, op_str, right_str)
}

fn code_desc_of_jsx_element<M: Dupe, T: Dupe>(elem: &ast::jsx::Element<M, T>) -> String {
    use ast::jsx::*;
    match &elem.opening_element.name {
        Name::Identifier(id) => format!("<{} />", id.name),
        Name::NamespacedName(ns) => {
            format!("<{}:{} />", ns.namespace.name, ns.name.name)
        }
        Name::MemberExpression(mem) => {
            fn loop_member<M: Dupe, T: Dupe>(mem: &ast::jsx::MemberExpression<M, T>) -> String {
                match &mem.object {
                    member_expression::Object::Identifier(id) => {
                        format!("{}.{}", id.name, mem.property.name)
                    }
                    member_expression::Object::MemberExpression(inner) => {
                        format!("{}.{}", loop_member(inner), mem.property.name)
                    }
                }
            }
            format!("<{} />", loop_member(mem))
        }
    }
}

fn code_desc_of_property<M: Dupe, T: Dupe>(
    optional: ast::expression::OptionalMemberKind<M>,
    property: &ast::expression::member::Property<M, T>,
) -> String {
    use ast::expression::OptionalMemberKind;
    use ast::expression::member::Property;

    match property {
        Property::PropertyIdentifier(id) => {
            let prefix = match optional {
                OptionalMemberKind::Optional => "?.",
                OptionalMemberKind::NonOptional => ".",
                OptionalMemberKind::AssertNonnull { .. } => "!.",
            };
            format!("{}{}", prefix, id.name)
        }
        Property::PropertyPrivateName(priv_name) => {
            let prefix = match optional {
                OptionalMemberKind::Optional => "?.#",
                OptionalMemberKind::NonOptional => ".#",
                OptionalMemberKind::AssertNonnull { .. } => "!.#",
            };
            format!("{}{}", prefix, priv_name.name)
        }
        Property::PropertyExpression(expr) => {
            let prefix = match optional {
                OptionalMemberKind::Optional => "?.[",
                OptionalMemberKind::NonOptional => "[",
                OptionalMemberKind::AssertNonnull { .. } => "![",
            };
            format!("{}{}]", prefix, code_desc_of_expression(false, expr))
        }
    }
}

pub fn mk_expression_reason<M: Dupe + PartialEq, T: Dupe + PartialEq>(
    expr: &ast::expression::Expression<M, T>,
) -> VirtualReason<T> {
    match expr.deref() {
        ExpressionInner::TypeCast { loc, inner } => {
            let inner_reason = mk_expression_reason(&inner.expression);
            inner_reason.reposition(loc.dupe())
        }
        ExpressionInner::Object { loc, .. } => mk_reason(VirtualReasonDesc::RObjectLit, loc.dupe()),
        ExpressionInner::Array { loc, .. } => mk_reason(VirtualReasonDesc::RArrayLit, loc.dupe()),
        ExpressionInner::ArrowFunction { loc, inner } => {
            let desc = if inner.async_ {
                VirtualReasonDesc::RFunction(ReasonDescFunction::RAsync)
            } else {
                VirtualReasonDesc::RFunction(ReasonDescFunction::RNormal)
            };
            mk_reason(desc, loc.dupe())
        }
        ExpressionInner::Function { loc, inner } => {
            let desc = if inner.async_ {
                if inner.generator {
                    VirtualReasonDesc::RFunction(ReasonDescFunction::RAsyncGenerator)
                } else {
                    VirtualReasonDesc::RFunction(ReasonDescFunction::RAsync)
                }
            } else if inner.generator {
                VirtualReasonDesc::RFunction(ReasonDescFunction::RGenerator)
            } else {
                VirtualReasonDesc::RFunction(ReasonDescFunction::RNormal)
            };
            mk_reason(desc, loc.dupe())
        }
        ExpressionInner::StringLiteral { loc, inner } if inner.value.is_empty() => {
            mk_reason(VirtualReasonDesc::RStringLit("".into()), loc.dupe())
        }
        ExpressionInner::Member { loc, inner } => {
            let object_ = code_desc_of_expression(true, &inner.object).into();
            let property = code_desc_of_property(
                ast::expression::OptionalMemberKind::NonOptional,
                &inner.property,
            )
            .into();
            mk_reason(VirtualReasonDesc::RMember { object_, property }, loc.dupe())
        }
        _ => {
            let code = code_desc_of_expression(false, expr).into();
            let loc = expr.loc();
            mk_reason(VirtualReasonDesc::RCode(code), loc.dupe())
        }
    }
}

pub fn mk_typed_expression_reason<M: Dupe + PartialEq, T: Dupe>(
    expr: &ast::expression::Expression<M, (M, T)>,
) -> VirtualReason<M> {
    // For typed expressions, we extract the M type from the (M, T) location pair
    match expr.deref() {
        ExpressionInner::TypeCast { loc, inner } => {
            let inner_reason = mk_typed_expression_reason(&inner.expression);
            inner_reason.reposition(loc.0.dupe())
        }
        ExpressionInner::Object { loc, .. } => {
            mk_reason(VirtualReasonDesc::RObjectLit, loc.0.dupe())
        }
        ExpressionInner::Array { loc, .. } => mk_reason(VirtualReasonDesc::RArrayLit, loc.0.dupe()),
        ExpressionInner::ArrowFunction { loc, inner } => {
            let desc = if inner.async_ {
                VirtualReasonDesc::RFunction(ReasonDescFunction::RAsync)
            } else {
                VirtualReasonDesc::RFunction(ReasonDescFunction::RNormal)
            };
            mk_reason(desc, loc.0.dupe())
        }
        ExpressionInner::Function { loc, inner } => {
            let desc = if inner.async_ {
                if inner.generator {
                    VirtualReasonDesc::RFunction(ReasonDescFunction::RAsyncGenerator)
                } else {
                    VirtualReasonDesc::RFunction(ReasonDescFunction::RAsync)
                }
            } else if inner.generator {
                VirtualReasonDesc::RFunction(ReasonDescFunction::RGenerator)
            } else {
                VirtualReasonDesc::RFunction(ReasonDescFunction::RNormal)
            };
            mk_reason(desc, loc.0.dupe())
        }
        ExpressionInner::StringLiteral { loc, inner } if inner.value.is_empty() => {
            mk_reason(VirtualReasonDesc::RStringLit("".into()), loc.0.dupe())
        }
        ExpressionInner::Member { loc, inner } => {
            let object_ = code_desc_of_expression(true, &inner.object).into();
            let property = code_desc_of_property(
                ast::expression::OptionalMemberKind::NonOptional,
                &inner.property,
            )
            .into();
            mk_reason(
                VirtualReasonDesc::RMember { object_, property },
                loc.0.dupe(),
            )
        }
        _ => {
            let code = code_desc_of_expression(false, expr).into();
            let loc = &expr.loc().0;
            mk_reason(VirtualReasonDesc::RCode(code), loc.dupe())
        }
    }
}

pub fn mk_initial_arguments_reason<M: Dupe + PartialEq, T: Dupe + PartialEq>(
    arg_list: &ast::expression::ArgList<M, T>,
) -> Vec<VirtualReason<T>> {
    use ast::expression::ExpressionOrSpread;
    let mut result = Vec::new();
    for arg in arg_list.arguments.iter() {
        match arg {
            ExpressionOrSpread::Expression(expr) => {
                result.push(mk_expression_reason(expr));
            }
            ExpressionOrSpread::Spread(_) => {
                // Stop collecting at spread
                break;
            }
        }
    }
    result
}

pub fn mk_pattern_reason<M: Dupe, T: Dupe>(patt: &ast::pattern::Pattern<M, T>) -> VirtualReason<T> {
    let code = code_desc_of_pattern(patt).into();
    let loc = patt.loc().dupe();
    mk_reason(VirtualReasonDesc::RCode(code), loc)
}

/// Classifies a reason description. These classifications can be used to
/// implement various asthetic behaviors in error messages when we would like to
/// distinguish between different error "classes".
///
/// The classifications we currently support:
///
/// - `Scalar`: The type *cannot* recursively hold any other types. For example,
///   number is a scalar but an object like {p: number} is not.
/// - `Nullish`: The type is null or undefined. Nullish types are also `Scalar`.
/// - `Array`: The type is an array. This depends on Flow's custom implementation
///   of arrays and tuples.
/// - `Unclassified`: Everything else which hasn't been classified yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReasonClassification {
    Scalar,
    Nullish,
    Array,
    Unclassified,
}

pub fn is_scalar_reason_desc<L: Dupe>(desc: &VirtualReasonDesc<L>) -> bool {
    desc.is_scalar()
}

pub fn react_element_desc_of_component_reason<L: Dupe>(
    reason: &VirtualReason<L>,
) -> VirtualReasonDesc<L> {
    match reason.desc(false) {
        Some(VirtualReasonDesc::RComponent(name)) => VirtualReasonDesc::RReactElement {
            name_opt: Some(Name::new(name.dupe())),
            from_component_syntax: true,
        },
        _ => VirtualReasonDesc::RReactElement {
            name_opt: None,
            from_component_syntax: false,
        },
    }
}

pub fn range_string_of_loc(strip_root: Option<&str>, loc: &Loc) -> String {
    let file = match &loc.source {
        Some(file) => string_of_source(strip_root, file),
        None => String::new(),
    };

    let (l0, c0) = (loc.start.line, loc.start.column + 1);
    let (l1, c1) = (loc.end.line, loc.end.column);

    format!("{}:{}:{},{}:{}", file, l0, c0, l1, c1)
}

pub type ReasonMap<V> = BTreeMap<ConcreteReason, V>;

pub type ReasonSet = BTreeSet<ConcreteReason>;
