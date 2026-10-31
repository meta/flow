/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::hash::Hash;
use std::str::FromStr;
use std::sync::Arc;

use dupe::Dupe;
pub use flow_common::error_ref::ExpressionFunctionKind;
pub use flow_common::error_ref::ExpressionReferenceData;
pub use flow_common::error_ref::ExpressionReferenceKind;
pub use flow_common::error_ref::FunctionReferenceKind;
use flow_common::flow_import_specifier::Userland;
use flow_common::polarity::Polarity;
use flow_common::reason::Name;
use flow_common::reason::VirtualReason;
use flow_common::reason::VirtualReasonDesc;
use flow_common::refinement_invalidation;
use flow_common_errors::error_codes::ErrorCode;
use flow_common_errors::error_utils::ErrorKind;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_env_builder::env_api::AnnotLoc;
use flow_env_builder::name_def_types::DefinitionReferenceKind;
use flow_lint_settings::lint_settings::LintParseError;
use flow_lint_settings::lints::PropertyAssignmentKind;
use flow_lint_settings::lints::SketchyNullKind;
use flow_parser::ast;
use flow_parser::ast::VariableKind;
use flow_parser::ast::types::RendersVariant;
use flow_parser::ast::types::TypeGuardKind;
use flow_parser::file_key::FileKey;
use flow_parser::loc::Loc;
use flow_parser::parse_error::ParseError;
use flow_parser_utils::graphql::GraphqlError;
use flow_type_sig::signature_error::SignatureError;
use flow_typing_type::type_::ImplicitInstantiationReferenceKind;
use flow_typing_type::type_::MergedDeclarationConflict;
use flow_typing_type::type_::UnionEnum;
use flow_typing_type::type_::aconstraint::AnnotationInferenceOperation;
use flow_typing_type::type_::type_or_type_desc::ErrorTypeDesc;
use flow_typing_type::type_::type_or_type_desc::TypeOrTypeDescT as TypeOrTypeDesc;
use flow_typing_type::type_::union_rep::OptimizedError;
use vec1::Vec1;

use crate::error_message::CallTypeArityCallee;
use crate::error_message::DefinitionReferenceData;
use crate::error_message::EnumReferenceData;
use crate::error_message::MatchExampleReference;
use crate::error_message::TypeGuardBindingKind;
use crate::error_message::TypeGuardFuncReference;
use crate::error_message::TypeGuardParameterData;
use crate::error_message::UninitializedPropertyKind;

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
pub enum AssignedConstLikeBindingType {
    ClassNameBinding,
    FunctionNameBinding,
    DeclaredFunctionNameBinding,
    ComponentNameBinding,
    RecordNameBinding,
}

impl AssignedConstLikeBindingType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ClassNameBinding => "class",
            Self::FunctionNameBinding => "function",
            Self::DeclaredFunctionNameBinding => "declared function",
            Self::ComponentNameBinding => "component",
            Self::RecordNameBinding => "record",
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
pub enum DocblockError {
    MultipleFlowAttributes,
    InvalidFlowMode(FlowSmolStr),
    MultipleJSXAttributes,
    InvalidJSXAttribute(Option<FlowSmolStr>),
    MultipleJSXRuntimeAttributes,
    InvalidJSXRuntimeAttribute,
    InvalidSupportsPlatform(FlowSmolStr),
    DisallowedSupportsPlatform,
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
pub enum ExactnessErrorKind {
    UnexpectedIndexer,
    UnexpectedInexact,
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
pub enum DeclareComponentInvalidParamKind {
    DeclareComponentParamAsBinding,
    DeclareComponentParamDefaultValue,
    DeclareComponentParamMissingAnnotation,
    DeclareComponentParamStringLiteralWithoutAs,
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
pub enum ExpectedModulePurpose {
    ReactModuleForJSXFragment,
    ReactModuleForReactClassComponent,
    ReactModuleForReactMixedElementType,
    ReactModuleForReactNodeType,
    ReactModuleForReactRefSetterType,
    ReactModuleForReactElementRefType,
    ReactModuleForReactManagedAttributesType,
}

/// The first lower bound that reached one operand of an exponential spread, and the most recent
/// later one, if any.
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
pub struct ExponentialSpreadOperandGroup<T> {
    pub first: T,
    pub second: Option<T>,
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
pub enum ContextDependentUnsupportedStatement {
    ToplevelLibraryImport,
    NonLibdefToplevelDeclareModule,
    UnsupportedStatementInLibdef(FlowSmolStr),
    UnsupportedStatementInDeclareModule(FlowSmolStr),
    UnsupportedStatementInDeclareNamespace(FlowSmolStr),
    UnsupportedStatementInDeclareGlobal(FlowSmolStr),
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
pub enum InternalType {
    DollarReactDeepReadOnly,
    DollarUtilityTypeWithNonDollarAliases(FlowSmolStr),
    ReactDollarUtilityTypesWithNonDollarAliases(FlowSmolStr),
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
pub enum UnsupportedSyntax {
    AnnotationInsideDestructuring,
    AsConstOnNonLiteral,
    ExistsType,
    MetaPropertyExpression,
    ObjectPropertyGetSet,
    ObjectPropertyComputedGetSet,
    ClassPropertyLiteral,
    ClassStaticBlock,
    ClassDeclareMethod,
    ClassIndexSignature,
    RequireDynamicArgument,
    CatchParameterDeclaration,
    DestructuringObjectPropertyInvalidLiteral,
    DestructuringExpressionPattern,
    PredicateFunction,
    MatchExpression,
    MatchStatement,
    MatchInstancePattern,
    MultipleIndexers,
    MultipleProtos,
    ExplicitCallAfterProto,
    ExplicitProtoAfterCall,
    SpreadArgument,
    ImportDynamic,
    ImportDynamicArgument,
    IllegalName,
    UserDefinedTypeGuards { kind: TypeGuardKind },
    UnsupportedInternalSlot { name: FlowSmolStr, static_: bool },
    ContextDependentUnsupportedStatement(ContextDependentUnsupportedStatement),
    WithStatement,
    ComponentSyntax,
    AsyncComponentSyntax,
    AsyncHookSyntax,
    DeclareGlobal,
    NonnullAssertion,
    Records,
    DeclareClassMethodMissingReturnType,
    DeclareVariableNonLiteralInit,
    DeclareVariableDestructuring,
    DeclareVariableMissingAnnotationOrInit,
    DeclareVariableAnnotationAndInit,
    DeclareClassProperty(DeclareClassPropKind),
    TSLibSyntax(TsLibSyntaxKind),
    NamespaceExportDeclaration(NamespaceExportDeclarationKind),
    ExportTypeSpecifierInExportType,
    ExportDefaultInterface,
    UniqueSymbolPosition,
    UniqueSymbolNotConst,
    UniqueSymbolNotReadOnly,
    UniqueSymbolNotStaticReadOnly,
    UniqueSymbolLoopBinding,
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
pub enum NamespaceExportDeclarationKind {
    NotTopLevel,
    NotInModule,
    NotInDeclarationFile,
    ModuleWithImports,
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
pub enum DeclareClassPropKind {
    AnnotationAndInit,
    MissingAnnotationOrInit,
    NonLiteralInit,
    InitWithoutReadonly,
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
pub enum TsLibSyntaxKind {
    DeclarationWithoutDeclare,
    ImportTypeAnnotation,
    DeclareExportNamespace,
    ExportAssignment,
    ExportTypeSpecifier,
    ImportEqualsDeclaration,
    ImportEqualsQualifiedName,
    DeclareVariableMultipleDeclarators,
    DeclareVariableLiteralInit,
    TemplateLiteralType,
    ConstructorType,
    UniqueSymbolType,
    TypeofImport,
    ImplementsDottedPath,
    OptionalClassProperty,
    OptionalShorthandMethod,
    AnonymousDefaultExportFunction,
    MappedTypeKeyRemapping,
    MappedTypeKeyRemappingOnArraySource,
    ReadonlyMappedTypeVarianceOp,
    OptionalUnlabeledTupleElement,
    OptionalIndexer,
    NamespaceExportDeclaration,
    PrivateClassField,
    GenericTaggedTemplate,
    TypeofThis,
    PropertyValueInitializer,
    ClassExtendsCall,
    OverrideModifier,
    Satisfies,
}

// A TypeScript enum member that Flow parses but TypeScript rejects.
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
pub enum TsEnumInvalidMemberKind {
    // A boolean or bigint initializer (TS enum members must be number or string).
    TSEnumMemberInvalidLiteral,
    // A defaulted member that cannot be auto-numbered (its predecessor is not a
    // numeric constant) in a non-ambient enum.
    TSEnumMemberMissingInitializer,
    // A member with a numeric name, e.g. `enum E { "1" = 5 }` (TS2452).
    TSEnumMemberNumericName,
}

// Enum-level syntax that Flow Enums allow but a TypeScript enum (in a .ts/.d.ts
// file) does not.
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
pub enum TsEnumInvalidSyntaxKind {
    // Unknown members (`...`).
    TSEnumUnknownMembers,
    // An explicit representation type (`enum E of string {...}`).
    TSEnumExplicitType,
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
pub enum SubComponentOfInvariantSubtypingError {
    ObjectProps(Vec<Name>),
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
pub enum MatchInvalidCaseSyntax<L: Dupe> {
    InvalidMatchCaseMultiple {
        invalid_prefix_case_locs: Vec<L>,
        invalid_infix_colon_locs: Vec<L>,
        invalid_suffix_semicolon_locs: Vec<L>,
    },
    InvalidMatchCasePrefixCase,
    InvalidMatchCaseInfixColon,
    InvalidMatchCaseSuffixSemicolon,
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
pub enum RecordDeclarationInvalidSyntax<L: Dupe> {
    InvalidRecordDeclarationSyntaxMultiple {
        invalid_infix_equals_loc: Option<L>,
        invalid_variance_locs: Vec<L>,
        invalid_optional_locs: Vec<L>,
        invalid_suffix_semicolon_locs: Vec<L>,
    },
    InvalidRecordDeclarationSyntaxVariance,
    InvalidRecordDeclarationSyntaxOptional,
    InvalidRecordDeclarationSyntaxSuffixSemicolon,
    InvalidRecordDeclarationSyntaxInfixEquals,
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
pub enum AbstractErrorKind<L: Dupe> {
    AbstractClassInstantiation,
    AbstractMemberNotImplemented {
        class_name: Option<FlowSmolStr>,
        member_name: FlowSmolStr,
        member_def_loc: L,
    },
    AbstractMemberOnNonAbstractClass {
        member_name: FlowSmolStr,
    },
    AbstractPrivateMember {
        member_name: FlowSmolStr,
    },
    AbstractSuperCall {
        member_name: FlowSmolStr,
    },
    AbstractConstructorAssignedToNonAbstract,
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
pub enum OverrideErrorKind<L: Dupe> {
    OverrideWithoutExtends {
        class_name: Option<FlowSmolStr>,
        member_name: FlowSmolStr,
    },
    OverrideOfNonInheritedMember {
        class_name: Option<FlowSmolStr>,
        base_class_name: Option<FlowSmolStr>,
        member_name: FlowSmolStr,
    },
    ImplicitOverrideMissingModifier {
        class_name: Option<FlowSmolStr>,
        base_class_name: Option<FlowSmolStr>,
        member_name: FlowSmolStr,
        inherited_def_loc: L,
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
pub enum InvalidRenderTypeKind<T> {
    InvalidRendersNullVoidFalse,
    InvalidRendersIterable,
    InvalidRendersStructural(T),
    InvalidRendersNonNominalElement(T),
    InvalidRendersGenericT,
    UncategorizedInvalidRenders,
}

#[derive(
    Debug,
    Clone,
    Dupe,
    Copy,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    serde::Serialize,
    serde::Deserialize
)]
pub enum ConstantConditionKind {
    ConstCondGeneral,
    UnawaitedPromise,
    UncalledFunction,
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
pub enum ConstantConditionWarning<L: Dupe> {
    Definite,
    Likely { suggested_loc: Option<L> },
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
pub enum StrictComparisonInfo<L, T> {
    General { left: T, right: T },
    Null { null_loc: L, other: T },
    Empty { empty: T },
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    serde::Serialize,
    serde::Deserialize
)]
pub enum ClassKind {
    Class,
    Record,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    serde::Serialize,
    serde::Deserialize
)]
pub enum MatchObjPatternKind {
    Object,
    Instance,
}

impl MatchObjPatternKind {
    pub fn to_string(self) -> &'static str {
        match self {
            Self::Object => "object pattern",
            Self::Instance => "instance pattern",
        }
    }
}

#[derive(
    Debug,
    Clone,
    Copy,
    Dupe,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    serde::Serialize,
    serde::Deserialize
)]
pub enum MatchPatternReferenceKind {
    Pattern,
    Wildcard,
    ArrayRest,
    ObjectRest,
}

impl MatchPatternReferenceKind {
    pub fn to_string(self) -> &'static str {
        match self {
            Self::Pattern => "match pattern",
            Self::Wildcard => "match wildcard",
            Self::ArrayRest => "rest of array pattern",
            Self::ObjectRest => "rest of object pattern",
        }
    }
}

#[derive(
    Debug,
    Clone,
    Dupe,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    serde::Serialize,
    serde::Deserialize
)]
pub struct MatchPatternReferenceData<L: Dupe> {
    pub loc: L,
    pub kind: MatchPatternReferenceKind,
}

/// The object type operation applied to a record, named after the object kit tool.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    serde::Serialize,
    serde::Deserialize
)]
pub enum RecordBannedTypeUtilOp {
    Exact,
    ReadOnly,
    Partial,
    Required,
    Omit,
    MappedType,
    Rest,
    ReactConfig,
    Object,
}

impl RecordBannedTypeUtilOp {
    pub fn to_string(self) -> &'static str {
        match self {
            Self::Exact => "`$Exact`",
            Self::ReadOnly => "`Readonly`",
            Self::Partial => "`Partial`",
            Self::Required => "`Required`",
            Self::Omit => "`Omit`",
            Self::MappedType => "mapped type",
            Self::Rest => "rest",
            Self::ReactConfig => "React config",
            Self::Object => "object",
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
pub enum IncorrectType {
    Partial,
    Shape,
    TSReadonly,
    TSReadonlyArray,
    TSReadonlyMap,
    TSReadonlySet,
    TSNonNullable,
    Values,
    DollarNonMaybeType,
    DollarReadOnly,
    DollarReadOnlyArray,
    DollarReadOnlyMap,
    DollarReadOnlySet,
    DollarReadOnlyWeakMap,
    DollarReadOnlyWeakSet,
    DollarKeys,
    DollarValues,
    Mixed,
}

impl IncorrectType {
    pub fn is_type_reserved(self) -> bool {
        matches!(
            self,
            Self::DollarNonMaybeType
                | Self::TSNonNullable
                | Self::DollarReadOnly
                | Self::TSReadonly
                | Self::DollarKeys
                | Self::Values
                | Self::DollarValues
        )
    }

    pub fn incorrect_of_kind(self) -> &'static str {
        match self {
            Self::Partial => "$Partial",
            Self::Shape => "$Shape",
            Self::TSReadonly => "Readonly",
            Self::TSReadonlyArray => "ReadonlyArray",
            Self::TSReadonlyMap => "ReadonlyMap",
            Self::TSReadonlySet => "ReadonlySet",
            Self::TSNonNullable => "NonNullable",
            Self::Values => "Values",
            Self::DollarNonMaybeType => "$NonMaybeType",
            Self::DollarReadOnly => "$ReadOnly",
            Self::DollarReadOnlyArray => "$ReadOnlyArray",
            Self::DollarReadOnlyMap => "$ReadOnlyMap",
            Self::DollarReadOnlySet => "$ReadOnlySet",
            Self::DollarReadOnlyWeakMap => "$ReadOnlyWeakMap",
            Self::DollarReadOnlyWeakSet => "$ReadOnlyWeakSet",
            Self::DollarKeys => "$Keys",
            Self::DollarValues => "$Values",
            Self::Mixed => "mixed",
        }
    }

    pub fn replacement_of_kind(self) -> &'static str {
        match self {
            Self::Partial => "Partial",
            Self::Shape => "Partial",
            Self::TSReadonly => "Readonly",
            Self::TSReadonlyArray => "ReadonlyArray",
            Self::TSReadonlyMap => "ReadonlyMap",
            Self::TSReadonlySet => "ReadonlySet",
            Self::TSNonNullable => "NonNullable",
            Self::Values => "Values",
            Self::DollarNonMaybeType => "NonNullable",
            Self::DollarReadOnly => "Readonly",
            Self::DollarReadOnlyArray => "ReadonlyArray",
            Self::DollarReadOnlyMap => "ReadonlyMap",
            Self::DollarReadOnlySet => "ReadonlySet",
            Self::DollarReadOnlyWeakMap => "ReadonlyWeakMap",
            Self::DollarReadOnlyWeakSet => "ReadonlyWeakSet",
            Self::DollarKeys => "keyof",
            Self::DollarValues => "Values",
            Self::Mixed => "unknown",
        }
    }

    pub fn error_type_of_kind(self) -> IncorrectTypeErrorType {
        match self {
            Self::Partial
            | Self::Shape
            | Self::DollarNonMaybeType
            | Self::DollarReadOnly
            | Self::DollarReadOnlyArray
            | Self::DollarReadOnlyMap
            | Self::DollarReadOnlySet
            | Self::DollarReadOnlyWeakMap
            | Self::DollarReadOnlyWeakSet
            | Self::DollarKeys
            | Self::DollarValues
            | Self::Mixed => IncorrectTypeErrorType::DeprecatedUtility,
            Self::TSReadonly
            | Self::TSReadonlyArray
            | Self::TSReadonlyMap
            | Self::TSReadonlySet
            | Self::TSNonNullable
            | Self::Values => IncorrectTypeErrorType::TSType,
        }
    }
}

impl FromStr for IncorrectType {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "$NonMaybeType" => Ok(Self::DollarNonMaybeType),
            "NonNullable" => Ok(Self::TSNonNullable),
            "$ReadOnly" => Ok(Self::DollarReadOnly),
            "Readonly" => Ok(Self::TSReadonly),
            "$Keys" => Ok(Self::DollarKeys),
            "Values" => Ok(Self::Values),
            "$Values" => Ok(Self::DollarValues),
            _ => Err(()),
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
pub enum IncorrectTypeErrorType {
    DeprecatedUtility,
    TSType,
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
pub enum InvalidObjKey {
    Other,
    /// The nullish part of a maybe type, kept together after concretization.
    NullOrVoid,
    NumberNonInt,
    NumberTooLarge,
    NumberTooSmall,
    /// A computed key whose type is not a single literal, so it names no one
    /// property.
    ComputedNotLiteral,
    /// A computed key whose name also names a type, which is almost always an
    /// index signature written without a label.
    ComputedTypeName,
}

impl InvalidObjKey {
    pub fn kind_of_num_value(value: f64) -> Self {
        use flow_common::js_number::MAX_SAFE_INTEGER;
        use flow_common::js_number::MIN_SAFE_INTEGER;

        if value.fract() != 0.0 {
            Self::NumberNonInt
        } else if value > MAX_SAFE_INTEGER {
            Self::NumberTooLarge
        } else if value < MIN_SAFE_INTEGER {
            Self::NumberTooSmall
        } else {
            Self::Other
        }
    }

    pub fn str_of_kind(self) -> &'static str {
        match self {
            Self::Other => "other",
            Self::NullOrVoid => "null or void",
            Self::NumberNonInt => "number non-int",
            Self::NumberTooLarge => "number too large",
            Self::NumberTooSmall => "number too small",
            Self::ComputedNotLiteral => "computed not literal",
            Self::ComputedTypeName => "computed type name",
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
pub enum ExplanationWithLazyParts<L: Dupe> {
    LazyExplanationInvariantSubtypingDueToMutableArray {
        lower_array_loc: L,
        upper_array_loc: L,
        lower_array_desc: TypeOrTypeDesc<L>,
        upper_array_desc: TypeOrTypeDesc<L>,
        upper_array_reference_loc: L,
    },
    LazyExplanationInvariantSubtypingDueToMutableProperty {
        lower_obj_loc: L,
        upper_obj_loc: L,
        lower_obj_desc: TypeOrTypeDesc<L>,
        upper_obj_desc: TypeOrTypeDesc<L>,
        upper_object_loc: L,
        property_name: Option<FlowSmolStr>,
    },
    LazyExplanationInvariantSubtypingDueToMutableProperties {
        lower_obj_loc: L,
        upper_obj_loc: L,
        lower_obj_desc: TypeOrTypeDesc<L>,
        upper_obj_desc: TypeOrTypeDesc<L>,
        upper_object_loc: L,
        properties: Vec<Name>,
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
pub struct ExplanationConstrainedAssignData<L: Dupe> {
    pub name: FlowSmolStr,
    pub declaration: L,
    pub providers: Arc<[L]>,
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
pub struct ExplanationCustomErrorData<L: Dupe> {
    pub name: FlowSmolStr,
    pub custom_error_loc: L,
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
pub struct ExplanationInvariantSubtypingDueToMutableArrayData<L: Dupe> {
    pub lower_array_loc: L,
    pub upper_array_loc: L,
    pub lower_array_desc: ErrorTypeDesc<L>,
    pub upper_array_desc: ErrorTypeDesc<L>,
    pub upper_array_reference_loc: L,
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
pub struct ExplanationInvariantSubtypingDueToMutablePropertyData<L: Dupe> {
    pub lower_obj_loc: L,
    pub upper_obj_loc: L,
    pub lower_obj_desc: ErrorTypeDesc<L>,
    pub upper_obj_desc: ErrorTypeDesc<L>,
    pub upper_object_loc: L,
    pub property_name: Option<FlowSmolStr>,
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
pub struct ExplanationInvariantSubtypingDueToMutablePropertiesData<L: Dupe> {
    pub lower_obj_loc: L,
    pub upper_obj_loc: L,
    pub lower_obj_desc: ErrorTypeDesc<L>,
    pub upper_obj_desc: ErrorTypeDesc<L>,
    pub upper_object_loc: L,
    pub properties: Vec<Name>,
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
pub struct ExplanationPropertyMissingDueToNeutralOptionalPropertyData<L: Dupe> {
    pub props_plural: bool,
    pub lower_obj_loc: L,
    pub upper_obj_loc: L,
    pub lower_obj_desc: ErrorTypeDesc<L>,
    pub upper_obj_desc: ErrorTypeDesc<L>,
    pub upper_object_loc: L,
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
pub struct ExplanationAdditionalUnionMembersData<L: Dupe> {
    pub left_loc: L,
    pub left_desc: TypeOrTypeDesc<L>,
    pub right_loc: L,
    pub right_desc: TypeOrTypeDesc<L>,
    pub members: Vec<FlowSmolStr>,
    pub extra_number: i32,
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
pub enum Explanation<L: Dupe> {
    ExplanationAbstractEnumCasting,
    ExplanationArrayInvariantTyping,
    ExplanationConstrainedAssign(Box<ExplanationConstrainedAssignData<L>>),
    ExplanationConcreteEnumCasting {
        representation_type: FlowSmolStr,
    },
    ExplanationCustomError(Box<ExplanationCustomErrorData<L>>),
    ExplanationFunctionsWithStaticsToObject,
    ExplanationInvariantSubtypingDueToMutableArray(
        Box<ExplanationInvariantSubtypingDueToMutableArrayData<L>>,
    ),
    ExplanationInvariantSubtypingDueToMutableProperty(
        Box<ExplanationInvariantSubtypingDueToMutablePropertyData<L>>,
    ),
    ExplanationInvariantSubtypingDueToMutableProperties(
        Box<ExplanationInvariantSubtypingDueToMutablePropertiesData<L>>,
    ),
    ExplanationMultiplatform,
    ExplanationPropertyInvariantTyping,
    ExplanationPropertyMissingDueToNeutralOptionalProperty(
        Box<ExplanationPropertyMissingDueToNeutralOptionalPropertyData<L>>,
    ),
    ExplanationReactComponentPropsDeepReadOnly(L),
    ExplanationReactHookArgsDeepReadOnly(L),
    ExplanationReactHookIncompatibleWithEachOther,
    ExplanationReactHookIncompatibleWithNormalFunctions,
    ExplanationReactHookReturnDeepReadOnly(L),
    ExplanationTypeGuardPositiveConsistency {
        return_: VirtualReason<L>,
        param: VirtualReason<L>,
        guard_type: MessageTypeReferenceData<L>,
        is_return_false_statement: bool,
    },
    ExplanationAssertionFunctionConsistency {
        param: VirtualReason<L>,
        guard_type: MessageTypeReferenceData<L>,
    },
    ExplanationAdditionalUnionMembers(Box<ExplanationAdditionalUnionMembersData<L>>),
    ExplanationObjectLiteralNeedsRecordSyntax {
        record_name: FlowSmolStr,
        obj_loc: L,
    },
    ExplanationStringCasingMustBeCanonical {
        kind_name: FlowSmolStr,
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
pub enum AccessChainSegment {
    PropSegment(Name),
    TupleIndexSegment(i32),
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
pub enum Frame<L: Dupe> {
    FrameAnonymous,
    FrameAccessChain {
        chain: Vec1<AccessChainSegment>,
        incompatibility_pair: Option<(MessageTypeReferenceData<L>, VirtualReason<L>)>,
    },
    FrameArrayElement {
        incompatibility_pair: Option<(MessageTypeReferenceData<L>, MessageTypeReferenceData<L>)>,
    },
    FrameCallableSignature {
        incompatibility_pair: Option<(MessageTypeReferenceData<L>, VirtualReason<L>)>,
    },
    FrameEnumRepresentationType,
    FrameFunNthArgument {
        n: i32,
        incompatibility_pair: Option<(MessageTypeReferenceData<L>, MessageTypeReferenceData<L>)>,
    },
    FrameFunThisArgument {
        incompatibility_pair: Option<(MessageTypeReferenceData<L>, MessageTypeReferenceData<L>)>,
    },
    FrameFunNthParam {
        n: i32,
        incompatibility_pair: Option<(MessageTypeReferenceData<L>, MessageTypeReferenceData<L>)>,
    },
    FrameFunThisParam {
        incompatibility_pair: Option<(MessageTypeReferenceData<L>, MessageTypeReferenceData<L>)>,
    },
    FrameIndexerProperty {
        incompatibility_pair: Option<(MessageTypeReferenceData<L>, VirtualReason<L>)>,
    },
    FrameIndexerPropertyKey {
        incompatibility_pair: Option<(MessageTypeReferenceData<L>, VirtualReason<L>)>,
    },
    FrameTypeArgument(VirtualReason<L>),
    FrameTypeParameterBound(FlowSmolStr),
    FrameTypePredicate,
    FrameReturnValue {
        incompatibility_pair: Option<(MessageTypeReferenceData<L>, MessageTypeReferenceData<L>)>,
    },
    FrameUnionRepresentative(MessageTypeReferenceData<L>),
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
pub enum AssignTargetDesc<L: Dupe> {
    Variable(Option<VirtualReasonDesc<L>>),
    Annotation(ErrorTypeDesc<L>),
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
pub enum ValueDesc<L: Dupe> {
    Reason(Option<VirtualReasonDesc<L>>),
    Type(ErrorTypeDesc<L>),
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
pub enum CalleeDesc<L: Dupe> {
    Reason(Option<VirtualReasonDesc<L>>),
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
pub enum RootMessage<L: Dupe> {
    RootCannotAccessIndex {
        index: ErrorTypeDesc<L>,
        object_: ErrorTypeDesc<L>,
    },
    RootCannotAddComputedProperty,
    RootCannotAssign {
        init: ValueDesc<L>,
        target: Option<AssignTargetDesc<L>>,
    },
    RootCannotCall(CalleeDesc<L>),
    RootCannotCallStandalone(Option<VirtualReasonDesc<L>>),
    RootCannotCallWithNamedParam {
        fn_: CalleeDesc<L>,
        lower: ValueDesc<L>,
        name: FlowSmolStr,
    },
    RootCannotCallWithNthParam {
        fn_: CalleeDesc<L>,
        lower: ValueDesc<L>,
        n: i32,
    },
    RootCannotCallObjectAssign(Option<VirtualReasonDesc<L>>),
    RootCannotCast {
        lower: Option<VirtualReasonDesc<L>>,
        upper: ErrorTypeDesc<L>,
    },
    RootCannotCheckAgainst {
        test: Option<VirtualReasonDesc<L>>,
        discriminant: VirtualReason<L>,
    },
    RootCannotCheckAgainstSwitchDiscriminant(L),
    RootCannotCoerce {
        from: Option<VirtualReasonDesc<L>>,
        target: Option<VirtualReasonDesc<L>>,
    },
    RootCannotConformToCommonInterface {
        originate_from_import: bool,
    },
    RootCannotMergeDeclaration {
        conflict: MergedDeclarationConflict,
        first_decl: MessageTypeReferenceData<L>,
    },
    RootCannotCreateElement(Option<VirtualReasonDesc<L>>),
    RootCannotCreateRecord(Option<VirtualReasonDesc<L>>),
    RootCannotDeclareRef,
    RootCannotDeclareTypeGuard {
        type_guard_loc: L,
        fn_: VirtualReason<L>,
    },
    RootCannotDefineClassMethod {
        method_: NamedReferenceData<L>,
        name: Option<VirtualReasonDesc<L>>,
    },
    RootCannotDefineShadowedProtoProperty,
    RootCannotDelete(Option<VirtualReasonDesc<L>>),
    RootCannotExpectImplicitReturn {
        upper: ErrorTypeDesc<L>,
        fn_: Option<VirtualReasonDesc<L>>,
    },
    RootCannotExtendClass {
        extends: MessageTypeReferenceData<L>,
        def: MessageTypeReferenceData<L>,
    },
    RootCannotGetProp(Option<VirtualReasonDesc<L>>),
    RootCannotGetRest(Option<VirtualReasonDesc<L>>),
    RootCannotImplementClass {
        implements: VirtualReason<L>,
        def: Option<VirtualReasonDesc<L>>,
    },
    RootCannotInitializeField {
        field: Option<VirtualReasonDesc<L>>,
        body: Option<VirtualReasonDesc<L>>,
    },
    RootCannotInstantiateEval(MessageTypeReferenceData<L>),
    RootCannotInstantiateRenderType,
    RootCannotInstantiateTypeApp(Option<VirtualReasonDesc<L>>),
    RootCannotMakeReadOnly,
    RootCannotReturn(Option<VirtualReasonDesc<L>>),
    RootCannotReturnFromAssertionFunction {
        implicit_return: bool,
    },
    RootCannotShadowProto(NamedReferenceData<L>),
    RootCannotShadowProtoProperty,
    RootCannotSpread(Option<VirtualReasonDesc<L>>),
    RootCannotUpdate(Option<VirtualReasonDesc<L>>),
    RootCannotUseInferTypeBound {
        infer: Option<VirtualReasonDesc<L>>,
    },
    RootCannotUseTypeGuard {
        guard_type: MessageTypeReferenceData<L>,
        param_name: FlowSmolStr,
    },
    RootCannotYield(Option<VirtualReasonDesc<L>>),
}

pub type UnionEnumMap<L> = BTreeMap<UnionEnum, Vec1<L>>;

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
pub enum ThisFinderKind {
    This,
    Super,
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
pub enum PrimitiveKind {
    Boolean,
    Number,
    String,
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
pub enum ObjKind {
    Type,
    Literal,
    Interface,
    DeclareClass,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MessageAlreadyExhaustivelyCheckOneEnumMemberData<L: Dupe> {
    pub member_name: FlowSmolStr,
    pub prev_check_loc: L,
    pub enum_: NamedReferenceData<L>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MessageCannotAccessEnumMemberData<L: Dupe> {
    pub member_name: Name,
    pub suggestion: Option<FlowSmolStr>,
    pub enum_: EnumReferenceData<L>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MessageCannotAddComputedPropertyDueToPotentialOverwriteData<L: Dupe> {
    pub key_loc: L,
    pub overwritten_locs: Vec<L>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MessageCannotCallMaybeReactHookData<L: Dupe> {
    pub callee_loc: L,
    pub hooks: Vec<L>,
    pub non_hooks: Vec<L>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MessageCannotCompareData<L: Dupe> {
    pub lower: MessageTypeReferenceData<L>,
    pub upper: MessageTypeReferenceData<L>,
    pub strict_comparison_opt: Option<StrictComparisonInfo<L, MessageTypeReferenceData<L>>>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MessageCannotExportRenamedDefaultData {
    pub name: Option<FlowSmolStr>,
    pub is_reexport: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MessageCannotExhaustivelyCheckAbstractEnumsData<L: Dupe> {
    pub description_name: Option<FlowSmolStr>,
    pub enum_loc: L,
    pub enum_name: Option<FlowSmolStr>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MessageCannotExhaustivelyCheckEnumWithUnknownsData<L: Dupe> {
    pub description_name: Option<FlowSmolStr>,
    pub enum_: NamedReferenceData<L>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MessageCannotInstantiateObjectUtilTypeWithEnumData<L: Dupe> {
    pub operation_name: FlowSmolStr,
    pub enum_: EnumReferenceData<L>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MessageCannotResolveBuiltinModuleData {
    pub name: FlowSmolStr,
    pub potential_generator: Option<FlowSmolStr>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MessageCannotImportGlobalLibdefData {
    pub module_name: FlowSmolStr,
    pub libdef_name: FlowSmolStr,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MessageCannotSpreadGeneralData<L: Dupe> {
    pub spread_loc: L,
    pub object1: MessageTypeReferenceData<L>,
    pub object2: MessageTypeReferenceData<L>,
    pub propname: Name,
    pub error_kind: ExactnessErrorKind,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MessageCannotSpreadInexactMayOverwriteIndexerData<L: Dupe> {
    pub spread_loc: L,
    pub object2: Box<MessageTypeReferenceData<L>>,
    pub key: Box<MessageTypeReferenceData<L>>,
    pub value: Box<MessageTypeReferenceData<L>>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MessageCannotUseEnumMemberUsedAsTypeData<L: Dupe> {
    pub description_name: Option<FlowSmolStr>,
    pub enum_: EnumReferenceData<L>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MessageCannotUseTypeForAnnotationInferenceData<L: Dupe> {
    pub operation: AnnotationInferenceOperation,
    pub operation_loc: L,
    pub target_loc: L,
    pub target_desc: ErrorTypeDesc<L>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MessageCannotUseTypeGuardWithFunctionParamHavocedData<L: Dupe> {
    pub type_guard: TypeGuardReferenceKind,
    pub param: TypeGuardReferenceData<L>,
    pub call_locs: Vec<L>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MessageCannotUseTypeInValuePositionData<L: Dupe> {
    pub binding: NamedReferenceData<L>,
    pub type_only_namespace: bool,
    pub imported_name: Option<FlowSmolStr>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MessageDefinitionInvalidRecursiveData<L: Dupe> {
    pub definition: DefinitionReferenceKind,
    pub recursion: Vec<L>,
    pub annot_locs: Vec<AnnotLoc<L>>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MessageDuplicateModuleProviderData<L: Dupe> {
    pub module_name: FlowSmolStr,
    pub provider: L,
    pub conflict: L,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MessageEnumDuplicateMemberNameData<L: Dupe> {
    pub member_name: String,
    pub prev_use_loc: L,
    pub enum_: EnumReferenceData<L>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MessageEnumInvalidMemberInitializerData<L: Dupe> {
    pub member_name: String,
    pub explicit_type: Option<flow_parser::ast::statement::enum_declaration::ExplicitType>,
    pub enum_: EnumReferenceData<L>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MessageExponentialSpreadData<L: Dupe> {
    pub spread_loc: L,
    pub operand1: ExponentialSpreadOperandGroup<MessageTypeReferenceData<L>>,
    pub operand2: ExponentialSpreadOperandGroup<MessageTypeReferenceData<L>>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MessageIncompatibleTupleArityData<L: Dupe> {
    pub lower: MessageTypeReferenceData<L>,
    pub lower_arity: (i32, i32),
    pub lower_inexact: bool,
    pub lower_rest: bool,
    pub upper: MessageTypeReferenceData<L>,
    pub upper_arity: (i32, i32),
    pub upper_inexact: bool,
    pub upper_rest: bool,
    pub unify: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MessageIncompatibleGeneralWithPrintedTypesData<L: Dupe> {
    pub lower_loc: L,
    pub upper_loc: L,
    pub lower_desc: ErrorTypeDesc<L>,
    pub upper_desc: ErrorTypeDesc<L>,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    serde::Serialize,
    serde::Deserialize
)]
pub struct MessageTypeReferenceData<L: Dupe> {
    pub loc: L,
    pub desc: ErrorTypeDesc<L>,
}

#[derive(
    Debug,
    Clone,
    Dupe,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    serde::Serialize,
    serde::Deserialize
)]
pub struct NamedReferenceData<L: Dupe> {
    pub loc: L,
    pub name: FlowSmolStr,
}

/// Why a statement-position call to an assertion function does not narrow.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    serde::Serialize,
    serde::Deserialize
)]
pub enum UnrecognizedAssertionCallee<L: Dupe> {
    /// The call target is not an identifier or a dotted name.
    UnsupportedTarget,
    /// The binding at the root of the call target is not declared with an
    /// explicit type annotation.
    UnannotatedRoot(NamedReferenceData<L>),
    /// The callee's declared type is not a single assertion function type,
    /// e.g. an overloaded function, an intersection, or a type parameter.
    UnsupportedType,
}

/// A side of a property polarity mismatch.
#[derive(
    Debug,
    Clone,
    Dupe,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    serde::Serialize,
    serde::Deserialize
)]
pub enum PropPolarityMismatchObject<T, L: Dupe> {
    Type(T),
    /// A named class or interface whose signature is checked: its own against its proto
    /// members, or its members against its super class.
    Class(NamedReferenceData<L>),
}

#[derive(
    Debug,
    Clone,
    Dupe,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    serde::Serialize,
    serde::Deserialize
)]
pub struct ComponentReferenceData<L: Dupe> {
    pub loc: L,
    pub name: Option<FlowSmolStr>,
}

#[derive(
    Debug,
    Clone,
    Dupe,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    serde::Serialize,
    serde::Deserialize
)]
pub struct FunctionReferenceData<L: Dupe> {
    pub loc: L,
    pub kind: FunctionReferenceKind,
}

#[derive(
    Debug,
    Clone,
    Dupe,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    serde::Serialize,
    serde::Deserialize
)]
pub struct ImplicitInstantiationReferenceData<L: Dupe> {
    pub loc: L,
    pub kind: ImplicitInstantiationReferenceKind,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    serde::Serialize,
    serde::Deserialize
)]
pub enum UnnecessaryInvariantConditionKind {
    Type,
    IntersectionType,
}

#[derive(
    Debug,
    Clone,
    Dupe,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    serde::Serialize,
    serde::Deserialize
)]
pub enum TypeGuardReferenceKind {
    TypeGuard,
    Parameter(Option<FlowSmolStr>),
    TypeGuardParameter(FlowSmolStr),
    This,
}

#[derive(
    Debug,
    Clone,
    Dupe,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    serde::Serialize,
    serde::Deserialize
)]
pub struct TypeGuardReferenceData<L: Dupe> {
    pub loc: L,
    pub kind: TypeGuardReferenceKind,
}

#[derive(
    Debug,
    Clone,
    Dupe,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    serde::Serialize,
    serde::Deserialize
)]
pub enum ValueAsTypeReference {
    Name(FlowSmolStr),
    ImportType(Userland),
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    serde::Serialize,
    serde::Deserialize
)]
pub struct TupleElementReferenceData<L: Dupe> {
    pub loc: L,
    pub name: Option<FlowSmolStr>,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    serde::Serialize,
    serde::Deserialize
)]
pub enum LowerRequirement {
    Array,
    ArrayIndex,
    Class,
    ClassWithPrivateProperties,
    Function,
    FunctionType,
    Inheritable,
    InstanceType,
    Object,
    ObjectOrArrayOrTuple,
    PolymorphicType,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MessageInvalidKeyMirrorArgumentData<L: Dupe> {
    pub lower: MessageTypeReferenceData<L>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MessageIncompatibleDueToInvariantSubtypingData<L: Dupe> {
    pub sub_component: Option<SubComponentOfInvariantSubtypingError>,
    pub lower_loc: L,
    pub upper_loc: L,
    pub lower_desc: ErrorTypeDesc<L>,
    pub upper_desc: ErrorTypeDesc<L>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MessageIncompleteExhausiveCheckEnumData<L: Dupe> {
    pub description_name: Option<FlowSmolStr>,
    pub enum_: NamedReferenceData<L>,
    pub left_to_check: Vec<FlowSmolStr>,
    pub default_case_loc: Option<L>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MessageInvalidEnumMemberCheckData<L: Dupe> {
    pub enum_: EnumReferenceData<L>,
    pub example_member: Option<FlowSmolStr>,
    pub from_match: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MessageInvalidKeyPropertyInSpreadData<L: Dupe> {
    pub key_loc: L,
    pub spread_loc: L,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MessageInvalidRendersTypeArgumentData<L: Dupe> {
    pub renders_variant: RendersVariant,
    pub invalid_render_type_kind: InvalidRenderTypeKind<MessageTypeReferenceData<L>>,
    pub invalid_types: Vec1<MessageTypeReferenceData<L>>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MessageInvalidSelfReferencingTypeAnnotationData<L: Dupe> {
    pub name: FlowSmolStr,
    pub loc: L,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MessageInvalidSelfReferencingDefaultData<L: Dupe> {
    pub name: FlowSmolStr,
    pub def_loc: L,
    pub ref_loc: L,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MessageMissingPlatformSupportWithAvailablePlatformsData {
    pub available_platforms: BTreeSet<FlowSmolStr>,
    pub required_platforms: BTreeSet<FlowSmolStr>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MessageNoDefaultExportData {
    pub module_name: FlowSmolStr,
    pub suggestion: Option<FlowSmolStr>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MessageNoNamedExportData {
    pub module_name: FlowSmolStr,
    pub export_name: FlowSmolStr,
    pub suggestion: Option<FlowSmolStr>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MessageOnlyDefaultExportData {
    pub module_name: FlowSmolStr,
    pub export_name: FlowSmolStr,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MessagePropExtraAgainstExactObjectData<L: Dupe> {
    pub lower: MessageTypeReferenceData<L>,
    pub upper: MessageTypeReferenceData<L>,
    pub upper_is_record: bool,
    pub props: Vec1<FlowSmolStr>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MessageConstructSignatureMissingData<L: Dupe> {
    pub lower: MessageTypeReferenceData<L>,
    pub upper: MessageTypeReferenceData<L>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MessagePropsMissingWithPrintedTypeData<L: Dupe> {
    pub lower: MessageTypeReferenceData<L>,
    pub upper: MessageTypeReferenceData<L>,
    pub props: Vec1<FlowSmolStr>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MessageIndexerCheckFailedData<L: Dupe> {
    pub prop: FlowSmolStr,
    pub lower: MessageTypeReferenceData<L>,
    pub upper: MessageTypeReferenceData<L>,
    pub indexer: MessageTypeReferenceData<L>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MessagePropPolarityMismatchData<L: Dupe> {
    pub lower: PropPolarityMismatchObject<MessageTypeReferenceData<L>, L>,
    pub upper: PropPolarityMismatchObject<MessageTypeReferenceData<L>, L>,
    pub props: Vec1<(Option<FlowSmolStr>, Polarity, Polarity)>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MessageReactIntrinsicOverlapData<L: Dupe> {
    pub use_: NamedReferenceData<L>,
    pub def: L,
    pub type_: L,
    pub mixed: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MessageRedeclareComponentPropData<L: Dupe> {
    pub duplicates: Vec1<(L, Name, L)>,
    pub spread_loc: L,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MessageShouldAnnotateVariableUsedInGenericContextData<L: Dupe> {
    pub declaration: NamedReferenceData<L>,
    pub null_loc: L,
    pub initialized: bool,
    pub possible_generic_escape_locs: Vec<L>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MessageSketchyNullCheckData<L: Dupe> {
    pub kind: SketchyNullKind,
    pub falsy_loc: L,
    pub null_loc: L,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MessageTupleElementNotReadableData<L: Dupe> {
    pub index_def_loc: L,
    pub index: i32,
    pub name: Option<FlowSmolStr>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MessageTupleElementNotWritableData<L: Dupe> {
    pub index_def_loc: L,
    pub index: i32,
    pub name: Option<FlowSmolStr>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MessageTupleIndexOutOfBoundData<L: Dupe> {
    pub tuple: MessageTypeReferenceData<L>,
    pub inexact: bool,
    pub length: i32,
    pub index: FlowSmolStr,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MessageTupleNonIntegerIndexData<L: Dupe> {
    pub index_def_loc: L,
    pub index: FlowSmolStr,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MessageVariableOnlyAssignedByNullData<L: Dupe> {
    pub declaration: NamedReferenceData<L>,
    pub null_loc: Option<L>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MessageMatchNonExhaustiveObjectPatternData<L: Dupe> {
    pub rest: Option<MessageTypeReferenceData<L>>,
    pub missing_props: Vec<FlowSmolStr>,
    pub pattern_kind: MatchObjPatternKind,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MessageMatchNonExplicitEnumCheckData<L: Dupe> {
    pub wildcard_loc: L,
    pub unchecked_members: Vec<FlowSmolStr>,
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
pub enum VarianceSigilParent {
    Property,
    TypeParam,
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
pub enum VarianceSigilKind {
    Plus(VarianceSigilParent),
    Minus(VarianceSigilParent),
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum Message<L: Dupe> {
    MessagePlainTextReservedForInternalErrorOnly(FlowSmolStr),

    MessageAlreadyExhaustivelyCheckOneEnumMember(
        Box<MessageAlreadyExhaustivelyCheckOneEnumMemberData<L>>,
    ),

    MessageAlreadyExhaustivelyCheckAllEnumMembers {
        enum_: NamedReferenceData<L>,
    },

    MessageAmbiguousNumericKeyWithVariance,
    MessageAmbiguousObjectType,

    MessageAnyValueUsedAsType {
        reference: Option<ValueAsTypeReference>,
        value: MessageTypeReferenceData<L>,
    },
    MessageBadLibdefModuleOverride(NamedReferenceData<L>),
    MessageBadLibdefNameOverride(NamedReferenceData<L>),
    MessageInterfaceMergePropertyConflict(NamedReferenceData<L>),
    MessageInterfaceMergeTparamMismatch(NamedReferenceData<L>),

    MessageCannotAccessEnumMember(Box<MessageCannotAccessEnumMemberData<L>>),

    MessageCannotAccessEnumWithComputedProp {
        description: ErrorTypeDesc<L>,
        enum_: EnumReferenceData<L>,
    },

    MessageCannotAccessObjectWithComputedProp {
        object: MessageTypeReferenceData<L>,
        property: MessageTypeReferenceData<L>,
        kind: InvalidObjKey,
    },

    MessageCannotAccessReactRefInRender {
        usage: ExpressionReferenceData<L>,
        in_hook: bool,
    },

    MessageCannotAddComputedPropertyDueToPotentialOverwrite(
        Box<MessageCannotAddComputedPropertyDueToPotentialOverwriteData<L>>,
    ),

    MessageCannotApplyNonPolymorphicType,

    MessageCannotAssignToObjectWithComputedProp(Box<MessageTypeReferenceData<L>>),

    MessageCannotAssignToObjectWithComputedPropWithKey {
        property: MessageTypeReferenceData<L>,
        key: ExpressionReferenceData<L>,
        kind: InvalidObjKey,
    },

    MessageCannotAssignToInvalidLHS,

    MessageCannotBuildTypedInterface(SignatureError<L>),

    MessageCannotCallMaybeReactHook(Box<MessageCannotCallMaybeReactHookData<L>>),

    MessageCannotCallNonHookSyntaxHook(L),

    MessageCannotCallObjectFunctionOnEnum {
        function: NamedReferenceData<L>,
        enum_: EnumReferenceData<L>,
    },

    MessageCannotCallReactComponent(MessageTypeReferenceData<L>),

    MessageCannotCallReactHookConditionally(L),
    MessageCannotCallReactHookInDefinitelyNonComponentOrHook(L),
    MessageCannotCallReactHookInNonComponentSyntaxComponentOrHookSyntaxHook(L),
    MessageCannotCallReactHookInUnknownContext(L),
    MessageCannotCallReactHookWithIllegalName(L),

    MessageCannotCallFunctionWithExtraArg {
        function_reference: FunctionReferenceData<L>,
        param_count: i32,
    },

    MessageCannotChangeEnumMember(EnumReferenceData<L>),

    MessageCannotCompare(Box<MessageCannotCompareData<L>>),

    MessageCannotCompareNonStrict {
        lower: MessageTypeReferenceData<L>,
        upper: MessageTypeReferenceData<L>,
    },

    MessageCannotCreateExactType(Box<MessageTypeReferenceData<L>>),

    MessageInvalidExactObjectIntersection {
        prop: FlowSmolStr,
        exact_object: MessageTypeReferenceData<L>,
    },

    MessageCannotDeclareAlreadyBoundName(NamedReferenceData<L>),
    MessageCannotDeclareAlreadyBoundNameInCoreJs(NamedReferenceData<L>),

    MessageCannotDeclareAlreadyBoundNameInNamespace(NamedReferenceData<L>),

    MessageCannotDeclareReservedType {
        keyword: IncorrectType,
        binding: NamedReferenceData<L>,
    },

    MessageCannotDelete(Box<MessageTypeReferenceData<L>>),
    MessageCannotDetermineEmptyArrayLiteralType,
    MessageCannotDetermineModuleType,

    MessageCannotExportRenamedDefault(Box<MessageCannotExportRenamedDefaultData>),

    MessageCannotExhaustivelyCheckAbstractEnums(
        Box<MessageCannotExhaustivelyCheckAbstractEnumsData<L>>,
    ),

    MessageCannotExhaustivelyCheckEnumWithUnknowns(
        Box<MessageCannotExhaustivelyCheckEnumWithUnknownsData<L>>,
    ),

    MessageCannotImplementNonInterface(Box<MessageTypeReferenceData<L>>),

    MessageCannotInstantiateObjectUtilTypeWithEnum(
        Box<MessageCannotInstantiateObjectUtilTypeWithEnumData<L>>,
    ),

    MessageCannotIterateEnum(EnumReferenceData<L>),

    MessageCannotIterateEnumForIn(EnumReferenceData<L>),

    MessageCannotIterateWithForIn(Box<MessageTypeReferenceData<L>>),
    MessageCannotMutateThisPrototype,
    MessageCannotNestComponents,
    MessageCannotNestHook,

    MessageCannotOptimizeUnionDueToNonUniqueKeys(BTreeMap<Name, UnionEnumMap<L>>),
    MessageCannotOptimizeUnionInternally(OptimizedError<L>),

    MessageCannotPassReactRefAsArgument {
        usage: ExpressionReferenceData<L>,
        in_hook: bool,
    },

    MessageCannotPerformArithOnNonNumbersOrBigInt(Box<MessageTypeReferenceData<L>>),
    MessageCannotPerformBigIntRShift3(Box<MessageTypeReferenceData<L>>),
    MessageCannotPerformBigIntUnaryPlus(Box<MessageTypeReferenceData<L>>),

    MessageCannotPerformBinaryArith {
        kind: flow_typing_type::type_::arith_kind::ArithKind,
        left: Box<MessageTypeReferenceData<L>>,
        right: Box<MessageTypeReferenceData<L>>,
    },

    MessageCannotReassignConstant(NamedReferenceData<L>),

    MessageCannotReassignConstantLikeBinding {
        definition: NamedReferenceData<L>,
        binding_kind: AssignedConstLikeBindingType,
    },

    MessageCannotReassignEnum(NamedReferenceData<L>),
    MessageCannotReassignImport(NamedReferenceData<L>),
    MessageCannotRedeclareVar(NamedReferenceData<L>),

    MessageCannotReferenceTypeGuardParameter {
        type_guard: TypeGuardParameterData<L>,
        binding_loc: L,
        binding_kind: TypeGuardBindingKind,
    },

    MessageCannotResolveBuiltinName(FlowSmolStr),

    MessageUmdGlobalValueInModule(FlowSmolStr),

    MessageCannotResolveBuiltinModule(Box<MessageCannotResolveBuiltinModuleData>),

    MessageCannotResolveExpectedModule {
        name: FlowSmolStr,
        expected_module_purpose: ExpectedModulePurpose,
    },

    MessageCannotImportGlobalLibdef(Box<MessageCannotImportGlobalLibdefData>),

    MessageCannotSpreadDueToPotentialOverwrite {
        spread_loc: L,
        object: Box<MessageTypeReferenceData<L>>,
        key: Box<MessageTypeReferenceData<L>>,
    },

    MessageCannotSpreadGeneral(Box<MessageCannotSpreadGeneralData<L>>),

    MessageCannotSpreadInexactMayOverwriteIndexer(
        Box<MessageCannotSpreadInexactMayOverwriteIndexerData<L>>,
    ),

    MessageCannotSpreadInterface {
        spread_loc: L,
        interface: MessageTypeReferenceData<L>,
    },

    MessageCannotUseAsConstructor(Box<MessageTypeReferenceData<L>>),
    MessageCannotUseAsPrototype(Box<MessageTypeReferenceData<L>>),
    MessageCannotUseAsSuperClass(ExpressionReferenceData<L>),
    MessageCannotUseBeforeDeclaration(NamedReferenceData<L>),

    MessageCannotUseDefaultImportWithDestrucuturing,
    MessageCannotUseDollarExports,

    MessageCannotUseEnumMemberUsedAsType(Box<MessageCannotUseEnumMemberUsedAsTypeData<L>>),

    MessageCannotUseExportInNonLegalToplevelContext(FlowSmolStr),
    MessageCannotUseImportStar(L),
    MessageCannotUseInOperatorDueToBadLHS(Box<MessageTypeReferenceData<L>>),
    MessageCannotUseInOperatorDueToBadRHS(Box<MessageTypeReferenceData<L>>),
    MessageCannotUseInstanceOfOperatorDueToBadRHS(Box<MessageTypeReferenceData<L>>),
    MessageCannotUseMixedImportAndRequire(L),

    MessageCannotUseNonPolymorphicTypeWithTypeArgs {
        is_new: bool,
        callee: Box<CallTypeArityCallee<MessageTypeReferenceData<L>>>,
        expected_arity: i32,
    },

    MessageCannotUsePrimitiveAsInterface {
        lower: MessageTypeReferenceData<L>,
        upper: MessageTypeReferenceData<L>,
        kind: PrimitiveKind,
    },

    MessageCannotUseThisSuperBeforeSuperCall(NamedReferenceData<L>),

    MessageCannotUseTypeDueToPolarityMismatch {
        type_param: NamedReferenceData<L>,
        expected_polarity: Polarity,
        actual_polarity: Polarity,
    },

    MessageCannotUseTypeForAnnotationInference(
        Box<MessageCannotUseTypeForAnnotationInferenceData<L>>,
    ),

    MessageCannotUseTypeGuardWithFunctionParamHavoced(
        Box<MessageCannotUseTypeGuardWithFunctionParamHavocedData<L>>,
    ),

    MessageCannotUseTypeInValuePosition(Box<MessageCannotUseTypeInValuePositionData<L>>),

    MessageCannotUseTypeWithoutAnyTypeArgs {
        type_: MessageTypeReferenceData<L>,
        min_arity: i32,
        max_arity: i32,
    },

    MessageCannotUseTypeWithoutExactlyNTypeArgs(i32),

    MessageCannotUseTypeWithTooFewTypeArgs {
        reason_arity: VirtualReason<L>,
        n: i32,
    },

    MessageCannotUseTypeWithTooManyTypeArgs {
        reason_arity: VirtualReason<L>,
        n: i32,
    },

    MessageComponentMissingReturn(ComponentReferenceData<L>),
    MessageComponentMissingBody,
    MessageComponentBodyInAmbientContext,
    MessageComponentNonUpperCase,
    MessageDeclareComponentInvalidParam(DeclareComponentInvalidParamKind),

    MessageDefinitionCycle(Vec1<(DefinitionReferenceData<L>, Vec<L>, Vec<AnnotLoc<L>>)>),

    MessageDefinitionInvalidRecursive(Box<MessageDefinitionInvalidRecursiveData<L>>),

    MessageDeprecatedBool,

    MessageDeprecatedTypeParamColonBound,

    MessageDevOnlyRefinedLocInfo {
        refining_locs: Vec<L>,
    },

    MessageDevOnlyInvalidatedRefinementInfo(Vec<(L, refinement_invalidation::Reason)>),
    MessageDocblockError(DocblockError),

    MessageDoesNotRender {
        lower: VirtualReason<L>,
        upper: VirtualReason<L>,
    },

    MessageDoesNotRenderWithPrintedTypes(Box<MessageIncompatibleGeneralWithPrintedTypesData<L>>),

    MessageDuplicateClassMember {
        name: FlowSmolStr,
        static_: bool,
        class_kind: ClassKind,
    },

    MessageDuplicateEnumMember {
        enum_: EnumReferenceData<L>,
        prev_use_loc: L,
    },

    MessageDuplicateModuleProvider(Box<MessageDuplicateModuleProviderData<L>>),

    MessageEnumsNotEnabled,

    MessageEnumConstNotSupported,

    MessageEnumDuplicateMemberName(Box<MessageEnumDuplicateMemberNameData<L>>),

    MessageEnumInconsistentMemberValues {
        enum_: EnumReferenceData<L>,
    },

    MessageEnumInvalidMemberInitializer(Box<MessageEnumInvalidMemberInitializerData<L>>),

    MessageEnumBooleanMemberNotInitialized {
        member_name: String,
        enum_: EnumReferenceData<L>,
    },

    MessageEnumNumberMemberNotInitialized {
        member_name: String,
        enum_: EnumReferenceData<L>,
    },

    MessageEnumBigIntMemberNotInitialized {
        member_name: String,
        enum_: EnumReferenceData<L>,
    },

    MessageEnumStringMemberInconsistentlyInitialized {
        enum_: EnumReferenceData<L>,
    },

    MessageTSEnumInvalidMember {
        member_name: String,
        enum_: EnumReferenceData<L>,
        kind: TsEnumInvalidMemberKind,
    },

    MessageTSEnumInvalidSyntax {
        enum_: EnumReferenceData<L>,
        kind: TsEnumInvalidSyntaxKind,
    },

    MessageEnumNonIdentifierMemberName {
        member_name: String,
        enum_: EnumReferenceData<L>,
    },

    MessageInvalidEnumMemberName {
        member_name: String,
        enum_: EnumReferenceData<L>,
    },

    MessageExponentialSpread(Box<MessageExponentialSpreadData<L>>),

    MessageExportValueAsType(FlowSmolStr),

    MessageFunctionRequiresAnotherArgument {
        def: MessageTypeReferenceData<L>,
        from: Option<VirtualReason<L>>,
    },

    MessageFunctionRequiresAnotherArgumentFromCall {
        def: MessageTypeReferenceData<L>,
        call: ImplicitInstantiationReferenceData<L>,
    },

    MessageImportTypeAsTypeof(FlowSmolStr),
    MessageImportTypeAsValue(FlowSmolStr),
    MessageImportValueAsType(FlowSmolStr),

    MessageIncompatibleTupleArity(Box<MessageIncompatibleTupleArityData<L>>),

    MessageIncompatibleClassToObject {
        lower: MessageTypeReferenceData<L>,
        upper: MessageTypeReferenceData<L>,
        kind: ClassKind,
    },

    MessageIncompatibleComponentRestParam(MessageTypeReferenceData<L>),

    MessageIncompatibleGeneral {
        lower: VirtualReason<L>,
        upper: VirtualReason<L>,
    },

    MessageStandaloneCallRequiresReceiver(MessageTypeReferenceData<L>),

    MessageIncompatibleWithUnionRepresentative {
        union: MessageTypeReferenceData<L>,
        lower: VirtualReason<L>,
        upper: VirtualReason<L>,
    },

    MessageIncompatibleGeneralWithPrintedTypes(
        Box<MessageIncompatibleGeneralWithPrintedTypesData<L>>,
    ),

    MessageIncompatibleDueToInvariantSubtyping(
        Box<MessageIncompatibleDueToInvariantSubtypingData<L>>,
    ),

    MessageIncompatibleMappedTypeKey {
        source_type: MessageTypeReferenceData<L>,
        mapped_type: MessageTypeReferenceData<L>,
    },

    MessageIncompatibleNonLiteralArrayToTuple {
        lower: MessageTypeReferenceData<L>,
        upper: MessageTypeReferenceData<L>,
    },

    MessageIncompatibleNonTypeGuardToTypeGuard {
        lower: TypeGuardFuncReference<L, MessageTypeReferenceData<L>>,
        upper: TypeGuardFuncReference<L, MessageTypeReferenceData<L>>,
    },

    MessageIncompatibleReactHooksDueToUniqueness {
        lower: MessageTypeReferenceData<L>,
        upper: MessageTypeReferenceData<L>,
    },

    MessageIncompatibleReactHooksWithNonReactHook {
        lower: MessageTypeReferenceData<L>,
        upper: MessageTypeReferenceData<L>,
        lower_is_hook: bool,
        hook_is_annot: bool,
    },

    MessageIncompatibleWithExact {
        kind: ExactnessErrorKind,
        lower: MessageTypeReferenceData<L>,
        upper: MessageTypeReferenceData<L>,
    },

    MessageIncompatibleWithIndexed {
        lower: MessageTypeReferenceData<L>,
        upper: MessageTypeReferenceData<L>,
    },

    MessageIncompleteExhausiveCheckEnum(Box<MessageIncompleteExhausiveCheckEnumData<L>>),

    MessageIncorrectType(IncorrectType),

    MessageInvalidCatchParameterAnnotation {
        ts_utility_syntax: bool,
    },

    MessageInvalidComponentRestParam,

    MessageInvalidEnumMemberCheck(Box<MessageInvalidEnumMemberCheckData<L>>),

    MessageInvalidGenericRef(FlowSmolStr),
    MessageInvalidGraphQL(GraphqlError),
    MessageInvalidHookNaming,

    MessageInvalidImportStarUse(L),
    MessageInvalidInferType,

    MessageInvalidKeyMirrorArgument(Box<MessageInvalidKeyMirrorArgumentData<L>>),

    MessageInvalidLintSettings(LintParseError),
    MessageInvalidMappedTypeInInterfaceOrDeclaredClass,
    MessageInvalidMappedTypeWithExactOrInexact,
    MessageInvalidMappedTypeWithExtraProps,
    MessageInvalidMappedTypeWithVarianceOnArrayInput,
    MessageInvalidTemplateLiteralTypeComplexity,
    MessageInvalidTemplateLiteralTypePlaceholder,

    MessageInvalidReactCreateElement(Box<NamedReferenceData<L>>),

    MessageInvalidThisArgMissingReceiver {
        name: FlowSmolStr,
        callee_expression: ExpressionReferenceData<L>,
    },

    MessageInvalidThisArgReceiverMismatch {
        name: FlowSmolStr,
        receiver_expression: ExpressionReferenceData<L>,
    },

    MessageInvalidKeyPropertyInSpread(Box<MessageInvalidKeyPropertyInSpreadData<L>>),

    MessageInvalidRendersTypeArgument(Box<MessageInvalidRendersTypeArgumentData<L>>),

    MessageInvalidSelfReferencingTypeAnnotation(
        Box<MessageInvalidSelfReferencingTypeAnnotationData<L>>,
    ),

    MessageInvalidSelfReferencingDefault(Box<MessageInvalidSelfReferencingDefaultData<L>>),

    MessageInvalidTrivialRecursiveDefinition,
    MessageInvalidTrivialRecursiveTypeParameter(FlowSmolStr),
    MessageClassTypeParamInStatic(FlowSmolStr),

    MessageInvalidTupleRequiredAfterOptional {
        tuple_loc: L,
        required: TupleElementReferenceData<L>,
        optional: TupleElementReferenceData<L>,
    },

    MessageInvalidTupleTypeSpread(Box<MessageTypeReferenceData<L>>),
    MessageTupleElementAfterInexactSpread,
    MessageTupleElementAfterRestSpread,

    MessageInternalType(InternalType),

    MessageInvalidTypeCastingSyntax,

    MessageInvalidTypeGuardFunctionKind(FlowSmolStr),

    MessageInvalidTypeGuardFunctionWritten {
        type_guard: NamedReferenceData<L>,
        write_locs: Vec<L>,
    },

    MessageNegativeTypeGuardConsistency {
        return_desc: Option<VirtualReasonDesc<L>>,
        type_: Box<MessageTypeReferenceData<L>>,
    },

    MessageAssertionFunctionFalsyAtExit {
        param: NamedReferenceData<L>,
        falsy: Box<MessageTypeReferenceData<L>>,
        implicit_return: bool,
    },

    /// A statement-position call to an assertion function that does not
    /// narrow.
    MessageUnrecognizedAssertionCall(UnrecognizedAssertionCallee<L>),

    MessageInvalidTypeGuardParamUnbound(TypeGuardParameterData<L>),
    MessageInvalidTypeGuardThisParam(L),
    MessageInvalidUseOfFlowEnforceOptimized(Box<MessageTypeReferenceData<L>>),

    MessageLowerIsNotReactComponent(Box<MessageTypeReferenceData<L>>),

    MessageLowerIsNotWithPrintedType {
        lower: Box<MessageTypeReferenceData<L>>,
        requirement: LowerRequirement,
    },

    MessageLowerIsNotSupportedByUnclassifiedUseWithPrintedType {
        lower: Box<MessageTypeReferenceData<L>>,
        ctor: FlowSmolStr,
    },

    MessageLowerDoesNotHavePropertiesWithPrintedType(Box<MessageTypeReferenceData<L>>),

    MessageMissingAnnotation(Option<VirtualReasonDesc<L>>),
    MessageMissingThisAnnotation,
    MessageMissingReturnAnnotation,
    MessageMissingReturnAnnotationForGenericFunction,
    MessageMissingParameterAnnotation,
    MessageMissingParameterAnnotationDueToContextualTypingFailure,
    MessageMissingAnnotationDueToContextualTypingFailure(Option<VirtualReasonDesc<L>>),
    MessageMissingAnnotationForGenericFunction(Option<VirtualReasonDesc<L>>),

    MessageMissingPlatformSupportWithAvailablePlatforms(
        Box<MessageMissingPlatformSupportWithAvailablePlatformsData>,
    ),

    MessageNoDefaultExport(Box<MessageNoDefaultExportData>),

    MessageNoNamedExport(Box<MessageNoNamedExportData>),

    MessageNonConstVarExport(Option<NamedReferenceData<L>>),
    MessageNonStrictImport,
    MessageNonToplevelExport,

    MessageOnlyDefaultExport(Box<MessageOnlyDefaultExportData>),

    MessageParseError(ParseError),
    MessagePlatformSpecificImplementationModuleLookupFailed(FlowSmolStr),

    MessagePropExtraAgainstExactObject(Box<MessagePropExtraAgainstExactObjectData<L>>),

    MessagePropMissingWithPrintedType {
        lower: Box<MessageTypeReferenceData<L>>,
        upper: Option<Box<MessageTypeReferenceData<L>>>,
        prop: Option<FlowSmolStr>,
        suggestion: Option<FlowSmolStr>,
    },

    MessagePrivatePropMissing {
        object: MessageTypeReferenceData<L>,
        prop: FlowSmolStr,
    },

    MessageConstructSignatureMissing(Box<MessageConstructSignatureMissingData<L>>),

    MessagePropsMissingWithPrintedType(Box<MessagePropsMissingWithPrintedTypeData<L>>),

    MessageIndexerCheckFailed(Box<MessageIndexerCheckFailedData<L>>),

    MessagePropPolarityMismatch(Box<MessagePropPolarityMismatchData<L>>),

    MessagePropNotReadable(Option<Name>),
    MessagePropNotWritable(Option<Name>),

    MessageReactIntrinsicOverlap(Box<MessageReactIntrinsicOverlapData<L>>),

    MessageReadonlyArraysCannotBeWrittenTo,
    MessageRecursionLimitExceeded,

    MessageRedeclareComponentProp(Box<MessageRedeclareComponentPropData<L>>),

    MessageShouldAnnotateVariableOnlyInitializedInGenericContext {
        declaration: NamedReferenceData<L>,
        possible_generic_escape_locs: Vec<L>,
    },

    MessageShouldAnnotateVariableUsedInGenericContext(
        Box<MessageShouldAnnotateVariableUsedInGenericContextData<L>>,
    ),

    MessageShouldNotBeCoerced(VirtualReason<L>),
    MessageShouldUseArrayLiteral,

    MessageSketchyNumber(MessageTypeReferenceData<L>),

    MessageSketchyNullCheck(Box<MessageSketchyNullCheckData<L>>),

    MessageSuppressionMalformedCode,
    MessageSuppressionMissingCode,

    MessageThisInComponent(L),
    MessageThisInExportedFunction,

    MessageThisSuperInObject {
        method_loc: L,
        method_name: Option<FlowSmolStr>,
        kind: ThisFinderKind,
    },

    MessageTSNeverType,
    MessageTSReadonlyOperatorOnArray,
    MessageTSReadonlyOperatorOnTuple,
    MessageTSReadonlyType,
    MessageTSVarianceInOut,
    MessageDeprecatedVarianceSigil(VarianceSigilKind),
    MessageTSClassAccessibility(ast::class::ts_accessibility::Kind),
    MessageTSParameterProperty,
    MessageAbstractClass,
    MessageAbstractMethod,
    MessageAbstract(AbstractErrorKind<L>),
    MessageOverride(OverrideErrorKind<L>),
    MessageTSUndefinedType,

    MessageTupleElementNotReadable(Box<MessageTupleElementNotReadableData<L>>),

    MessageTupleElementNotWritable(Box<MessageTupleElementNotWritableData<L>>),

    MessageTupleIndexOutOfBound(Box<MessageTupleIndexOutOfBoundData<L>>),

    MessageTupleNonIntegerIndex(Box<MessageTupleNonIntegerIndexData<L>>),

    MessageTupleNonStaticallyKnownIndex,

    MessageTuplePolarityMismatch {
        index: i32,
        lower: MessageTypeReferenceData<L>,
        upper: MessageTypeReferenceData<L>,
        polarity_lower: Polarity,
        polarity_upper: Polarity,
    },

    MessageTypeGuardIndexMismatch {
        lower: TypeGuardParameterData<L>,
        upper: TypeGuardParameterData<L>,
    },

    MessageTypeGuardImpliesMismatch {
        lower: TypeGuardReferenceData<L>,
        upper: TypeGuardReferenceData<L>,
    },

    MessageIncompatiblETypeParamConstIncompatibility {
        lower: NamedReferenceData<L>,
        upper: NamedReferenceData<L>,
    },

    MessageTypeParamConstInvalidPosition(NamedReferenceData<L>),
    MessageUnclearType,

    MessageUnderconstrainedImplicitInstantiaton {
        call: ImplicitInstantiationReferenceData<L>,
        type_param: NamedReferenceData<L>,
    },

    MessageUndocumentedFeature,

    MessageIllegalAssertOperator {
        obj: Box<ExpressionReferenceData<L>>,
        specialized: bool,
    },

    MessageUnexpectedUseOfThisType,

    MessageUninitializedInstanceProperty(PropertyAssignmentKind),

    MessageUninitializedProperty {
        name: FlowSmolStr,
        kind: UninitializedPropertyKind,
    },

    /// A value known only to be some function is called or flows into a function type. The loc is
    /// that value's.
    MessageUnknownParameterTypes(L),
    MessageUnnecessaryDeclareTypeOnlyExport,
    MessageUnnecessaryInvariant {
        callee: FlowSmolStr,
        condition: Box<MessageTypeReferenceData<L>>,
        condition_kind: UnnecessaryInvariantConditionKind,
    },
    MessageUnnecessaryOptionalChain {
        lhs_expression: ExpressionReferenceData<L>,
    },
    MessageUnreachableCode,
    MessageUnsafeGetterSetter,
    MessageUnsafeObjectAssign,

    MessageUnsupportedKeyInObject {
        key_error_kind: InvalidObjKey,
        obj_kind: ObjKind,
    },

    MessageUnsupportedComputedKeyInClass,

    MessageUnsupportedSyntax(UnsupportedSyntax),
    MessageUnsupportedVarianceAnnotation(FlowSmolStr),

    MessageUntypedImport(FlowSmolStr),
    MessageUntypedTypeImport(FlowSmolStr),

    MessageUnusedPromiseInAsyncScope,
    MessageUnusedPromiseInSyncScope,
    MessageUnusedSuppression,

    MessageValueUsedAsType {
        reference: Option<ValueAsTypeReference>,
        value: MessageTypeReferenceData<L>,
    },
    MessageVariableNeverInitAssignedAnnotated(NamedReferenceData<L>),

    MessageVariableOnlyAssignedByNull(Box<MessageVariableOnlyAssignedByNullData<L>>),

    MessageMatchNotExhaustive {
        examples: Vec<(
            FlowSmolStr,
            Vec<MatchExampleReference<L, MessageTypeReferenceData<L>>>,
        )>,
    },

    MessageMatchUnnecessaryPattern {
        pattern: MatchPatternReferenceData<L>,
        already_seen: Option<MatchPatternReferenceData<L>>,
    },

    MessageMatchNonExhaustiveObjectPattern(Box<MessageMatchNonExhaustiveObjectPatternData<L>>),

    MessageMatchNonExplicitEnumCheck(Box<MessageMatchNonExplicitEnumCheckData<L>>),

    MessageMatchInvalidGuardedWildcard,

    MessageMatchInvalidIdentOrMemberPattern {
        type_: Box<MessageTypeReferenceData<L>>,
    },

    MessageMatchInvalidBindingKind {
        kind: VariableKind,
    },

    MessageMatchInvalidObjectPropertyLiteral {
        pattern_kind: MatchObjPatternKind,
    },

    MessageMatchInvalidUnaryZero,
    MessageMatchInvalidUnaryPlusBigInt,

    MessageMatchDuplicateObjectProperty {
        name: FlowSmolStr,
        pattern_kind: MatchObjPatternKind,
    },

    MessageMatchBindingInOrPattern,
    MessageMatchInvalidAsPattern,

    MessageMatchInvalidPatternReference {
        binding: NamedReferenceData<L>,
    },

    MessageMatchInvalidObjectShorthand {
        name: FlowSmolStr,
        pattern_kind: MatchObjPatternKind,
    },

    MessageMatchStatementInvalidBody,

    MessageMatchInvalidCaseSyntax(MatchInvalidCaseSyntax<L>),
    MessageMatchInvalidWildcardSyntax,
    MessageMatchInvalidInstancePattern,

    MessageRecordBannedTypeUtil {
        op_loc: L,
        op: RecordBannedTypeUtilOp,
        record: MessageTypeReferenceData<L>,
    },

    MessageRecordInvalidNew {
        record_name: FlowSmolStr,
    },

    MessageRecordInvalidName {
        name: FlowSmolStr,
    },

    MessageRecordDeclarationInvalidSyntax(RecordDeclarationInvalidSyntax<L>),

    MessageConstantCondition {
        is_truthy: bool,
        warning: ConstantConditionWarning<L>,
        constant_condition_kind: ConstantConditionKind,
    },
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct IntermediateError<L: Dupe> {
    pub kind: ErrorKind,
    pub loc: Loc,
    pub error_code: Option<ErrorCode>,
    pub root: Option<(Loc, RootMessage<L>)>,
    pub message: ErrorMessage<L>,
    pub misplaced_source_file: Option<FileKey>,
    pub unsuppressable: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum ErrorMessage<L: Dupe> {
    SingletonMessage {
        message: Message<L>,
        frames: Option<Vec<Frame<L>>>,
        explanations: Option<Vec<Explanation<L>>>,
    },
    SingletonMessageWithExample {
        message: Message<L>,
        frames: Option<Vec<Frame<L>>>,
        explanations: Option<Vec<Explanation<L>>>,
        example: Box<IntermediateError<L>>,
    },
    SpeculationMessage {
        frames: Vec<Frame<L>>,
        explanations: Vec<Explanation<L>>,
        branches: Vec<(i32, IntermediateError<L>)>,
    },
}

impl Default for IntermediateError<Loc> {
    fn default() -> Self {
        IntermediateError {
            kind: ErrorKind::InferError,
            loc: Loc::default(),
            error_code: None,
            root: None,
            message: ErrorMessage::SingletonMessage {
                message: Message::MessagePlainTextReservedForInternalErrorOnly(
                    "default error".into(),
                ),
                frames: None,
                explanations: None,
            },
            misplaced_source_file: None,
            unsuppressable: false,
        }
    }
}
