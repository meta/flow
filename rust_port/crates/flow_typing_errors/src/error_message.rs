/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::hash::Hash;
use std::hash::Hasher;
use std::sync::Arc;
use std::time::Duration;

use dupe::Dupe;
use flow_aloc::ALoc;
use flow_common::error_ref::ErrorReference;
use flow_common::flow_import_specifier::Userland;
use flow_common::polarity::Polarity;
use flow_common::reason::Name;
use flow_common::reason::VirtualReason;
use flow_common::refinement_invalidation;
use flow_common_errors::error_codes::ErrorCode;
use flow_common_errors::error_utils::ErrorKind;
use flow_common_errors::error_utils::InferWarningKind;
use flow_common_errors::error_utils::friendly::MessageFeature;
use flow_common_errors::error_utils::friendly::code;
use flow_common_errors::error_utils::friendly::text;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_env_builder::env_api::AnnotLoc;
use flow_env_builder::env_api::DefLocType;
use flow_env_builder::env_api::EnvInvariantFailure;
use flow_env_builder::name_def_types::DefinitionReferenceKind;
use flow_lint_settings::lint_settings::LintParseError;
use flow_lint_settings::lints::LintKind;
use flow_lint_settings::lints::PropertyAssignmentKind;
use flow_lint_settings::lints::SketchyNullKind;
use flow_lint_settings::lints::SketchyNumberKind;
use flow_parser::ast;
use flow_parser::ast::VariableKind;
use flow_parser::ast::match_pattern::MatchPattern;
use flow_parser::ast::types::RendersVariant;
use flow_parser::loc::Loc;
use flow_parser::parse_error::ParseError;
use flow_parser_utils::graphql::GraphqlError;
use flow_type_sig::signature_error::BindingValidation;
use flow_type_sig::signature_error::SignatureError;
use flow_typing_type::type_::ArrayElementCompatibilityData;
use flow_typing_type::type_::AssertionFunctionConsistencyData;
use flow_typing_type::type_::AssignVarDefaultData;
use flow_typing_type::type_::AssignVarInit;
use flow_typing_type::type_::AssignVarTarget;
use flow_typing_type::type_::ClassExtendsCheckData;
use flow_typing_type::type_::ComponentRestParamCompatibilityData;
use flow_typing_type::type_::DroType;
use flow_typing_type::type_::EvalMappedTypeData;
use flow_typing_type::type_::FunImplicitReturnData;
use flow_typing_type::type_::FunMissingArgData;
use flow_typing_type::type_::FunParamData;
use flow_typing_type::type_::FunReturnData;
use flow_typing_type::type_::IndexedTypeAccessData;
use flow_typing_type::type_::IndexerKeyCompatibilityData;
use flow_typing_type::type_::MappedTypeKeyCompatibilityData;
use flow_typing_type::type_::MergedDeclarationData;
use flow_typing_type::type_::OpaqueTypeCustomErrorCompatibilityData;
use flow_typing_type::type_::PositiveTypeGuardConsistencyData;
use flow_typing_type::type_::PropertyCompatibilityData;
use flow_typing_type::type_::StandaloneCallThisData;
use flow_typing_type::type_::TupleElementCompatibilityData;
use flow_typing_type::type_::TypeGuardIncompatibilityData;
use flow_typing_type::type_::UnionEnum;
use flow_typing_type::type_::UnionRepresentativeData;
use flow_typing_type::type_::VirtualFrameUseOp;
use flow_typing_type::type_::VirtualRootUseOp;
use flow_typing_type::type_::VirtualUseOp;
use flow_typing_type::type_::aconstraint::AnnotationInferenceOperation;
use flow_typing_type::type_::fold_virtual_use_op;
use flow_typing_type::type_::type_or_type_desc;
use flow_typing_type::type_::type_or_type_desc::ErrorTypeDesc;
pub use flow_typing_type::type_::type_or_type_desc::TypeOrTypeDescT as TypeOrTypeDesc;
use flow_typing_type::type_::union_rep::OptimizedError;
use flow_typing_type::type_util::mod_loc_of_virtual_use_op;
use vec1::Vec1;

use crate::intermediate_error_types::AbstractErrorKind;
use crate::intermediate_error_types::AssignedConstLikeBindingType;
use crate::intermediate_error_types::ClassKind;
use crate::intermediate_error_types::ComponentReferenceData;
use crate::intermediate_error_types::ConstantConditionKind;
use crate::intermediate_error_types::ConstantConditionWarning;
use crate::intermediate_error_types::DeclareComponentInvalidParamKind;
use crate::intermediate_error_types::DocblockError;
use crate::intermediate_error_types::ExactnessErrorKind;
use crate::intermediate_error_types::ExpectedModulePurpose;
use crate::intermediate_error_types::Explanation;
use crate::intermediate_error_types::ExplanationAdditionalUnionMembersData;
use crate::intermediate_error_types::ExplanationConstrainedAssignData;
use crate::intermediate_error_types::ExplanationCustomErrorData;
use crate::intermediate_error_types::ExplanationInvariantSubtypingDueToMutableArrayData;
use crate::intermediate_error_types::ExplanationInvariantSubtypingDueToMutablePropertiesData;
use crate::intermediate_error_types::ExplanationInvariantSubtypingDueToMutablePropertyData;
use crate::intermediate_error_types::ExplanationPropertyMissingDueToNeutralOptionalPropertyData;
use crate::intermediate_error_types::ExplanationWithLazyParts;
use crate::intermediate_error_types::ExponentialSpreadOperandGroup;
use crate::intermediate_error_types::ExpressionReferenceData;
use crate::intermediate_error_types::ExpressionReferenceKind;
use crate::intermediate_error_types::FunctionReferenceData;
use crate::intermediate_error_types::ImplicitInstantiationReferenceData;
use crate::intermediate_error_types::IncorrectType;
use crate::intermediate_error_types::IncorrectTypeErrorType;
use crate::intermediate_error_types::InternalType;
use crate::intermediate_error_types::InvalidObjKey;
use crate::intermediate_error_types::InvalidRenderTypeKind;
use crate::intermediate_error_types::LowerRequirement;
use crate::intermediate_error_types::MatchInvalidCaseSyntax;
use crate::intermediate_error_types::MatchObjPatternKind;
use crate::intermediate_error_types::MatchPatternReferenceData;
use crate::intermediate_error_types::Message;
use crate::intermediate_error_types::MessageAlreadyExhaustivelyCheckOneEnumMemberData;
use crate::intermediate_error_types::MessageCannotAccessEnumMemberData;
use crate::intermediate_error_types::MessageCannotAddComputedPropertyDueToPotentialOverwriteData;
use crate::intermediate_error_types::MessageCannotCallMaybeReactHookData;
use crate::intermediate_error_types::MessageCannotCompareData;
use crate::intermediate_error_types::MessageCannotExhaustivelyCheckAbstractEnumsData;
use crate::intermediate_error_types::MessageCannotExhaustivelyCheckEnumWithUnknownsData;
use crate::intermediate_error_types::MessageCannotExportRenamedDefaultData;
use crate::intermediate_error_types::MessageCannotImportGlobalLibdefData;
use crate::intermediate_error_types::MessageCannotInstantiateObjectUtilTypeWithEnumData;
use crate::intermediate_error_types::MessageCannotResolveBuiltinModuleData;
use crate::intermediate_error_types::MessageCannotSpreadGeneralData;
use crate::intermediate_error_types::MessageCannotSpreadInexactMayOverwriteIndexerData;
use crate::intermediate_error_types::MessageCannotUseEnumMemberUsedAsTypeData;
use crate::intermediate_error_types::MessageCannotUseTypeForAnnotationInferenceData;
use crate::intermediate_error_types::MessageCannotUseTypeGuardWithFunctionParamHavocedData;
use crate::intermediate_error_types::MessageCannotUseTypeInValuePositionData;
use crate::intermediate_error_types::MessageConstructSignatureMissingData;
use crate::intermediate_error_types::MessageDefinitionInvalidRecursiveData;
use crate::intermediate_error_types::MessageDuplicateModuleProviderData;
use crate::intermediate_error_types::MessageEnumDuplicateMemberNameData;
use crate::intermediate_error_types::MessageEnumInvalidMemberInitializerData;
use crate::intermediate_error_types::MessageExponentialSpreadData;
use crate::intermediate_error_types::MessageIncompatibleGeneralWithPrintedTypesData;
use crate::intermediate_error_types::MessageIncompatibleTupleArityData;
use crate::intermediate_error_types::MessageIncompleteExhausiveCheckEnumData;
use crate::intermediate_error_types::MessageInvalidEnumMemberCheckData;
use crate::intermediate_error_types::MessageInvalidKeyPropertyInSpreadData;
use crate::intermediate_error_types::MessageInvalidRendersTypeArgumentData;
use crate::intermediate_error_types::MessageInvalidSelfReferencingDefaultData;
use crate::intermediate_error_types::MessageInvalidSelfReferencingTypeAnnotationData;
use crate::intermediate_error_types::MessageMatchNonExhaustiveObjectPatternData;
use crate::intermediate_error_types::MessageMatchNonExplicitEnumCheckData;
use crate::intermediate_error_types::MessageMissingPlatformSupportWithAvailablePlatformsData;
use crate::intermediate_error_types::MessageNoDefaultExportData;
use crate::intermediate_error_types::MessageNoNamedExportData;
use crate::intermediate_error_types::MessageOnlyDefaultExportData;
use crate::intermediate_error_types::MessageReactIntrinsicOverlapData;
use crate::intermediate_error_types::MessageRedeclareComponentPropData;
use crate::intermediate_error_types::MessageShouldAnnotateVariableUsedInGenericContextData;
use crate::intermediate_error_types::MessageSketchyNullCheckData;
use crate::intermediate_error_types::MessageTupleElementNotReadableData;
use crate::intermediate_error_types::MessageTupleElementNotWritableData;
use crate::intermediate_error_types::MessageTupleIndexOutOfBoundData;
use crate::intermediate_error_types::MessageTupleNonIntegerIndexData;
use crate::intermediate_error_types::MessageTypeReferenceData;
use crate::intermediate_error_types::MessageVariableOnlyAssignedByNullData;
use crate::intermediate_error_types::NamedReferenceData;
use crate::intermediate_error_types::ObjKind;
use crate::intermediate_error_types::OverrideErrorKind;
use crate::intermediate_error_types::PrimitiveKind;
use crate::intermediate_error_types::PropPolarityMismatchObject;
use crate::intermediate_error_types::RecordBannedTypeUtilOp;
use crate::intermediate_error_types::RecordDeclarationInvalidSyntax;
use crate::intermediate_error_types::StrictComparisonInfo;
use crate::intermediate_error_types::SubComponentOfInvariantSubtypingError;
use crate::intermediate_error_types::TupleElementReferenceData;
use crate::intermediate_error_types::TypeGuardReferenceData;
use crate::intermediate_error_types::TypeGuardReferenceKind;
use crate::intermediate_error_types::UnnecessaryInvariantConditionKind;
use crate::intermediate_error_types::UnrecognizedAssertionCallee;
use crate::intermediate_error_types::UnsupportedSyntax;
use crate::intermediate_error_types::ValueAsTypeReference;

/// Data struct for boxed `ErrorMessage::EIncompatibleSpeculation` variant.
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
pub struct EIncompatibleSpeculationData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub loc: L,
    pub use_op: Option<VirtualUseOp<L>>,
    pub branches: Vec<ErrorMessage<L>>,
}

/// Data struct for boxed `ErrorMessage::EPropsNotFoundInInvariantSubtyping` variant.
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
pub struct EPropsNotFoundInInvariantSubtypingData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub prop_names: Vec1<Name>,
    pub lower: ErrorTypeReferenceWithLocData<L>,
    pub upper: ErrorTypeReferenceWithLocData<L>,
    pub use_op: VirtualUseOp<L>,
}

/// Data struct for boxed `ErrorMessage::EUnionSpeculationFailed` variant.
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
pub struct EUnionSpeculationFailedData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub use_op: VirtualUseOp<L>,
    pub loc: L,
    pub branches: Vec<ErrorMessage<L>>,
}

/// Data struct for boxed `ErrorMessage::EInvariantSubtypingWithUseOp` variant.
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
pub struct EInvariantSubtypingWithUseOpData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub sub_component: Option<SubComponentOfInvariantSubtypingError>,
    pub use_op: VirtualUseOp<L>,
    pub lower_loc: L,
    pub upper_loc: L,
    pub lower_desc: TypeOrTypeDesc<L>,
    pub upper_desc: TypeOrTypeDesc<L>,
    pub explanation: Option<ExplanationWithLazyParts<L>>,
}

/// Data struct for boxed `ErrorMessage::EUnionPartialOptimizationNonUniqueKey` variant.
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
pub struct EUnionPartialOptimizationNonUniqueKeyData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub loc: L,
    pub non_unique_keys:
        BTreeMap<Name, BTreeMap<UnionEnum, Vec1<ErrorTypeReferenceWithLocData<L>>>>,
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
pub struct EnumInvalidMemberAccessData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub member: EnumMemberAccess<L>,
    pub member_loc: L,
    pub enum_: EnumReferenceData<L>,
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
pub enum EnumMemberAccess<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    /// `E.name`
    Named {
        name: Name,
        suggestion: Option<FlowSmolStr>,
    },
    /// `E[key]`
    Computed(ErrorTypeReferenceData<L>),
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
pub struct EnumModificationData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub loc: L,
    pub enum_: EnumReferenceData<L>,
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
pub struct EnumReferenceData<L: Dupe> {
    pub loc: L,
    /// `None` for an abstract enum.
    pub name: Option<FlowSmolStr>,
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
pub struct EnumMemberDuplicateValueData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub loc: L,
    pub prev_use_loc: L,
    pub enum_: EnumReferenceData<L>,
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
pub struct EnumInvalidObjectUtilTypeData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub operation_loc: L,
    pub operation_name: FlowSmolStr,
    pub enum_: EnumReferenceData<L>,
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
pub struct EnumInvalidObjectFunctionData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub operation_loc: L,
    pub function_name: FlowSmolStr,
    pub enum_: EnumReferenceData<L>,
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
pub struct EnumNotIterableData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub enum_: EnumReferenceData<L>,
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
pub struct EnumNotIterableForInData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub loc: L,
    pub enum_: EnumReferenceData<L>,
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
pub struct EnumMemberAlreadyCheckedData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub case_test_loc: L,
    pub prev_check_loc: L,
    pub enum_: NamedReferenceData<L>,
    pub member_name: FlowSmolStr,
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
pub struct EnumAllMembersAlreadyCheckedData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub loc: L,
    pub enum_: NamedReferenceData<L>,
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
pub struct EnumNotAllCheckedData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub loc: L,
    pub description_name: Option<FlowSmolStr>,
    pub enum_: NamedReferenceData<L>,
    pub left_to_check: Vec<FlowSmolStr>,
    pub default_case_loc: Option<L>,
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
pub struct EnumUnknownNotCheckedData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub loc: L,
    pub description_name: Option<FlowSmolStr>,
    pub enum_: NamedReferenceData<L>,
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
pub struct EnumInvalidCheckData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub loc: L,
    pub enum_: EnumReferenceData<L>,
    pub example_member: Option<FlowSmolStr>,
    pub from_match: bool,
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
pub struct EnumMemberUsedAsTypeData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub loc: L,
    pub description_name: Option<FlowSmolStr>,
    pub enum_: EnumReferenceData<L>,
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
pub struct EnumInvalidAbstractUseData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub loc: L,
    pub description_name: Option<FlowSmolStr>,
    pub enum_loc: L,
    pub enum_name: Option<FlowSmolStr>,
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
pub struct EnumInvalidMemberNameData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub loc: L,
    pub enum_: EnumReferenceData<L>,
    pub member_name: String,
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
pub struct EnumNonIdentifierMemberNameData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub loc: L,
    pub enum_: EnumReferenceData<L>,
    pub member_name: String,
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
pub struct EnumDuplicateMemberNameData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub loc: L,
    pub prev_use_loc: L,
    pub enum_: EnumReferenceData<L>,
    pub member_name: String,
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
pub struct EnumInconsistentMemberValuesData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub loc: L,
    pub enum_: EnumReferenceData<L>,
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
pub struct EnumInvalidMemberInitializerData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub loc: L,
    pub enum_: EnumReferenceData<L>,
    pub explicit_type: Option<flow_parser::ast::statement::enum_declaration::ExplicitType>,
    pub member_name: String,
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
pub struct EnumBooleanMemberNotInitializedData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub loc: L,
    pub enum_: EnumReferenceData<L>,
    pub member_name: String,
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
pub struct EnumNumberMemberNotInitializedData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub loc: L,
    pub enum_: EnumReferenceData<L>,
    pub member_name: String,
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
pub struct EnumBigIntMemberNotInitializedData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub loc: L,
    pub enum_: EnumReferenceData<L>,
    pub member_name: String,
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
pub struct EnumStringMemberInconsistentlyInitializedData<
    L: Dupe + PartialOrd + Ord + PartialEq + Eq,
> {
    pub loc: L,
    pub enum_: EnumReferenceData<L>,
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
pub struct TSEnumInvalidMemberData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub loc: L,
    pub enum_: EnumReferenceData<L>,
    pub member_name: String,
    pub kind: crate::intermediate_error_types::TsEnumInvalidMemberKind,
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
pub struct TSEnumInvalidSyntaxData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub loc: L,
    pub enum_: EnumReferenceData<L>,
    pub kind: crate::intermediate_error_types::TsEnumInvalidSyntaxKind,
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
pub enum EnumErrorKind<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    EnumsNotEnabled(L),
    EnumConstNotSupported(L),
    EnumInvalidMemberAccess(Box<EnumInvalidMemberAccessData<L>>),
    EnumModification(Box<EnumModificationData<L>>),
    EnumMemberDuplicateValue(Box<EnumMemberDuplicateValueData<L>>),
    EnumInvalidObjectUtilType(Box<EnumInvalidObjectUtilTypeData<L>>),
    EnumInvalidObjectFunction(Box<EnumInvalidObjectFunctionData<L>>),
    EnumNotIterable(Box<EnumNotIterableData<L>>),
    EnumNotIterableForIn(Box<EnumNotIterableForInData<L>>),
    EnumMemberAlreadyChecked(Box<EnumMemberAlreadyCheckedData<L>>),
    EnumAllMembersAlreadyChecked(Box<EnumAllMembersAlreadyCheckedData<L>>),
    EnumNotAllChecked(Box<EnumNotAllCheckedData<L>>),
    EnumUnknownNotChecked(Box<EnumUnknownNotCheckedData<L>>),
    EnumInvalidCheck(Box<EnumInvalidCheckData<L>>),
    EnumMemberUsedAsType(Box<EnumMemberUsedAsTypeData<L>>),
    EnumIncompatible(Box<EIncompatibleTypesWithUseOpData<L>>),
    EnumInvalidAbstractUse(Box<EnumInvalidAbstractUseData<L>>),
    EnumInvalidMemberName(Box<EnumInvalidMemberNameData<L>>),
    EnumNonIdentifierMemberName(Box<EnumNonIdentifierMemberNameData<L>>),
    EnumDuplicateMemberName(Box<EnumDuplicateMemberNameData<L>>),
    EnumInconsistentMemberValues(Box<EnumInconsistentMemberValuesData<L>>),
    EnumInvalidMemberInitializer(Box<EnumInvalidMemberInitializerData<L>>),
    EnumBooleanMemberNotInitialized(Box<EnumBooleanMemberNotInitializedData<L>>),
    EnumNumberMemberNotInitialized(Box<EnumNumberMemberNotInitializedData<L>>),
    EnumBigIntMemberNotInitialized(Box<EnumBigIntMemberNotInitializedData<L>>),
    EnumStringMemberInconsistentlyInitialized(
        Box<EnumStringMemberInconsistentlyInitializedData<L>>,
    ),
    TSEnumInvalidMember(Box<TSEnumInvalidMemberData<L>>),
    TSEnumInvalidSyntax(Box<TSEnumInvalidSyntaxData<L>>),
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
pub struct MatchNotExhaustiveData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub loc: L,
    pub examples: Vec<(
        FlowSmolStr,
        Vec<MatchExampleReference<L, ErrorTypeReferenceWithLocData<L>>>,
    )>,
    pub missing_pattern_asts: Vec<MatchPattern<Loc, Loc>>,
}

/// What a missing `match` pattern would match.
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
pub enum MatchExampleReference<L: Dupe, T> {
    Type(T),
    EnumMember {
        loc: L,
        member_name: FlowSmolStr,
        enum_name: FlowSmolStr,
    },
    /// The members of an enum declared with `...`.
    EnumUnknownMembers {
        loc: L,
        enum_name: FlowSmolStr,
    },
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
pub struct MatchUnusedPatternData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub pattern: MatchPatternReferenceData<L>,
    pub already_seen: Option<MatchPatternReferenceData<L>>,
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
pub struct MatchNonExhaustiveObjectPatternData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub loc: L,
    pub rest: Option<ErrorTypeReferenceData<L>>,
    pub missing_props: Vec<FlowSmolStr>,
    pub pattern_kind: MatchObjPatternKind,
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
pub struct MatchNonExplicitEnumCheckData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub loc: L,
    pub wildcard_loc: L,
    pub unchecked_members: Vec<FlowSmolStr>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MatchInvalidIdentOrMemberPatternData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub loc: L,
    pub type_loc: L,
    pub type_desc: TypeOrTypeDesc<L>,
}

impl<L: Dupe + PartialOrd + Ord + PartialEq + Eq> PartialEq
    for MatchInvalidIdentOrMemberPatternData<L>
{
    fn eq(&self, other: &Self) -> bool {
        self.loc == other.loc && self.type_loc == other.type_loc
    }
}

impl<L: Dupe + PartialOrd + Ord + PartialEq + Eq> Eq for MatchInvalidIdentOrMemberPatternData<L> {}

impl<L: Dupe + PartialOrd + Ord + PartialEq + Eq + Hash> Hash
    for MatchInvalidIdentOrMemberPatternData<L>
{
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.loc.hash(state);
        self.type_loc.hash(state);
    }
}

impl<L: Dupe + PartialOrd + Ord + PartialEq + Eq> PartialOrd
    for MatchInvalidIdentOrMemberPatternData<L>
{
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<L: Dupe + PartialOrd + Ord + PartialEq + Eq> Ord for MatchInvalidIdentOrMemberPatternData<L> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.loc
            .cmp(&other.loc)
            .then_with(|| self.type_loc.cmp(&other.type_loc))
    }
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
pub struct MatchDuplicateObjectPropertyData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub loc: L,
    pub name: FlowSmolStr,
    pub pattern_kind: MatchObjPatternKind,
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
pub struct MatchInvalidPatternReferenceData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub loc: L,
    pub binding_loc: L,
    pub name: FlowSmolStr,
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
pub struct MatchInvalidObjectShorthandData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub loc: L,
    pub name: FlowSmolStr,
    pub pattern_kind: MatchObjPatternKind,
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
pub struct MatchInvalidCaseSyntaxData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub loc: L,
    pub kind: MatchInvalidCaseSyntax<L>,
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
pub enum MatchErrorKind<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    MatchNotExhaustive(Box<MatchNotExhaustiveData<L>>),
    MatchUnusedPattern(Box<MatchUnusedPatternData<L>>),
    MatchNonExhaustiveObjectPattern(Box<MatchNonExhaustiveObjectPatternData<L>>),
    MatchNonExplicitEnumCheck(Box<MatchNonExplicitEnumCheckData<L>>),
    MatchInvalidGuardedWildcard(L),
    MatchInvalidIdentOrMemberPattern(Box<MatchInvalidIdentOrMemberPatternData<L>>),
    MatchInvalidBindingKind {
        loc: L,
        kind: VariableKind,
    },
    MatchInvalidObjectPropertyLiteral {
        loc: L,
        pattern_kind: MatchObjPatternKind,
    },
    MatchInvalidUnaryZero {
        loc: L,
    },
    MatchInvalidUnaryPlusBigInt {
        loc: L,
    },
    MatchDuplicateObjectProperty(Box<MatchDuplicateObjectPropertyData<L>>),
    MatchBindingInOrPattern {
        loc: L,
    },
    MatchInvalidAsPattern {
        loc: L,
    },
    MatchInvalidPatternReference(Box<MatchInvalidPatternReferenceData<L>>),
    MatchInvalidObjectShorthand(Box<MatchInvalidObjectShorthandData<L>>),
    MatchStatementInvalidBody {
        loc: L,
    },
    MatchInvalidCaseSyntax(Box<MatchInvalidCaseSyntaxData<L>>),
    MatchInvalidWildcardSyntax(L),
    MatchInvalidInstancePattern(L),
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
pub enum RecordErrorKind<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    RecordBannedTypeUtil {
        op_loc: L,
        op: RecordBannedTypeUtilOp,
        record: ErrorTypeReferenceWithLocData<L>,
    },
    RecordInvalidName {
        loc: L,
        name: FlowSmolStr,
    },
    RecordInvalidNew {
        loc: L,
        record_name: FlowSmolStr,
    },
    RecordDeclarationInvalidSyntax {
        loc: L,
        kind: RecordDeclarationInvalidSyntax<L>,
    },
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EAnnotationInferenceData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub loc: L,
    pub operation: AnnotationInferenceOperation,
    pub operation_loc: L,
    pub target_loc: L,
    pub target_desc: ErrorTypeDesc<L>,
}

// The normalized target type is presentation-only, so it does not affect error identity.
impl<L: Dupe + PartialOrd + Ord + PartialEq + Eq> PartialEq for EAnnotationInferenceData<L> {
    fn eq(&self, other: &Self) -> bool {
        self.loc == other.loc
            && self.operation == other.operation
            && self.operation_loc == other.operation_loc
            && self.target_loc == other.target_loc
    }
}

impl<L: Dupe + PartialOrd + Ord + PartialEq + Eq> Eq for EAnnotationInferenceData<L> {}

impl<L: Dupe + PartialOrd + Ord + PartialEq + Eq + Hash> Hash for EAnnotationInferenceData<L> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.loc.hash(state);
        self.operation.hash(state);
        self.operation_loc.hash(state);
        self.target_loc.hash(state);
    }
}

impl<L: Dupe + PartialOrd + Ord + PartialEq + Eq> PartialOrd for EAnnotationInferenceData<L> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<L: Dupe + PartialOrd + Ord + PartialEq + Eq> Ord for EAnnotationInferenceData<L> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.loc
            .cmp(&other.loc)
            .then_with(|| self.operation.cmp(&other.operation))
            .then_with(|| self.operation_loc.cmp(&other.operation_loc))
            .then_with(|| self.target_loc.cmp(&other.target_loc))
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EIncompatibleTypeData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub lower_kind: Option<LowerKind>,
    pub lower_loc: L,
    pub lower_def_loc: L,
    pub lower_desc: TypeOrTypeDesc<L>,
    pub upper: IncompatibleUpperData<L>,
    pub use_op: Option<VirtualUseOp<L>>,
}

// The normalized type is presentation-only, so it does not affect error identity.
impl<L: Dupe + PartialOrd + Ord + PartialEq + Eq> PartialEq for EIncompatibleTypeData<L> {
    fn eq(&self, other: &Self) -> bool {
        self.lower_loc == other.lower_loc
            && self.lower_def_loc == other.lower_def_loc
            && self.lower_kind == other.lower_kind
            && self.upper == other.upper
            && self.use_op == other.use_op
    }
}

impl<L: Dupe + PartialOrd + Ord + PartialEq + Eq> Eq for EIncompatibleTypeData<L> {}

impl<L: Dupe + PartialOrd + Ord + PartialEq + Eq + Hash> Hash for EIncompatibleTypeData<L> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.lower_loc.hash(state);
        self.lower_def_loc.hash(state);
        self.lower_kind.hash(state);
        self.upper.hash(state);
        self.use_op.hash(state);
    }
}

impl<L: Dupe + PartialOrd + Ord + PartialEq + Eq> PartialOrd for EIncompatibleTypeData<L> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<L: Dupe + PartialOrd + Ord + PartialEq + Eq> Ord for EIncompatibleTypeData<L> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.lower_loc
            .cmp(&other.lower_loc)
            .then_with(|| self.lower_def_loc.cmp(&other.lower_def_loc))
            .then_with(|| self.lower_kind.cmp(&other.lower_kind))
            .then_with(|| self.upper.cmp(&other.upper))
            .then_with(|| self.use_op.cmp(&other.use_op))
    }
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
pub struct IncompatibleUpperData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub loc: L,
    pub kind: UpperKind<L>,
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
pub struct EIncompatiblePropData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub prop: Option<Name>,
    pub prop_loc: L,
    pub object: ErrorTypeReferenceWithLocData<L>,
    pub special: Option<LowerKind>,
    pub use_op: Option<VirtualUseOp<L>>,
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
pub struct EMissingTypeArgsData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub loc: L,
    pub type_: ErrorTypeReferenceWithLocData<L>,
    pub arity_loc: L,
    pub min_arity: i32,
    pub max_arity: i32,
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
pub struct EExpectedStringLitData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub lower: ErrorTypeReferenceWithLocData<L>,
    pub upper: ErrorTypeReferenceWithLocData<L>,
    pub use_op: VirtualUseOp<L>,
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
pub struct EExpectedNumberLitData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub lower: ErrorTypeReferenceWithLocData<L>,
    pub upper: ErrorTypeReferenceWithLocData<L>,
    pub use_op: VirtualUseOp<L>,
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
pub struct EExpectedBooleanLitData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub lower: ErrorTypeReferenceWithLocData<L>,
    pub upper: ErrorTypeReferenceWithLocData<L>,
    pub use_op: VirtualUseOp<L>,
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
pub struct EExpectedBigIntLitData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub lower: ErrorTypeReferenceWithLocData<L>,
    pub upper: ErrorTypeReferenceWithLocData<L>,
    pub use_op: VirtualUseOp<L>,
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
pub struct EPropNotFoundInLookupData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub prop_name: Option<Name>,
    pub prop_loc: L,
    pub object: ErrorTypeReferenceWithLocData<L>,
    pub use_op: VirtualUseOp<L>,
    pub suggestion: Option<FlowSmolStr>,
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
pub struct EPropNotFoundInSubtypingData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub prop_name: Option<Name>,
    pub lower: ErrorTypeReferenceWithLocData<L>,
    pub upper: ErrorTypeReferenceWithLocData<L>,
    pub use_op: VirtualUseOp<L>,
    pub suggestion: Option<FlowSmolStr>,
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
pub struct EPropsNotFoundInSubtypingData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub prop_names: Vec1<Name>,
    pub lower: ErrorTypeReferenceWithLocData<L>,
    pub upper: ErrorTypeReferenceWithLocData<L>,
    pub use_op: VirtualUseOp<L>,
}

// Construct signature ([new(): T] on an interface) is missing from the lower
// where the upper interface expects one. Distinct from [EPropNotFoundInSubtyping]
// because construct sigs live in their own [inst_construct_t] slot, not in
// [own_props] — there is no [name] to thread through.
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
pub struct EConstructSignatureMissingInSubtypingData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub lower: ErrorTypeReferenceWithLocData<L>,
    pub upper: ErrorTypeReferenceWithLocData<L>,
    pub use_op: VirtualUseOp<L>,
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
pub struct EPropsExtraAgainstExactObjectData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub prop_names: Vec1<Name>,
    pub lower: ErrorTypeReferenceWithLocData<L>,
    pub upper: ErrorTypeReferenceWithLocData<L>,
    pub upper_is_record: bool,
    pub use_op: VirtualUseOp<L>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EIndexerCheckFailedData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub prop_name: Name,
    pub lower: ErrorTypeReferenceWithLocData<L>,
    pub upper: ErrorTypeReferenceWithLocData<L>,
    pub indexer_loc: L,
    pub indexer_desc: TypeOrTypeDesc<L>,
    pub use_op: VirtualUseOp<L>,
}

impl<L: Dupe + PartialOrd + Ord + PartialEq + Eq> PartialEq for EIndexerCheckFailedData<L> {
    fn eq(&self, other: &Self) -> bool {
        self.prop_name == other.prop_name
            && self.lower == other.lower
            && self.upper == other.upper
            && self.indexer_loc == other.indexer_loc
            && self.use_op == other.use_op
    }
}

impl<L: Dupe + PartialOrd + Ord + PartialEq + Eq> Eq for EIndexerCheckFailedData<L> {}

impl<L: Dupe + PartialOrd + Ord + PartialEq + Eq + Hash> Hash for EIndexerCheckFailedData<L> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.prop_name.hash(state);
        self.lower.hash(state);
        self.upper.hash(state);
        self.indexer_loc.hash(state);
        self.use_op.hash(state);
    }
}

impl<L: Dupe + PartialOrd + Ord + PartialEq + Eq> PartialOrd for EIndexerCheckFailedData<L> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<L: Dupe + PartialOrd + Ord + PartialEq + Eq> Ord for EIndexerCheckFailedData<L> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.prop_name
            .cmp(&other.prop_name)
            .then_with(|| self.lower.cmp(&other.lower))
            .then_with(|| self.upper.cmp(&other.upper))
            .then_with(|| self.indexer_loc.cmp(&other.indexer_loc))
            .then_with(|| self.use_op.cmp(&other.use_op))
    }
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
pub struct EPropNotReadableData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub prop_loc: L,
    pub prop_name: Option<Name>,
    pub use_op: VirtualUseOp<L>,
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
pub struct EPropNotWritableData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub prop_loc: L,
    pub prop_name: Option<Name>,
    pub use_op: VirtualUseOp<L>,
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
pub struct EPrivateLookupFailedData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub loc: L,
    pub object: ErrorTypeReferenceWithLocData<L>,
    pub prop_name: Name,
    pub use_op: VirtualUseOp<L>,
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
pub struct EPropPolarityMismatchData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub lower: PropPolarityMismatchObject<ErrorTypeReferenceWithLocData<L>, L>,
    pub upper: PropPolarityMismatchObject<ErrorTypeReferenceWithLocData<L>, L>,
    pub props: Vec1<(Option<Name>, (Polarity, Polarity))>,
    pub use_op: VirtualUseOp<L>,
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
pub struct EPolarityMismatchData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub loc: L,
    pub type_param_loc: L,
    pub name: FlowSmolStr,
    pub expected_polarity: Polarity,
    pub actual_polarity: Polarity,
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
pub struct EBuiltinNameLookupFailedData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub loc: L,
    pub name: FlowSmolStr,
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
pub struct EUmdGlobalValueInModuleData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub loc: L,
    pub name: FlowSmolStr,
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
pub struct EBuiltinModuleLookupFailedData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub loc: L,
    pub name: FlowSmolStr,
    pub potential_generator: Option<FlowSmolStr>,
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
pub struct EExpectedModuleLookupFailedData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub loc: L,
    pub name: FlowSmolStr,
    pub expected_module_purpose: ExpectedModulePurpose,
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
pub struct ECannotImportGlobalLibdefData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub loc: L,
    /// The import specifier as written, e.g. `./globals`.
    pub module_name: FlowSmolStr,
    /// The resolved global libdef file, e.g. `.../globals.js`.
    pub libdef_name: FlowSmolStr,
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
pub struct EPlatformSpecificImplementationModuleLookupFailedData<
    L: Dupe + PartialOrd + Ord + PartialEq + Eq,
> {
    pub loc: L,
    pub name: FlowSmolStr,
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
pub struct EComparisonData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub r1: ErrorTypeReferenceWithLocData<L>,
    pub r2: ErrorTypeReferenceWithLocData<L>,
    pub loc_opt: Option<L>,
    pub strict_comparison_opt: Option<StrictComparisonInfo<L, ErrorTypeReferenceWithLocData<L>>>,
}

/// Types rejected by a non-strict equality comparison.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ENonStrictEqualityComparisonData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub lower: ErrorTypeReferenceWithLocData<L>,
    pub upper: ErrorTypeReferenceWithLocData<L>,
}

impl<L: Dupe + PartialOrd + Ord + PartialEq + Eq> PartialEq
    for ENonStrictEqualityComparisonData<L>
{
    fn eq(&self, other: &Self) -> bool {
        self.lower == other.lower && self.upper == other.upper
    }
}

impl<L: Dupe + PartialOrd + Ord + PartialEq + Eq> Eq for ENonStrictEqualityComparisonData<L> {}

impl<L: Dupe + PartialOrd + Ord + PartialEq + Eq + Hash> Hash
    for ENonStrictEqualityComparisonData<L>
{
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.lower.hash(state);
        self.upper.hash(state);
    }
}

impl<L: Dupe + PartialOrd + Ord + PartialEq + Eq> PartialOrd
    for ENonStrictEqualityComparisonData<L>
{
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<L: Dupe + PartialOrd + Ord + PartialEq + Eq> Ord for ENonStrictEqualityComparisonData<L> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
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
    Hash,
    PartialOrd,
    Ord,
    serde::Serialize,
    serde::Deserialize
)]
pub struct ETupleArityMismatchData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub use_op: VirtualUseOp<L>,
    pub lower: ErrorTypeReferenceWithLocData<L>,
    pub lower_arity: (i32, i32),
    pub lower_inexact: bool,
    pub lower_rest: bool,
    pub upper: ErrorTypeReferenceWithLocData<L>,
    pub upper_arity: (i32, i32),
    pub upper_inexact: bool,
    pub upper_rest: bool,
    pub unify: bool,
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
pub struct ETupleOutOfBoundsData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub use_op: VirtualUseOp<L>,
    pub loc: L,
    pub tuple: ErrorTypeReferenceWithLocData<L>,
    pub inexact: bool,
    pub length: i32,
    pub index: FlowSmolStr,
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
pub struct ETupleNonIntegerIndexData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub use_op: VirtualUseOp<L>,
    pub loc: L,
    pub index_def_loc: L,
    pub index: FlowSmolStr,
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
pub struct ETupleElementNotReadableData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub loc: L,
    pub index_def_loc: L,
    pub index: i32,
    pub name: Option<FlowSmolStr>,
    pub use_op: VirtualUseOp<L>,
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
pub struct ETupleElementNotWritableData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub loc: L,
    pub index_def_loc: L,
    pub index: i32,
    pub name: Option<FlowSmolStr>,
    pub use_op: VirtualUseOp<L>,
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
pub struct ETupleElementPolarityMismatchData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub index: i32,
    pub lower: ErrorTypeReferenceWithLocData<L>,
    pub polarity_lower: Polarity,
    pub upper: ErrorTypeReferenceWithLocData<L>,
    pub polarity_upper: Polarity,
    pub use_op: VirtualUseOp<L>,
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
pub struct ETupleRequiredAfterOptionalData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub tuple_loc: L,
    pub required: TupleElementReferenceData<L>,
    pub optional: TupleElementReferenceData<L>,
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
pub struct ETupleInvalidTypeSpreadData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub spread_loc: L,
    pub argument: ErrorTypeReferenceWithLocData<L>,
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
pub struct ECallTypeArityData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub call_loc: L,
    pub is_new: bool,
    pub callee: CallTypeArityCallee<ErrorTypeReferenceData<L>>,
    pub expected_arity: i32,
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
pub enum CallTypeArityCallee<T> {
    Type(T),
    /// A builtin checked syntactically, such as `new Array<T>(..)`. There is no callee type.
    Builtin(FlowSmolStr),
    /// A call checked syntactically, such as `require<T>(..)`. There is no callee type.
    Function,
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
pub struct EUnsupportedExactData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub value_loc: L,
    pub value_desc: TypeOrTypeDesc<L>,
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
pub struct EInvalidExactObjectIntersectionData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub loc: L,
    pub prop: Name,
    pub exact_object_loc: L,
    pub exact_object_desc: TypeOrTypeDesc<L>,
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
pub struct ETooManyTypeArgsData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub reason_tapp: ErrorReference<L>,
    pub arity_loc: L,
    pub maximum_arity: i32,
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
pub struct ETooFewTypeArgsData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub reason_tapp: ErrorReference<L>,
    pub arity_loc: L,
    pub minimum_arity: i32,
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
pub struct EConstantConditionData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub loc: L,
    pub is_truthy: bool,
    pub warning: ConstantConditionWarning<L>,
    pub constant_condition_kind: ConstantConditionKind,
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
pub enum TypeGuardFuncReference<L: Dupe, T> {
    Type(T),
    /// A type guard that has no guard type.
    TypeGuard(TypeGuardReferenceData<L>),
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
pub struct TypeGuardParameterData<L: Dupe> {
    pub loc: L,
    pub name: FlowSmolStr,
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
pub enum TypeGuardBindingKind {
    RestParameter,
    PatternParameter,
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
pub struct ETypeGuardInvalidParameterData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub type_guard: TypeGuardParameterData<L>,
    pub binding_loc: L,
    pub binding_kind: TypeGuardBindingKind,
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
pub struct ETypeGuardFunctionInvalidWritesData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub loc: L,
    pub type_guard: NamedReferenceData<L>,
    pub write_locs: Vec<L>,
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
pub struct ETypeGuardFunctionParamHavocedData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub type_guard: TypeGuardParameterData<L>,
    pub param_loc: L,
    pub is_this: bool,
    pub call_locs: Vec<L>,
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
pub struct ETypeGuardIncompatibleWithFunctionKindData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub loc: L,
    pub kind: FlowSmolStr,
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
pub struct EAssertionFunctionFalsyAtExitData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub loc: L,
    pub param: NamedReferenceData<L>,
    pub falsy: ErrorTypeReferenceData<L>,
    pub implicit_return: bool,
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
pub struct EUnrecognizedAssertionCallData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub loc: L,
    pub callee: UnrecognizedAssertionCallee<L>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ENegativeTypeGuardConsistencyData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub return_reason: ErrorReference<L>,
    pub type_: ErrorTypeReferenceData<L>,
}

impl<L: Dupe + PartialOrd + Ord + PartialEq + Eq> PartialEq
    for ENegativeTypeGuardConsistencyData<L>
{
    fn eq(&self, other: &Self) -> bool {
        self.return_reason == other.return_reason && self.type_ == other.type_
    }
}

impl<L: Dupe + PartialOrd + Ord + PartialEq + Eq> Eq for ENegativeTypeGuardConsistencyData<L> {}

impl<L: Dupe + PartialOrd + Ord + PartialEq + Eq + Hash> Hash
    for ENegativeTypeGuardConsistencyData<L>
{
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.return_reason.hash(state);
        self.type_.hash(state);
    }
}

impl<L: Dupe + PartialOrd + Ord + PartialEq + Eq> PartialOrd
    for ENegativeTypeGuardConsistencyData<L>
{
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<L: Dupe + PartialOrd + Ord + PartialEq + Eq> Ord for ENegativeTypeGuardConsistencyData<L> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.return_reason
            .cmp(&other.return_reason)
            .then_with(|| self.type_.cmp(&other.type_))
    }
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
pub struct ETypeParamConstIncompatibilityData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub use_op: VirtualUseOp<L>,
    pub lower: NamedReferenceData<L>,
    pub upper: NamedReferenceData<L>,
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
pub struct EExportRenamedDefaultData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub loc: L,
    pub name: Option<FlowSmolStr>,
    pub is_reexport: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EInvalidObjectKitData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub loc: L,
    pub value: ErrorTypeReferenceData<L>,
    pub use_op: VirtualUseOp<L>,
    pub requirement: LowerRequirement,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ECannotDeleteData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub loc: L,
    pub expression: ErrorTypeReferenceData<L>,
}

/// Error data for an assertion call (builtin `invariant` or a bare `asserts x`
/// callee) whose condition is always truthy.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EUnnecessaryInvariantData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub loc: L,
    pub callee: FlowSmolStr,
    pub condition: ErrorTypeReferenceData<L>,
    pub condition_kind: UnnecessaryInvariantConditionKind,
}

impl<L: Dupe + PartialOrd + Ord + PartialEq + Eq> PartialEq for EUnnecessaryInvariantData<L> {
    fn eq(&self, other: &Self) -> bool {
        self.loc == other.loc
            && self.callee == other.callee
            && self.condition == other.condition
            && self.condition_kind == other.condition_kind
    }
}

impl<L: Dupe + PartialOrd + Ord + PartialEq + Eq> Eq for EUnnecessaryInvariantData<L> {}

impl<L: Dupe + PartialOrd + Ord + PartialEq + Eq + Hash> Hash for EUnnecessaryInvariantData<L> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.loc.hash(state);
        self.callee.hash(state);
        self.condition.hash(state);
        self.condition_kind.hash(state);
    }
}

impl<L: Dupe + PartialOrd + Ord + PartialEq + Eq> PartialOrd for EUnnecessaryInvariantData<L> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<L: Dupe + PartialOrd + Ord + PartialEq + Eq> Ord for EUnnecessaryInvariantData<L> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.loc
            .cmp(&other.loc)
            .then_with(|| self.callee.cmp(&other.callee))
            .then_with(|| self.condition.cmp(&other.condition))
            .then_with(|| self.condition_kind.cmp(&other.condition_kind))
    }
}

impl<L: Dupe + PartialOrd + Ord + PartialEq + Eq> PartialEq for ECannotDeleteData<L> {
    fn eq(&self, other: &Self) -> bool {
        self.loc == other.loc && self.expression == other.expression
    }
}

impl<L: Dupe + PartialOrd + Ord + PartialEq + Eq> Eq for ECannotDeleteData<L> {}

impl<L: Dupe + PartialOrd + Ord + PartialEq + Eq + Hash> Hash for ECannotDeleteData<L> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.loc.hash(state);
        self.expression.hash(state);
    }
}

impl<L: Dupe + PartialOrd + Ord + PartialEq + Eq> PartialOrd for ECannotDeleteData<L> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<L: Dupe + PartialOrd + Ord + PartialEq + Eq> Ord for ECannotDeleteData<L> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.loc
            .cmp(&other.loc)
            .then_with(|| self.expression.cmp(&other.expression))
    }
}

impl<L: Dupe + PartialOrd + Ord + PartialEq + Eq> PartialEq for EInvalidObjectKitData<L> {
    fn eq(&self, other: &Self) -> bool {
        self.loc == other.loc
            && self.value == other.value
            && self.use_op == other.use_op
            && self.requirement == other.requirement
    }
}

impl<L: Dupe + PartialOrd + Ord + PartialEq + Eq> Eq for EInvalidObjectKitData<L> {}

impl<L: Dupe + PartialOrd + Ord + PartialEq + Eq + Hash> Hash for EInvalidObjectKitData<L> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.loc.hash(state);
        self.value.hash(state);
        self.use_op.hash(state);
        self.requirement.hash(state);
    }
}

impl<L: Dupe + PartialOrd + Ord + PartialEq + Eq> PartialOrd for EInvalidObjectKitData<L> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<L: Dupe + PartialOrd + Ord + PartialEq + Eq> Ord for EInvalidObjectKitData<L> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.loc
            .cmp(&other.loc)
            .then_with(|| self.value.cmp(&other.value))
            .then_with(|| self.use_op.cmp(&other.use_op))
            .then_with(|| self.requirement.cmp(&other.requirement))
    }
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
pub struct EObjectComputedPropertyAccessData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub object: ErrorTypeReferenceWithLocData<L>,
    pub property: ErrorTypeReferenceWithLocData<L>,
    pub kind: InvalidObjKey,
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
pub struct EObjectComputedPropertyPotentialOverwriteData<
    L: Dupe + PartialOrd + Ord + PartialEq + Eq,
> {
    pub key_loc: L,
    pub overwritten_locs: Vec<L>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EIncompatibleTypesWithUseOpData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub use_op: VirtualUseOp<L>,
    pub lower_loc: L,
    pub lower_def_loc: L,
    pub upper_loc: L,
    pub upper_def_loc: L,
    pub lower_desc: TypeOrTypeDesc<L>,
    pub upper_desc: TypeOrTypeDesc<L>,
    pub explanation: Option<Explanation<L>>,
    pub example: Option<Box<ErrorMessage<L>>>,
    pub branches: Vec<ErrorMessage<L>>,
}

// Source references preserve provenance while type descriptions keep distinct inferred branches
// from collapsing when they happen to originate at the same locations.
impl<L: Dupe + PartialOrd + Ord + PartialEq + Eq> PartialEq for EIncompatibleTypesWithUseOpData<L> {
    fn eq(&self, other: &Self) -> bool {
        self.use_op == other.use_op
            && self.lower_loc == other.lower_loc
            && self.lower_def_loc == other.lower_def_loc
            && self.upper_loc == other.upper_loc
            && self.upper_def_loc == other.upper_def_loc
            && self.lower_desc == other.lower_desc
            && self.upper_desc == other.upper_desc
            && self.explanation == other.explanation
            && self.example == other.example
            && self.branches == other.branches
    }
}

impl<L: Dupe + PartialOrd + Ord + PartialEq + Eq> Eq for EIncompatibleTypesWithUseOpData<L> {}

impl<L: Dupe + PartialOrd + Ord + PartialEq + Eq + Hash> Hash
    for EIncompatibleTypesWithUseOpData<L>
{
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.use_op.hash(state);
        self.lower_loc.hash(state);
        self.lower_def_loc.hash(state);
        self.upper_loc.hash(state);
        self.upper_def_loc.hash(state);
        self.lower_desc.hash(state);
        self.upper_desc.hash(state);
        self.explanation.hash(state);
        self.example.hash(state);
        self.branches.hash(state);
    }
}

impl<L: Dupe + PartialOrd + Ord + PartialEq + Eq> PartialOrd
    for EIncompatibleTypesWithUseOpData<L>
{
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<L: Dupe + PartialOrd + Ord + PartialEq + Eq> Ord for EIncompatibleTypesWithUseOpData<L> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.use_op
            .cmp(&other.use_op)
            .then_with(|| self.lower_loc.cmp(&other.lower_loc))
            .then_with(|| self.lower_def_loc.cmp(&other.lower_def_loc))
            .then_with(|| self.upper_loc.cmp(&other.upper_loc))
            .then_with(|| self.upper_def_loc.cmp(&other.upper_def_loc))
            .then_with(|| self.lower_desc.cmp(&other.lower_desc))
            .then_with(|| self.upper_desc.cmp(&other.upper_desc))
            .then_with(|| self.explanation.cmp(&other.explanation))
            .then_with(|| self.example.cmp(&other.example))
            .then_with(|| self.branches.cmp(&other.branches))
    }
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
pub struct EInvalidReactCreateElementData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub create_element_loc: L,
    pub react: NamedReferenceData<L>,
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
pub enum InvalidThisArgKind {
    /// The callee of `.call`/`.apply`/`.bind` is not a member access, so there
    /// is no receiver that the first argument could be required to match.
    MissingReceiver,
    /// The first argument is absent, spread, or is not the same expression as
    /// the receiver the callee was read from.
    ReceiverMismatch,
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
pub struct EInvalidThisArgData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    /// Location of the `call`/`apply`/`bind` identifier.
    pub loc: L,
    pub name: FlowSmolStr,
    /// The receiver the first argument had to match, or, for
    /// [`InvalidThisArgKind::MissingReceiver`], the callee that has no receiver.
    pub receiver_expression: ExpressionReferenceData<L>,
    pub kind: InvalidThisArgKind,
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
pub struct EDuplicateModuleProviderData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub module_name: FlowSmolStr,
    pub provider: L,
    pub conflict: L,
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
pub struct EIncorrectTypeWithReplacementData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub loc: L,
    pub kind: IncorrectType,
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
pub struct ESketchyNullLintData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub kind: SketchyNullKind,
    pub loc: L,
    pub null_loc: L,
    pub falsy_loc: L,
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
pub struct EPrimitiveAsInterfaceData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub use_op: VirtualUseOp<L>,
    pub lower: ErrorTypeReferenceWithLocData<L>,
    pub upper: ErrorTypeReferenceWithLocData<L>,
    pub kind: PrimitiveKind,
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
pub struct ENotAReactComponentData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub component: ErrorTypeReferenceWithLocData<L>,
    pub use_op: VirtualUseOp<L>,
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
pub struct ECannotSpreadInterfaceData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub spread_loc: L,
    pub interface: ErrorTypeReferenceWithLocData<L>,
    pub use_op: VirtualUseOp<L>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ECannotSpreadIndexerOnRightData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub spread_loc: L,
    pub object: ErrorTypeReferenceWithLocData<L>,
    pub key_loc: L,
    pub key_desc: TypeOrTypeDesc<L>,
    pub use_op: VirtualUseOp<L>,
}

impl<L: Dupe + PartialOrd + Ord + PartialEq + Eq> PartialEq for ECannotSpreadIndexerOnRightData<L> {
    fn eq(&self, other: &Self) -> bool {
        self.spread_loc == other.spread_loc
            && self.object == other.object
            && self.key_loc == other.key_loc
            && self.use_op == other.use_op
    }
}

impl<L: Dupe + PartialOrd + Ord + PartialEq + Eq> Eq for ECannotSpreadIndexerOnRightData<L> {}

impl<L: Dupe + PartialOrd + Ord + PartialEq + Eq + Hash> Hash
    for ECannotSpreadIndexerOnRightData<L>
{
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.spread_loc.hash(state);
        self.object.hash(state);
        self.key_loc.hash(state);
        self.use_op.hash(state);
    }
}

impl<L: Dupe + PartialOrd + Ord + PartialEq + Eq> PartialOrd
    for ECannotSpreadIndexerOnRightData<L>
{
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<L: Dupe + PartialOrd + Ord + PartialEq + Eq> Ord for ECannotSpreadIndexerOnRightData<L> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.spread_loc
            .cmp(&other.spread_loc)
            .then_with(|| self.object.cmp(&other.object))
            .then_with(|| self.key_loc.cmp(&other.key_loc))
            .then_with(|| self.use_op.cmp(&other.use_op))
    }
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
pub struct EUnableToSpreadData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub spread_loc: L,
    pub object1: ErrorTypeReferenceWithLocData<L>,
    pub object2: ErrorTypeReferenceWithLocData<L>,
    pub propname: Name,
    pub error_kind: ExactnessErrorKind,
    pub use_op: VirtualUseOp<L>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EInexactMayOverwriteIndexerData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub spread_loc: L,
    pub key_loc: L,
    pub key_desc: TypeOrTypeDesc<L>,
    pub value_loc: L,
    pub value_desc: TypeOrTypeDesc<L>,
    pub object2: ErrorTypeReferenceWithLocData<L>,
    pub use_op: VirtualUseOp<L>,
}

impl<L: Dupe + PartialOrd + Ord + PartialEq + Eq> PartialEq for EInexactMayOverwriteIndexerData<L> {
    fn eq(&self, other: &Self) -> bool {
        self.spread_loc == other.spread_loc
            && self.key_loc == other.key_loc
            && self.value_loc == other.value_loc
            && self.object2 == other.object2
            && self.use_op == other.use_op
    }
}

impl<L: Dupe + PartialOrd + Ord + PartialEq + Eq> Eq for EInexactMayOverwriteIndexerData<L> {}

impl<L: Dupe + PartialOrd + Ord + PartialEq + Eq + Hash> Hash
    for EInexactMayOverwriteIndexerData<L>
{
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.spread_loc.hash(state);
        self.key_loc.hash(state);
        self.value_loc.hash(state);
        self.object2.hash(state);
        self.use_op.hash(state);
    }
}

impl<L: Dupe + PartialOrd + Ord + PartialEq + Eq> PartialOrd
    for EInexactMayOverwriteIndexerData<L>
{
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<L: Dupe + PartialOrd + Ord + PartialEq + Eq> Ord for EInexactMayOverwriteIndexerData<L> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.spread_loc
            .cmp(&other.spread_loc)
            .then_with(|| self.key_loc.cmp(&other.key_loc))
            .then_with(|| self.value_loc.cmp(&other.value_loc))
            .then_with(|| self.object2.cmp(&other.object2))
            .then_with(|| self.use_op.cmp(&other.use_op))
    }
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
pub struct EExponentialSpreadData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub spread_loc: L,
    pub operand1: ExponentialSpreadOperandGroup<ErrorTypeReferenceWithLocData<L>>,
    pub operand2: ExponentialSpreadOperandGroup<ErrorTypeReferenceWithLocData<L>>,
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
pub struct EAssignConstLikeBindingData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub loc: L,
    pub definition_loc: L,
    pub name: FlowSmolStr,
    pub binding_kind: AssignedConstLikeBindingType,
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
pub struct EObjectThisSuperReferenceData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub loc: L,
    pub method_loc: L,
    pub method_name: Option<FlowSmolStr>,
    pub kind: ThisFinderKind,
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
pub struct EImplicitInstantiationUnderconstrainedErrorData<
    L: Dupe + PartialOrd + Ord + PartialEq + Eq,
> {
    pub call: ImplicitInstantiationReferenceData<L>,
    pub type_param_loc: L,
    pub type_param_name: FlowSmolStr,
    pub use_op: VirtualUseOp<L>,
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
pub struct EClassToObjectData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub lower: ErrorTypeReferenceWithLocData<L>,
    pub upper: ErrorTypeReferenceWithLocData<L>,
    pub use_op: VirtualUseOp<L>,
    pub kind: ClassKind,
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
pub struct EHookIncompatibleData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub use_op: VirtualUseOp<L>,
    pub lower: ErrorTypeReferenceWithLocData<L>,
    pub upper: ErrorTypeReferenceWithLocData<L>,
    pub lower_is_hook: bool,
    pub hook_is_annot: bool,
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
pub struct EHookUniqueIncompatibleData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub use_op: VirtualUseOp<L>,
    pub lower: ErrorTypeReferenceWithLocData<L>,
    pub upper: ErrorTypeReferenceWithLocData<L>,
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
pub struct EHookRuleViolationData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub hook_rule: HookRule<L>,
    pub callee_loc: L,
    pub call_loc: L,
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
pub struct EComponentThisReferenceData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub component_loc: L,
    pub this_loc: L,
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
pub struct EInvalidDeclarationData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub declaration_loc: L,
    pub name: FlowSmolStr,
    pub null_write: Option<NullWrite<L>>,
    pub possible_generic_escape_locs: Vec<L>,
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
pub struct ERecursiveDefinitionData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub definition: DefinitionReferenceData<L>,
    pub recursion: Vec<L>,
    pub annot_locs: Vec<AnnotLoc<L>>,
}

/// A definition location and its semantic kind for recursive-definition errors.
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
pub struct DefinitionReferenceData<L: Dupe> {
    pub loc: L,
    pub kind: DefinitionReferenceKind,
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
pub struct EDuplicateClassMemberData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub loc: L,
    pub name: FlowSmolStr,
    pub is_static: bool,
    pub class_kind: ClassKind,
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
pub struct ETSSyntaxData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub kind: TSSyntaxKind,
    pub loc: L,
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
pub struct EAbstractClassData<L: Dupe> {
    pub kind: AbstractErrorKind<L>,
    pub loc: L,
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
pub struct EOverrideData<L: Dupe> {
    pub kind: OverrideErrorKind<L>,
    pub loc: L,
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
pub struct EVarianceKeywordData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub kind: VarianceKeywordKind,
    pub loc: L,
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
pub enum VarianceKeywordKind {
    Plus(crate::intermediate_error_types::VarianceSigilParent),
    Minus(crate::intermediate_error_types::VarianceSigilParent),
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EInvalidBinaryArithData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub loc: L,
    pub left: ErrorTypeReferenceData<L>,
    pub right: ErrorTypeReferenceData<L>,
    pub kind: flow_typing_type::type_::arith_kind::ArithKind,
}

impl<L: Dupe + PartialOrd + Ord + PartialEq + Eq> PartialEq for EInvalidBinaryArithData<L> {
    fn eq(&self, other: &Self) -> bool {
        self.loc == other.loc
            && self.left == other.left
            && self.right == other.right
            && self.kind == other.kind
    }
}

impl<L: Dupe + PartialOrd + Ord + PartialEq + Eq> Eq for EInvalidBinaryArithData<L> {}

impl<L: Dupe + PartialOrd + Ord + PartialEq + Eq + Hash> Hash for EInvalidBinaryArithData<L> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.loc.hash(state);
        self.left.hash(state);
        self.right.hash(state);
        self.kind.hash(state);
    }
}

impl<L: Dupe + PartialOrd + Ord + PartialEq + Eq> PartialOrd for EInvalidBinaryArithData<L> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<L: Dupe + PartialOrd + Ord + PartialEq + Eq> Ord for EInvalidBinaryArithData<L> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.loc
            .cmp(&other.loc)
            .then_with(|| self.left.cmp(&other.left))
            .then_with(|| self.right.cmp(&other.right))
            .then_with(|| self.kind.cmp(&other.kind))
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EArithmeticOperandData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub loc: L,
    pub operand: ErrorTypeReferenceData<L>,
}

impl<L: Dupe + PartialOrd + Ord + PartialEq + Eq> PartialEq for EArithmeticOperandData<L> {
    fn eq(&self, other: &Self) -> bool {
        self.loc == other.loc && self.operand == other.operand
    }
}

impl<L: Dupe + PartialOrd + Ord + PartialEq + Eq> Eq for EArithmeticOperandData<L> {}

impl<L: Dupe + PartialOrd + Ord + PartialEq + Eq + Hash> Hash for EArithmeticOperandData<L> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.loc.hash(state);
        self.operand.hash(state);
    }
}

impl<L: Dupe + PartialOrd + Ord + PartialEq + Eq> PartialOrd for EArithmeticOperandData<L> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<L: Dupe + PartialOrd + Ord + PartialEq + Eq> Ord for EArithmeticOperandData<L> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.loc
            .cmp(&other.loc)
            .then_with(|| self.operand.cmp(&other.operand))
    }
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
pub struct EForInRHSData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub loc: L,
    pub rhs_loc: L,
    pub rhs_desc: TypeOrTypeDesc<L>,
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
pub struct EInstanceofRHSData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub loc: L,
    pub rhs_loc: L,
    pub rhs_desc: TypeOrTypeDesc<L>,
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
pub struct EBinaryInLHSData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub loc: L,
    pub lhs_loc: L,
    pub lhs_desc: TypeOrTypeDesc<L>,
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
pub struct EBinaryInRHSData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub loc: L,
    pub rhs_loc: L,
    pub rhs_desc: TypeOrTypeDesc<L>,
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
pub struct EInvalidConstructorData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub loc: L,
    pub value_loc: L,
    pub value_desc: TypeOrTypeDesc<L>,
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
pub struct EInvalidPrototypeData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub loc: L,
    pub prototype_loc: L,
    pub prototype_desc: TypeOrTypeDesc<L>,
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
pub struct EDuplicateComponentPropData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub spread: L,
    pub duplicates: Vec1<(L, Name, L)>,
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
pub struct EKeySpreadPropData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub spread: L,
    pub loc: L,
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
pub struct EReactIntrinsicOverlapData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub use_: NamedReferenceData<L>,
    pub def: L,
    pub type_: L,
    pub mixed: bool,
}

/// A type-backed error reference with explicit primary and reference locations.
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
pub struct ErrorTypeReferenceData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub loc: L,
    pub reference_loc: L,
    pub type_desc: TypeOrTypeDesc<L>,
}

/// A type-backed error reference whose explicit locations determine error identity.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ErrorTypeReferenceWithLocData<L: Dupe> {
    pub loc: L,
    pub definition_loc: L,
    pub reference_loc: L,
    pub type_desc: TypeOrTypeDesc<L>,
}

impl<L: Dupe> ErrorTypeReferenceWithLocData<L> {
    pub fn new(reason: VirtualReason<L>, reference_loc: L, type_desc: TypeOrTypeDesc<L>) -> Self {
        Self {
            loc: reason.loc().dupe(),
            definition_loc: reason.def_loc().dupe(),
            reference_loc,
            type_desc,
        }
    }
}

impl<L: Dupe + PartialEq> PartialEq for ErrorTypeReferenceWithLocData<L> {
    fn eq(&self, other: &Self) -> bool {
        self.loc == other.loc
            && self.definition_loc == other.definition_loc
            && self.reference_loc == other.reference_loc
    }
}

impl<L: Dupe + Eq> Eq for ErrorTypeReferenceWithLocData<L> {}

impl<L: Dupe + Hash> Hash for ErrorTypeReferenceWithLocData<L> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.loc.hash(state);
        self.definition_loc.hash(state);
        self.reference_loc.hash(state);
    }
}

impl<L: Dupe + PartialOrd> PartialOrd for ErrorTypeReferenceWithLocData<L> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.loc
            .partial_cmp(&other.loc)
            .and_then(|ordering| match ordering {
                std::cmp::Ordering::Equal => self
                    .definition_loc
                    .partial_cmp(&other.definition_loc)
                    .and_then(|ordering| match ordering {
                        std::cmp::Ordering::Equal => {
                            self.reference_loc.partial_cmp(&other.reference_loc)
                        }
                        _ => Some(ordering),
                    }),
                _ => Some(ordering),
            })
    }
}

impl<L: Dupe + Ord> Ord for ErrorTypeReferenceWithLocData<L> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.loc
            .cmp(&other.loc)
            .then_with(|| self.definition_loc.cmp(&other.definition_loc))
            .then_with(|| self.reference_loc.cmp(&other.reference_loc))
    }
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
pub struct EInvalidRendersTypeArgumentData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub loc: L,
    pub renders_variant: RendersVariant,
    pub invalid_render_type_kind: InvalidRenderTypeKind<ErrorTypeReferenceData<L>>,
    pub invalid_types: Vec1<ErrorTypeReferenceData<L>>,
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
pub struct EMissingPlatformSupportWithAvailablePlatformsData<
    L: Dupe + PartialOrd + Ord + PartialEq + Eq,
> {
    pub loc: L,
    pub available_platforms: BTreeSet<FlowSmolStr>,
    pub required_platforms: BTreeSet<FlowSmolStr>,
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
pub struct EUnionOptimizationData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub loc: L,
    pub kind: OptimizedError<L>,
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
pub struct EUnionOptimizationOnNonUnionData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub loc: L,
    pub arg_loc: L,
    pub arg_desc: TypeOrTypeDesc<L>,
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
pub struct EIllegalAssertOperatorData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub op_loc: L,
    pub obj: ExpressionReferenceData<L>,
    pub specialized: bool,
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
pub struct EFunctionCallExtraArgData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub loc: L,
    pub function_reference: FunctionReferenceData<L>,
    pub param_count: i32,
    pub use_op: VirtualUseOp<L>,
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
pub struct EDevOnlyRefinedLocInfoData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub refined_loc: L,
    pub refining_locs: Vec<L>,
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
pub struct EDevOnlyInvalidatedRefinementInfoData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub read_loc: L,
    pub invalidation_info: Vec<(L, refinement_invalidation::Reason)>,
}

/// A problem the definite assignment check for class fields reports
/// (`experimental.strict_property_initialization`).
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
pub enum UninitializedPropertyKind {
    /// Some path through the constructor reaches its end without assigning the field.
    NotDefinitelyAssigned,
    /// The constructor may read the field before assigning it.
    UsedBeforeAssigned,
    /// A field initializer reads the field before the field's own initializer runs.
    UsedBeforeInitialization,
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
pub enum MissingLocalAnnotationTarget {
    /// A class or record property, by its name as written, e.g. `x` or `#x`.
    Property(FlowSmolStr),
    Return,
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
pub enum ErrorMessage<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    EIncompatibleType(Box<EIncompatibleTypeData<L>>),

    EIncompatibleSpeculation(Box<EIncompatibleSpeculationData<L>>),

    EIncompatibleProp(Box<EIncompatiblePropData<L>>),

    EExportValueAsType(Box<(L, Name)>),

    EImportValueAsType(Box<(L, FlowSmolStr)>),

    EImportTypeAsTypeof(Box<(L, FlowSmolStr)>),

    EImportTypeAsValue(Box<(L, FlowSmolStr)>),

    ENoDefaultExport(Box<(L, Userland, Option<FlowSmolStr>)>),

    EOnlyDefaultExport(Box<(L, Userland, FlowSmolStr)>),

    ENoNamedExport(Box<(L, Userland, FlowSmolStr, Option<FlowSmolStr>)>),

    EMissingTypeArgs(Box<EMissingTypeArgsData<L>>),

    EAnyValueUsedAsType {
        reference: Option<ValueAsTypeReference>,
        value: ErrorTypeReferenceData<L>,
    },

    EValueUsedAsType {
        reference: Option<ValueAsTypeReference>,
        value: ErrorTypeReferenceData<L>,
    },

    EExpectedStringLit(Box<EExpectedStringLitData<L>>),

    EExpectedNumberLit(Box<EExpectedNumberLitData<L>>),

    EExpectedBooleanLit(Box<EExpectedBooleanLitData<L>>),

    EExpectedBigIntLit(Box<EExpectedBigIntLitData<L>>),

    EPropNotFoundInLookup(Box<EPropNotFoundInLookupData<L>>),

    EPropNotFoundInSubtyping(Box<EPropNotFoundInSubtypingData<L>>),

    EConstructSignatureMissingInSubtyping(Box<EConstructSignatureMissingInSubtypingData<L>>),

    EPropsNotFoundInSubtyping(Box<EPropsNotFoundInSubtypingData<L>>),

    EPropsNotFoundInInvariantSubtyping(Box<EPropsNotFoundInInvariantSubtypingData<L>>),

    EPropsExtraAgainstExactObject(Box<EPropsExtraAgainstExactObjectData<L>>),

    EIndexerCheckFailed(Box<EIndexerCheckFailedData<L>>),

    EPropNotReadable(Box<EPropNotReadableData<L>>),

    EPropNotWritable(Box<EPropNotWritableData<L>>),

    EPropPolarityMismatch(Box<EPropPolarityMismatchData<L>>),

    EPolarityMismatch(Box<EPolarityMismatchData<L>>),

    EBuiltinNameLookupFailed(Box<EBuiltinNameLookupFailedData<L>>),

    EUmdGlobalValueInModule(Box<EUmdGlobalValueInModuleData<L>>),

    EBuiltinModuleLookupFailed(Box<EBuiltinModuleLookupFailedData<L>>),

    EExpectedModuleLookupFailed(Box<EExpectedModuleLookupFailedData<L>>),

    ECannotImportGlobalLibdef(Box<ECannotImportGlobalLibdefData<L>>),

    EPrivateLookupFailed(Box<EPrivateLookupFailedData<L>>),

    EPlatformSpecificImplementationModuleLookupFailed(
        Box<EPlatformSpecificImplementationModuleLookupFailedData<L>>,
    ),

    EComparison(Box<EComparisonData<L>>),

    ENonStrictEqualityComparison(Box<ENonStrictEqualityComparisonData<L>>),

    ETupleArityMismatch(Box<ETupleArityMismatchData<L>>),

    ENonLitArrayToTuple(
        (
            ErrorTypeReferenceWithLocData<L>,
            ErrorTypeReferenceWithLocData<L>,
        ),
        VirtualUseOp<L>,
    ),

    ETupleOutOfBounds(Box<ETupleOutOfBoundsData<L>>),

    ETupleNonIntegerIndex(Box<ETupleNonIntegerIndexData<L>>),

    ETupleUnsafeWrite {
        loc: L,
        use_op: VirtualUseOp<L>,
    },

    ETupleElementNotReadable(Box<ETupleElementNotReadableData<L>>),

    ETupleElementNotWritable(Box<ETupleElementNotWritableData<L>>),

    ETupleElementPolarityMismatch(Box<ETupleElementPolarityMismatchData<L>>),

    ETupleRequiredAfterOptional(Box<ETupleRequiredAfterOptionalData<L>>),

    ETupleInvalidTypeSpread(Box<ETupleInvalidTypeSpreadData<L>>),

    ETupleElementAfterInexactSpread(L),

    ETupleElementAfterRestSpread(L),

    EROArrayWrite(L, VirtualUseOp<L>),

    EUnionSpeculationFailed(Box<EUnionSpeculationFailedData<L>>),

    EIncompatibleWithExact(
        (
            ErrorTypeReferenceWithLocData<L>,
            ErrorTypeReferenceWithLocData<L>,
        ),
        VirtualUseOp<L>,
        ExactnessErrorKind,
    ),

    EFunctionIncompatibleWithIndexer(
        (
            ErrorTypeReferenceWithLocData<L>,
            ErrorTypeReferenceWithLocData<L>,
        ),
        VirtualUseOp<L>,
    ),

    EUnsupportedExact(Box<EUnsupportedExactData<L>>),

    EInvalidExactObjectIntersection(Box<EInvalidExactObjectIntersectionData<L>>),

    EUnexpectedThisType(L),

    ETypeParamArity(L, i32),

    ECallTypeArity(Box<ECallTypeArityData<L>>),

    ETooManyTypeArgs(Box<ETooManyTypeArgsData<L>>),

    ETooFewTypeArgs(Box<ETooFewTypeArgsData<L>>),

    EInvalidInfer(L),

    EConstantCondition(Box<EConstantConditionData<L>>),

    EInvalidExtends(Box<(L, FlowSmolStr)>),

    EExportsAnnot(L),

    EInvalidConstructor(Box<EInvalidConstructorData<L>>),

    EUnsupportedKeyInObject {
        loc: L,
        obj_kind: ObjKind,
        key_error_kind: InvalidObjKey,
    },

    EUnsupportedComputedKeyInClass(L),

    EAmbiguousNumericKeyWithVariance(L),

    ETypeGuardFuncIncompatibility {
        use_op: VirtualUseOp<L>,
        lower: TypeGuardFuncReference<L, ErrorTypeReferenceWithLocData<L>>,
        upper: TypeGuardFuncReference<L, ErrorTypeReferenceWithLocData<L>>,
    },

    ETypeGuardInvalidParameter(Box<ETypeGuardInvalidParameterData<L>>),

    ETypeGuardIndexMismatch {
        use_op: VirtualUseOp<L>,
        parameters: (TypeGuardParameterData<L>, TypeGuardParameterData<L>),
    },

    ETypeGuardImpliesMismatch {
        use_op: VirtualUseOp<L>,
        lower: TypeGuardReferenceData<L>,
        upper: TypeGuardReferenceData<L>,
    },

    ETypeGuardParamUnbound(TypeGuardParameterData<L>),

    ETypeGuardThisParam(L),

    ETypeGuardFunctionInvalidWrites(Box<ETypeGuardFunctionInvalidWritesData<L>>),

    ETypeGuardFunctionParamHavoced(Box<ETypeGuardFunctionParamHavocedData<L>>),

    ETypeGuardIncompatibleWithFunctionKind(Box<ETypeGuardIncompatibleWithFunctionKindData<L>>),

    ENegativeTypeGuardConsistency(Box<ENegativeTypeGuardConsistencyData<L>>),

    EAssertionFunctionFalsyAtExit(Box<EAssertionFunctionFalsyAtExitData<L>>),
    EUnrecognizedAssertionCall(Box<EUnrecognizedAssertionCallData<L>>),

    ETypeParamConstIncompatibility(Box<ETypeParamConstIncompatibilityData<L>>),

    ETypeParamConstInvalidPosition(Box<(L, FlowSmolStr)>),

    EInternal(Box<(L, InternalError)>),

    EUnsupportedSyntax(Box<(L, UnsupportedSyntax)>),

    EUseArrayLiteral(L),

    EMissingLocalAnnotation {
        loc: L,
        target: MissingLocalAnnotationTarget,
        hint_available: bool,
        from_generic_function: bool,
    },

    EMissingThisAnnotation(L),

    EMissingParameterAnnotation {
        loc: L,
        hint_available: bool,
    },

    EBindingError(Box<(BindingError, L, Name, L)>),

    ERecursionLimit(L),

    EUninitializedInstanceProperty(L, PropertyAssignmentKind),

    /// A class field's name (with `#` if private), from the definite assignment check.
    EUninitializedProperty(Box<(L, FlowSmolStr, UninitializedPropertyKind)>),

    EEnumError(EnumErrorKind<L>),

    EIndeterminateModuleType(L),

    EBadExportPosition(L),

    EBadExportContext(Box<(FlowSmolStr, L)>),

    EBadDefaultImportAccess(Box<(L, L)>),

    EBadDefaultImportDestructuring(L),

    EInvalidImportStarUse(Box<(L, L)>),

    ENonConstVarExport(Box<(L, Option<(L, FlowSmolStr)>)>),

    EThisInExportedFunction(L),

    EMixedImportAndRequire(Box<(L, L)>),

    EUnsupportedVarianceAnnotation(Box<(L, FlowSmolStr)>),

    EExportRenamedDefault(Box<EExportRenamedDefaultData<L>>),

    EUnreachable(L),

    EInvalidObjectKit(Box<EInvalidObjectKitData<L>>),

    EInvalidTypeof(Box<(L, FlowSmolStr)>),

    EBinaryInLHS(Box<EBinaryInLHSData<L>>),

    EBinaryInRHS(Box<EBinaryInRHSData<L>>),

    EArithmeticOperand(Box<EArithmeticOperandData<L>>),

    EForInRHS(Box<EForInRHSData<L>>),

    EInstanceofRHS(Box<EInstanceofRHSData<L>>),

    EObjectComputedPropertyAccess(Box<EObjectComputedPropertyAccessData<L>>),

    EObjectComputedPropertyAssign(
        Box<(
            ErrorTypeReferenceWithLocData<L>,
            Option<ExpressionReferenceData<L>>,
            InvalidObjKey,
        )>,
    ),

    EObjectComputedPropertyPotentialOverwrite(
        Box<EObjectComputedPropertyPotentialOverwriteData<L>>,
    ),

    EInvalidLHSInAssignment(L),

    EIncompatibleTypesWithUseOp(Box<EIncompatibleTypesWithUseOpData<L>>),

    EInvariantSubtypingWithUseOp(Box<EInvariantSubtypingWithUseOpData<L>>),

    EUnsupportedImplements(ErrorTypeReferenceData<L>),

    ENotAReactComponent(Box<ENotAReactComponentData<L>>),

    EInvalidReactCreateElement(Box<EInvalidReactCreateElementData<L>>),

    EInvalidThisArg(Box<EInvalidThisArgData<L>>),

    EReactRefInRender {
        usage: ExpressionReferenceData<L>,
        kind: RefInRenderKind,
        in_hook: bool,
    },

    EFunctionCallExtraArg(Box<EFunctionCallExtraArgData<L>>),

    EUnsupportedSetProto(L),

    EDuplicateModuleProvider(Box<EDuplicateModuleProviderData<L>>),

    EParseError(Box<(L, ParseError)>),

    EDocblockError(Box<(L, DocblockError)>),

    EAmbiguousObjectType(L),

    // The string is either the name of a module or "the module that exports `_`".
    EUntypedTypeImport(Box<(L, Userland)>),

    EUntypedImport(Box<(L, Userland)>),

    ENonstrictImport(L),

    EUnclearType(L),

    EDeprecatedBool(L),

    EInternalType(L, InternalType),

    EIncorrectTypeWithReplacement(Box<EIncorrectTypeWithReplacementData<L>>),

    EUnsafeGettersSetters(L),

    EUnsafeObjectAssign(L),

    EUnusedSuppression(L),

    ECodelessSuppression(L),

    EMalformedCode(L),

    ELintSetting(Box<(L, LintParseError)>),

    ESketchyNullLint(Box<ESketchyNullLintData<L>>),

    ESketchyNumberLint(SketchyNumberKind, ErrorTypeReferenceWithLocData<L>),

    EInvalidPrototype(Box<EInvalidPrototypeData<L>>),

    EUnnecessaryOptionalChain(Box<(L, ExpressionReferenceData<L>)>),

    EUnnecessaryInvariant(Box<EUnnecessaryInvariantData<L>>),

    EUnnecessaryDeclareTypeOnlyExport(L),

    ECannotDelete(Box<ECannotDeleteData<L>>),

    ESignatureBindingValidation(BindingValidation<L>),

    ESignatureVerification(SignatureError<L>),

    EPrimitiveAsInterface(Box<EPrimitiveAsInterfaceData<L>>),

    ECannotSpreadInterface(Box<ECannotSpreadInterfaceData<L>>),

    ECannotSpreadIndexerOnRight(Box<ECannotSpreadIndexerOnRightData<L>>),

    EUnableToSpread(Box<EUnableToSpreadData<L>>),

    EInexactMayOverwriteIndexer(Box<EInexactMayOverwriteIndexerData<L>>),

    EExponentialSpread(Box<EExponentialSpreadData<L>>),

    EAssignConstLikeBinding(Box<EAssignConstLikeBindingData<L>>),

    EImplicitInstantiationUnderconstrainedError(
        Box<EImplicitInstantiationUnderconstrainedErrorData<L>>,
    ),

    EClassToObject(Box<EClassToObjectData<L>>),

    EHookIncompatible(Box<EHookIncompatibleData<L>>),

    EHookUniqueIncompatible(Box<EHookUniqueIncompatibleData<L>>),

    EHookRuleViolation(Box<EHookRuleViolationData<L>>),

    EHookNaming(L),

    EObjectThisSuperReference(Box<EObjectThisSuperReferenceData<L>>),

    EComponentThisReference(Box<EComponentThisReferenceData<L>>),

    EComponentCase(L),

    EDeclareComponentInvalidParam {
        loc: L,
        kind: DeclareComponentInvalidParamKind,
    },

    EComponentMissingReturn(ComponentReferenceData<L>),

    EComponentMissingBody(L),

    EComponentBodyInAmbientContext(L),

    ENestedComponent(L),

    ENestedHook(L),

    EInvalidDeclaration(Box<EInvalidDeclarationData<L>>),

    EInvalidGraphQL(Box<(L, GraphqlError)>),

    EAnnotationInference(Box<EAnnotationInferenceData<L>>),

    ETrivialRecursiveDefinition(L),

    ETrivialRecursiveTypeParameter(Box<(L, FlowSmolStr)>),

    EClassTypeParamInStatic(Box<(L, FlowSmolStr)>),

    EDefinitionCycle(Vec1<(DefinitionReferenceData<L>, Vec<L>, Vec<AnnotLoc<L>>)>),

    ERecursiveDefinition(Box<ERecursiveDefinitionData<L>>),

    EReferenceInAnnotation(Box<(L, FlowSmolStr, L)>),

    EDuplicateClassMember(Box<EDuplicateClassMemberData<L>>),

    EEmptyArrayNoProvider {
        loc: L,
    },

    EUnusedPromise {
        loc: L,
        async_: bool,
    },

    EBigIntRShift3(Box<ErrorTypeReferenceData<L>>),

    EBigIntNumCoerce(Box<ErrorTypeReferenceData<L>>),

    EInvalidCatchParameterAnnotation {
        loc: L,
        ts_utility_syntax: bool,
    },

    ETSSyntax(Box<ETSSyntaxData<L>>),

    EAbstractClass(Box<EAbstractClassData<L>>),

    EOverride(Box<EOverrideData<L>>),

    EVarianceKeyword(Box<EVarianceKeywordData<L>>),

    EInvalidBinaryArith(Box<EInvalidBinaryArithData<L>>),

    EInvalidMappedType {
        loc: L,
        kind: InvalidMappedTypeErrorKind,
    },

    EInvalidTemplateLiteralType {
        loc: L,
        kind: InvalidTemplateLiteralTypeErrorKind,
    },

    EDuplicateComponentProp(Box<EDuplicateComponentPropData<L>>),

    EKeySpreadProp(Box<EKeySpreadPropData<L>>),

    EReactIntrinsicOverlap(Box<EReactIntrinsicOverlapData<L>>),

    EInvalidComponentRestParam(L),

    EInvalidRendersTypeArgument(Box<EInvalidRendersTypeArgumentData<L>>),

    EInvalidTypeCastSyntax {
        loc: L,
    },

    EMissingPlatformSupportWithAvailablePlatforms(
        Box<EMissingPlatformSupportWithAvailablePlatformsData<L>>,
    ),

    EUnionPartialOptimizationNonUniqueKey(Box<EUnionPartialOptimizationNonUniqueKeyData<L>>),

    EUnionOptimization(Box<EUnionOptimizationData<L>>),

    EUnionOptimizationOnNonUnion(Box<EUnionOptimizationOnNonUnionData<L>>),

    ECannotCallReactComponent {
        component: ErrorTypeReferenceWithLocData<L>,
    },

    EMatchError(MatchErrorKind<L>),

    ERecordError(RecordErrorKind<L>),

    EReferenceInDefault(Box<(L, FlowSmolStr, L)>),

    EUndocumentedFeature {
        loc: L,
    },

    EIllegalAssertOperator(Box<EIllegalAssertOperatorData<L>>),

    // Dev only
    EDevOnlyRefinedLocInfo(Box<EDevOnlyRefinedLocInfoData<L>>),

    EDevOnlyInvalidatedRefinementInfo(Box<EDevOnlyInvalidatedRefinementInfoData<L>>),
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
pub struct NullWrite<L: Dupe> {
    pub null_loc: L,
    pub initialized: bool,
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
pub enum RefInRenderKind {
    Argument,
    Access,
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
pub enum BindingError {
    ENameAlreadyBound,
    ENameAlreadyBoundInCoreJs,
    EVarRedeclaration,
    EReferencedBeforeDeclaration,
    EReferencedThisSuperBeforeSuperCall,
    ETypeInValuePosition {
        imported: bool,
        type_only_namespace: bool,
        name: FlowSmolStr,
    },
    EConstReassigned,
    EConstParamReassigned,
    EImportReassigned,
    EEnumReassigned,
    EReservedKeyword {
        keyword: IncorrectType,
    },
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
pub enum InternalError {
    MethodNotAFunction,
    OptionalMethod,
    PropertyDescriptorPropertyCannotBeRead,
    ForInLHS,
    ForOfLHS,
    PropRefComputedOpen,
    PropRefComputedLiteral,
    RestParameterNotIdentifierPattern,
    InterfaceTypeSpread,
    DebugThrow,
    ParseJobException(FlowSmolStr),
    CheckTimeout(String),
    UnexpectedAnnotationInference(FlowSmolStr),
    UnexpectedInlineInterfaceType,
    MissingSwitchExhaustiveCheck,
    MissingEnvRead(ALoc),
    MissingEnvWrite(ALoc),
    ReadOfUnreachedTvar(DefLocType),
    ForcedReadOfUnderResolutionTvar(DefLocType),
    EnvInvariant(EnvInvariantFailure<ALoc>),
    ImplicitInstantiationInvariant(FlowSmolStr),
    UnresolvedTvarInConditionalType,
    MethodBivariantInvariant(FlowSmolStr),
    WorkerCanceled,
}

impl InternalError {
    pub fn check_timeout(elapsed: Duration) -> Self {
        Self::CheckTimeout(format!("{:.2}", elapsed.as_secs_f64()))
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
pub enum LowerKind {
    PossiblyNull,
    PossiblyVoid,
    PossiblyNullOrVoid,
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
pub enum UpperKind<L: Dupe> {
    IncompatibleGetPropT(L, Option<Name>),
    IncompatibleSetPropT(L, Option<Name>),
    IncompatibleGetPrivatePropT,
    IncompatibleSetPrivatePropT,
    IncompatibleMethodT(L, Option<Name>),
    IncompatibleCallT,
    IncompatibleMixedCallT,
    IncompatibleGetElemT(L),
    IncompatibleSetElemT(L),
    IncompatibleCallElemT(L),
    IncompatibleElemTOfArrT,
    IncompatibleObjAssignFromTSpread,
    IncompatibleObjAssignFromT,
    IncompatibleObjRestT,
    IncompatibleArrRestT,
    IncompatibleSuperT,
    IncompatibleMixinT,
    IncompatibleSpecializeT,
    IncompatibleThisSpecializeT,
    IncompatibleVarianceCheckT,
    IncompatibleGetKeysT,
    IncompatibleHasOwnPropT(L, Option<Name>),
    IncompatibleGetValuesT,
    IncompatibleMapTypeTObject,
    IncompatibleGetStaticsT,
    IncompatibleUnclassified(FlowSmolStr),
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
pub enum TSSyntaxKind {
    TSNever,
    TSUndefined,
    TSInOutVariance,
    TSReadonlyType(Option<ReadonlyTypeKind>),
    TSClassAccessibility(ast::class::ts_accessibility::Kind),
    TSParameterProperty,
    AbstractClass,
    AbstractMethod,
    DeprecatedTypeParamColon,
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
pub enum InvalidMappedTypeErrorKind {
    InterfaceOrDeclaredClass,
    ExtraProperties,
    ExplicitExactOrInexact,
    VarianceOnArrayInput,
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
pub enum InvalidTemplateLiteralTypeErrorKind {
    TooComplex,
    InvalidPlaceholderType,
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
pub enum ReadonlyTypeKind {
    Tuple,
    Array,
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
pub enum HookRule<L: Dupe> {
    ConditionalHook,
    HookHasIllegalName,
    NotHookSyntaxHook,
    MaybeHook { hooks: Vec<L>, non_hooks: Vec<L> },
    HookDefinitelyNotInComponentOrHook,
    HookInUnknownContext,
    HookNotInComponentSyntaxComponentOrHookSyntaxHook,
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
pub enum ThisFinderKind {
    This,
    Super,
}

pub fn string_of_invalid_render_type_kind<T>(kind: &InvalidRenderTypeKind<T>) -> &'static str {
    match kind {
        InvalidRenderTypeKind::InvalidRendersNullVoidFalse => "null | void | false",
        InvalidRenderTypeKind::InvalidRendersIterable => "iterable",
        InvalidRenderTypeKind::InvalidRendersStructural(_) => "non-nominal-return",
        InvalidRenderTypeKind::InvalidRendersNonNominalElement(_) => "non-nominal",
        InvalidRenderTypeKind::InvalidRendersGenericT => "generic",
        InvalidRenderTypeKind::UncategorizedInvalidRenders => "uncategorized",
    }
}

fn map_exponential_spread_operand_group<T, U, F>(
    mut f: F,
    group: ExponentialSpreadOperandGroup<T>,
) -> ExponentialSpreadOperandGroup<U>
where
    F: FnMut(T) -> U,
{
    ExponentialSpreadOperandGroup {
        first: f(group.first),
        second: group.second.map(f),
    }
}

fn map_invalid_render_type_kind<T, U, F: FnMut(T) -> U>(
    mut f: F,
    kind: InvalidRenderTypeKind<T>,
) -> InvalidRenderTypeKind<U> {
    match kind {
        InvalidRenderTypeKind::InvalidRendersNullVoidFalse => {
            InvalidRenderTypeKind::InvalidRendersNullVoidFalse
        }
        InvalidRenderTypeKind::InvalidRendersIterable => {
            InvalidRenderTypeKind::InvalidRendersIterable
        }
        InvalidRenderTypeKind::InvalidRendersStructural(r) => {
            InvalidRenderTypeKind::InvalidRendersStructural(f(r))
        }
        InvalidRenderTypeKind::InvalidRendersNonNominalElement(r) => {
            InvalidRenderTypeKind::InvalidRendersNonNominalElement(f(r))
        }
        InvalidRenderTypeKind::InvalidRendersGenericT => {
            InvalidRenderTypeKind::InvalidRendersGenericT
        }
        InvalidRenderTypeKind::UncategorizedInvalidRenders => {
            InvalidRenderTypeKind::UncategorizedInvalidRenders
        }
    }
}

fn map_strict_comparison_info<L, M, T, U>(
    info: StrictComparisonInfo<L, T>,
    map_loc: impl Fn(L) -> M,
    mut map_type: impl FnMut(T) -> U,
) -> StrictComparisonInfo<M, U> {
    match info {
        StrictComparisonInfo::General { left, right } => StrictComparisonInfo::General {
            left: map_type(left),
            right: map_type(right),
        },
        StrictComparisonInfo::Null { null_loc, other } => StrictComparisonInfo::Null {
            null_loc: map_loc(null_loc),
            other: map_type(other),
        },
        StrictComparisonInfo::Empty { empty } => StrictComparisonInfo::Empty {
            empty: map_type(empty),
        },
    }
}

fn map_loc_of_lazy_explanation<L: Dupe, M: Dupe, F: Fn(&L) -> M>(
    f: &F,
    explanation: ExplanationWithLazyParts<L>,
) -> ExplanationWithLazyParts<M> {
    match explanation {
        ExplanationWithLazyParts::LazyExplanationInvariantSubtypingDueToMutableArray {
            lower_array_loc,
            upper_array_loc,
            lower_array_desc,
            upper_array_desc,
            upper_array_reference_loc,
        } => ExplanationWithLazyParts::LazyExplanationInvariantSubtypingDueToMutableArray {
            lower_array_loc: f(&lower_array_loc),
            upper_array_loc: f(&upper_array_loc),
            lower_array_desc: type_or_type_desc::map_loc(f, lower_array_desc),
            upper_array_desc: type_or_type_desc::map_loc(f, upper_array_desc),
            upper_array_reference_loc: f(&upper_array_reference_loc),
        },
        ExplanationWithLazyParts::LazyExplanationInvariantSubtypingDueToMutableProperty {
            lower_obj_loc,
            upper_obj_loc,
            lower_obj_desc,
            upper_obj_desc,
            upper_object_loc,
            property_name,
        } => ExplanationWithLazyParts::LazyExplanationInvariantSubtypingDueToMutableProperty {
            lower_obj_loc: f(&lower_obj_loc),
            upper_obj_loc: f(&upper_obj_loc),
            lower_obj_desc: type_or_type_desc::map_loc(f, lower_obj_desc),
            upper_obj_desc: type_or_type_desc::map_loc(f, upper_obj_desc),
            upper_object_loc: f(&upper_object_loc),
            property_name,
        },
        ExplanationWithLazyParts::LazyExplanationInvariantSubtypingDueToMutableProperties {
            lower_obj_loc,
            upper_obj_loc,
            lower_obj_desc,
            upper_obj_desc,
            upper_object_loc,
            properties,
        } => ExplanationWithLazyParts::LazyExplanationInvariantSubtypingDueToMutableProperties {
            lower_obj_loc: f(&lower_obj_loc),
            upper_obj_loc: f(&upper_obj_loc),
            lower_obj_desc: type_or_type_desc::map_loc(f, lower_obj_desc),
            upper_obj_desc: type_or_type_desc::map_loc(f, upper_obj_desc),
            upper_object_loc: f(&upper_object_loc),
            properties,
        },
    }
}

fn map_loc_of_explanation<L: Dupe, M: Dupe, F: Fn(&L) -> M>(
    f: &F,
    explanation: Explanation<L>,
) -> Explanation<M> {
    let map_reason = |r: VirtualReason<L>| -> VirtualReason<M> { r.map_locs(f) };

    match explanation {
        Explanation::ExplanationAbstractEnumCasting => Explanation::ExplanationAbstractEnumCasting,
        Explanation::ExplanationArrayInvariantTyping => {
            Explanation::ExplanationArrayInvariantTyping
        }
        Explanation::ExplanationConstrainedAssign(data) => {
            let ExplanationConstrainedAssignData {
                name,
                declaration,
                providers,
            } = *data;
            Explanation::ExplanationConstrainedAssign(Box::new(ExplanationConstrainedAssignData {
                name,
                declaration: f(&declaration),
                providers: providers.iter().map(f).collect(),
            }))
        }
        Explanation::ExplanationConcreteEnumCasting {
            representation_type,
        } => Explanation::ExplanationConcreteEnumCasting {
            representation_type,
        },
        Explanation::ExplanationCustomError(data) => {
            let ExplanationCustomErrorData {
                name,
                custom_error_loc,
            } = *data;
            Explanation::ExplanationCustomError(Box::new(ExplanationCustomErrorData {
                name,
                custom_error_loc: f(&custom_error_loc),
            }))
        }
        Explanation::ExplanationFunctionsWithStaticsToObject => {
            Explanation::ExplanationFunctionsWithStaticsToObject
        }
        Explanation::ExplanationInvariantSubtypingDueToMutableArray(data) => {
            let ExplanationInvariantSubtypingDueToMutableArrayData {
                lower_array_loc,
                upper_array_loc,
                lower_array_desc,
                upper_array_desc,
                upper_array_reference_loc,
            } = *data;
            Explanation::ExplanationInvariantSubtypingDueToMutableArray(Box::new(
                ExplanationInvariantSubtypingDueToMutableArrayData {
                    lower_array_loc: f(&lower_array_loc),
                    upper_array_loc: f(&upper_array_loc),
                    lower_array_desc: lower_array_desc.map_locs(f),
                    upper_array_desc: upper_array_desc.map_locs(f),
                    upper_array_reference_loc: f(&upper_array_reference_loc),
                },
            ))
        }
        Explanation::ExplanationInvariantSubtypingDueToMutableProperty(data) => {
            let ExplanationInvariantSubtypingDueToMutablePropertyData {
                lower_obj_loc,
                upper_obj_loc,
                lower_obj_desc,
                upper_obj_desc,
                upper_object_loc,
                property_name,
            } = *data;
            Explanation::ExplanationInvariantSubtypingDueToMutableProperty(Box::new(
                ExplanationInvariantSubtypingDueToMutablePropertyData {
                    lower_obj_loc: f(&lower_obj_loc),
                    upper_obj_loc: f(&upper_obj_loc),
                    lower_obj_desc: lower_obj_desc.map_locs(f),
                    upper_obj_desc: upper_obj_desc.map_locs(f),
                    upper_object_loc: f(&upper_object_loc),
                    property_name,
                },
            ))
        }
        Explanation::ExplanationInvariantSubtypingDueToMutableProperties(data) => {
            let ExplanationInvariantSubtypingDueToMutablePropertiesData {
                lower_obj_loc,
                upper_obj_loc,
                lower_obj_desc,
                upper_obj_desc,
                upper_object_loc,
                properties,
            } = *data;
            Explanation::ExplanationInvariantSubtypingDueToMutableProperties(Box::new(
                ExplanationInvariantSubtypingDueToMutablePropertiesData {
                    lower_obj_loc: f(&lower_obj_loc),
                    upper_obj_loc: f(&upper_obj_loc),
                    lower_obj_desc: lower_obj_desc.map_locs(f),
                    upper_obj_desc: upper_obj_desc.map_locs(f),
                    upper_object_loc: f(&upper_object_loc),
                    properties,
                },
            ))
        }
        Explanation::ExplanationMultiplatform => Explanation::ExplanationMultiplatform,
        Explanation::ExplanationPropertyInvariantTyping => {
            Explanation::ExplanationPropertyInvariantTyping
        }
        Explanation::ExplanationPropertyMissingDueToNeutralOptionalProperty(data) => {
            let ExplanationPropertyMissingDueToNeutralOptionalPropertyData {
                props_plural,
                lower_obj_loc,
                upper_obj_loc,
                lower_obj_desc,
                upper_obj_desc,
                upper_object_loc,
            } = *data;
            Explanation::ExplanationPropertyMissingDueToNeutralOptionalProperty(Box::new(
                ExplanationPropertyMissingDueToNeutralOptionalPropertyData {
                    props_plural,
                    lower_obj_loc: f(&lower_obj_loc),
                    upper_obj_loc: f(&upper_obj_loc),
                    lower_obj_desc: lower_obj_desc.map_locs(f),
                    upper_obj_desc: upper_obj_desc.map_locs(f),
                    upper_object_loc: f(&upper_object_loc),
                },
            ))
        }
        Explanation::ExplanationReactComponentPropsDeepReadOnly(loc) => {
            Explanation::ExplanationReactComponentPropsDeepReadOnly(f(&loc))
        }
        Explanation::ExplanationReactHookArgsDeepReadOnly(loc) => {
            Explanation::ExplanationReactHookArgsDeepReadOnly(f(&loc))
        }
        Explanation::ExplanationReactHookIncompatibleWithEachOther => {
            Explanation::ExplanationReactHookIncompatibleWithEachOther
        }
        Explanation::ExplanationReactHookIncompatibleWithNormalFunctions => {
            Explanation::ExplanationReactHookIncompatibleWithNormalFunctions
        }
        Explanation::ExplanationReactHookReturnDeepReadOnly(loc) => {
            Explanation::ExplanationReactHookReturnDeepReadOnly(f(&loc))
        }
        Explanation::ExplanationTypeGuardPositiveConsistency {
            return_,
            param,
            guard_type,
            is_return_false_statement,
        } => Explanation::ExplanationTypeGuardPositiveConsistency {
            return_: map_reason(return_),
            param: map_reason(param),
            guard_type: MessageTypeReferenceData {
                loc: f(&guard_type.loc),
                desc: guard_type.desc.map_locs(f),
            },
            is_return_false_statement,
        },
        Explanation::ExplanationAssertionFunctionConsistency { param, guard_type } => {
            Explanation::ExplanationAssertionFunctionConsistency {
                param: map_reason(param),
                guard_type: MessageTypeReferenceData {
                    loc: f(&guard_type.loc),
                    desc: guard_type.desc.map_locs(f),
                },
            }
        }
        Explanation::ExplanationAdditionalUnionMembers(data) => {
            let ExplanationAdditionalUnionMembersData {
                left_loc,
                left_desc,
                right_loc,
                right_desc,
                members,
                extra_number,
            } = *data;
            Explanation::ExplanationAdditionalUnionMembers(Box::new(
                ExplanationAdditionalUnionMembersData {
                    left_loc: f(&left_loc),
                    left_desc: type_or_type_desc::map_loc(f, left_desc),
                    right_loc: f(&right_loc),
                    right_desc: type_or_type_desc::map_loc(f, right_desc),
                    members,
                    extra_number,
                },
            ))
        }
        Explanation::ExplanationObjectLiteralNeedsRecordSyntax {
            record_name,
            obj_loc,
        } => Explanation::ExplanationObjectLiteralNeedsRecordSyntax {
            record_name,
            obj_loc: f(&obj_loc),
        },
        Explanation::ExplanationStringCasingMustBeCanonical { kind_name } => {
            Explanation::ExplanationStringCasingMustBeCanonical { kind_name }
        }
    }
}

impl<L: Dupe + PartialEq + Eq + PartialOrd + Ord> ErrorMessage<L> {
    pub fn map_loc_of_error_message<F, M>(f: F, msg: ErrorMessage<L>) -> ErrorMessage<M>
    where
        F: Fn(L) -> M + Clone,
        L: Clone + Dupe + PartialEq + Eq + PartialOrd + Ord,
        M: Clone + Dupe + PartialEq + Eq + PartialOrd + Ord,
    {
        use ErrorMessage::*;

        let map_use_op =
            |use_op: VirtualUseOp<L>| -> VirtualUseOp<M> { mod_loc_of_virtual_use_op(&f, use_op) };
        let map_error_ref =
            |r: ErrorReference<L>| -> ErrorReference<M> { r.map_locs(&|l: &L| f(l.dupe())) };
        let map_enum_ref = |r: EnumReferenceData<L>| -> EnumReferenceData<M> {
            EnumReferenceData {
                loc: f(r.loc),
                name: r.name,
            }
        };
        let map_error_type_ref = |r: ErrorTypeReferenceData<L>| -> ErrorTypeReferenceData<M> {
            ErrorTypeReferenceData {
                loc: f(r.loc),
                reference_loc: f(r.reference_loc),
                type_desc: type_or_type_desc::map_loc(|l: &L| f(l.dupe()), r.type_desc),
            }
        };
        let map_error_type_ref_with_reason =
            |r: ErrorTypeReferenceWithLocData<L>| -> ErrorTypeReferenceWithLocData<M> {
                ErrorTypeReferenceWithLocData {
                    loc: f(r.loc),
                    definition_loc: f(r.definition_loc),
                    reference_loc: f(r.reference_loc),
                    type_desc: type_or_type_desc::map_loc(|l: &L| f(l.dupe()), r.type_desc),
                }
            };
        let map_warning = |warning: ConstantConditionWarning<L>| match warning {
            ConstantConditionWarning::Definite => ConstantConditionWarning::Definite,
            ConstantConditionWarning::Likely { suggested_loc } => {
                ConstantConditionWarning::Likely {
                    suggested_loc: suggested_loc.map(&f),
                }
            }
        };
        let map_branch = |e: ErrorMessage<L>| -> ErrorMessage<M> {
            Self::map_loc_of_error_message(f.clone(), e)
        };
        let map_upper_kind = |uk: UpperKind<L>| -> UpperKind<M> {
            match uk {
                UpperKind::IncompatibleGetPropT(loc, s) => {
                    UpperKind::IncompatibleGetPropT(f(loc), s)
                }
                UpperKind::IncompatibleSetPropT(loc, s) => {
                    UpperKind::IncompatibleSetPropT(f(loc), s)
                }
                UpperKind::IncompatibleMethodT(loc, s) => UpperKind::IncompatibleMethodT(f(loc), s),
                UpperKind::IncompatibleHasOwnPropT(loc, s) => {
                    UpperKind::IncompatibleHasOwnPropT(f(loc), s)
                }
                UpperKind::IncompatibleGetElemT(loc) => UpperKind::IncompatibleGetElemT(f(loc)),
                UpperKind::IncompatibleSetElemT(loc) => UpperKind::IncompatibleSetElemT(f(loc)),
                UpperKind::IncompatibleCallElemT(loc) => UpperKind::IncompatibleCallElemT(f(loc)),
                UpperKind::IncompatibleGetPrivatePropT => UpperKind::IncompatibleGetPrivatePropT,
                UpperKind::IncompatibleSetPrivatePropT => UpperKind::IncompatibleSetPrivatePropT,
                UpperKind::IncompatibleCallT => UpperKind::IncompatibleCallT,
                UpperKind::IncompatibleMixedCallT => UpperKind::IncompatibleMixedCallT,
                UpperKind::IncompatibleElemTOfArrT => UpperKind::IncompatibleElemTOfArrT,
                UpperKind::IncompatibleObjAssignFromTSpread => {
                    UpperKind::IncompatibleObjAssignFromTSpread
                }
                UpperKind::IncompatibleObjAssignFromT => UpperKind::IncompatibleObjAssignFromT,
                UpperKind::IncompatibleObjRestT => UpperKind::IncompatibleObjRestT,
                UpperKind::IncompatibleArrRestT => UpperKind::IncompatibleArrRestT,
                UpperKind::IncompatibleSuperT => UpperKind::IncompatibleSuperT,
                UpperKind::IncompatibleMixinT => UpperKind::IncompatibleMixinT,
                UpperKind::IncompatibleSpecializeT => UpperKind::IncompatibleSpecializeT,
                UpperKind::IncompatibleThisSpecializeT => UpperKind::IncompatibleThisSpecializeT,
                UpperKind::IncompatibleVarianceCheckT => UpperKind::IncompatibleVarianceCheckT,
                UpperKind::IncompatibleGetKeysT => UpperKind::IncompatibleGetKeysT,
                UpperKind::IncompatibleGetValuesT => UpperKind::IncompatibleGetValuesT,
                UpperKind::IncompatibleMapTypeTObject => UpperKind::IncompatibleMapTypeTObject,
                UpperKind::IncompatibleGetStaticsT => UpperKind::IncompatibleGetStaticsT,
                UpperKind::IncompatibleUnclassified(s) => UpperKind::IncompatibleUnclassified(s),
            }
        };

        match msg {
            EIncompatibleType(box EIncompatibleTypeData {
                lower_kind,
                lower_loc,
                lower_def_loc,
                lower_desc,
                upper:
                    IncompatibleUpperData {
                        loc: upper_loc,
                        kind: upper_kind,
                    },
                use_op,
            }) => EIncompatibleType(Box::new(EIncompatibleTypeData {
                lower_kind,
                lower_loc: f(lower_loc),
                lower_def_loc: f(lower_def_loc),
                lower_desc: type_or_type_desc::map_loc(|l: &L| f(l.dupe()), lower_desc),
                upper: IncompatibleUpperData {
                    loc: f(upper_loc),
                    kind: map_upper_kind(upper_kind),
                },
                use_op: use_op.map(map_use_op),
            })),

            EIncompatibleSpeculation(box EIncompatibleSpeculationData {
                use_op,
                loc,
                branches,
            }) => EIncompatibleSpeculation(Box::new(EIncompatibleSpeculationData {
                use_op: use_op.map(map_use_op),
                loc: f(loc),
                branches: branches.into_iter().map(map_branch).collect(),
            })),

            EIncompatibleProp(box EIncompatiblePropData {
                use_op,
                prop,
                prop_loc,
                object,
                special,
            }) => EIncompatibleProp(Box::new(EIncompatiblePropData {
                use_op: use_op.map(map_use_op),
                prop,
                prop_loc: f(prop_loc),
                object: map_error_type_ref_with_reason(object),
                special,
            })),

            EExpectedStringLit(box EExpectedStringLitData {
                lower,
                upper,
                use_op,
            }) => EExpectedStringLit(Box::new(EExpectedStringLitData {
                lower: map_error_type_ref_with_reason(lower),
                upper: map_error_type_ref_with_reason(upper),
                use_op: map_use_op(use_op),
            })),

            EExpectedNumberLit(box EExpectedNumberLitData {
                lower,
                upper,
                use_op,
            }) => EExpectedNumberLit(Box::new(EExpectedNumberLitData {
                lower: map_error_type_ref_with_reason(lower),
                upper: map_error_type_ref_with_reason(upper),
                use_op: map_use_op(use_op),
            })),

            EExpectedBooleanLit(box EExpectedBooleanLitData {
                lower,
                upper,
                use_op,
            }) => EExpectedBooleanLit(Box::new(EExpectedBooleanLitData {
                lower: map_error_type_ref_with_reason(lower),
                upper: map_error_type_ref_with_reason(upper),
                use_op: map_use_op(use_op),
            })),

            EExpectedBigIntLit(box EExpectedBigIntLitData {
                lower,
                upper,
                use_op,
            }) => EExpectedBigIntLit(Box::new(EExpectedBigIntLitData {
                lower: map_error_type_ref_with_reason(lower),
                upper: map_error_type_ref_with_reason(upper),
                use_op: map_use_op(use_op),
            })),

            EPropNotFoundInLookup(box EPropNotFoundInLookupData {
                prop_name,
                prop_loc,
                object,
                use_op,
                suggestion,
            }) => EPropNotFoundInLookup(Box::new(EPropNotFoundInLookupData {
                prop_name,
                prop_loc: f(prop_loc),
                object: map_error_type_ref_with_reason(object),
                use_op: map_use_op(use_op),
                suggestion,
            })),

            EPropNotFoundInSubtyping(box EPropNotFoundInSubtypingData {
                prop_name,
                lower,
                upper,
                use_op,
                suggestion,
            }) => EPropNotFoundInSubtyping(Box::new(EPropNotFoundInSubtypingData {
                prop_name,
                lower: map_error_type_ref_with_reason(lower),
                upper: map_error_type_ref_with_reason(upper),
                use_op: map_use_op(use_op),
                suggestion,
            })),

            EConstructSignatureMissingInSubtyping(
                box EConstructSignatureMissingInSubtypingData {
                    lower,
                    upper,
                    use_op,
                },
            ) => EConstructSignatureMissingInSubtyping(Box::new(
                EConstructSignatureMissingInSubtypingData {
                    lower: map_error_type_ref_with_reason(lower),
                    upper: map_error_type_ref_with_reason(upper),
                    use_op: map_use_op(use_op),
                },
            )),

            EPropsNotFoundInSubtyping(box EPropsNotFoundInSubtypingData {
                prop_names,
                lower,
                upper,
                use_op,
            }) => EPropsNotFoundInSubtyping(Box::new(EPropsNotFoundInSubtypingData {
                prop_names,
                lower: map_error_type_ref_with_reason(lower),
                upper: map_error_type_ref_with_reason(upper),
                use_op: map_use_op(use_op),
            })),

            EPropsNotFoundInInvariantSubtyping(box EPropsNotFoundInInvariantSubtypingData {
                prop_names,
                lower,
                upper,
                use_op,
            }) => EPropsNotFoundInInvariantSubtyping(Box::new(
                EPropsNotFoundInInvariantSubtypingData {
                    prop_names,
                    lower: map_error_type_ref_with_reason(lower),
                    upper: map_error_type_ref_with_reason(upper),
                    use_op: map_use_op(use_op),
                },
            )),

            EPropsExtraAgainstExactObject(box EPropsExtraAgainstExactObjectData {
                prop_names,
                lower,
                upper,
                upper_is_record,
                use_op,
            }) => EPropsExtraAgainstExactObject(Box::new(EPropsExtraAgainstExactObjectData {
                prop_names,
                lower: map_error_type_ref_with_reason(lower),
                upper: map_error_type_ref_with_reason(upper),
                upper_is_record,
                use_op: map_use_op(use_op),
            })),

            EIndexerCheckFailed(box EIndexerCheckFailedData {
                prop_name,
                lower,
                upper,
                indexer_loc,
                indexer_desc,
                use_op,
            }) => EIndexerCheckFailed(Box::new(EIndexerCheckFailedData {
                prop_name,
                lower: map_error_type_ref_with_reason(lower),
                upper: map_error_type_ref_with_reason(upper),
                indexer_loc: f(indexer_loc),
                indexer_desc: type_or_type_desc::map_loc(|l: &L| f(l.dupe()), indexer_desc),
                use_op: map_use_op(use_op),
            })),

            EPropNotReadable(box EPropNotReadableData {
                prop_loc,
                prop_name,
                use_op,
            }) => EPropNotReadable(Box::new(EPropNotReadableData {
                prop_loc: f(prop_loc),
                prop_name,
                use_op: map_use_op(use_op),
            })),

            EPropNotWritable(box EPropNotWritableData {
                prop_loc,
                prop_name,
                use_op,
            }) => EPropNotWritable(Box::new(EPropNotWritableData {
                prop_loc: f(prop_loc),
                prop_name,
                use_op: map_use_op(use_op),
            })),

            EPropPolarityMismatch(box EPropPolarityMismatchData {
                lower,
                upper,
                props,
                use_op,
            }) => {
                let map_object = |object| match object {
                    PropPolarityMismatchObject::Type(t) => {
                        PropPolarityMismatchObject::Type(map_error_type_ref_with_reason(t))
                    }
                    PropPolarityMismatchObject::Class(NamedReferenceData { loc, name }) => {
                        PropPolarityMismatchObject::Class(NamedReferenceData { loc: f(loc), name })
                    }
                };
                EPropPolarityMismatch(Box::new(EPropPolarityMismatchData {
                    lower: map_object(lower),
                    upper: map_object(upper),
                    props,
                    use_op: map_use_op(use_op),
                }))
            }

            EBuiltinNameLookupFailed(box EBuiltinNameLookupFailedData { loc, name }) => {
                EBuiltinNameLookupFailed(Box::new(EBuiltinNameLookupFailedData {
                    loc: f(loc),
                    name,
                }))
            }

            EUmdGlobalValueInModule(box EUmdGlobalValueInModuleData { loc, name }) => {
                EUmdGlobalValueInModule(Box::new(EUmdGlobalValueInModuleData { loc: f(loc), name }))
            }

            EBuiltinModuleLookupFailed(box EBuiltinModuleLookupFailedData {
                loc,
                name,
                potential_generator,
            }) => EBuiltinModuleLookupFailed(Box::new(EBuiltinModuleLookupFailedData {
                loc: f(loc),
                name,
                potential_generator,
            })),

            EExpectedModuleLookupFailed(box EExpectedModuleLookupFailedData {
                loc,
                name,
                expected_module_purpose,
            }) => EExpectedModuleLookupFailed(Box::new(EExpectedModuleLookupFailedData {
                loc: f(loc),
                name,
                expected_module_purpose,
            })),

            ECannotImportGlobalLibdef(box ECannotImportGlobalLibdefData {
                loc,
                module_name,
                libdef_name,
            }) => ECannotImportGlobalLibdef(Box::new(ECannotImportGlobalLibdefData {
                loc: f(loc),
                module_name,
                libdef_name,
            })),

            EPrivateLookupFailed(box EPrivateLookupFailedData {
                loc,
                object,
                prop_name,
                use_op,
            }) => EPrivateLookupFailed(Box::new(EPrivateLookupFailedData {
                loc: f(loc),
                object: map_error_type_ref_with_reason(object),
                prop_name,
                use_op: map_use_op(use_op),
            })),

            EPlatformSpecificImplementationModuleLookupFailed(
                box EPlatformSpecificImplementationModuleLookupFailedData { loc, name },
            ) => EPlatformSpecificImplementationModuleLookupFailed(Box::new(
                EPlatformSpecificImplementationModuleLookupFailedData { loc: f(loc), name },
            )),

            ETupleArityMismatch(box ETupleArityMismatchData {
                use_op,
                lower,
                lower_arity,
                lower_inexact,
                lower_rest,
                upper,
                upper_arity,
                upper_inexact,
                upper_rest,
                unify,
            }) => ETupleArityMismatch(Box::new(ETupleArityMismatchData {
                use_op: map_use_op(use_op),
                lower: map_error_type_ref_with_reason(lower),
                lower_arity,
                lower_inexact,
                lower_rest,
                upper: map_error_type_ref_with_reason(upper),
                upper_arity,
                upper_inexact,
                upper_rest,
                unify,
            })),

            ENonLitArrayToTuple((r1, r2), op) => ENonLitArrayToTuple(
                (
                    map_error_type_ref_with_reason(r1),
                    map_error_type_ref_with_reason(r2),
                ),
                map_use_op(op),
            ),

            ETupleOutOfBounds(box ETupleOutOfBoundsData {
                use_op,
                loc,
                tuple,
                inexact,
                length,
                index,
            }) => ETupleOutOfBounds(Box::new(ETupleOutOfBoundsData {
                use_op: map_use_op(use_op),
                loc: f(loc),
                tuple: map_error_type_ref_with_reason(tuple),
                inexact,
                length,
                index,
            })),

            ETupleNonIntegerIndex(box ETupleNonIntegerIndexData {
                use_op,
                loc,
                index_def_loc,
                index,
            }) => ETupleNonIntegerIndex(Box::new(ETupleNonIntegerIndexData {
                use_op: map_use_op(use_op),
                loc: f(loc),
                index_def_loc: f(index_def_loc),
                index,
            })),

            ETupleUnsafeWrite { loc, use_op } => ETupleUnsafeWrite {
                loc: f(loc),
                use_op: map_use_op(use_op),
            },

            ETupleElementNotReadable(box ETupleElementNotReadableData {
                loc,
                index_def_loc,
                index,
                name,
                use_op,
            }) => ETupleElementNotReadable(Box::new(ETupleElementNotReadableData {
                loc: f(loc),
                index_def_loc: f(index_def_loc),
                index,
                name,
                use_op: map_use_op(use_op),
            })),

            ETupleElementNotWritable(box ETupleElementNotWritableData {
                loc,
                index_def_loc,
                index,
                name,
                use_op,
            }) => ETupleElementNotWritable(Box::new(ETupleElementNotWritableData {
                loc: f(loc),
                index_def_loc: f(index_def_loc),
                index,
                name,
                use_op: map_use_op(use_op),
            })),

            ETupleElementPolarityMismatch(box ETupleElementPolarityMismatchData {
                index,
                lower,
                polarity_lower,
                upper,
                polarity_upper,
                use_op,
            }) => ETupleElementPolarityMismatch(Box::new(ETupleElementPolarityMismatchData {
                index,
                lower: map_error_type_ref_with_reason(lower),
                polarity_lower,
                upper: map_error_type_ref_with_reason(upper),
                polarity_upper,
                use_op: map_use_op(use_op),
            })),

            ETupleRequiredAfterOptional(box ETupleRequiredAfterOptionalData {
                tuple_loc,
                required,
                optional,
            }) => ETupleRequiredAfterOptional(Box::new(ETupleRequiredAfterOptionalData {
                tuple_loc: f(tuple_loc),
                required: TupleElementReferenceData {
                    loc: f(required.loc),
                    name: required.name,
                },
                optional: TupleElementReferenceData {
                    loc: f(optional.loc),
                    name: optional.name,
                },
            })),

            ETupleInvalidTypeSpread(box ETupleInvalidTypeSpreadData {
                spread_loc,
                argument,
            }) => ETupleInvalidTypeSpread(Box::new(ETupleInvalidTypeSpreadData {
                spread_loc: f(spread_loc),
                argument: map_error_type_ref_with_reason(argument),
            })),

            ETupleElementAfterInexactSpread(loc) => ETupleElementAfterInexactSpread(f(loc)),

            ETupleElementAfterRestSpread(loc) => ETupleElementAfterRestSpread(f(loc)),

            EROArrayWrite(loc, op) => EROArrayWrite(f(loc), map_use_op(op)),

            EUnionSpeculationFailed(box EUnionSpeculationFailedData {
                use_op,
                loc,
                branches,
            }) => EUnionSpeculationFailed(Box::new(EUnionSpeculationFailedData {
                use_op: map_use_op(use_op),
                loc: f(loc),
                branches: branches.into_iter().map(map_branch).collect(),
            })),

            EIncompatibleWithExact((r1, r2), op, kind) => EIncompatibleWithExact(
                (
                    map_error_type_ref_with_reason(r1),
                    map_error_type_ref_with_reason(r2),
                ),
                map_use_op(op),
                kind,
            ),

            EFunctionIncompatibleWithIndexer((r1, r2), op) => EFunctionIncompatibleWithIndexer(
                (
                    map_error_type_ref_with_reason(r1),
                    map_error_type_ref_with_reason(r2),
                ),
                map_use_op(op),
            ),

            EInvalidConstructor(box EInvalidConstructorData {
                loc,
                value_loc,
                value_desc,
            }) => EInvalidConstructor(Box::new(EInvalidConstructorData {
                loc: f(loc),
                value_loc: f(value_loc),
                value_desc: type_or_type_desc::map_loc(|l: &L| f(l.dupe()), value_desc),
            })),

            EInvalidObjectKit(box EInvalidObjectKitData {
                loc,
                value,
                use_op,
                requirement,
            }) => EInvalidObjectKit(Box::new(EInvalidObjectKitData {
                loc: f(loc),
                value: map_error_type_ref(value),
                use_op: map_use_op(use_op),
                requirement,
            })),

            EIncompatibleTypesWithUseOp(box EIncompatibleTypesWithUseOpData {
                use_op,
                lower_loc,
                lower_def_loc,
                upper_loc,
                upper_def_loc,
                lower_desc,
                upper_desc,
                explanation,
                example,
                branches,
            }) => EIncompatibleTypesWithUseOp(Box::new(EIncompatibleTypesWithUseOpData {
                use_op: map_use_op(use_op),
                lower_loc: f(lower_loc),
                lower_def_loc: f(lower_def_loc),
                upper_loc: f(upper_loc),
                upper_def_loc: f(upper_def_loc),
                lower_desc: type_or_type_desc::map_loc(|l: &L| f(l.dupe()), lower_desc),
                upper_desc: type_or_type_desc::map_loc(|l: &L| f(l.dupe()), upper_desc),
                explanation: explanation.map(|e| map_loc_of_explanation(&|l: &L| f(l.dupe()), e)),
                example: example.map(|example| Box::new(map_branch(*example))),
                branches: branches.into_iter().map(map_branch).collect(),
            })),

            EInvariantSubtypingWithUseOp(box EInvariantSubtypingWithUseOpData {
                sub_component,
                use_op,
                lower_loc,
                lower_desc,
                upper_loc,
                upper_desc,
                explanation,
            }) => EInvariantSubtypingWithUseOp(Box::new(EInvariantSubtypingWithUseOpData {
                sub_component,
                use_op: map_use_op(use_op),
                lower_loc: f(lower_loc),
                lower_desc: type_or_type_desc::map_loc(|l: &L| f(l.dupe()), lower_desc),
                upper_loc: f(upper_loc),
                upper_desc: type_or_type_desc::map_loc(|l: &L| f(l.dupe()), upper_desc),
                explanation: explanation
                    .map(|e| map_loc_of_lazy_explanation(&|l: &L| f(l.dupe()), e)),
            })),

            ENotAReactComponent(box ENotAReactComponentData { component, use_op }) => {
                ENotAReactComponent(Box::new(ENotAReactComponentData {
                    component: map_error_type_ref_with_reason(component),
                    use_op: map_use_op(use_op),
                }))
            }

            EInvalidReactCreateElement(box EInvalidReactCreateElementData {
                create_element_loc,
                react,
            }) => EInvalidReactCreateElement(Box::new(EInvalidReactCreateElementData {
                create_element_loc: f(create_element_loc),
                react: NamedReferenceData {
                    loc: f(react.loc),
                    name: react.name,
                },
            })),

            EInvalidThisArg(box EInvalidThisArgData {
                loc,
                name,
                receiver_expression,
                kind,
            }) => EInvalidThisArg(Box::new(EInvalidThisArgData {
                loc: f(loc),
                name,
                receiver_expression: ExpressionReferenceData {
                    loc: f(receiver_expression.loc),
                    kind: receiver_expression.kind,
                },
                kind,
            })),

            EFunctionCallExtraArg(box EFunctionCallExtraArgData {
                loc,
                function_reference,
                param_count,
                use_op,
            }) => EFunctionCallExtraArg(Box::new(EFunctionCallExtraArgData {
                loc: f(loc),
                function_reference: FunctionReferenceData {
                    loc: f(function_reference.loc),
                    kind: function_reference.kind,
                },
                param_count,
                use_op: map_use_op(use_op),
            })),

            EExportValueAsType(box (loc, s)) => EExportValueAsType(Box::new((f(loc), s))),
            EImportValueAsType(box (loc, s)) => EImportValueAsType(Box::new((f(loc), s))),
            EImportTypeAsTypeof(box (loc, s)) => EImportTypeAsTypeof(Box::new((f(loc), s))),
            EImportTypeAsValue(box (loc, s)) => EImportTypeAsValue(Box::new((f(loc), s))),

            ENoDefaultExport(box (loc, s1, s2)) => ENoDefaultExport(Box::new((f(loc), s1, s2))),
            EOnlyDefaultExport(box (loc, s1, s2)) => EOnlyDefaultExport(Box::new((f(loc), s1, s2))),
            ENoNamedExport(box (loc, s1, s2, s3)) => ENoNamedExport(Box::new((f(loc), s1, s2, s3))),

            EMissingTypeArgs(box EMissingTypeArgsData {
                loc,
                type_,
                arity_loc,
                min_arity,
                max_arity,
            }) => EMissingTypeArgs(Box::new(EMissingTypeArgsData {
                loc: f(loc),
                type_: map_error_type_ref_with_reason(type_),
                arity_loc: f(arity_loc),
                min_arity,
                max_arity,
            })),

            EAnyValueUsedAsType { reference, value } => EAnyValueUsedAsType {
                reference,
                value: map_error_type_ref(value),
            },

            EValueUsedAsType { reference, value } => EValueUsedAsType {
                reference,
                value: map_error_type_ref(value),
            },

            EPolarityMismatch(box EPolarityMismatchData {
                loc,
                type_param_loc,
                name,
                expected_polarity,
                actual_polarity,
            }) => EPolarityMismatch(Box::new(EPolarityMismatchData {
                loc: f(loc),
                type_param_loc: f(type_param_loc),
                name,
                expected_polarity,
                actual_polarity,
            })),

            EComparison(box EComparisonData {
                r1,
                r2,
                loc_opt,
                strict_comparison_opt,
            }) => EComparison(Box::new(EComparisonData {
                r1: map_error_type_ref_with_reason(r1),
                r2: map_error_type_ref_with_reason(r2),
                loc_opt: loc_opt.map(&f),
                strict_comparison_opt: strict_comparison_opt.map(|info| {
                    map_strict_comparison_info(info, &f, map_error_type_ref_with_reason)
                }),
            })),

            ENonStrictEqualityComparison(box ENonStrictEqualityComparisonData { lower, upper }) => {
                ENonStrictEqualityComparison(Box::new(ENonStrictEqualityComparisonData {
                    lower: map_error_type_ref_with_reason(lower),
                    upper: map_error_type_ref_with_reason(upper),
                }))
            }

            EUnsupportedExact(box EUnsupportedExactData {
                value_loc,
                value_desc,
            }) => EUnsupportedExact(Box::new(EUnsupportedExactData {
                value_loc: f(value_loc),
                value_desc: type_or_type_desc::map_loc(|l: &L| f(l.dupe()), value_desc),
            })),

            EInvalidExactObjectIntersection(box EInvalidExactObjectIntersectionData {
                loc,
                prop,
                exact_object_loc,
                exact_object_desc,
            }) => EInvalidExactObjectIntersection(Box::new(EInvalidExactObjectIntersectionData {
                loc: f(loc),
                prop,
                exact_object_loc: f(exact_object_loc),
                exact_object_desc: type_or_type_desc::map_loc(
                    |l: &L| f(l.dupe()),
                    exact_object_desc,
                ),
            })),

            EUnexpectedThisType(loc) => EUnexpectedThisType(f(loc)),

            ETypeParamArity(loc, i) => ETypeParamArity(f(loc), i),

            ECallTypeArity(box ECallTypeArityData {
                call_loc,
                is_new,
                callee,
                expected_arity,
            }) => ECallTypeArity(Box::new(ECallTypeArityData {
                call_loc: f(call_loc),
                is_new,
                callee: match callee {
                    CallTypeArityCallee::Type(callee) => {
                        CallTypeArityCallee::Type(map_error_type_ref(callee))
                    }
                    CallTypeArityCallee::Builtin(name) => CallTypeArityCallee::Builtin(name),
                    CallTypeArityCallee::Function => CallTypeArityCallee::Function,
                },
                expected_arity,
            })),

            ETooManyTypeArgs(box ETooManyTypeArgsData {
                reason_tapp,
                arity_loc,
                maximum_arity,
            }) => ETooManyTypeArgs(Box::new(ETooManyTypeArgsData {
                reason_tapp: map_error_ref(reason_tapp),
                arity_loc: f(arity_loc),
                maximum_arity,
            })),

            ETooFewTypeArgs(box ETooFewTypeArgsData {
                reason_tapp,
                arity_loc,
                minimum_arity,
            }) => ETooFewTypeArgs(Box::new(ETooFewTypeArgsData {
                reason_tapp: map_error_ref(reason_tapp),
                arity_loc: f(arity_loc),
                minimum_arity,
            })),

            EInvalidInfer(l) => EInvalidInfer(f(l)),

            EConstantCondition(box EConstantConditionData {
                loc,
                is_truthy,
                warning,
                constant_condition_kind,
            }) => EConstantCondition(Box::new(EConstantConditionData {
                loc: f(loc),
                is_truthy,
                warning: map_warning(warning),
                constant_condition_kind,
            })),

            EInvalidExtends(box (loc, expression)) => {
                EInvalidExtends(Box::new((f(loc), expression)))
            }

            EExportsAnnot(loc) => EExportsAnnot(f(loc)),

            EUnsupportedKeyInObject {
                loc,
                obj_kind,
                key_error_kind,
            } => EUnsupportedKeyInObject {
                loc: f(loc),
                obj_kind,
                key_error_kind,
            },

            EUnsupportedComputedKeyInClass(loc) => EUnsupportedComputedKeyInClass(f(loc)),

            EAmbiguousNumericKeyWithVariance(loc) => EAmbiguousNumericKeyWithVariance(f(loc)),

            ETypeGuardFuncIncompatibility {
                use_op,
                lower,
                upper,
            } => {
                let map_reference = |reference| match reference {
                    TypeGuardFuncReference::Type(reference) => {
                        TypeGuardFuncReference::Type(map_error_type_ref_with_reason(reference))
                    }
                    TypeGuardFuncReference::TypeGuard(reference) => {
                        TypeGuardFuncReference::TypeGuard(TypeGuardReferenceData {
                            loc: f(reference.loc),
                            kind: reference.kind,
                        })
                    }
                };
                ETypeGuardFuncIncompatibility {
                    use_op: map_use_op(use_op),
                    lower: map_reference(lower),
                    upper: map_reference(upper),
                }
            }

            ETypeGuardInvalidParameter(box ETypeGuardInvalidParameterData {
                type_guard,
                binding_loc,
                binding_kind,
            }) => ETypeGuardInvalidParameter(Box::new(ETypeGuardInvalidParameterData {
                type_guard: TypeGuardParameterData {
                    loc: f(type_guard.loc),
                    name: type_guard.name,
                },
                binding_loc: f(binding_loc),
                binding_kind,
            })),

            ETypeGuardIndexMismatch {
                use_op,
                parameters: (p1, p2),
            } => ETypeGuardIndexMismatch {
                use_op: map_use_op(use_op),
                parameters: (
                    TypeGuardParameterData {
                        loc: f(p1.loc),
                        name: p1.name,
                    },
                    TypeGuardParameterData {
                        loc: f(p2.loc),
                        name: p2.name,
                    },
                ),
            },

            ETypeGuardImpliesMismatch {
                use_op,
                lower,
                upper,
            } => ETypeGuardImpliesMismatch {
                use_op: map_use_op(use_op),
                lower: TypeGuardReferenceData {
                    loc: f(lower.loc),
                    kind: lower.kind,
                },
                upper: TypeGuardReferenceData {
                    loc: f(upper.loc),
                    kind: upper.kind,
                },
            },

            ETypeGuardParamUnbound(parameter) => ETypeGuardParamUnbound(TypeGuardParameterData {
                loc: f(parameter.loc),
                name: parameter.name,
            }),

            ETypeGuardThisParam(loc) => ETypeGuardThisParam(f(loc)),

            ETypeGuardFunctionInvalidWrites(box ETypeGuardFunctionInvalidWritesData {
                loc,
                type_guard,
                write_locs,
            }) => ETypeGuardFunctionInvalidWrites(Box::new(ETypeGuardFunctionInvalidWritesData {
                loc: f(loc),
                type_guard: NamedReferenceData {
                    loc: f(type_guard.loc),
                    name: type_guard.name,
                },
                write_locs: write_locs.into_iter().map(&f).collect(),
            })),

            ETypeGuardFunctionParamHavoced(box ETypeGuardFunctionParamHavocedData {
                type_guard,
                param_loc,
                is_this,
                call_locs,
            }) => ETypeGuardFunctionParamHavoced(Box::new(ETypeGuardFunctionParamHavocedData {
                type_guard: TypeGuardParameterData {
                    loc: f(type_guard.loc),
                    name: type_guard.name,
                },
                param_loc: f(param_loc),
                is_this,
                call_locs: call_locs.into_iter().map(&f).collect(),
            })),

            ETypeGuardIncompatibleWithFunctionKind(
                box ETypeGuardIncompatibleWithFunctionKindData { loc, kind },
            ) => ETypeGuardIncompatibleWithFunctionKind(Box::new(
                ETypeGuardIncompatibleWithFunctionKindData { loc: f(loc), kind },
            )),

            ENegativeTypeGuardConsistency(box ENegativeTypeGuardConsistencyData {
                return_reason,
                type_,
            }) => ENegativeTypeGuardConsistency(Box::new(ENegativeTypeGuardConsistencyData {
                return_reason: map_error_ref(return_reason),
                type_: map_error_type_ref(type_),
            })),

            EAssertionFunctionFalsyAtExit(box EAssertionFunctionFalsyAtExitData {
                loc,
                param,
                falsy,
                implicit_return,
            }) => EAssertionFunctionFalsyAtExit(Box::new(EAssertionFunctionFalsyAtExitData {
                loc: f(loc),
                param: NamedReferenceData {
                    loc: f(param.loc),
                    name: param.name,
                },
                falsy: map_error_type_ref(falsy),
                implicit_return,
            })),

            EUnrecognizedAssertionCall(box EUnrecognizedAssertionCallData { loc, callee }) => {
                EUnrecognizedAssertionCall(Box::new(EUnrecognizedAssertionCallData {
                    loc: f(loc),
                    callee: match callee {
                        UnrecognizedAssertionCallee::UnsupportedTarget => {
                            UnrecognizedAssertionCallee::UnsupportedTarget
                        }
                        UnrecognizedAssertionCallee::UnannotatedRoot(root) => {
                            UnrecognizedAssertionCallee::UnannotatedRoot(NamedReferenceData {
                                loc: f(root.loc),
                                name: root.name,
                            })
                        }
                        UnrecognizedAssertionCallee::UnsupportedType => {
                            UnrecognizedAssertionCallee::UnsupportedType
                        }
                    },
                }))
            }

            ETypeParamConstIncompatibility(box ETypeParamConstIncompatibilityData {
                use_op,
                lower,
                upper,
            }) => ETypeParamConstIncompatibility(Box::new(ETypeParamConstIncompatibilityData {
                use_op: map_use_op(use_op),
                lower: NamedReferenceData {
                    loc: f(lower.loc),
                    name: lower.name,
                },
                upper: NamedReferenceData {
                    loc: f(upper.loc),
                    name: upper.name,
                },
            })),

            ETypeParamConstInvalidPosition(reason) => {
                ETypeParamConstInvalidPosition(Box::new((f(reason.0), reason.1)))
            }

            EInternal(box (loc, i)) => EInternal(Box::new((f(loc), i))),
            EUnsupportedSyntax(box (loc, u)) => EUnsupportedSyntax(Box::new((f(loc), u))),
            EUseArrayLiteral(loc) => EUseArrayLiteral(f(loc)),

            EMissingLocalAnnotation {
                loc,
                target,
                hint_available,
                from_generic_function,
            } => EMissingLocalAnnotation {
                loc: f(loc),
                target,
                hint_available,
                from_generic_function,
            },

            EMissingThisAnnotation(loc) => EMissingThisAnnotation(f(loc)),

            EMissingParameterAnnotation {
                loc,
                hint_available,
            } => EMissingParameterAnnotation {
                loc: f(loc),
                hint_available,
            },

            EBindingError(box (b, loc, s, scope)) => {
                EBindingError(Box::new((b, f(loc), s, f(scope))))
            }

            ERecursionLimit(loc) => ERecursionLimit(f(loc)),

            EUninitializedInstanceProperty(loc, e) => EUninitializedInstanceProperty(f(loc), e),

            EUninitializedProperty(box (loc, name, kind)) => {
                EUninitializedProperty(Box::new((f(loc), name, kind)))
            }

            EEnumError(enum_error) => {
                use EnumErrorKind::*;
                EEnumError(match enum_error {
                    EnumsNotEnabled(loc) => EnumsNotEnabled(f(loc)),
                    EnumConstNotSupported(loc) => EnumConstNotSupported(f(loc)),
                    EnumInvalidMemberAccess(box EnumInvalidMemberAccessData {
                        member,
                        member_loc,
                        enum_,
                    }) => EnumInvalidMemberAccess(Box::new(EnumInvalidMemberAccessData {
                        member: match member {
                            EnumMemberAccess::Named { name, suggestion } => {
                                EnumMemberAccess::Named { name, suggestion }
                            }
                            EnumMemberAccess::Computed(key) => {
                                EnumMemberAccess::Computed(map_error_type_ref(key))
                            }
                        },
                        member_loc: f(member_loc),
                        enum_: map_enum_ref(enum_),
                    })),
                    EnumModification(box EnumModificationData { loc, enum_ }) => {
                        EnumModification(Box::new(EnumModificationData {
                            loc: f(loc),
                            enum_: map_enum_ref(enum_),
                        }))
                    }
                    EnumMemberDuplicateValue(box EnumMemberDuplicateValueData {
                        loc,
                        prev_use_loc,
                        enum_,
                    }) => EnumMemberDuplicateValue(Box::new(EnumMemberDuplicateValueData {
                        loc: f(loc),
                        prev_use_loc: f(prev_use_loc),
                        enum_: map_enum_ref(enum_),
                    })),
                    EnumInvalidObjectUtilType(box EnumInvalidObjectUtilTypeData {
                        operation_loc,
                        operation_name,
                        enum_,
                    }) => EnumInvalidObjectUtilType(Box::new(EnumInvalidObjectUtilTypeData {
                        operation_loc: f(operation_loc),
                        operation_name,
                        enum_: map_enum_ref(enum_),
                    })),
                    EnumInvalidObjectFunction(box EnumInvalidObjectFunctionData {
                        operation_loc,
                        function_name,
                        enum_,
                    }) => EnumInvalidObjectFunction(Box::new(EnumInvalidObjectFunctionData {
                        operation_loc: f(operation_loc),
                        function_name,
                        enum_: map_enum_ref(enum_),
                    })),
                    EnumNotIterable(box EnumNotIterableData { enum_ }) => {
                        EnumNotIterable(Box::new(EnumNotIterableData {
                            enum_: map_enum_ref(enum_),
                        }))
                    }
                    EnumNotIterableForIn(box EnumNotIterableForInData { loc, enum_ }) => {
                        EnumNotIterableForIn(Box::new(EnumNotIterableForInData {
                            loc: f(loc),
                            enum_: map_enum_ref(enum_),
                        }))
                    }
                    EnumMemberAlreadyChecked(box EnumMemberAlreadyCheckedData {
                        case_test_loc,
                        prev_check_loc,
                        enum_,
                        member_name,
                    }) => EnumMemberAlreadyChecked(Box::new(EnumMemberAlreadyCheckedData {
                        case_test_loc: f(case_test_loc),
                        prev_check_loc: f(prev_check_loc),
                        enum_: NamedReferenceData {
                            loc: f(enum_.loc),
                            name: enum_.name,
                        },
                        member_name,
                    })),
                    EnumAllMembersAlreadyChecked(box EnumAllMembersAlreadyCheckedData {
                        loc,
                        enum_,
                    }) => {
                        EnumAllMembersAlreadyChecked(Box::new(EnumAllMembersAlreadyCheckedData {
                            loc: f(loc),
                            enum_: NamedReferenceData {
                                loc: f(enum_.loc),
                                name: enum_.name,
                            },
                        }))
                    }
                    EnumNotAllChecked(box EnumNotAllCheckedData {
                        loc,
                        description_name,
                        enum_,
                        left_to_check,
                        default_case_loc,
                    }) => EnumNotAllChecked(Box::new(EnumNotAllCheckedData {
                        loc: f(loc),
                        description_name,
                        enum_: NamedReferenceData {
                            loc: f(enum_.loc),
                            name: enum_.name,
                        },
                        left_to_check,
                        default_case_loc: default_case_loc.map(&f),
                    })),
                    EnumUnknownNotChecked(box EnumUnknownNotCheckedData {
                        loc,
                        description_name,
                        enum_,
                    }) => EnumUnknownNotChecked(Box::new(EnumUnknownNotCheckedData {
                        loc: f(loc),
                        description_name,
                        enum_: NamedReferenceData {
                            loc: f(enum_.loc),
                            name: enum_.name,
                        },
                    })),
                    EnumInvalidCheck(box EnumInvalidCheckData {
                        loc,
                        enum_,
                        example_member,
                        from_match,
                    }) => EnumInvalidCheck(Box::new(EnumInvalidCheckData {
                        loc: f(loc),
                        enum_: map_enum_ref(enum_),
                        example_member,
                        from_match,
                    })),
                    EnumMemberUsedAsType(box EnumMemberUsedAsTypeData {
                        loc,
                        description_name,
                        enum_,
                    }) => EnumMemberUsedAsType(Box::new(EnumMemberUsedAsTypeData {
                        loc: f(loc),
                        description_name,
                        enum_: map_enum_ref(enum_),
                    })),
                    EnumIncompatible(box EIncompatibleTypesWithUseOpData {
                        use_op,
                        lower_loc,
                        lower_def_loc,
                        upper_loc,
                        upper_def_loc,
                        lower_desc,
                        upper_desc,
                        explanation,
                        example,
                        branches,
                    }) => EnumIncompatible(Box::new(EIncompatibleTypesWithUseOpData {
                        use_op: map_use_op(use_op),
                        lower_loc: f(lower_loc),
                        lower_def_loc: f(lower_def_loc),
                        upper_loc: f(upper_loc),
                        upper_def_loc: f(upper_def_loc),
                        lower_desc: type_or_type_desc::map_loc(|l: &L| f(l.dupe()), lower_desc),
                        upper_desc: type_or_type_desc::map_loc(|l: &L| f(l.dupe()), upper_desc),
                        explanation: explanation
                            .map(|e| map_loc_of_explanation(&|l: &L| f(l.dupe()), e)),
                        example: example.map(|example| Box::new(map_branch(*example))),
                        branches: branches.into_iter().map(map_branch).collect(),
                    })),
                    EnumInvalidAbstractUse(box EnumInvalidAbstractUseData {
                        loc,
                        description_name,
                        enum_loc,
                        enum_name,
                    }) => EnumInvalidAbstractUse(Box::new(EnumInvalidAbstractUseData {
                        loc: f(loc),
                        description_name,
                        enum_loc: f(enum_loc),
                        enum_name,
                    })),
                    EnumInvalidMemberName(box EnumInvalidMemberNameData {
                        loc,
                        enum_,
                        member_name,
                    }) => EnumInvalidMemberName(Box::new(EnumInvalidMemberNameData {
                        loc: f(loc),
                        enum_: map_enum_ref(enum_),
                        member_name,
                    })),
                    EnumNonIdentifierMemberName(box EnumNonIdentifierMemberNameData {
                        loc,
                        enum_,
                        member_name,
                    }) => EnumNonIdentifierMemberName(Box::new(EnumNonIdentifierMemberNameData {
                        loc: f(loc),
                        enum_: map_enum_ref(enum_),
                        member_name,
                    })),
                    EnumDuplicateMemberName(box EnumDuplicateMemberNameData {
                        loc,
                        prev_use_loc,
                        enum_,
                        member_name,
                    }) => EnumDuplicateMemberName(Box::new(EnumDuplicateMemberNameData {
                        loc: f(loc),
                        prev_use_loc: f(prev_use_loc),
                        enum_: map_enum_ref(enum_),
                        member_name,
                    })),
                    EnumInconsistentMemberValues(box EnumInconsistentMemberValuesData {
                        loc,
                        enum_,
                    }) => {
                        EnumInconsistentMemberValues(Box::new(EnumInconsistentMemberValuesData {
                            loc: f(loc),
                            enum_: map_enum_ref(enum_),
                        }))
                    }
                    EnumInvalidMemberInitializer(box EnumInvalidMemberInitializerData {
                        loc,
                        enum_,
                        explicit_type,
                        member_name,
                    }) => {
                        EnumInvalidMemberInitializer(Box::new(EnumInvalidMemberInitializerData {
                            loc: f(loc),
                            enum_: map_enum_ref(enum_),
                            explicit_type,
                            member_name,
                        }))
                    }
                    EnumBooleanMemberNotInitialized(box EnumBooleanMemberNotInitializedData {
                        loc,
                        enum_,
                        member_name,
                    }) => EnumBooleanMemberNotInitialized(Box::new(
                        EnumBooleanMemberNotInitializedData {
                            loc: f(loc),
                            enum_: map_enum_ref(enum_),
                            member_name,
                        },
                    )),
                    EnumNumberMemberNotInitialized(box EnumNumberMemberNotInitializedData {
                        loc,
                        enum_,
                        member_name,
                    }) => EnumNumberMemberNotInitialized(Box::new(
                        EnumNumberMemberNotInitializedData {
                            loc: f(loc),
                            enum_: map_enum_ref(enum_),
                            member_name,
                        },
                    )),
                    EnumBigIntMemberNotInitialized(box EnumBigIntMemberNotInitializedData {
                        loc,
                        enum_,
                        member_name,
                    }) => EnumBigIntMemberNotInitialized(Box::new(
                        EnumBigIntMemberNotInitializedData {
                            loc: f(loc),
                            enum_: map_enum_ref(enum_),
                            member_name,
                        },
                    )),
                    EnumStringMemberInconsistentlyInitialized(
                        box EnumStringMemberInconsistentlyInitializedData { loc, enum_ },
                    ) => EnumStringMemberInconsistentlyInitialized(Box::new(
                        EnumStringMemberInconsistentlyInitializedData {
                            loc: f(loc),
                            enum_: map_enum_ref(enum_),
                        },
                    )),
                    TSEnumInvalidMember(box TSEnumInvalidMemberData {
                        loc,
                        enum_,
                        member_name,
                        kind,
                    }) => TSEnumInvalidMember(Box::new(TSEnumInvalidMemberData {
                        loc: f(loc),
                        enum_: map_enum_ref(enum_),
                        member_name,
                        kind,
                    })),
                    TSEnumInvalidSyntax(box TSEnumInvalidSyntaxData { loc, enum_, kind }) => {
                        TSEnumInvalidSyntax(Box::new(TSEnumInvalidSyntaxData {
                            loc: f(loc),
                            enum_: map_enum_ref(enum_),
                            kind,
                        }))
                    }
                })
            }
            EIndeterminateModuleType(loc) => EIndeterminateModuleType(f(loc)),
            EBadExportPosition(loc) => EBadExportPosition(f(loc)),
            EBadExportContext(box (s, loc)) => EBadExportContext(Box::new((s, f(loc)))),

            EBadDefaultImportAccess(box (loc, import_star_loc)) => {
                EBadDefaultImportAccess(Box::new((f(loc), f(import_star_loc))))
            }
            EBadDefaultImportDestructuring(loc) => EBadDefaultImportDestructuring(f(loc)),
            EInvalidImportStarUse(box (loc, import_star_loc)) => {
                EInvalidImportStarUse(Box::new((f(loc), f(import_star_loc))))
            }
            ENonConstVarExport(box (loc, declaration)) => ENonConstVarExport(Box::new((
                f(loc),
                declaration.map(|(declaration_loc, name)| (f(declaration_loc), name)),
            ))),
            EThisInExportedFunction(loc) => EThisInExportedFunction(f(loc)),
            EMixedImportAndRequire(box (require_loc, import_loc)) => {
                EMixedImportAndRequire(Box::new((f(require_loc), f(import_loc))))
            }
            EUnsupportedVarianceAnnotation(box (loc, k)) => {
                EUnsupportedVarianceAnnotation(Box::new((f(loc), k)))
            }

            EExportRenamedDefault(box EExportRenamedDefaultData {
                loc,
                name,
                is_reexport,
            }) => EExportRenamedDefault(Box::new(EExportRenamedDefaultData {
                loc: f(loc),
                name,
                is_reexport,
            })),

            EUnreachable(loc) => EUnreachable(f(loc)),
            EInvalidTypeof(box (loc, s)) => EInvalidTypeof(Box::new((f(loc), s))),
            EBinaryInLHS(box EBinaryInLHSData {
                loc,
                lhs_loc,
                lhs_desc,
            }) => EBinaryInLHS(Box::new(EBinaryInLHSData {
                loc: f(loc),
                lhs_loc: f(lhs_loc),
                lhs_desc: type_or_type_desc::map_loc(|l: &L| f(l.dupe()), lhs_desc),
            })),
            EBinaryInRHS(box EBinaryInRHSData {
                loc,
                rhs_loc,
                rhs_desc,
            }) => EBinaryInRHS(Box::new(EBinaryInRHSData {
                loc: f(loc),
                rhs_loc: f(rhs_loc),
                rhs_desc: type_or_type_desc::map_loc(|l: &L| f(l.dupe()), rhs_desc),
            })),
            EArithmeticOperand(box EArithmeticOperandData { loc, operand }) => {
                EArithmeticOperand(Box::new(EArithmeticOperandData {
                    loc: f(loc),
                    operand: map_error_type_ref(operand),
                }))
            }
            EForInRHS(box EForInRHSData {
                loc,
                rhs_loc,
                rhs_desc,
            }) => EForInRHS(Box::new(EForInRHSData {
                loc: f(loc),
                rhs_loc: f(rhs_loc),
                rhs_desc: type_or_type_desc::map_loc(|l: &L| f(l.dupe()), rhs_desc),
            })),
            EInstanceofRHS(box EInstanceofRHSData {
                loc,
                rhs_loc,
                rhs_desc,
            }) => EInstanceofRHS(Box::new(EInstanceofRHSData {
                loc: f(loc),
                rhs_loc: f(rhs_loc),
                rhs_desc: type_or_type_desc::map_loc(|l: &L| f(l.dupe()), rhs_desc),
            })),

            EObjectComputedPropertyAccess(box EObjectComputedPropertyAccessData {
                object,
                property,
                kind,
            }) => EObjectComputedPropertyAccess(Box::new(EObjectComputedPropertyAccessData {
                object: map_error_type_ref_with_reason(object),
                property: map_error_type_ref_with_reason(property),
                kind,
            })),

            EObjectComputedPropertyAssign(box (r1, r2, kind)) => {
                EObjectComputedPropertyAssign(Box::new((
                    map_error_type_ref_with_reason(r1),
                    r2.map(|expression| ExpressionReferenceData {
                        loc: f(expression.loc),
                        kind: expression.kind,
                    }),
                    kind,
                )))
            }

            EObjectComputedPropertyPotentialOverwrite(
                box EObjectComputedPropertyPotentialOverwriteData {
                    key_loc,
                    overwritten_locs,
                },
            ) => EObjectComputedPropertyPotentialOverwrite(Box::new(
                EObjectComputedPropertyPotentialOverwriteData {
                    key_loc: f(key_loc),
                    overwritten_locs: overwritten_locs.into_iter().map(&f).collect(),
                },
            )),

            EInvalidLHSInAssignment(l) => EInvalidLHSInAssignment(f(l)),
            EUnsupportedImplements(type_) => EUnsupportedImplements(map_error_type_ref(type_)),

            EReactRefInRender {
                usage,
                kind,
                in_hook,
            } => EReactRefInRender {
                usage: ExpressionReferenceData {
                    loc: f(usage.loc),
                    kind: usage.kind,
                },
                kind,
                in_hook,
            },

            EUnsupportedSetProto(loc) => EUnsupportedSetProto(f(loc)),

            EDuplicateModuleProvider(box EDuplicateModuleProviderData {
                module_name,
                provider,
                conflict,
            }) => EDuplicateModuleProvider(Box::new(EDuplicateModuleProviderData {
                module_name,
                provider: f(provider),
                conflict: f(conflict),
            })),

            EParseError(box (loc, p)) => EParseError(Box::new((f(loc), p))),
            EDocblockError(box (loc, e)) => EDocblockError(Box::new((f(loc), e))),
            EAmbiguousObjectType(loc) => EAmbiguousObjectType(f(loc)),
            EUntypedTypeImport(box (loc, s)) => EUntypedTypeImport(Box::new((f(loc), s))),
            EUntypedImport(box (loc, s)) => EUntypedImport(Box::new((f(loc), s))),
            ENonstrictImport(loc) => ENonstrictImport(f(loc)),
            EUnclearType(loc) => EUnclearType(f(loc)),
            EDeprecatedBool(loc) => EDeprecatedBool(f(loc)),
            EInternalType(loc, kind) => EInternalType(f(loc), kind),

            EIncorrectTypeWithReplacement(box EIncorrectTypeWithReplacementData { loc, kind }) => {
                EIncorrectTypeWithReplacement(Box::new(EIncorrectTypeWithReplacementData {
                    loc: f(loc),
                    kind,
                }))
            }

            EUnsafeGettersSetters(loc) => EUnsafeGettersSetters(f(loc)),
            EUnsafeObjectAssign(loc) => EUnsafeObjectAssign(f(loc)),
            EUnusedSuppression(loc) => EUnusedSuppression(f(loc)),
            ECodelessSuppression(loc) => ECodelessSuppression(f(loc)),
            ELintSetting(box (loc, err)) => ELintSetting(Box::new((f(loc), err))),

            ESketchyNullLint(box ESketchyNullLintData {
                kind,
                loc,
                null_loc,
                falsy_loc,
            }) => ESketchyNullLint(Box::new(ESketchyNullLintData {
                kind,
                loc: f(loc),
                null_loc: f(null_loc),
                falsy_loc: f(falsy_loc),
            })),

            ESketchyNumberLint(kind, value) => {
                ESketchyNumberLint(kind, map_error_type_ref_with_reason(value))
            }
            EInvalidPrototype(box EInvalidPrototypeData {
                loc,
                prototype_loc,
                prototype_desc,
            }) => EInvalidPrototype(Box::new(EInvalidPrototypeData {
                loc: f(loc),
                prototype_loc: f(prototype_loc),
                prototype_desc: type_or_type_desc::map_loc(|l: &L| f(l.dupe()), prototype_desc),
            })),
            EUnnecessaryOptionalChain(box (loc, lhs_expression)) => {
                EUnnecessaryOptionalChain(Box::new((
                    f(loc),
                    ExpressionReferenceData {
                        loc: f(lhs_expression.loc),
                        kind: lhs_expression.kind,
                    },
                )))
            }
            EUnnecessaryInvariant(box EUnnecessaryInvariantData {
                loc,
                callee,
                condition,
                condition_kind,
            }) => EUnnecessaryInvariant(Box::new(EUnnecessaryInvariantData {
                loc: f(loc),
                callee,
                condition: map_error_type_ref(condition),
                condition_kind,
            })),
            EUnnecessaryDeclareTypeOnlyExport(loc) => EUnnecessaryDeclareTypeOnlyExport(f(loc)),
            ECannotDelete(box ECannotDeleteData { loc, expression }) => {
                ECannotDelete(Box::new(ECannotDeleteData {
                    loc: f(loc),
                    expression: map_error_type_ref(expression),
                }))
            }

            ESignatureBindingValidation(sve) => {
                ESignatureBindingValidation(map_binding_validation(&f, sve))
            }

            ESignatureVerification(sve) => ESignatureVerification(map_signature_error(&f, sve)),

            EPrimitiveAsInterface(box EPrimitiveAsInterfaceData {
                use_op,
                lower,
                upper,
                kind,
            }) => EPrimitiveAsInterface(Box::new(EPrimitiveAsInterfaceData {
                use_op: map_use_op(use_op),
                lower: map_error_type_ref_with_reason(lower),
                upper: map_error_type_ref_with_reason(upper),
                kind,
            })),

            ECannotSpreadInterface(box ECannotSpreadInterfaceData {
                spread_loc,
                interface,
                use_op,
            }) => ECannotSpreadInterface(Box::new(ECannotSpreadInterfaceData {
                spread_loc: f(spread_loc),
                interface: map_error_type_ref_with_reason(interface),
                use_op: map_use_op(use_op),
            })),

            ECannotSpreadIndexerOnRight(box ECannotSpreadIndexerOnRightData {
                spread_loc,
                object,
                key_loc,
                key_desc,
                use_op,
            }) => ECannotSpreadIndexerOnRight(Box::new(ECannotSpreadIndexerOnRightData {
                spread_loc: f(spread_loc),
                object: map_error_type_ref_with_reason(object),
                key_loc: f(key_loc),
                key_desc: type_or_type_desc::map_loc(|l: &L| f(l.dupe()), key_desc),
                use_op: map_use_op(use_op),
            })),

            EUnableToSpread(box EUnableToSpreadData {
                spread_loc,
                object1,
                object2,
                propname,
                error_kind,
                use_op,
            }) => EUnableToSpread(Box::new(EUnableToSpreadData {
                spread_loc: f(spread_loc),
                object1: map_error_type_ref_with_reason(object1),
                object2: map_error_type_ref_with_reason(object2),
                propname,
                error_kind,
                use_op: map_use_op(use_op),
            })),

            EInexactMayOverwriteIndexer(box EInexactMayOverwriteIndexerData {
                spread_loc,
                key_loc,
                key_desc,
                value_loc,
                value_desc,
                object2,
                use_op,
            }) => EInexactMayOverwriteIndexer(Box::new(EInexactMayOverwriteIndexerData {
                spread_loc: f(spread_loc),
                key_loc: f(key_loc),
                key_desc: type_or_type_desc::map_loc(|l: &L| f(l.dupe()), key_desc),
                value_loc: f(value_loc),
                value_desc: type_or_type_desc::map_loc(|l: &L| f(l.dupe()), value_desc),
                object2: map_error_type_ref_with_reason(object2),
                use_op: map_use_op(use_op),
            })),

            EExponentialSpread(box EExponentialSpreadData {
                spread_loc,
                operand1,
                operand2,
            }) => EExponentialSpread(Box::new(EExponentialSpreadData {
                spread_loc: f(spread_loc),
                operand1: map_exponential_spread_operand_group(
                    map_error_type_ref_with_reason,
                    operand1,
                ),
                operand2: map_exponential_spread_operand_group(
                    map_error_type_ref_with_reason,
                    operand2,
                ),
            })),

            EAssignConstLikeBinding(box EAssignConstLikeBindingData {
                loc,
                definition_loc,
                name,
                binding_kind,
            }) => EAssignConstLikeBinding(Box::new(EAssignConstLikeBindingData {
                loc: f(loc),
                definition_loc: f(definition_loc),
                name,
                binding_kind,
            })),

            EMalformedCode(loc) => EMalformedCode(f(loc)),

            EImplicitInstantiationUnderconstrainedError(
                box EImplicitInstantiationUnderconstrainedErrorData {
                    call,
                    type_param_loc,
                    type_param_name,
                    use_op,
                },
            ) => EImplicitInstantiationUnderconstrainedError(Box::new(
                EImplicitInstantiationUnderconstrainedErrorData {
                    call: ImplicitInstantiationReferenceData {
                        loc: f(call.loc),
                        kind: call.kind,
                    },
                    type_param_loc: f(type_param_loc),
                    type_param_name,
                    use_op: map_use_op(use_op),
                },
            )),

            EClassToObject(box EClassToObjectData {
                lower,
                upper,
                use_op,
                kind,
            }) => EClassToObject(Box::new(EClassToObjectData {
                lower: map_error_type_ref_with_reason(lower),
                upper: map_error_type_ref_with_reason(upper),
                use_op: map_use_op(use_op),
                kind,
            })),

            EHookIncompatible(box EHookIncompatibleData {
                use_op,
                lower,
                upper,
                lower_is_hook,
                hook_is_annot,
            }) => EHookIncompatible(Box::new(EHookIncompatibleData {
                use_op: map_use_op(use_op),
                lower: map_error_type_ref_with_reason(lower),
                upper: map_error_type_ref_with_reason(upper),
                lower_is_hook,
                hook_is_annot,
            })),

            EHookUniqueIncompatible(box EHookUniqueIncompatibleData {
                use_op,
                lower,
                upper,
            }) => EHookUniqueIncompatible(Box::new(EHookUniqueIncompatibleData {
                use_op: map_use_op(use_op),
                lower: map_error_type_ref_with_reason(lower),
                upper: map_error_type_ref_with_reason(upper),
            })),

            EHookRuleViolation(box EHookRuleViolationData {
                callee_loc,
                call_loc,
                hook_rule,
            }) => {
                let hook_rule = match hook_rule {
                    HookRule::MaybeHook { hooks, non_hooks } => HookRule::MaybeHook {
                        hooks: hooks.into_iter().map(&f).collect(),
                        non_hooks: non_hooks.into_iter().map(&f).collect(),
                    },
                    HookRule::HookHasIllegalName => HookRule::HookHasIllegalName,
                    HookRule::NotHookSyntaxHook => HookRule::NotHookSyntaxHook,
                    HookRule::HookDefinitelyNotInComponentOrHook => {
                        HookRule::HookDefinitelyNotInComponentOrHook
                    }
                    HookRule::HookInUnknownContext => HookRule::HookInUnknownContext,
                    HookRule::HookNotInComponentSyntaxComponentOrHookSyntaxHook => {
                        HookRule::HookNotInComponentSyntaxComponentOrHookSyntaxHook
                    }
                    HookRule::ConditionalHook => HookRule::ConditionalHook,
                };
                EHookRuleViolation(Box::new(EHookRuleViolationData {
                    callee_loc: f(callee_loc),
                    call_loc: f(call_loc),
                    hook_rule,
                }))
            }

            EHookNaming(l) => EHookNaming(f(l)),

            EObjectThisSuperReference(box EObjectThisSuperReferenceData {
                loc,
                method_loc,
                method_name,
                kind,
            }) => EObjectThisSuperReference(Box::new(EObjectThisSuperReferenceData {
                loc: f(loc),
                method_loc: f(method_loc),
                method_name,
                kind,
            })),

            EComponentThisReference(box EComponentThisReferenceData {
                component_loc,
                this_loc,
            }) => EComponentThisReference(Box::new(EComponentThisReferenceData {
                component_loc: f(component_loc),
                this_loc: f(this_loc),
            })),

            EComponentCase(loc) => EComponentCase(f(loc)),
            EDeclareComponentInvalidParam { loc, kind } => {
                EDeclareComponentInvalidParam { loc: f(loc), kind }
            }
            EComponentMissingReturn(ComponentReferenceData { loc, name }) => {
                EComponentMissingReturn(ComponentReferenceData { loc: f(loc), name })
            }
            EComponentMissingBody(loc) => EComponentMissingBody(f(loc)),
            EComponentBodyInAmbientContext(loc) => EComponentBodyInAmbientContext(f(loc)),
            ENestedComponent(loc) => ENestedComponent(f(loc)),
            ENestedHook(loc) => ENestedHook(f(loc)),

            EInvalidDeclaration(box EInvalidDeclarationData {
                declaration_loc,
                name,
                null_write,
                possible_generic_escape_locs,
            }) => EInvalidDeclaration(Box::new(EInvalidDeclarationData {
                declaration_loc: f(declaration_loc),
                name,
                null_write: null_write.map(|nw| NullWrite {
                    null_loc: f(nw.null_loc),
                    initialized: nw.initialized,
                }),
                possible_generic_escape_locs: possible_generic_escape_locs
                    .into_iter()
                    .map(&f)
                    .collect(),
            })),

            EInvalidGraphQL(box (loc, err)) => EInvalidGraphQL(Box::new((f(loc), err))),

            EAnnotationInference(box EAnnotationInferenceData {
                loc,
                operation,
                operation_loc,
                target_loc,
                target_desc,
            }) => EAnnotationInference(Box::new(EAnnotationInferenceData {
                loc: f(loc),
                operation,
                operation_loc: f(operation_loc),
                target_loc: f(target_loc),
                target_desc: target_desc.map_locs(|l: &L| f(l.dupe())),
            })),

            ETrivialRecursiveDefinition(loc) => ETrivialRecursiveDefinition(f(loc)),
            ETrivialRecursiveTypeParameter(box (loc, name)) => {
                ETrivialRecursiveTypeParameter(Box::new((f(loc), name)))
            }
            EClassTypeParamInStatic(box (loc, name)) => {
                EClassTypeParamInStatic(Box::new((f(loc), name)))
            }

            EDefinitionCycle(elts) => EDefinitionCycle(
                Vec1::try_from_vec(
                    elts.into_iter()
                        .map(|(definition, recur, annot)| {
                            (
                                DefinitionReferenceData {
                                    loc: f(definition.loc),
                                    kind: definition.kind,
                                },
                                recur.into_iter().map(&f).collect(),
                                annot
                                    .into_iter()
                                    .map(|a| match a {
                                        AnnotLoc::Loc(l) => AnnotLoc::Loc(f(l)),
                                        AnnotLoc::Object { loc, props } => AnnotLoc::Object {
                                            loc: f(loc),
                                            props: props.into_iter().map(&f).collect(),
                                        },
                                    })
                                    .collect(),
                            )
                        })
                        .collect(),
                )
                .unwrap(),
            ),

            ERecursiveDefinition(box ERecursiveDefinitionData {
                definition,
                recursion,
                annot_locs,
            }) => ERecursiveDefinition(Box::new(ERecursiveDefinitionData {
                definition: DefinitionReferenceData {
                    loc: f(definition.loc),
                    kind: definition.kind,
                },
                recursion: recursion.into_iter().map(&f).collect(),
                annot_locs: annot_locs
                    .into_iter()
                    .map(|a| match a {
                        AnnotLoc::Loc(l) => AnnotLoc::Loc(f(l)),
                        AnnotLoc::Object { loc, props } => AnnotLoc::Object {
                            loc: f(loc),
                            props: props.into_iter().map(&f).collect(),
                        },
                    })
                    .collect(),
            })),

            EReferenceInAnnotation(box (bind_loc, name, loc)) => {
                EReferenceInAnnotation(Box::new((f(bind_loc), name, f(loc))))
            }
            EReferenceInDefault(box (bind_loc, name, loc)) => {
                EReferenceInDefault(Box::new((f(bind_loc), name, f(loc))))
            }

            EDuplicateClassMember(box EDuplicateClassMemberData {
                loc,
                name,
                is_static,
                class_kind,
            }) => EDuplicateClassMember(Box::new(EDuplicateClassMemberData {
                loc: f(loc),
                name,
                is_static,
                class_kind,
            })),

            EEmptyArrayNoProvider { loc } => EEmptyArrayNoProvider { loc: f(loc) },
            EUnusedPromise { loc, async_ } => EUnusedPromise {
                loc: f(loc),
                async_,
            },

            EReactIntrinsicOverlap(box EReactIntrinsicOverlapData {
                use_,
                def,
                type_,
                mixed,
            }) => EReactIntrinsicOverlap(Box::new(EReactIntrinsicOverlapData {
                use_: NamedReferenceData {
                    loc: f(use_.loc),
                    name: use_.name,
                },
                def: f(def),
                type_: f(type_),
                mixed,
            })),

            EInvalidComponentRestParam(loc) => EInvalidComponentRestParam(f(loc)),
            EBigIntRShift3(box operand) => EBigIntRShift3(Box::new(map_error_type_ref(operand))),
            EBigIntNumCoerce(box operand) => {
                EBigIntNumCoerce(Box::new(map_error_type_ref(operand)))
            }

            EInvalidCatchParameterAnnotation {
                loc,
                ts_utility_syntax,
            } => EInvalidCatchParameterAnnotation {
                loc: f(loc),
                ts_utility_syntax,
            },

            ETSSyntax(box ETSSyntaxData { kind, loc }) => {
                ETSSyntax(Box::new(ETSSyntaxData { kind, loc: f(loc) }))
            }

            EAbstractClass(box EAbstractClassData { kind, loc }) => {
                let kind = match kind {
                    AbstractErrorKind::AbstractClassInstantiation => {
                        AbstractErrorKind::AbstractClassInstantiation
                    }
                    AbstractErrorKind::AbstractMemberNotImplemented {
                        class_name,
                        member_name,
                        member_def_loc,
                    } => AbstractErrorKind::AbstractMemberNotImplemented {
                        class_name,
                        member_name,
                        member_def_loc: f(member_def_loc),
                    },
                    AbstractErrorKind::AbstractMemberOnNonAbstractClass { member_name } => {
                        AbstractErrorKind::AbstractMemberOnNonAbstractClass { member_name }
                    }
                    AbstractErrorKind::AbstractPrivateMember { member_name } => {
                        AbstractErrorKind::AbstractPrivateMember { member_name }
                    }
                    AbstractErrorKind::AbstractSuperCall { member_name } => {
                        AbstractErrorKind::AbstractSuperCall { member_name }
                    }
                    AbstractErrorKind::AbstractConstructorAssignedToNonAbstract => {
                        AbstractErrorKind::AbstractConstructorAssignedToNonAbstract
                    }
                };
                EAbstractClass(Box::new(EAbstractClassData { kind, loc: f(loc) }))
            }

            EOverride(box EOverrideData { kind, loc }) => {
                let kind = match kind {
                    OverrideErrorKind::OverrideWithoutExtends {
                        class_name,
                        member_name,
                    } => OverrideErrorKind::OverrideWithoutExtends {
                        class_name,
                        member_name,
                    },
                    OverrideErrorKind::OverrideOfNonInheritedMember {
                        class_name,
                        base_class_name,
                        member_name,
                    } => OverrideErrorKind::OverrideOfNonInheritedMember {
                        class_name,
                        base_class_name,
                        member_name,
                    },
                    OverrideErrorKind::ImplicitOverrideMissingModifier {
                        class_name,
                        base_class_name,
                        member_name,
                        inherited_def_loc,
                    } => OverrideErrorKind::ImplicitOverrideMissingModifier {
                        class_name,
                        base_class_name,
                        member_name,
                        inherited_def_loc: f(inherited_def_loc),
                    },
                };
                EOverride(Box::new(EOverrideData { kind, loc: f(loc) }))
            }

            EVarianceKeyword(box EVarianceKeywordData { kind, loc }) => {
                EVarianceKeyword(Box::new(EVarianceKeywordData { kind, loc: f(loc) }))
            }

            EInvalidBinaryArith(box EInvalidBinaryArithData {
                loc,
                left,
                right,
                kind,
            }) => EInvalidBinaryArith(Box::new(EInvalidBinaryArithData {
                loc: f(loc),
                left: map_error_type_ref(left),
                right: map_error_type_ref(right),
                kind,
            })),

            EInvalidMappedType { loc, kind } => EInvalidMappedType { loc: f(loc), kind },
            EInvalidTemplateLiteralType { loc, kind } => {
                EInvalidTemplateLiteralType { loc: f(loc), kind }
            }

            EDuplicateComponentProp(box EDuplicateComponentPropData { spread, duplicates }) => {
                EDuplicateComponentProp(Box::new(EDuplicateComponentPropData {
                    spread: f(spread),
                    duplicates: Vec1::try_from_vec(
                        duplicates
                            .into_iter()
                            .map(|(first, name, second)| (f(first), name, f(second)))
                            .collect(),
                    )
                    .unwrap(),
                }))
            }

            EKeySpreadProp(box EKeySpreadPropData { spread, loc }) => {
                EKeySpreadProp(Box::new(EKeySpreadPropData {
                    spread: f(spread),
                    loc: f(loc),
                }))
            }

            EInvalidRendersTypeArgument(box EInvalidRendersTypeArgumentData {
                loc,
                renders_variant,
                invalid_render_type_kind,
                invalid_types,
            }) => EInvalidRendersTypeArgument(Box::new(EInvalidRendersTypeArgumentData {
                loc: f(loc),
                renders_variant,
                invalid_render_type_kind: map_invalid_render_type_kind(
                    map_error_type_ref,
                    invalid_render_type_kind,
                ),
                invalid_types: Vec1::try_from_vec(
                    invalid_types.into_iter().map(map_error_type_ref).collect(),
                )
                .unwrap(),
            })),

            EInvalidTypeCastSyntax { loc } => EInvalidTypeCastSyntax { loc: f(loc) },

            EMissingPlatformSupportWithAvailablePlatforms(
                box EMissingPlatformSupportWithAvailablePlatformsData {
                    loc,
                    available_platforms,
                    required_platforms,
                },
            ) => EMissingPlatformSupportWithAvailablePlatforms(Box::new(
                EMissingPlatformSupportWithAvailablePlatformsData {
                    loc: f(loc),
                    available_platforms,
                    required_platforms,
                },
            )),

            EUnionPartialOptimizationNonUniqueKey(
                box EUnionPartialOptimizationNonUniqueKeyData {
                    loc,
                    non_unique_keys,
                },
            ) => EUnionPartialOptimizationNonUniqueKey(Box::new(
                EUnionPartialOptimizationNonUniqueKeyData {
                    loc: f(loc),
                    non_unique_keys: non_unique_keys
                        .into_iter()
                        .map(|(name, inner_map)| {
                            (
                                name,
                                inner_map
                                    .into_iter()
                                    .map(|(enum_key, reasons)| {
                                        (enum_key, reasons.mapped(map_error_type_ref_with_reason))
                                    })
                                    .collect(),
                            )
                        })
                        .collect(),
                },
            )),

            EUnionOptimization(box EUnionOptimizationData { loc, kind }) => {
                let kind = match kind {
                    OptimizedError::ContainsUnresolved {
                        loc: member_loc,
                        desc,
                    } => OptimizedError::ContainsUnresolved {
                        loc: f(member_loc),
                        desc: type_or_type_desc::map_loc(|l: &L| f(l.dupe()), desc),
                    },
                    OptimizedError::NoCandidateMembers => OptimizedError::NoCandidateMembers,
                    OptimizedError::NoCommonKeys => OptimizedError::NoCommonKeys,
                };
                EUnionOptimization(Box::new(EUnionOptimizationData { loc: f(loc), kind }))
            }

            EUnionOptimizationOnNonUnion(box EUnionOptimizationOnNonUnionData {
                loc,
                arg_loc,
                arg_desc,
            }) => EUnionOptimizationOnNonUnion(Box::new(EUnionOptimizationOnNonUnionData {
                loc: f(loc),
                arg_loc: f(arg_loc),
                arg_desc: type_or_type_desc::map_loc(|l: &L| f(l.dupe()), arg_desc),
            })),

            ECannotCallReactComponent { component } => ECannotCallReactComponent {
                component: map_error_type_ref_with_reason(component),
            },

            EDevOnlyRefinedLocInfo(box EDevOnlyRefinedLocInfoData {
                refined_loc,
                refining_locs,
            }) => EDevOnlyRefinedLocInfo(Box::new(EDevOnlyRefinedLocInfoData {
                refined_loc: f(refined_loc),
                refining_locs: refining_locs.into_iter().map(&f).collect(),
            })),

            EMatchError(match_error) => {
                use MatchErrorKind::*;
                EMatchError(match match_error {
                    MatchNotExhaustive(box MatchNotExhaustiveData {
                        loc,
                        examples,
                        missing_pattern_asts,
                    }) => MatchNotExhaustive(Box::new(MatchNotExhaustiveData {
                        loc: f(loc),
                        examples: examples
                            .into_iter()
                            .map(|(pattern, references)| {
                                (
                                    pattern,
                                    references
                                        .into_iter()
                                        .map(|reference| match reference {
                                            MatchExampleReference::Type(reference) => {
                                                MatchExampleReference::Type(
                                                    map_error_type_ref_with_reason(reference),
                                                )
                                            }
                                            MatchExampleReference::EnumMember {
                                                loc,
                                                member_name,
                                                enum_name,
                                            } => MatchExampleReference::EnumMember {
                                                loc: f(loc),
                                                member_name,
                                                enum_name,
                                            },
                                            MatchExampleReference::EnumUnknownMembers {
                                                loc,
                                                enum_name,
                                            } => MatchExampleReference::EnumUnknownMembers {
                                                loc: f(loc),
                                                enum_name,
                                            },
                                        })
                                        .collect(),
                                )
                            })
                            .collect(),
                        missing_pattern_asts,
                    })),
                    MatchUnusedPattern(box MatchUnusedPatternData {
                        pattern,
                        already_seen,
                    }) => MatchUnusedPattern(Box::new(MatchUnusedPatternData {
                        pattern: MatchPatternReferenceData {
                            loc: f(pattern.loc),
                            kind: pattern.kind,
                        },
                        already_seen: already_seen.map(|already_seen| MatchPatternReferenceData {
                            loc: f(already_seen.loc),
                            kind: already_seen.kind,
                        }),
                    })),
                    MatchNonExhaustiveObjectPattern(box MatchNonExhaustiveObjectPatternData {
                        loc,
                        rest,
                        missing_props,
                        pattern_kind,
                    }) => MatchNonExhaustiveObjectPattern(Box::new(
                        MatchNonExhaustiveObjectPatternData {
                            loc: f(loc),
                            rest: rest.map(map_error_type_ref),
                            missing_props,
                            pattern_kind,
                        },
                    )),
                    MatchNonExplicitEnumCheck(box MatchNonExplicitEnumCheckData {
                        loc,
                        wildcard_loc,
                        unchecked_members,
                    }) => MatchNonExplicitEnumCheck(Box::new(MatchNonExplicitEnumCheckData {
                        loc: f(loc),
                        wildcard_loc: f(wildcard_loc),
                        unchecked_members,
                    })),
                    MatchInvalidGuardedWildcard(loc) => MatchInvalidGuardedWildcard(f(loc)),
                    MatchInvalidIdentOrMemberPattern(
                        box MatchInvalidIdentOrMemberPatternData {
                            loc,
                            type_loc,
                            type_desc,
                        },
                    ) => MatchInvalidIdentOrMemberPattern(Box::new(
                        MatchInvalidIdentOrMemberPatternData {
                            loc: f(loc),
                            type_loc: f(type_loc),
                            type_desc: type_or_type_desc::map_loc(|l: &L| f(l.dupe()), type_desc),
                        },
                    )),
                    MatchInvalidBindingKind { loc, kind } => {
                        MatchInvalidBindingKind { loc: f(loc), kind }
                    }
                    MatchInvalidObjectPropertyLiteral { loc, pattern_kind } => {
                        MatchInvalidObjectPropertyLiteral {
                            loc: f(loc),
                            pattern_kind,
                        }
                    }
                    MatchInvalidUnaryZero { loc } => MatchInvalidUnaryZero { loc: f(loc) },
                    MatchInvalidUnaryPlusBigInt { loc } => {
                        MatchInvalidUnaryPlusBigInt { loc: f(loc) }
                    }
                    MatchDuplicateObjectProperty(box MatchDuplicateObjectPropertyData {
                        loc,
                        name,
                        pattern_kind,
                    }) => {
                        MatchDuplicateObjectProperty(Box::new(MatchDuplicateObjectPropertyData {
                            loc: f(loc),
                            name,
                            pattern_kind,
                        }))
                    }
                    MatchBindingInOrPattern { loc } => MatchBindingInOrPattern { loc: f(loc) },
                    MatchInvalidAsPattern { loc } => MatchInvalidAsPattern { loc: f(loc) },
                    MatchInvalidPatternReference(box MatchInvalidPatternReferenceData {
                        loc,
                        binding_loc,
                        name,
                    }) => {
                        MatchInvalidPatternReference(Box::new(MatchInvalidPatternReferenceData {
                            loc: f(loc),
                            binding_loc: f(binding_loc),
                            name,
                        }))
                    }
                    MatchInvalidObjectShorthand(box MatchInvalidObjectShorthandData {
                        loc,
                        name,
                        pattern_kind,
                    }) => MatchInvalidObjectShorthand(Box::new(MatchInvalidObjectShorthandData {
                        loc: f(loc),
                        name,
                        pattern_kind,
                    })),
                    MatchStatementInvalidBody { loc } => MatchStatementInvalidBody { loc: f(loc) },
                    MatchInvalidCaseSyntax(box MatchInvalidCaseSyntaxData { loc, kind }) => {
                        use crate::intermediate_error_types::MatchInvalidCaseSyntax as MICS;
                        let kind = match kind {
                            MICS::InvalidMatchCaseMultiple {
                                invalid_prefix_case_locs,
                                invalid_infix_colon_locs,
                                invalid_suffix_semicolon_locs,
                            } => MICS::InvalidMatchCaseMultiple {
                                invalid_prefix_case_locs: invalid_prefix_case_locs
                                    .into_iter()
                                    .map(&f)
                                    .collect(),
                                invalid_infix_colon_locs: invalid_infix_colon_locs
                                    .into_iter()
                                    .map(&f)
                                    .collect(),
                                invalid_suffix_semicolon_locs: invalid_suffix_semicolon_locs
                                    .into_iter()
                                    .map(&f)
                                    .collect(),
                            },
                            MICS::InvalidMatchCasePrefixCase => MICS::InvalidMatchCasePrefixCase,
                            MICS::InvalidMatchCaseInfixColon => MICS::InvalidMatchCaseInfixColon,
                            MICS::InvalidMatchCaseSuffixSemicolon => {
                                MICS::InvalidMatchCaseSuffixSemicolon
                            }
                        };
                        MatchInvalidCaseSyntax(Box::new(MatchInvalidCaseSyntaxData {
                            loc: f(loc),
                            kind,
                        }))
                    }
                    MatchInvalidWildcardSyntax(loc) => MatchInvalidWildcardSyntax(f(loc)),
                    MatchInvalidInstancePattern(loc) => MatchInvalidInstancePattern(f(loc)),
                })
            }

            ERecordError(record_error) => ERecordError(match record_error {
                RecordErrorKind::RecordBannedTypeUtil { op_loc, op, record } => {
                    RecordErrorKind::RecordBannedTypeUtil {
                        op_loc: f(op_loc),
                        op,
                        record: map_error_type_ref_with_reason(record),
                    }
                }
                RecordErrorKind::RecordInvalidName { loc, name } => {
                    RecordErrorKind::RecordInvalidName { loc: f(loc), name }
                }
                RecordErrorKind::RecordInvalidNew { loc, record_name } => {
                    RecordErrorKind::RecordInvalidNew {
                        loc: f(loc),
                        record_name,
                    }
                }
                RecordErrorKind::RecordDeclarationInvalidSyntax { loc, kind } => {
                    let kind = match kind {
                            RecordDeclarationInvalidSyntax::InvalidRecordDeclarationSyntaxMultiple {
                                invalid_infix_equals_loc,
                                invalid_variance_locs,
                                invalid_optional_locs,
                                invalid_suffix_semicolon_locs,
                            } => RecordDeclarationInvalidSyntax::InvalidRecordDeclarationSyntaxMultiple {
                                invalid_infix_equals_loc: invalid_infix_equals_loc.map(&f),
                                invalid_variance_locs: invalid_variance_locs.into_iter().map(&f).collect(),
                                invalid_optional_locs: invalid_optional_locs.into_iter().map(&f).collect(),
                                invalid_suffix_semicolon_locs: invalid_suffix_semicolon_locs.into_iter().map(&f).collect(),
                            },
                            RecordDeclarationInvalidSyntax::InvalidRecordDeclarationSyntaxVariance => {
                                RecordDeclarationInvalidSyntax::InvalidRecordDeclarationSyntaxVariance
                            },
                            RecordDeclarationInvalidSyntax::InvalidRecordDeclarationSyntaxOptional => {
                                RecordDeclarationInvalidSyntax::InvalidRecordDeclarationSyntaxOptional
                            },
                            RecordDeclarationInvalidSyntax::InvalidRecordDeclarationSyntaxSuffixSemicolon => {
                                RecordDeclarationInvalidSyntax::InvalidRecordDeclarationSyntaxSuffixSemicolon
                            },
                            RecordDeclarationInvalidSyntax::InvalidRecordDeclarationSyntaxInfixEquals => {
                                RecordDeclarationInvalidSyntax::InvalidRecordDeclarationSyntaxInfixEquals
                            },
                        };
                    RecordErrorKind::RecordDeclarationInvalidSyntax { loc: f(loc), kind }
                }
            }),

            EUndocumentedFeature { loc } => EUndocumentedFeature { loc: f(loc) },

            EIllegalAssertOperator(box EIllegalAssertOperatorData {
                op_loc,
                obj,
                specialized,
            }) => EIllegalAssertOperator(Box::new(EIllegalAssertOperatorData {
                op_loc: f(op_loc),
                obj: ExpressionReferenceData {
                    loc: f(obj.loc),
                    kind: obj.kind,
                },
                specialized,
            })),

            EDevOnlyInvalidatedRefinementInfo(box EDevOnlyInvalidatedRefinementInfoData {
                read_loc,
                invalidation_info,
            }) => {
                EDevOnlyInvalidatedRefinementInfo(Box::new(EDevOnlyInvalidatedRefinementInfoData {
                    read_loc: f(read_loc),
                    invalidation_info: invalidation_info
                        .into_iter()
                        .map(|(l, r)| (f(l), r))
                        .collect(),
                }))
            }
        }
    }

    pub fn convert_type_to_type_desc(
        f: &mut dyn FnMut(TypeOrTypeDesc<L>) -> TypeOrTypeDesc<L>,
        msg: ErrorMessage<L>,
    ) -> ErrorMessage<L>
    where
        L: Clone,
    {
        use self::ErrorMessage::*;
        fn map_use_op<L: Dupe + PartialEq + Eq + PartialOrd + Ord>(
            f: &mut dyn FnMut(TypeOrTypeDesc<L>) -> TypeOrTypeDesc<L>,
            use_op: VirtualUseOp<L>,
        ) -> VirtualUseOp<L> {
            match use_op {
                VirtualUseOp::Op(root) => match root.as_ref() {
                    VirtualRootUseOp::ClassExtendsCheck(box ClassExtendsCheckData {
                        def_loc,
                        def,
                        extends_loc,
                        extends,
                    }) => VirtualUseOp::Op(Arc::new(VirtualRootUseOp::ClassExtendsCheck(
                        Box::new(ClassExtendsCheckData {
                            def_loc: def_loc.dupe(),
                            def: f(def.clone()),
                            extends_loc: extends_loc.dupe(),
                            extends: f(extends.clone()),
                        }),
                    ))),
                    VirtualRootUseOp::EvalMappedType(box EvalMappedTypeData {
                        mapped_type_loc,
                        mapped_type,
                    }) => VirtualUseOp::Op(Arc::new(VirtualRootUseOp::EvalMappedType(Box::new(
                        EvalMappedTypeData {
                            mapped_type_loc: mapped_type_loc.dupe(),
                            mapped_type: f(mapped_type.clone()),
                        },
                    )))),
                    VirtualRootUseOp::ComponentRestParamCompatibility(
                        box ComponentRestParamCompatibilityData {
                            rest_param_loc,
                            rest_param,
                        },
                    ) => VirtualUseOp::Op(Arc::new(
                        VirtualRootUseOp::ComponentRestParamCompatibility(Box::new(
                            ComponentRestParamCompatibilityData {
                                rest_param_loc: rest_param_loc.dupe(),
                                rest_param: f(rest_param.clone()),
                            },
                        )),
                    )),
                    VirtualRootUseOp::MergedDeclaration(box MergedDeclarationData {
                        conflict,
                        first_decl_loc,
                        first_decl,
                        current_decl,
                    }) => VirtualUseOp::Op(Arc::new(VirtualRootUseOp::MergedDeclaration(
                        Box::new(MergedDeclarationData {
                            conflict: conflict.dupe(),
                            first_decl_loc: first_decl_loc.dupe(),
                            first_decl: f(first_decl.clone()),
                            current_decl: current_decl.dupe(),
                        }),
                    ))),
                    VirtualRootUseOp::AssignVar { var, init } => {
                        let var = var.as_deref().map(|target| {
                            Box::new(match target {
                                AssignVarTarget::Variable(reason) => {
                                    AssignVarTarget::Variable(reason.dupe())
                                }
                                AssignVarTarget::Annotation(t) => {
                                    AssignVarTarget::Annotation(f(t.clone()))
                                }
                                AssignVarTarget::Pattern { reason, annot } => {
                                    AssignVarTarget::Pattern {
                                        reason: reason.dupe(),
                                        annot: f(annot.clone()),
                                    }
                                }
                            })
                        });
                        let init = match init {
                            AssignVarInit::Reason(reason) => AssignVarInit::Reason(reason.dupe()),
                            AssignVarInit::Default(box AssignVarDefaultData { loc, default }) => {
                                AssignVarInit::Default(Box::new(AssignVarDefaultData {
                                    loc: loc.dupe(),
                                    default: f(default.clone()),
                                }))
                            }
                        };
                        VirtualUseOp::Op(Arc::new(VirtualRootUseOp::AssignVar { var, init }))
                    }
                    VirtualRootUseOp::FunImplicitReturn(box FunImplicitReturnData {
                        fn_,
                        upper,
                        upper_t,
                        type_guard,
                    }) => VirtualUseOp::Op(Arc::new(VirtualRootUseOp::FunImplicitReturn(
                        Box::new(FunImplicitReturnData {
                            fn_: fn_.dupe(),
                            upper: upper.dupe(),
                            upper_t: f(upper_t.clone()),
                            type_guard: *type_guard,
                        }),
                    ))),
                    VirtualRootUseOp::Cast {
                        lower,
                        upper: box upper,
                    } => VirtualUseOp::Op(Arc::new(VirtualRootUseOp::Cast {
                        lower: lower.dupe(),
                        upper: Box::new(f(upper.clone())),
                    })),
                    VirtualRootUseOp::IndexedTypeAccess(box IndexedTypeAccessData {
                        index_loc,
                        object,
                        index,
                    }) => VirtualUseOp::Op(Arc::new(VirtualRootUseOp::IndexedTypeAccess(
                        Box::new(IndexedTypeAccessData {
                            index_loc: index_loc.dupe(),
                            object: f(object.clone()),
                            index: f(index.clone()),
                        }),
                    ))),
                    VirtualRootUseOp::TypeGuardIncompatibility(
                        box TypeGuardIncompatibilityData {
                            guard_type_loc,
                            guard_type_reference_loc,
                            guard_type,
                            param_name,
                        },
                    ) => VirtualUseOp::Op(Arc::new(VirtualRootUseOp::TypeGuardIncompatibility(
                        Box::new(TypeGuardIncompatibilityData {
                            guard_type_loc: guard_type_loc.dupe(),
                            guard_type_reference_loc: guard_type_reference_loc.dupe(),
                            guard_type: f(guard_type.clone()),
                            param_name: param_name.dupe(),
                        }),
                    ))),
                    VirtualRootUseOp::PositiveTypeGuardConsistency(
                        box PositiveTypeGuardConsistencyData {
                            reason,
                            return_reason,
                            param_reason,
                            guard_type_reference_loc,
                            guard_type,
                            is_return_false_statement,
                        },
                    ) => {
                        VirtualUseOp::Op(Arc::new(VirtualRootUseOp::PositiveTypeGuardConsistency(
                            Box::new(PositiveTypeGuardConsistencyData {
                                reason: reason.dupe(),
                                return_reason: return_reason.dupe(),
                                param_reason: param_reason.dupe(),
                                guard_type_reference_loc: guard_type_reference_loc.dupe(),
                                guard_type: f(guard_type.clone()),
                                is_return_false_statement: *is_return_false_statement,
                            }),
                        )))
                    }
                    VirtualRootUseOp::AssertionFunctionConsistency(
                        box AssertionFunctionConsistencyData {
                            exit_reason,
                            param_reason,
                            guard_type_reference_loc,
                            guard_type,
                            implicit_return,
                        },
                    ) => {
                        VirtualUseOp::Op(Arc::new(VirtualRootUseOp::AssertionFunctionConsistency(
                            Box::new(AssertionFunctionConsistencyData {
                                exit_reason: exit_reason.dupe(),
                                param_reason: param_reason.dupe(),
                                guard_type_reference_loc: guard_type_reference_loc.dupe(),
                                guard_type: f(guard_type.clone()),
                                implicit_return: *implicit_return,
                            }),
                        )))
                    }
                    VirtualRootUseOp::Speculation(inner_op) => {
                        VirtualUseOp::Op(Arc::new(VirtualRootUseOp::Speculation(Arc::new(
                            map_use_op(f, inner_op.as_ref().clone()),
                        ))))
                    }
                    _ => VirtualUseOp::Op(root),
                },
                VirtualUseOp::Frame(frame_rc, inner_op) => {
                    let frame_rc = match frame_rc.as_ref() {
                        VirtualFrameUseOp::StandaloneCallThis(box StandaloneCallThisData {
                            op,
                            fn_,
                            receiver_loc,
                            receiver: box receiver,
                        }) => Arc::new(VirtualFrameUseOp::StandaloneCallThis(Box::new(
                            StandaloneCallThisData {
                                op: op.dupe(),
                                fn_: fn_.dupe(),
                                receiver_loc: receiver_loc.dupe(),
                                receiver: Box::new(f(receiver.clone())),
                            },
                        ))),
                        VirtualFrameUseOp::MappedTypeKeyCompatibility(
                            box MappedTypeKeyCompatibilityData {
                                source_type_loc,
                                source_type,
                                mapped_type_loc,
                                mapped_type,
                            },
                        ) => Arc::new(VirtualFrameUseOp::MappedTypeKeyCompatibility(Box::new(
                            MappedTypeKeyCompatibilityData {
                                source_type_loc: source_type_loc.dupe(),
                                source_type: f(source_type.clone()),
                                mapped_type_loc: mapped_type_loc.dupe(),
                                mapped_type: f(mapped_type.clone()),
                            },
                        ))),
                        VirtualFrameUseOp::OpaqueTypeCustomErrorCompatibility(
                            box OpaqueTypeCustomErrorCompatibilityData {
                                lower,
                                upper,
                                lower_t,
                                upper_t,
                                name,
                                custom_error_loc,
                            },
                        ) => Arc::new(VirtualFrameUseOp::OpaqueTypeCustomErrorCompatibility(
                            Box::new(OpaqueTypeCustomErrorCompatibilityData {
                                lower: lower.dupe(),
                                upper: upper.dupe(),
                                lower_t: f(lower_t.clone()),
                                upper_t: f(upper_t.clone()),
                                name: name.dupe(),
                                custom_error_loc: custom_error_loc.dupe(),
                            }),
                        )),
                        VirtualFrameUseOp::FunParam(box FunParamData {
                            n,
                            name,
                            lower,
                            upper,
                            lower_t,
                            upper_t,
                        }) => Arc::new(VirtualFrameUseOp::FunParam(Box::new(FunParamData {
                            n: *n,
                            name: name.dupe(),
                            lower: lower.dupe(),
                            upper: upper.dupe(),
                            lower_t: f(lower_t.clone()),
                            upper_t: f(upper_t.clone()),
                        }))),
                        VirtualFrameUseOp::FunMissingArg(box FunMissingArgData {
                            n,
                            op,
                            def,
                            def_t,
                        }) => Arc::new(VirtualFrameUseOp::FunMissingArg(Box::new(
                            FunMissingArgData {
                                n: *n,
                                op: op.dupe(),
                                def: def.dupe(),
                                def_t: f(def_t.clone()),
                            },
                        ))),
                        VirtualFrameUseOp::FunReturn(box FunReturnData {
                            lower,
                            upper,
                            lower_t,
                            upper_t,
                        }) => Arc::new(VirtualFrameUseOp::FunReturn(Box::new(FunReturnData {
                            lower: lower.dupe(),
                            upper: upper.dupe(),
                            lower_t: f(lower_t.clone()),
                            upper_t: f(upper_t.clone()),
                        }))),
                        VirtualFrameUseOp::ArrayElementCompatibility(
                            box ArrayElementCompatibilityData {
                                lower,
                                upper,
                                lower_t,
                                upper_t,
                            },
                        ) => Arc::new(VirtualFrameUseOp::ArrayElementCompatibility(Box::new(
                            ArrayElementCompatibilityData {
                                lower: lower.dupe(),
                                upper: upper.dupe(),
                                lower_t: f(lower_t.clone()),
                                upper_t: f(upper_t.clone()),
                            },
                        ))),
                        VirtualFrameUseOp::IndexerKeyCompatibility(
                            box IndexerKeyCompatibilityData {
                                lower,
                                upper,
                                lower_t,
                                upper_t,
                            },
                        ) => Arc::new(VirtualFrameUseOp::IndexerKeyCompatibility(Box::new(
                            IndexerKeyCompatibilityData {
                                lower: lower.dupe(),
                                upper: upper.dupe(),
                                lower_t: f(lower_t.clone()),
                                upper_t: f(upper_t.clone()),
                            },
                        ))),
                        VirtualFrameUseOp::PropertyCompatibility(
                            box PropertyCompatibilityData {
                                prop,
                                lower,
                                upper,
                                lower_t,
                                upper_t,
                            },
                        ) => Arc::new(VirtualFrameUseOp::PropertyCompatibility(Box::new(
                            PropertyCompatibilityData {
                                prop: prop.dupe(),
                                lower: lower.dupe(),
                                upper: upper.dupe(),
                                lower_t: f(lower_t.clone()),
                                upper_t: f(upper_t.clone()),
                            },
                        ))),
                        VirtualFrameUseOp::TupleElementCompatibility(
                            box TupleElementCompatibilityData {
                                n,
                                lower,
                                upper,
                                lower_t,
                                upper_t,
                            },
                        ) => Arc::new(VirtualFrameUseOp::TupleElementCompatibility(Box::new(
                            TupleElementCompatibilityData {
                                n: *n,
                                lower: lower.dupe(),
                                upper: upper.dupe(),
                                lower_t: f(lower_t.clone()),
                                upper_t: f(upper_t.clone()),
                            },
                        ))),
                        VirtualFrameUseOp::UnionRepresentative(box UnionRepresentativeData {
                            loc,
                            reference_loc,
                            union,
                        }) => Arc::new(VirtualFrameUseOp::UnionRepresentative(Box::new(
                            UnionRepresentativeData {
                                loc: loc.dupe(),
                                reference_loc: reference_loc.dupe(),
                                union: f(union.clone()),
                            },
                        ))),
                        _ => frame_rc.dupe(),
                    };
                    VirtualUseOp::Frame(
                        frame_rc,
                        Arc::new(map_use_op(f, inner_op.as_ref().clone())),
                    )
                }
            }
        }
        fn map_explanation<L: Dupe + PartialEq + Eq + PartialOrd + Ord>(
            f: &mut dyn FnMut(TypeOrTypeDesc<L>) -> TypeOrTypeDesc<L>,
            explanation: Option<ExplanationWithLazyParts<L>>,
        ) -> Option<ExplanationWithLazyParts<L>> {
            explanation.map(|e| match e {
                    ExplanationWithLazyParts::LazyExplanationInvariantSubtypingDueToMutableArray {
                        lower_array_loc,
                        upper_array_loc,
                        lower_array_desc,
                        upper_array_desc,
                        upper_array_reference_loc,
                    } => ExplanationWithLazyParts::LazyExplanationInvariantSubtypingDueToMutableArray {
                        lower_array_loc,
                        upper_array_loc,
                        lower_array_desc: f(lower_array_desc),
                        upper_array_desc: f(upper_array_desc),
                        upper_array_reference_loc,
                    },
                    ExplanationWithLazyParts::LazyExplanationInvariantSubtypingDueToMutableProperty {
                        lower_obj_loc,
                        upper_obj_loc,
                        lower_obj_desc,
                        upper_obj_desc,
                        upper_object_loc,
                        property_name,
                    } => ExplanationWithLazyParts::LazyExplanationInvariantSubtypingDueToMutableProperty {
                        lower_obj_loc,
                        upper_obj_loc,
                        lower_obj_desc: f(lower_obj_desc),
                        upper_obj_desc: f(upper_obj_desc),
                        upper_object_loc,
                        property_name,
                    },
                    ExplanationWithLazyParts::LazyExplanationInvariantSubtypingDueToMutableProperties {
                        lower_obj_loc,
                        upper_obj_loc,
                        lower_obj_desc,
                        upper_obj_desc,
                        upper_object_loc,
                        properties,
                    } => ExplanationWithLazyParts::LazyExplanationInvariantSubtypingDueToMutableProperties {
                        lower_obj_loc,
                        upper_obj_loc,
                        lower_obj_desc: f(lower_obj_desc),
                        upper_obj_desc: f(upper_obj_desc),
                        upper_object_loc,
                        properties,
                    },
                })
        }
        fn map_eager_explanation<L: Dupe + PartialEq + Eq + PartialOrd + Ord>(
            f: &mut dyn FnMut(TypeOrTypeDesc<L>) -> TypeOrTypeDesc<L>,
            explanation: Option<Explanation<L>>,
        ) -> Option<Explanation<L>> {
            explanation.map(|e| match e {
                Explanation::ExplanationAdditionalUnionMembers(
                    box ExplanationAdditionalUnionMembersData {
                        left_loc,
                        left_desc,
                        right_loc,
                        right_desc,
                        members,
                        extra_number,
                    },
                ) => Explanation::ExplanationAdditionalUnionMembers(Box::new(
                    ExplanationAdditionalUnionMembersData {
                        left_loc,
                        left_desc: f(left_desc),
                        right_loc,
                        right_desc: f(right_desc),
                        members,
                        extra_number,
                    },
                )),
                e => e,
            })
        }
        fn map_error_type_ref<L: Dupe + PartialEq + Eq + PartialOrd + Ord>(
            f: &mut dyn FnMut(TypeOrTypeDesc<L>) -> TypeOrTypeDesc<L>,
            r: ErrorTypeReferenceData<L>,
        ) -> ErrorTypeReferenceData<L> {
            ErrorTypeReferenceData {
                loc: r.loc,
                reference_loc: r.reference_loc,
                type_desc: f(r.type_desc),
            }
        }
        fn map_error_type_ref_with_reason<L: Dupe + PartialEq + Eq + PartialOrd + Ord>(
            f: &mut dyn FnMut(TypeOrTypeDesc<L>) -> TypeOrTypeDesc<L>,
            r: ErrorTypeReferenceWithLocData<L>,
        ) -> ErrorTypeReferenceWithLocData<L> {
            ErrorTypeReferenceWithLocData {
                loc: r.loc,
                definition_loc: r.definition_loc,
                reference_loc: r.reference_loc,
                type_desc: f(r.type_desc),
            }
        }

        match msg {
            EUnsupportedImplements(type_) => EUnsupportedImplements(map_error_type_ref(f, type_)),

            ECannotSpreadInterface(box ECannotSpreadInterfaceData {
                spread_loc,
                interface,
                use_op,
            }) => ECannotSpreadInterface(Box::new(ECannotSpreadInterfaceData {
                spread_loc,
                interface: map_error_type_ref_with_reason(f, interface),
                use_op: map_use_op(f, use_op),
            })),

            ENotAReactComponent(box ENotAReactComponentData { component, use_op }) => {
                ENotAReactComponent(Box::new(ENotAReactComponentData {
                    component: map_error_type_ref_with_reason(f, component),
                    use_op: map_use_op(f, use_op),
                }))
            }

            ECannotCallReactComponent { component } => ECannotCallReactComponent {
                component: map_error_type_ref_with_reason(f, component),
            },

            EReactRefInRender {
                usage,
                kind,
                in_hook,
            } => EReactRefInRender {
                usage,
                kind,
                in_hook,
            },

            ESketchyNumberLint(kind, value) => {
                ESketchyNumberLint(kind, map_error_type_ref_with_reason(f, value))
            }

            ETypeGuardFuncIncompatibility {
                use_op,
                lower,
                upper,
            } => {
                let mut map_reference = |reference| match reference {
                    TypeGuardFuncReference::Type(reference) => {
                        TypeGuardFuncReference::Type(map_error_type_ref_with_reason(f, reference))
                    }
                    reference @ TypeGuardFuncReference::TypeGuard(_) => reference,
                };
                let lower = map_reference(lower);
                let upper = map_reference(upper);
                ETypeGuardFuncIncompatibility {
                    use_op: map_use_op(f, use_op),
                    lower,
                    upper,
                }
            }

            EHookIncompatible(box EHookIncompatibleData {
                use_op,
                lower,
                upper,
                lower_is_hook,
                hook_is_annot,
            }) => EHookIncompatible(Box::new(EHookIncompatibleData {
                use_op: map_use_op(f, use_op),
                lower: map_error_type_ref_with_reason(f, lower),
                upper: map_error_type_ref_with_reason(f, upper),
                lower_is_hook,
                hook_is_annot,
            })),

            EHookUniqueIncompatible(box EHookUniqueIncompatibleData {
                use_op,
                lower,
                upper,
            }) => EHookUniqueIncompatible(Box::new(EHookUniqueIncompatibleData {
                use_op: map_use_op(f, use_op),
                lower: map_error_type_ref_with_reason(f, lower),
                upper: map_error_type_ref_with_reason(f, upper),
            })),

            EConstructSignatureMissingInSubtyping(
                box EConstructSignatureMissingInSubtypingData {
                    lower,
                    upper,
                    use_op,
                },
            ) => EConstructSignatureMissingInSubtyping(Box::new(
                EConstructSignatureMissingInSubtypingData {
                    lower: map_error_type_ref_with_reason(f, lower),
                    upper: map_error_type_ref_with_reason(f, upper),
                    use_op: map_use_op(f, use_op),
                },
            )),

            EPrivateLookupFailed(box EPrivateLookupFailedData {
                loc,
                object,
                prop_name,
                use_op,
            }) => EPrivateLookupFailed(Box::new(EPrivateLookupFailedData {
                loc,
                object: map_error_type_ref_with_reason(f, object),
                prop_name,
                use_op: map_use_op(f, use_op),
            })),

            EExpectedStringLit(box EExpectedStringLitData {
                lower,
                upper,
                use_op,
            }) => EExpectedStringLit(Box::new(EExpectedStringLitData {
                lower: map_error_type_ref_with_reason(f, lower),
                upper: map_error_type_ref_with_reason(f, upper),
                use_op: map_use_op(f, use_op),
            })),

            EExpectedBooleanLit(box EExpectedBooleanLitData {
                lower,
                upper,
                use_op,
            }) => EExpectedBooleanLit(Box::new(EExpectedBooleanLitData {
                lower: map_error_type_ref_with_reason(f, lower),
                upper: map_error_type_ref_with_reason(f, upper),
                use_op: map_use_op(f, use_op),
            })),

            EExpectedNumberLit(box EExpectedNumberLitData {
                lower,
                upper,
                use_op,
            }) => EExpectedNumberLit(Box::new(EExpectedNumberLitData {
                lower: map_error_type_ref_with_reason(f, lower),
                upper: map_error_type_ref_with_reason(f, upper),
                use_op: map_use_op(f, use_op),
            })),

            EExpectedBigIntLit(box EExpectedBigIntLitData {
                lower,
                upper,
                use_op,
            }) => EExpectedBigIntLit(Box::new(EExpectedBigIntLitData {
                lower: map_error_type_ref_with_reason(f, lower),
                upper: map_error_type_ref_with_reason(f, upper),
                use_op: map_use_op(f, use_op),
            })),

            ETupleOutOfBounds(box ETupleOutOfBoundsData {
                use_op,
                loc,
                tuple,
                inexact,
                length,
                index,
            }) => ETupleOutOfBounds(Box::new(ETupleOutOfBoundsData {
                use_op: map_use_op(f, use_op),
                loc,
                tuple: map_error_type_ref_with_reason(f, tuple),
                inexact,
                length,
                index,
            })),

            ETupleInvalidTypeSpread(box ETupleInvalidTypeSpreadData {
                spread_loc,
                argument,
            }) => ETupleInvalidTypeSpread(Box::new(ETupleInvalidTypeSpreadData {
                spread_loc,
                argument: map_error_type_ref_with_reason(f, argument),
            })),

            EPropsExtraAgainstExactObject(box EPropsExtraAgainstExactObjectData {
                prop_names,
                lower,
                upper,
                upper_is_record,
                use_op,
            }) => EPropsExtraAgainstExactObject(Box::new(EPropsExtraAgainstExactObjectData {
                prop_names,
                lower: map_error_type_ref_with_reason(f, lower),
                upper: map_error_type_ref_with_reason(f, upper),
                upper_is_record,
                use_op: map_use_op(f, use_op),
            })),

            EPrimitiveAsInterface(box EPrimitiveAsInterfaceData {
                use_op,
                lower,
                upper,
                kind,
            }) => EPrimitiveAsInterface(Box::new(EPrimitiveAsInterfaceData {
                use_op: map_use_op(f, use_op),
                lower: map_error_type_ref_with_reason(f, lower),
                upper: map_error_type_ref_with_reason(f, upper),
                kind,
            })),

            EClassToObject(box EClassToObjectData {
                lower,
                upper,
                use_op,
                kind,
            }) => EClassToObject(Box::new(EClassToObjectData {
                lower: map_error_type_ref_with_reason(f, lower),
                upper: map_error_type_ref_with_reason(f, upper),
                use_op: map_use_op(f, use_op),
                kind,
            })),

            EObjectComputedPropertyAccess(box EObjectComputedPropertyAccessData {
                object,
                property,
                kind,
            }) => EObjectComputedPropertyAccess(Box::new(EObjectComputedPropertyAccessData {
                object: map_error_type_ref_with_reason(f, object),
                property: map_error_type_ref_with_reason(f, property),
                kind,
            })),

            EObjectComputedPropertyAssign(box (property, key, kind)) => {
                EObjectComputedPropertyAssign(Box::new((
                    map_error_type_ref_with_reason(f, property),
                    key,
                    kind,
                )))
            }

            EBigIntRShift3(box operand) => EBigIntRShift3(Box::new(map_error_type_ref(f, operand))),

            EBigIntNumCoerce(box operand) => {
                EBigIntNumCoerce(Box::new(map_error_type_ref(f, operand)))
            }

            EComparison(box EComparisonData {
                r1,
                r2,
                loc_opt,
                strict_comparison_opt,
            }) => EComparison(Box::new(EComparisonData {
                r1: map_error_type_ref_with_reason(f, r1),
                r2: map_error_type_ref_with_reason(f, r2),
                loc_opt,
                strict_comparison_opt: strict_comparison_opt.map(|info| {
                    map_strict_comparison_info(
                        info,
                        |loc| loc,
                        |r| map_error_type_ref_with_reason(f, r),
                    )
                }),
            })),

            ENonStrictEqualityComparison(box ENonStrictEqualityComparisonData { lower, upper }) => {
                ENonStrictEqualityComparison(Box::new(ENonStrictEqualityComparisonData {
                    lower: map_error_type_ref_with_reason(f, lower),
                    upper: map_error_type_ref_with_reason(f, upper),
                }))
            }

            ETupleArityMismatch(box ETupleArityMismatchData {
                use_op,
                lower,
                lower_arity,
                lower_inexact,
                lower_rest,
                upper,
                upper_arity,
                upper_inexact,
                upper_rest,
                unify,
            }) => ETupleArityMismatch(Box::new(ETupleArityMismatchData {
                use_op: map_use_op(f, use_op),
                lower: map_error_type_ref_with_reason(f, lower),
                lower_arity,
                lower_inexact,
                lower_rest,
                upper: map_error_type_ref_with_reason(f, upper),
                upper_arity,
                upper_inexact,
                upper_rest,
                unify,
            })),

            ENonLitArrayToTuple((lower, upper), use_op) => ENonLitArrayToTuple(
                (
                    map_error_type_ref_with_reason(f, lower),
                    map_error_type_ref_with_reason(f, upper),
                ),
                map_use_op(f, use_op),
            ),

            ETupleElementPolarityMismatch(box ETupleElementPolarityMismatchData {
                index,
                lower,
                polarity_lower,
                upper,
                polarity_upper,
                use_op,
            }) => ETupleElementPolarityMismatch(Box::new(ETupleElementPolarityMismatchData {
                index,
                lower: map_error_type_ref_with_reason(f, lower),
                polarity_lower,
                upper: map_error_type_ref_with_reason(f, upper),
                polarity_upper,
                use_op: map_use_op(f, use_op),
            })),

            EIncompatibleWithExact((lower, upper), use_op, kind) => EIncompatibleWithExact(
                (
                    map_error_type_ref_with_reason(f, lower),
                    map_error_type_ref_with_reason(f, upper),
                ),
                map_use_op(f, use_op),
                kind,
            ),

            EFunctionIncompatibleWithIndexer((lower, upper), use_op) => {
                EFunctionIncompatibleWithIndexer(
                    (
                        map_error_type_ref_with_reason(f, lower),
                        map_error_type_ref_with_reason(f, upper),
                    ),
                    map_use_op(f, use_op),
                )
            }

            EInvalidRendersTypeArgument(box EInvalidRendersTypeArgumentData {
                loc,
                renders_variant,
                invalid_render_type_kind,
                invalid_types,
            }) => EInvalidRendersTypeArgument(Box::new(EInvalidRendersTypeArgumentData {
                loc,
                renders_variant,
                invalid_render_type_kind: map_invalid_render_type_kind(
                    |r| map_error_type_ref(f, r),
                    invalid_render_type_kind,
                ),
                invalid_types: Vec1::try_from_vec(
                    invalid_types
                        .into_iter()
                        .map(|r| map_error_type_ref(f, r))
                        .collect(),
                )
                .unwrap(),
            })),

            ETupleRequiredAfterOptional(box ETupleRequiredAfterOptionalData {
                tuple_loc,
                required,
                optional,
            }) => ETupleRequiredAfterOptional(Box::new(ETupleRequiredAfterOptionalData {
                tuple_loc,
                required,
                optional,
            })),

            EAnyValueUsedAsType { reference, value } => EAnyValueUsedAsType {
                reference,
                value: map_error_type_ref(f, value),
            },

            EValueUsedAsType { reference, value } => EValueUsedAsType {
                reference,
                value: map_error_type_ref(f, value),
            },

            EUnnecessaryInvariant(box EUnnecessaryInvariantData {
                loc,
                callee,
                condition,
                condition_kind,
            }) => EUnnecessaryInvariant(Box::new(EUnnecessaryInvariantData {
                loc,
                callee,
                condition: map_error_type_ref(f, condition),
                condition_kind,
            })),

            EUnnecessaryOptionalChain(box (loc, lhs_expression)) => {
                EUnnecessaryOptionalChain(Box::new((loc, lhs_expression)))
            }

            EInvalidThisArg(box EInvalidThisArgData {
                loc,
                name,
                receiver_expression,
                kind,
            }) => EInvalidThisArg(Box::new(EInvalidThisArgData {
                loc,
                name,
                receiver_expression,
                kind,
            })),

            EFunctionCallExtraArg(box EFunctionCallExtraArgData {
                loc,
                function_reference,
                param_count,
                use_op,
            }) => EFunctionCallExtraArg(Box::new(EFunctionCallExtraArgData {
                loc,
                function_reference,
                param_count,
                use_op: map_use_op(f, use_op),
            })),

            EPropNotReadable(box EPropNotReadableData {
                prop_loc,
                prop_name,
                use_op,
            }) => EPropNotReadable(Box::new(EPropNotReadableData {
                prop_loc,
                prop_name,
                use_op: map_use_op(f, use_op),
            })),

            EPropNotWritable(box EPropNotWritableData {
                prop_loc,
                prop_name,
                use_op,
            }) => EPropNotWritable(Box::new(EPropNotWritableData {
                prop_loc,
                prop_name,
                use_op: map_use_op(f, use_op),
            })),

            ETupleNonIntegerIndex(box ETupleNonIntegerIndexData {
                use_op,
                loc,
                index_def_loc,
                index,
            }) => ETupleNonIntegerIndex(Box::new(ETupleNonIntegerIndexData {
                use_op: map_use_op(f, use_op),
                loc,
                index_def_loc,
                index,
            })),

            ETupleUnsafeWrite { loc, use_op } => ETupleUnsafeWrite {
                loc,
                use_op: map_use_op(f, use_op),
            },

            ETupleElementNotReadable(box ETupleElementNotReadableData {
                loc,
                index_def_loc,
                index,
                name,
                use_op,
            }) => ETupleElementNotReadable(Box::new(ETupleElementNotReadableData {
                loc,
                index_def_loc,
                index,
                name,
                use_op: map_use_op(f, use_op),
            })),

            ETupleElementNotWritable(box ETupleElementNotWritableData {
                loc,
                index_def_loc,
                index,
                name,
                use_op,
            }) => ETupleElementNotWritable(Box::new(ETupleElementNotWritableData {
                loc,
                index_def_loc,
                index,
                name,
                use_op: map_use_op(f, use_op),
            })),

            EROArrayWrite(loc, use_op) => EROArrayWrite(loc, map_use_op(f, use_op)),

            ETypeGuardIndexMismatch { use_op, parameters } => ETypeGuardIndexMismatch {
                use_op: map_use_op(f, use_op),
                parameters,
            },

            EIllegalAssertOperator(box EIllegalAssertOperatorData {
                op_loc,
                obj,
                specialized,
            }) => EIllegalAssertOperator(Box::new(EIllegalAssertOperatorData {
                op_loc,
                obj,
                specialized,
            })),

            ECannotDelete(box ECannotDeleteData { loc, expression }) => {
                ECannotDelete(Box::new(ECannotDeleteData {
                    loc,
                    expression: map_error_type_ref(f, expression),
                }))
            }

            EEnumError(EnumErrorKind::EnumIncompatible(box EIncompatibleTypesWithUseOpData {
                use_op,
                lower_loc,
                lower_def_loc,
                upper_loc,
                upper_def_loc,
                lower_desc,
                upper_desc,
                explanation,
                example,
                branches,
            })) => EEnumError(EnumErrorKind::EnumIncompatible(Box::new(
                EIncompatibleTypesWithUseOpData {
                    use_op: map_use_op(f, use_op),
                    lower_loc,
                    lower_def_loc,
                    upper_loc,
                    upper_def_loc,
                    lower_desc: f(lower_desc),
                    upper_desc: f(upper_desc),
                    explanation: map_eager_explanation(f, explanation),
                    example: example
                        .map(|example| Box::new(Self::convert_type_to_type_desc(f, *example))),
                    branches: branches
                        .into_iter()
                        .map(|branch| Self::convert_type_to_type_desc(f, branch))
                        .collect(),
                },
            ))),

            EEnumError(EnumErrorKind::EnumInvalidMemberAccess(
                box EnumInvalidMemberAccessData {
                    member,
                    member_loc,
                    enum_,
                },
            )) => EEnumError(EnumErrorKind::EnumInvalidMemberAccess(Box::new(
                EnumInvalidMemberAccessData {
                    member: match member {
                        EnumMemberAccess::Named { name, suggestion } => {
                            EnumMemberAccess::Named { name, suggestion }
                        }
                        EnumMemberAccess::Computed(key) => {
                            EnumMemberAccess::Computed(map_error_type_ref(f, key))
                        }
                    },
                    member_loc,
                    enum_,
                },
            ))),

            EEnumError(enum_error) => EEnumError(match enum_error {
                EnumErrorKind::EnumMemberDuplicateValue(box EnumMemberDuplicateValueData {
                    loc,
                    prev_use_loc,
                    enum_,
                }) => EnumErrorKind::EnumMemberDuplicateValue(Box::new(
                    EnumMemberDuplicateValueData {
                        loc,
                        prev_use_loc,
                        enum_,
                    },
                )),
                EnumErrorKind::EnumInvalidMemberName(box EnumInvalidMemberNameData {
                    loc,
                    enum_,
                    member_name,
                }) => EnumErrorKind::EnumInvalidMemberName(Box::new(EnumInvalidMemberNameData {
                    loc,
                    enum_,
                    member_name,
                })),
                EnumErrorKind::EnumNonIdentifierMemberName(
                    box EnumNonIdentifierMemberNameData {
                        loc,
                        enum_,
                        member_name,
                    },
                ) => EnumErrorKind::EnumNonIdentifierMemberName(Box::new(
                    EnumNonIdentifierMemberNameData {
                        loc,
                        enum_,
                        member_name,
                    },
                )),
                EnumErrorKind::EnumDuplicateMemberName(box EnumDuplicateMemberNameData {
                    loc,
                    prev_use_loc,
                    enum_,
                    member_name,
                }) => {
                    EnumErrorKind::EnumDuplicateMemberName(Box::new(EnumDuplicateMemberNameData {
                        loc,
                        prev_use_loc,
                        enum_,
                        member_name,
                    }))
                }
                EnumErrorKind::EnumInconsistentMemberValues(
                    box EnumInconsistentMemberValuesData { loc, enum_ },
                ) => EnumErrorKind::EnumInconsistentMemberValues(Box::new(
                    EnumInconsistentMemberValuesData { loc, enum_ },
                )),
                EnumErrorKind::EnumInvalidMemberInitializer(
                    box EnumInvalidMemberInitializerData {
                        loc,
                        enum_,
                        explicit_type,
                        member_name,
                    },
                ) => EnumErrorKind::EnumInvalidMemberInitializer(Box::new(
                    EnumInvalidMemberInitializerData {
                        loc,
                        enum_,
                        explicit_type,
                        member_name,
                    },
                )),
                EnumErrorKind::EnumBooleanMemberNotInitialized(
                    box EnumBooleanMemberNotInitializedData {
                        loc,
                        enum_,
                        member_name,
                    },
                ) => EnumErrorKind::EnumBooleanMemberNotInitialized(Box::new(
                    EnumBooleanMemberNotInitializedData {
                        loc,
                        enum_,
                        member_name,
                    },
                )),
                EnumErrorKind::EnumNumberMemberNotInitialized(
                    box EnumNumberMemberNotInitializedData {
                        loc,
                        enum_,
                        member_name,
                    },
                ) => EnumErrorKind::EnumNumberMemberNotInitialized(Box::new(
                    EnumNumberMemberNotInitializedData {
                        loc,
                        enum_,
                        member_name,
                    },
                )),
                EnumErrorKind::EnumBigIntMemberNotInitialized(
                    box EnumBigIntMemberNotInitializedData {
                        loc,
                        enum_,
                        member_name,
                    },
                ) => EnumErrorKind::EnumBigIntMemberNotInitialized(Box::new(
                    EnumBigIntMemberNotInitializedData {
                        loc,
                        enum_,
                        member_name,
                    },
                )),
                EnumErrorKind::EnumStringMemberInconsistentlyInitialized(
                    box EnumStringMemberInconsistentlyInitializedData { loc, enum_ },
                ) => EnumErrorKind::EnumStringMemberInconsistentlyInitialized(Box::new(
                    EnumStringMemberInconsistentlyInitializedData { loc, enum_ },
                )),
                EnumErrorKind::TSEnumInvalidMember(box TSEnumInvalidMemberData {
                    loc,
                    enum_,
                    member_name,
                    kind,
                }) => EnumErrorKind::TSEnumInvalidMember(Box::new(TSEnumInvalidMemberData {
                    loc,
                    enum_,
                    member_name,
                    kind,
                })),
                EnumErrorKind::TSEnumInvalidSyntax(box TSEnumInvalidSyntaxData {
                    loc,
                    enum_,
                    kind,
                }) => EnumErrorKind::TSEnumInvalidSyntax(Box::new(TSEnumInvalidSyntaxData {
                    loc,
                    enum_,
                    kind,
                })),
                enum_error => enum_error,
            }),

            EIndexerCheckFailed(box EIndexerCheckFailedData {
                prop_name,
                lower,
                upper,
                indexer_loc,
                indexer_desc,
                use_op,
            }) => EIndexerCheckFailed(Box::new(EIndexerCheckFailedData {
                prop_name,
                lower: map_error_type_ref_with_reason(f, lower),
                upper: map_error_type_ref_with_reason(f, upper),
                indexer_loc,
                indexer_desc: f(indexer_desc),
                use_op: map_use_op(f, use_op),
            })),

            EMatchError(MatchErrorKind::MatchInvalidIdentOrMemberPattern(
                box MatchInvalidIdentOrMemberPatternData {
                    loc,
                    type_loc,
                    type_desc,
                },
            )) => EMatchError(MatchErrorKind::MatchInvalidIdentOrMemberPattern(Box::new(
                MatchInvalidIdentOrMemberPatternData {
                    loc,
                    type_loc,
                    type_desc: f(type_desc),
                },
            ))),

            EMatchError(MatchErrorKind::MatchNonExhaustiveObjectPattern(
                box MatchNonExhaustiveObjectPatternData {
                    loc,
                    rest,
                    missing_props,
                    pattern_kind,
                },
            )) => EMatchError(MatchErrorKind::MatchNonExhaustiveObjectPattern(Box::new(
                MatchNonExhaustiveObjectPatternData {
                    loc,
                    rest: rest.map(|r| map_error_type_ref(f, r)),
                    missing_props,
                    pattern_kind,
                },
            ))),

            ETypeGuardImpliesMismatch {
                use_op,
                lower,
                upper,
            } => ETypeGuardImpliesMismatch {
                use_op: map_use_op(f, use_op),
                lower,
                upper,
            },

            ETypeGuardFunctionInvalidWrites(box ETypeGuardFunctionInvalidWritesData {
                loc,
                type_guard,
                write_locs,
            }) => ETypeGuardFunctionInvalidWrites(Box::new(ETypeGuardFunctionInvalidWritesData {
                loc,
                type_guard,
                write_locs,
            })),

            ETypeParamConstIncompatibility(box ETypeParamConstIncompatibilityData {
                use_op,
                lower,
                upper,
            }) => ETypeParamConstIncompatibility(Box::new(ETypeParamConstIncompatibilityData {
                use_op: map_use_op(f, use_op),
                lower,
                upper,
            })),

            ENegativeTypeGuardConsistency(box ENegativeTypeGuardConsistencyData {
                return_reason,
                type_,
            }) => ENegativeTypeGuardConsistency(Box::new(ENegativeTypeGuardConsistencyData {
                return_reason,
                type_: map_error_type_ref(f, type_),
            })),

            EAssertionFunctionFalsyAtExit(box EAssertionFunctionFalsyAtExitData {
                loc,
                param,
                falsy,
                implicit_return,
            }) => EAssertionFunctionFalsyAtExit(Box::new(EAssertionFunctionFalsyAtExitData {
                loc,
                param,
                falsy: map_error_type_ref(f, falsy),
                implicit_return,
            })),

            EUnrecognizedAssertionCall(data) => EUnrecognizedAssertionCall(data),

            EUnableToSpread(box EUnableToSpreadData {
                spread_loc,
                object1,
                object2,
                propname,
                error_kind,
                use_op,
            }) => EUnableToSpread(Box::new(EUnableToSpreadData {
                spread_loc,
                object1: map_error_type_ref_with_reason(f, object1),
                object2: map_error_type_ref_with_reason(f, object2),
                propname,
                error_kind,
                use_op: map_use_op(f, use_op),
            })),

            EExponentialSpread(box EExponentialSpreadData {
                spread_loc,
                operand1,
                operand2,
            }) => EExponentialSpread(Box::new(EExponentialSpreadData {
                spread_loc,
                operand1: map_exponential_spread_operand_group(
                    |r| map_error_type_ref_with_reason(f, r),
                    operand1,
                ),
                operand2: map_exponential_spread_operand_group(
                    |r| map_error_type_ref_with_reason(f, r),
                    operand2,
                ),
            })),

            EInexactMayOverwriteIndexer(box EInexactMayOverwriteIndexerData {
                spread_loc,
                key_loc,
                key_desc,
                value_loc,
                value_desc,
                object2,
                use_op,
            }) => EInexactMayOverwriteIndexer(Box::new(EInexactMayOverwriteIndexerData {
                spread_loc,
                key_loc,
                key_desc: f(key_desc),
                value_loc,
                value_desc: f(value_desc),
                object2: map_error_type_ref_with_reason(f, object2),
                use_op: map_use_op(f, use_op),
            })),

            ECannotSpreadIndexerOnRight(box ECannotSpreadIndexerOnRightData {
                spread_loc,
                object,
                key_loc,
                key_desc,
                use_op,
            }) => ECannotSpreadIndexerOnRight(Box::new(ECannotSpreadIndexerOnRightData {
                spread_loc,
                object: map_error_type_ref_with_reason(f, object),
                key_loc,
                key_desc: f(key_desc),
                use_op: map_use_op(f, use_op),
            })),

            EInvalidObjectKit(box EInvalidObjectKitData {
                loc,
                value,
                use_op,
                requirement,
            }) => EInvalidObjectKit(Box::new(EInvalidObjectKitData {
                loc,
                value: map_error_type_ref(f, value),
                use_op: map_use_op(f, use_op),
                requirement,
            })),

            EUnsupportedExact(box EUnsupportedExactData {
                value_loc,
                value_desc,
            }) => EUnsupportedExact(Box::new(EUnsupportedExactData {
                value_loc,
                value_desc: f(value_desc),
            })),

            EInvalidExactObjectIntersection(box EInvalidExactObjectIntersectionData {
                loc,
                prop,
                exact_object_loc,
                exact_object_desc,
            }) => EInvalidExactObjectIntersection(Box::new(EInvalidExactObjectIntersectionData {
                loc,
                prop,
                exact_object_loc,
                exact_object_desc: f(exact_object_desc),
            })),

            ECallTypeArity(box ECallTypeArityData {
                call_loc,
                is_new,
                callee,
                expected_arity,
            }) => ECallTypeArity(Box::new(ECallTypeArityData {
                call_loc,
                is_new,
                callee: match callee {
                    CallTypeArityCallee::Type(callee) => {
                        CallTypeArityCallee::Type(map_error_type_ref(f, callee))
                    }
                    CallTypeArityCallee::Builtin(name) => CallTypeArityCallee::Builtin(name),
                    CallTypeArityCallee::Function => CallTypeArityCallee::Function,
                },
                expected_arity,
            })),

            EUnionOptimizationOnNonUnion(box EUnionOptimizationOnNonUnionData {
                loc,
                arg_loc,
                arg_desc,
            }) => EUnionOptimizationOnNonUnion(Box::new(EUnionOptimizationOnNonUnionData {
                loc,
                arg_loc,
                arg_desc: f(arg_desc),
            })),

            EUnionOptimization(box EUnionOptimizationData {
                loc,
                kind:
                    OptimizedError::ContainsUnresolved {
                        loc: member_loc,
                        desc,
                    },
            }) => EUnionOptimization(Box::new(EUnionOptimizationData {
                loc,
                kind: OptimizedError::ContainsUnresolved {
                    loc: member_loc,
                    desc: f(desc),
                },
            })),

            EInvalidBinaryArith(box EInvalidBinaryArithData {
                loc,
                left,
                right,
                kind,
            }) => EInvalidBinaryArith(Box::new(EInvalidBinaryArithData {
                loc,
                left: map_error_type_ref(f, left),
                right: map_error_type_ref(f, right),
                kind,
            })),

            EInvalidReactCreateElement(box EInvalidReactCreateElementData {
                create_element_loc,
                react,
            }) => EInvalidReactCreateElement(Box::new(EInvalidReactCreateElementData {
                create_element_loc,
                react,
            })),

            EInvalidPrototype(box EInvalidPrototypeData {
                loc,
                prototype_loc,
                prototype_desc,
            }) => EInvalidPrototype(Box::new(EInvalidPrototypeData {
                loc,
                prototype_loc,
                prototype_desc: f(prototype_desc),
            })),

            EInvalidConstructor(box EInvalidConstructorData {
                loc,
                value_loc,
                value_desc,
            }) => EInvalidConstructor(Box::new(EInvalidConstructorData {
                loc,
                value_loc,
                value_desc: f(value_desc),
            })),

            EBinaryInRHS(box EBinaryInRHSData {
                loc,
                rhs_loc,
                rhs_desc,
            }) => EBinaryInRHS(Box::new(EBinaryInRHSData {
                loc,
                rhs_loc,
                rhs_desc: f(rhs_desc),
            })),

            EBinaryInLHS(box EBinaryInLHSData {
                loc,
                lhs_loc,
                lhs_desc,
            }) => EBinaryInLHS(Box::new(EBinaryInLHSData {
                loc,
                lhs_loc,
                lhs_desc: f(lhs_desc),
            })),

            EInstanceofRHS(box EInstanceofRHSData {
                loc,
                rhs_loc,
                rhs_desc,
            }) => EInstanceofRHS(Box::new(EInstanceofRHSData {
                loc,
                rhs_loc,
                rhs_desc: f(rhs_desc),
            })),

            EForInRHS(box EForInRHSData {
                loc,
                rhs_loc,
                rhs_desc,
            }) => EForInRHS(Box::new(EForInRHSData {
                loc,
                rhs_loc,
                rhs_desc: f(rhs_desc),
            })),

            EArithmeticOperand(box EArithmeticOperandData { loc, operand }) => {
                EArithmeticOperand(Box::new(EArithmeticOperandData {
                    loc,
                    operand: map_error_type_ref(f, operand),
                }))
            }

            EImplicitInstantiationUnderconstrainedError(
                box EImplicitInstantiationUnderconstrainedErrorData {
                    call,
                    type_param_loc,
                    type_param_name,
                    use_op,
                },
            ) => EImplicitInstantiationUnderconstrainedError(Box::new(
                EImplicitInstantiationUnderconstrainedErrorData {
                    call,
                    type_param_loc,
                    type_param_name,
                    use_op: map_use_op(f, use_op),
                },
            )),

            EIncompatibleType(box EIncompatibleTypeData {
                lower_kind,
                lower_loc,
                lower_def_loc,
                lower_desc,
                upper,
                use_op,
            }) => EIncompatibleType(Box::new(EIncompatibleTypeData {
                lower_kind,
                lower_loc,
                lower_def_loc,
                lower_desc: f(lower_desc),
                upper,
                use_op: use_op.map(|use_op| map_use_op(f, use_op)),
            })),

            EIncompatibleProp(box EIncompatiblePropData {
                prop,
                prop_loc,
                object,
                special,
                use_op,
            }) => EIncompatibleProp(Box::new(EIncompatiblePropData {
                prop,
                prop_loc,
                object: map_error_type_ref_with_reason(f, object),
                special,
                use_op: use_op.map(|use_op| map_use_op(f, use_op)),
            })),

            EPropNotFoundInLookup(box EPropNotFoundInLookupData {
                prop_name,
                prop_loc,
                object,
                use_op,
                suggestion,
            }) => EPropNotFoundInLookup(Box::new(EPropNotFoundInLookupData {
                prop_name,
                prop_loc,
                object: map_error_type_ref_with_reason(f, object),
                use_op: map_use_op(f, use_op),
                suggestion,
            })),

            EMissingTypeArgs(box EMissingTypeArgsData {
                loc,
                type_,
                arity_loc,
                min_arity,
                max_arity,
            }) => EMissingTypeArgs(Box::new(EMissingTypeArgsData {
                loc,
                type_: map_error_type_ref_with_reason(f, type_),
                arity_loc,
                min_arity,
                max_arity,
            })),

            EIncompatibleTypesWithUseOp(box EIncompatibleTypesWithUseOpData {
                use_op,
                lower_loc,
                lower_def_loc,
                upper_loc,
                upper_def_loc,
                lower_desc,
                upper_desc,
                explanation,
                example,
                branches,
            }) => EIncompatibleTypesWithUseOp(Box::new(EIncompatibleTypesWithUseOpData {
                use_op: map_use_op(f, use_op),
                lower_loc,
                lower_def_loc,
                upper_loc,
                upper_def_loc,
                lower_desc: f(lower_desc),
                upper_desc: f(upper_desc),
                explanation: map_eager_explanation(f, explanation),
                example: example
                    .map(|example| Box::new(Self::convert_type_to_type_desc(f, *example))),
                branches: branches
                    .into_iter()
                    .map(|branch| Self::convert_type_to_type_desc(f, branch))
                    .collect(),
            })),

            EInvariantSubtypingWithUseOp(box EInvariantSubtypingWithUseOpData {
                sub_component,
                lower_loc,
                upper_loc,
                lower_desc,
                upper_desc,
                use_op,
                explanation,
            }) => EInvariantSubtypingWithUseOp(Box::new(EInvariantSubtypingWithUseOpData {
                sub_component,
                lower_loc,
                upper_loc,
                lower_desc: f(lower_desc),
                upper_desc: f(upper_desc),
                use_op: map_use_op(f, use_op),
                explanation: map_explanation(f, explanation),
            })),

            EPropNotFoundInSubtyping(box EPropNotFoundInSubtypingData {
                prop_name,
                lower,
                upper,
                use_op,
                suggestion,
            }) => EPropNotFoundInSubtyping(Box::new(EPropNotFoundInSubtypingData {
                prop_name,
                lower: map_error_type_ref_with_reason(f, lower),
                upper: map_error_type_ref_with_reason(f, upper),
                use_op: map_use_op(f, use_op),
                suggestion,
            })),

            EPropPolarityMismatch(box EPropPolarityMismatchData {
                lower,
                upper,
                props,
                use_op,
            }) => {
                let mut map_object = |object| match object {
                    PropPolarityMismatchObject::Type(t) => {
                        PropPolarityMismatchObject::Type(map_error_type_ref_with_reason(f, t))
                    }
                    object @ PropPolarityMismatchObject::Class(_) => object,
                };
                let lower = map_object(lower);
                let upper = map_object(upper);
                EPropPolarityMismatch(Box::new(EPropPolarityMismatchData {
                    lower,
                    upper,
                    props,
                    use_op: map_use_op(f, use_op),
                }))
            }

            EPropsNotFoundInSubtyping(box EPropsNotFoundInSubtypingData {
                prop_names,
                lower,
                upper,
                use_op,
            }) => EPropsNotFoundInSubtyping(Box::new(EPropsNotFoundInSubtypingData {
                prop_names,
                lower: map_error_type_ref_with_reason(f, lower),
                upper: map_error_type_ref_with_reason(f, upper),
                use_op: map_use_op(f, use_op),
            })),

            EPropsNotFoundInInvariantSubtyping(box EPropsNotFoundInInvariantSubtypingData {
                prop_names,
                lower,
                upper,
                use_op,
            }) => EPropsNotFoundInInvariantSubtyping(Box::new(
                EPropsNotFoundInInvariantSubtypingData {
                    prop_names,
                    lower: map_error_type_ref_with_reason(f, lower),
                    upper: map_error_type_ref_with_reason(f, upper),
                    use_op: map_use_op(f, use_op),
                },
            )),

            EMatchError(MatchErrorKind::MatchNotExhaustive(box MatchNotExhaustiveData {
                loc,
                examples,
                missing_pattern_asts,
            })) => EMatchError(MatchErrorKind::MatchNotExhaustive(Box::new(
                MatchNotExhaustiveData {
                    loc,
                    examples: examples
                        .into_iter()
                        .map(|(pattern, references)| {
                            (
                                pattern,
                                references
                                    .into_iter()
                                    .map(|reference| {
                                        match reference {
                                        MatchExampleReference::Type(reference) => {
                                            MatchExampleReference::Type(
                                                map_error_type_ref_with_reason(f, reference),
                                            )
                                        }
                                        reference @ (MatchExampleReference::EnumMember { .. }
                                        | MatchExampleReference::EnumUnknownMembers { .. }) => {
                                            reference
                                        }
                                    }
                                    })
                                    .collect(),
                            )
                        })
                        .collect(),
                    missing_pattern_asts,
                },
            ))),

            EUnionPartialOptimizationNonUniqueKey(
                box EUnionPartialOptimizationNonUniqueKeyData {
                    loc,
                    non_unique_keys,
                },
            ) => EUnionPartialOptimizationNonUniqueKey(Box::new(
                EUnionPartialOptimizationNonUniqueKeyData {
                    loc,
                    non_unique_keys: non_unique_keys
                        .into_iter()
                        .map(|(name, inner_map)| {
                            (
                                name,
                                inner_map
                                    .into_iter()
                                    .map(|(enum_key, references)| {
                                        (
                                            enum_key,
                                            references
                                                .mapped(|r| map_error_type_ref_with_reason(f, r)),
                                        )
                                    })
                                    .collect(),
                            )
                        })
                        .collect(),
                },
            )),

            EIncompatibleSpeculation(box EIncompatibleSpeculationData {
                use_op,
                loc,
                branches,
            }) => {
                let use_op = use_op.map(|u| map_use_op(f, u));
                let branches = branches
                    .into_iter()
                    .map(|b| Self::convert_type_to_type_desc(f, b))
                    .collect();
                EIncompatibleSpeculation(Box::new(EIncompatibleSpeculationData {
                    use_op,
                    loc,
                    branches,
                }))
            }

            EUnionSpeculationFailed(box EUnionSpeculationFailedData {
                use_op,
                loc,
                branches,
            }) => {
                let use_op = map_use_op(f, use_op);
                let branches = branches
                    .into_iter()
                    .map(|b| Self::convert_type_to_type_desc(f, b))
                    .collect();
                EUnionSpeculationFailed(Box::new(EUnionSpeculationFailedData {
                    use_op,
                    loc,
                    branches,
                }))
            }

            ERecordError(RecordErrorKind::RecordBannedTypeUtil { op_loc, op, record }) => {
                ERecordError(RecordErrorKind::RecordBannedTypeUtil {
                    op_loc,
                    op,
                    record: map_error_type_ref_with_reason(f, record),
                })
            }

            EComponentMissingReturn(component) => EComponentMissingReturn(component),

            EReactIntrinsicOverlap(box EReactIntrinsicOverlapData {
                use_,
                def,
                type_,
                mixed,
            }) => EReactIntrinsicOverlap(Box::new(EReactIntrinsicOverlapData {
                use_,
                def,
                type_,
                mixed,
            })),

            ERecursiveDefinition(box ERecursiveDefinitionData {
                definition,
                recursion,
                annot_locs,
            }) => ERecursiveDefinition(Box::new(ERecursiveDefinitionData {
                definition,
                recursion,
                annot_locs,
            })),

            e => e,
        }
    }
}

/// A utility function for getting and updating the use_op in error messages.
pub fn util_use_op_of_msg<L, T, F>(nope: T, util: F, msg: &ErrorMessage<L>) -> T
where
    L: Dupe + PartialEq + Eq + PartialOrd + Ord,
    F: Fn(&VirtualUseOp<L>) -> T,
{
    match msg {
        ErrorMessage::EIncompatibleType(box EIncompatibleTypeData { use_op, .. }) => {
            use_op.as_ref().map_or_else(|| nope, util)
        }
        ErrorMessage::EIncompatibleSpeculation(box EIncompatibleSpeculationData {
            use_op, ..
        }) => use_op.as_ref().map_or_else(|| nope, util),
        ErrorMessage::EIncompatibleProp(box EIncompatiblePropData { use_op, .. }) => {
            use_op.as_ref().map_or_else(|| nope, util)
        }
        ErrorMessage::EExpectedStringLit(box EExpectedStringLitData { use_op, .. }) => util(use_op),
        ErrorMessage::EExpectedNumberLit(box EExpectedNumberLitData { use_op, .. }) => util(use_op),
        ErrorMessage::EExpectedBooleanLit(box EExpectedBooleanLitData { use_op, .. }) => {
            util(use_op)
        }
        ErrorMessage::EExpectedBigIntLit(box EExpectedBigIntLitData { use_op, .. }) => util(use_op),
        ErrorMessage::EPropNotFoundInLookup(box EPropNotFoundInLookupData { use_op, .. }) => {
            util(use_op)
        }
        ErrorMessage::EPropNotFoundInSubtyping(box EPropNotFoundInSubtypingData {
            use_op, ..
        }) => util(use_op),
        ErrorMessage::EConstructSignatureMissingInSubtyping(
            box EConstructSignatureMissingInSubtypingData { use_op, .. },
        ) => util(use_op),
        ErrorMessage::EPropsNotFoundInSubtyping(box EPropsNotFoundInSubtypingData {
            use_op,
            ..
        }) => util(use_op),
        ErrorMessage::EPropsNotFoundInInvariantSubtyping(
            box EPropsNotFoundInInvariantSubtypingData { use_op, .. },
        ) => util(use_op),
        ErrorMessage::EPropsExtraAgainstExactObject(box EPropsExtraAgainstExactObjectData {
            use_op,
            ..
        }) => util(use_op),
        ErrorMessage::EIndexerCheckFailed(box EIndexerCheckFailedData { use_op, .. }) => {
            util(use_op)
        }
        ErrorMessage::EPropNotReadable(box EPropNotReadableData { use_op, .. }) => util(use_op),
        ErrorMessage::EPropNotWritable(box EPropNotWritableData { use_op, .. }) => util(use_op),
        ErrorMessage::EPropPolarityMismatch(box EPropPolarityMismatchData { use_op, .. }) => {
            util(use_op)
        }
        ErrorMessage::EPrivateLookupFailed(box EPrivateLookupFailedData { use_op, .. }) => {
            util(use_op)
        }
        ErrorMessage::ETupleArityMismatch(box ETupleArityMismatchData { use_op, .. }) => {
            util(use_op)
        }
        ErrorMessage::ENonLitArrayToTuple(_, use_op) => util(use_op),
        ErrorMessage::ETupleOutOfBounds(box ETupleOutOfBoundsData { use_op, .. }) => util(use_op),
        ErrorMessage::ETupleNonIntegerIndex(box ETupleNonIntegerIndexData { use_op, .. }) => {
            util(use_op)
        }
        ErrorMessage::ETupleUnsafeWrite { use_op, .. } => util(use_op),
        ErrorMessage::ETupleElementNotReadable(box ETupleElementNotReadableData {
            use_op, ..
        }) => util(use_op),
        ErrorMessage::ETupleElementNotWritable(box ETupleElementNotWritableData {
            use_op, ..
        }) => util(use_op),
        ErrorMessage::ETupleElementPolarityMismatch(box ETupleElementPolarityMismatchData {
            use_op,
            ..
        }) => util(use_op),
        ErrorMessage::EROArrayWrite(_, use_op) => util(use_op),
        ErrorMessage::EUnionSpeculationFailed(box EUnionSpeculationFailedData {
            use_op, ..
        }) => util(use_op),
        ErrorMessage::EIncompatibleWithExact(_, use_op, _) => util(use_op),
        ErrorMessage::EFunctionIncompatibleWithIndexer(_, use_op) => util(use_op),
        ErrorMessage::EInvalidObjectKit(box EInvalidObjectKitData { use_op, .. }) => util(use_op),
        ErrorMessage::EIncompatibleTypesWithUseOp(box EIncompatibleTypesWithUseOpData {
            use_op,
            ..
        }) => util(use_op),
        ErrorMessage::EInvariantSubtypingWithUseOp(box EInvariantSubtypingWithUseOpData {
            use_op,
            ..
        }) => util(use_op),
        ErrorMessage::EEnumError(EnumErrorKind::EnumIncompatible(
            box EIncompatibleTypesWithUseOpData { use_op, .. },
        )) => util(use_op),
        ErrorMessage::ENotAReactComponent(box ENotAReactComponentData { use_op, .. }) => {
            util(use_op)
        }
        ErrorMessage::EFunctionCallExtraArg(box EFunctionCallExtraArgData { use_op, .. }) => {
            util(use_op)
        }
        ErrorMessage::EPrimitiveAsInterface(box EPrimitiveAsInterfaceData { use_op, .. }) => {
            util(use_op)
        }
        ErrorMessage::ECannotSpreadInterface(box ECannotSpreadInterfaceData { use_op, .. }) => {
            util(use_op)
        }
        ErrorMessage::ECannotSpreadIndexerOnRight(box ECannotSpreadIndexerOnRightData {
            use_op,
            ..
        }) => util(use_op),
        ErrorMessage::EUnableToSpread(box EUnableToSpreadData { use_op, .. }) => util(use_op),
        ErrorMessage::EInexactMayOverwriteIndexer(box EInexactMayOverwriteIndexerData {
            use_op,
            ..
        }) => util(use_op),
        ErrorMessage::EImplicitInstantiationUnderconstrainedError(
            box EImplicitInstantiationUnderconstrainedErrorData { use_op, .. },
        ) => util(use_op),
        _ => nope,
    }
}

impl<L: Dupe + PartialOrd + Ord + PartialEq + Eq> ErrorMessage<L> {
    /// Not all messages (i.e. those whose locations are based on use_ops) have locations that can be
    /// determined while locations are abstract. We just return None in this case.
    pub fn loc_of_msg(&self) -> Option<L>
    where
        L: Clone,
    {
        match self {
            Self::EAnyValueUsedAsType { value, .. } | Self::EValueUsedAsType { value, .. } => {
                Some(value.loc.dupe())
            }

            Self::ENonStrictEqualityComparison(box ENonStrictEqualityComparisonData {
                lower,
                ..
            }) => Some(lower.loc.dupe()),

            Self::ETooFewTypeArgs(box ETooFewTypeArgsData { reason_tapp, .. })
            | Self::ETooManyTypeArgs(box ETooManyTypeArgsData { reason_tapp, .. }) => {
                Some(reason_tapp.loc.dupe())
            }

            Self::EComparison(box EComparisonData { r1, loc_opt, .. }) => {
                loc_opt.as_ref().or(Some(&r1.loc)).map(|l| l.dupe())
            }

            Self::EUnsupportedSetProto(loc)
            | Self::ERecursionLimit(loc)
            | Self::EInvalidExactObjectIntersection(box EInvalidExactObjectIntersectionData {
                loc,
                ..
            })
            | Self::ENestedComponent(loc)
            | Self::ENestedHook(loc)
            | Self::EPolarityMismatch(box EPolarityMismatchData { loc, .. })
            | Self::ENoNamedExport(box (loc, _, _, _))
            | Self::EOnlyDefaultExport(box (loc, _, _))
            | Self::ENoDefaultExport(box (loc, _, _))
            | Self::EImportTypeAsValue(box (loc, _))
            | Self::EImportTypeAsTypeof(box (loc, _))
            | Self::EExportValueAsType(box (loc, _))
            | Self::EImportValueAsType(box (loc, _)) => Some(loc.dupe()),

            Self::EUnsupportedImplements(type_) => Some(type_.loc.dupe()),

            Self::EMissingLocalAnnotation { loc, .. } => Some(loc.dupe()),

            Self::EMissingThisAnnotation(loc) => Some(loc.dupe()),

            Self::EMissingParameterAnnotation { loc, .. } => Some(loc.dupe()),

            Self::ESketchyNumberLint(_, value) => Some(value.loc.dupe()),

            Self::EInvalidExtends(box (loc, _)) => Some(loc.dupe()),

            Self::EReactRefInRender { usage, .. } => Some(usage.loc.dupe()),

            Self::EComponentMissingReturn(reason) => Some(reason.loc.dupe()),

            Self::ETypeParamConstInvalidPosition(reason) => Some(reason.0.dupe()),

            Self::EObjectComputedPropertyAssign(box (property, _, _)) => Some(property.loc.dupe()),

            Self::EObjectComputedPropertyAccess(box EObjectComputedPropertyAccessData {
                property,
                ..
            }) => Some(property.loc.dupe()),

            Self::EUnsupportedExact(box EUnsupportedExactData { value_loc, .. }) => {
                Some(value_loc.dupe())
            }

            Self::EArithmeticOperand(box EArithmeticOperandData { loc, .. }) => Some(loc.dupe()),

            Self::EForInRHS(box EForInRHSData { loc, .. }) => Some(loc.dupe()),

            Self::EInstanceofRHS(box EInstanceofRHSData { loc, .. }) => Some(loc.dupe()),

            Self::EBinaryInLHS(box EBinaryInLHSData { loc, .. }) => Some(loc.dupe()),

            Self::EBinaryInRHS(box EBinaryInRHSData { loc, .. }) => Some(loc.dupe()),

            Self::EInvalidConstructor(box EInvalidConstructorData { loc, .. }) => Some(loc.dupe()),

            Self::EObjectComputedPropertyPotentialOverwrite(
                box EObjectComputedPropertyPotentialOverwriteData { key_loc, .. },
            ) => Some(key_loc.dupe()),

            Self::EEnumError(
                EnumErrorKind::EnumAllMembersAlreadyChecked(box EnumAllMembersAlreadyCheckedData {
                    loc: key_loc,
                    ..
                })
                | EnumErrorKind::EnumMemberAlreadyChecked(box EnumMemberAlreadyCheckedData {
                    case_test_loc: key_loc,
                    ..
                })
                | EnumErrorKind::EnumInvalidCheck(box EnumInvalidCheckData { loc: key_loc, .. })
                | EnumErrorKind::EnumInvalidMemberName(box EnumInvalidMemberNameData {
                    loc: key_loc,
                    ..
                })
                | EnumErrorKind::EnumNonIdentifierMemberName(box EnumNonIdentifierMemberNameData {
                    loc: key_loc,
                    ..
                }),
            ) => Some(key_loc.dupe()),

            Self::EEnumError(
                EnumErrorKind::EnumNotAllChecked(box EnumNotAllCheckedData { loc, .. })
                | EnumErrorKind::EnumUnknownNotChecked(box EnumUnknownNotCheckedData { loc, .. })
                | EnumErrorKind::EnumInvalidAbstractUse(box EnumInvalidAbstractUseData {
                    loc, ..
                })
                | EnumErrorKind::EnumMemberUsedAsType(box EnumMemberUsedAsTypeData { loc, .. })
                | EnumErrorKind::EnumInvalidMemberAccess(box EnumInvalidMemberAccessData {
                    member_loc: loc,
                    ..
                })
                | EnumErrorKind::EnumInvalidObjectUtilType(box EnumInvalidObjectUtilTypeData {
                    operation_loc: loc,
                    ..
                }),
            ) => Some(loc.dupe()),

            Self::EEnumError(EnumErrorKind::EnumNotIterable(box EnumNotIterableData {
                enum_: EnumReferenceData { loc, .. },
            })) => Some(loc.dupe()),

            Self::EEnumError(EnumErrorKind::EnumNotIterableForIn(
                box EnumNotIterableForInData { loc, .. },
            ))
            | Self::EEnumError(EnumErrorKind::EnumInvalidObjectFunction(
                box EnumInvalidObjectFunctionData {
                    operation_loc: loc, ..
                },
            )) => Some(loc.dupe()),

            Self::ETupleRequiredAfterOptional(box ETupleRequiredAfterOptionalData {
                tuple_loc: loc,
                ..
            }) => Some(loc.dupe()),

            Self::EInvalidDeclaration(box EInvalidDeclarationData {
                declaration_loc, ..
            }) => Some(declaration_loc.dupe()),

            Self::ETypeGuardInvalidParameter(box ETypeGuardInvalidParameterData {
                type_guard: TypeGuardParameterData { loc, .. },
                ..
            })
            | Self::ETypeGuardParamUnbound(TypeGuardParameterData { loc, .. })
            | Self::ETypeGuardThisParam(loc) => Some(loc.dupe()),

            Self::EBigIntRShift3(box ErrorTypeReferenceData { loc, .. }) => Some(loc.dupe()),

            Self::EBigIntNumCoerce(box ErrorTypeReferenceData { loc, .. }) => Some(loc.dupe()),

            Self::ERecursiveDefinition(box ERecursiveDefinitionData { definition, .. }) => {
                Some(definition.loc.dupe())
            }

            Self::EInvalidBinaryArith(box EInvalidBinaryArithData { loc, .. })
            | Self::ETupleInvalidTypeSpread(box ETupleInvalidTypeSpreadData {
                spread_loc: loc,
                ..
            })
            | Self::ETupleElementAfterInexactSpread(loc)
            | Self::ETupleElementAfterRestSpread(loc)
            | Self::ETypeGuardFunctionInvalidWrites(box ETypeGuardFunctionInvalidWritesData {
                loc,
                ..
            })
            | Self::EAssertionFunctionFalsyAtExit(box EAssertionFunctionFalsyAtExitData {
                loc,
                ..
            })
            | Self::EUnrecognizedAssertionCall(box EUnrecognizedAssertionCallData {
                loc, ..
            })
            | Self::EIllegalAssertOperator(box EIllegalAssertOperatorData {
                op_loc: loc, ..
            }) => Some(loc.dupe()),

            Self::ENegativeTypeGuardConsistency(box ENegativeTypeGuardConsistencyData {
                return_reason,
                ..
            }) => Some(return_reason.loc.dupe()),

            Self::ETypeGuardFunctionParamHavoced(box ETypeGuardFunctionParamHavocedData {
                type_guard,
                ..
            }) => Some(type_guard.loc.dupe()),

            Self::EDefinitionCycle(dependencies) => Some(dependencies.first().0.loc.dupe()),

            Self::EExponentialSpread(box EExponentialSpreadData {
                operand1, operand2, ..
            }) => {
                let union = match (&operand1.second, &operand2.second) {
                    (None, _) => &operand1.first,
                    (_, None) => &operand2.first,
                    (Some(t), _) => t,
                };
                Some(union.loc.dupe())
            }

            Self::EBindingError(box (_, loc, _, _)) => Some(loc.dupe()),

            Self::EComponentCase(loc)
            | Self::EComponentMissingBody(loc)
            | Self::EComponentBodyInAmbientContext(loc)
            | Self::EDeclareComponentInvalidParam { loc, .. }
            | Self::EHookNaming(loc)
            | Self::EIncorrectTypeWithReplacement(box EIncorrectTypeWithReplacementData {
                loc,
                ..
            })
            | Self::EUnsafeGettersSetters(loc)
            | Self::EUnsafeObjectAssign(loc)
            | Self::ETSSyntax(box ETSSyntaxData { kind: _, loc })
            | Self::EAbstractClass(box EAbstractClassData { kind: _, loc })
            | Self::EOverride(box EOverrideData { kind: _, loc })
            | Self::EVarianceKeyword(box EVarianceKeywordData { kind: _, loc }) => Some(loc.dupe()),

            Self::EInvalidTypeof(box (loc, _)) => Some(loc.dupe()),
            Self::EReactIntrinsicOverlap(box EReactIntrinsicOverlapData { def, .. }) => {
                Some(def.dupe())
            }
            Self::EInvalidPrototype(box EInvalidPrototypeData { loc, .. })
            | Self::EUntypedTypeImport(box (loc, _))
            | Self::EUntypedImport(box (loc, _))
            | Self::EInvalidInfer(loc)
            | Self::EConstantCondition(box EConstantConditionData { loc, .. })
            | Self::EInvalidReactCreateElement(box EInvalidReactCreateElementData {
                create_element_loc: loc,
                ..
            })
            | Self::EInvalidThisArg(box EInvalidThisArgData { loc, .. })
            | Self::ENonstrictImport(loc)
            | Self::EUnclearType(loc)
            | Self::EDeprecatedBool(loc)
            | Self::EInternalType(loc, _)
            | Self::EUnnecessaryOptionalChain(box (loc, _))
            | Self::EUnnecessaryInvariant(box EUnnecessaryInvariantData { loc, .. })
            | Self::EUnnecessaryDeclareTypeOnlyExport(loc)
            | Self::EUnusedSuppression(loc)
            | Self::ECodelessSuppression(loc)
            | Self::EDocblockError(box (loc, _))
            | Self::EInvalidComponentRestParam(loc)
            | Self::EAmbiguousObjectType(loc)
            | Self::EParseError(box (loc, _))
            | Self::EInvalidLHSInAssignment(loc)
            | Self::EUnreachable(loc)
            | Self::ECannotDelete(box ECannotDeleteData { loc, .. })
            | Self::EBadExportContext(box (_, loc))
            | Self::EBadExportPosition(loc)
            | Self::EBadDefaultImportAccess(box (loc, _))
            | Self::EBadDefaultImportDestructuring(loc)
            | Self::EInvalidImportStarUse(box (loc, _))
            | Self::ENonConstVarExport(box (loc, _))
            | Self::EThisInExportedFunction(loc)
            | Self::EMixedImportAndRequire(box (loc, _))
            | Self::EUnsupportedVarianceAnnotation(box (loc, _))
            | Self::EExportRenamedDefault(box EExportRenamedDefaultData { loc, .. })
            | Self::EIndeterminateModuleType(loc)
            | Self::EEnumError(
                EnumErrorKind::EnumsNotEnabled(loc) | EnumErrorKind::EnumConstNotSupported(loc),
            )
            | Self::EUninitializedInstanceProperty(loc, _)
            | Self::EUninitializedProperty(box (loc, _, _))
            | Self::EUseArrayLiteral(loc)
            | Self::EUnsupportedSyntax(box (loc, _))
            | Self::EInternal(box (loc, _))
            | Self::EUnsupportedKeyInObject { loc, .. }
            | Self::EUnsupportedComputedKeyInClass(loc)
            | Self::EAmbiguousNumericKeyWithVariance(loc)
            | Self::EHookRuleViolation(box EHookRuleViolationData { call_loc: loc, .. })
            | Self::EExportsAnnot(loc)
            | Self::EUnexpectedThisType(loc)
            | Self::EAssignConstLikeBinding(box EAssignConstLikeBindingData { loc, .. })
            | Self::EMalformedCode(loc)
            | Self::EObjectThisSuperReference(box EObjectThisSuperReferenceData { loc, .. })
            | Self::EComponentThisReference(box EComponentThisReferenceData {
                this_loc: loc,
                ..
            })
            | Self::EInvalidGraphQL(box (loc, _))
            | Self::EAnnotationInference(box EAnnotationInferenceData { loc, .. })
            | Self::EInvalidCatchParameterAnnotation { loc, .. }
            | Self::EInvalidMappedType { loc, .. }
            | Self::EInvalidTemplateLiteralType { loc, .. }
            | Self::EReferenceInAnnotation(box (loc, _, _))
            | Self::EReferenceInDefault(box (_, _, loc))
            | Self::EDuplicateComponentProp(box EDuplicateComponentPropData {
                spread: loc, ..
            })
            | Self::EKeySpreadProp(box EKeySpreadPropData { spread: loc, .. })
            | Self::ETypeGuardIncompatibleWithFunctionKind(
                box ETypeGuardIncompatibleWithFunctionKindData { loc, .. },
            )
            | Self::EMissingPlatformSupportWithAvailablePlatforms(
                box EMissingPlatformSupportWithAvailablePlatformsData { loc, .. },
            )
            | Self::EUnionOptimization(box EUnionOptimizationData { loc, .. })
            | Self::EUnionOptimizationOnNonUnion(box EUnionOptimizationOnNonUnionData {
                loc,
                ..
            })
            | Self::ELintSetting(box (loc, _))
            | Self::ETypeParamArity(loc, _)
            | Self::ESketchyNullLint(box ESketchyNullLintData { loc, .. }) => Some(loc.dupe()),

            Self::EUnionPartialOptimizationNonUniqueKey(
                box EUnionPartialOptimizationNonUniqueKeyData { loc, .. },
            ) => Some(loc.dupe()),

            Self::ETrivialRecursiveDefinition(loc) => Some(loc.dupe()),

            Self::ETrivialRecursiveTypeParameter(data) => Some(data.0.dupe()),

            Self::EClassTypeParamInStatic(data) => Some(data.0.dupe()),

            Self::ECallTypeArity(box ECallTypeArityData { call_loc, .. }) => Some(call_loc.dupe()),
            Self::EMissingTypeArgs(box EMissingTypeArgsData { loc, .. }) => Some(loc.dupe()),
            Self::EInvalidRendersTypeArgument(box EInvalidRendersTypeArgumentData {
                loc, ..
            }) => Some(loc.dupe()),
            Self::EInvalidTypeCastSyntax { loc, .. } => Some(loc.dupe()),

            Self::ESignatureBindingValidation(e) => match e {
                BindingValidation::ModuleOverride {
                    override_binding_loc,
                    ..
                }
                | BindingValidation::NameOverride {
                    override_binding_loc,
                    ..
                } => Some(override_binding_loc.dupe()),
                BindingValidation::NamespacedNameAlreadyBound {
                    invalid_binding_loc,
                    ..
                } => Some(invalid_binding_loc.dupe()),
                BindingValidation::InterfaceMergePropertyConflict {
                    existing_binding_loc,
                    ..
                } => Some(existing_binding_loc.dupe()),
                BindingValidation::InterfaceMergeTparamMismatch {
                    existing_binding_loc,
                    ..
                } => Some(existing_binding_loc.dupe()),
            },

            Self::ESignatureVerification(sve) => match sve {
                SignatureError::ExpectedAnnotation(loc, _)
                | SignatureError::UnexpectedObjectKey(loc, _)
                | SignatureError::UnexpectedArraySpread(loc, _)
                | SignatureError::UnexpectedArrayHole(loc)
                | SignatureError::EmptyArray(loc)
                | SignatureError::EmptyObject(loc)
                | SignatureError::UnexpectedExpression(loc, _) => Some(loc.dupe()),
            },

            Self::EDuplicateModuleProvider(box EDuplicateModuleProviderData {
                conflict, ..
            }) => Some(conflict.dupe()),
            Self::EEnumError(
                EnumErrorKind::EnumModification(box EnumModificationData { loc, .. })
                | EnumErrorKind::EnumMemberDuplicateValue(box EnumMemberDuplicateValueData {
                    loc,
                    ..
                })
                | EnumErrorKind::EnumDuplicateMemberName(box EnumDuplicateMemberNameData {
                    loc, ..
                })
                | EnumErrorKind::EnumInconsistentMemberValues(box EnumInconsistentMemberValuesData {
                    loc,
                    ..
                })
                | EnumErrorKind::EnumInvalidMemberInitializer(box EnumInvalidMemberInitializerData {
                    loc,
                    ..
                })
                | EnumErrorKind::EnumBooleanMemberNotInitialized(
                    box EnumBooleanMemberNotInitializedData { loc, .. },
                )
                | EnumErrorKind::EnumNumberMemberNotInitialized(
                    box EnumNumberMemberNotInitializedData { loc, .. },
                )
                | EnumErrorKind::EnumBigIntMemberNotInitialized(
                    box EnumBigIntMemberNotInitializedData { loc, .. },
                )
                | EnumErrorKind::EnumStringMemberInconsistentlyInitialized(
                    box EnumStringMemberInconsistentlyInitializedData { loc, .. },
                )
                | EnumErrorKind::TSEnumInvalidMember(box TSEnumInvalidMemberData { loc, .. })
                | EnumErrorKind::TSEnumInvalidSyntax(box TSEnumInvalidSyntaxData { loc, .. }),
            )
            | Self::EBuiltinNameLookupFailed(box EBuiltinNameLookupFailedData { loc, .. })
            | Self::EUmdGlobalValueInModule(box EUmdGlobalValueInModuleData { loc, .. })
            | Self::EBuiltinModuleLookupFailed(box EBuiltinModuleLookupFailedData {
                loc, ..
            })
            | Self::EExpectedModuleLookupFailed(box EExpectedModuleLookupFailedData {
                loc, ..
            })
            | Self::ECannotImportGlobalLibdef(box ECannotImportGlobalLibdefData { loc, .. })
            | Self::EPlatformSpecificImplementationModuleLookupFailed(
                box EPlatformSpecificImplementationModuleLookupFailedData { loc, .. },
            )
            | Self::EDuplicateClassMember(box EDuplicateClassMemberData { loc, .. })
            | Self::EEmptyArrayNoProvider { loc }
            | Self::EUnusedPromise { loc, .. } => Some(loc.dupe()),

            Self::ECannotCallReactComponent { component } => Some(component.loc.dupe()),

            Self::EMatchError(e) => match e {
                MatchErrorKind::MatchNotExhaustive(box MatchNotExhaustiveData { loc, .. })
                | MatchErrorKind::MatchNonExhaustiveObjectPattern(
                    box MatchNonExhaustiveObjectPatternData { loc, .. },
                )
                | MatchErrorKind::MatchNonExplicitEnumCheck(box MatchNonExplicitEnumCheckData {
                    loc,
                    ..
                })
                | MatchErrorKind::MatchInvalidBindingKind { loc, .. }
                | MatchErrorKind::MatchInvalidObjectPropertyLiteral { loc, .. }
                | MatchErrorKind::MatchInvalidUnaryZero { loc }
                | MatchErrorKind::MatchInvalidUnaryPlusBigInt { loc }
                | MatchErrorKind::MatchDuplicateObjectProperty(
                    box MatchDuplicateObjectPropertyData { loc, .. },
                )
                | MatchErrorKind::MatchBindingInOrPattern { loc }
                | MatchErrorKind::MatchInvalidAsPattern { loc }
                | MatchErrorKind::MatchInvalidPatternReference(
                    box MatchInvalidPatternReferenceData { loc, .. },
                )
                | MatchErrorKind::MatchInvalidObjectShorthand(
                    box MatchInvalidObjectShorthandData { loc, .. },
                )
                | MatchErrorKind::MatchStatementInvalidBody { loc }
                | MatchErrorKind::MatchInvalidCaseSyntax(box MatchInvalidCaseSyntaxData {
                    loc,
                    ..
                })
                | MatchErrorKind::MatchInvalidIdentOrMemberPattern(
                    box MatchInvalidIdentOrMemberPatternData { loc, .. },
                ) => Some(loc.dupe()),
                MatchErrorKind::MatchInvalidWildcardSyntax(loc)
                | MatchErrorKind::MatchInvalidInstancePattern(loc)
                | MatchErrorKind::MatchInvalidGuardedWildcard(loc) => Some(loc.dupe()),
                MatchErrorKind::MatchUnusedPattern(box MatchUnusedPatternData {
                    pattern, ..
                }) => Some(pattern.loc.dupe()),
            },

            Self::ERecordError(e) => match e {
                RecordErrorKind::RecordBannedTypeUtil { record, .. } => Some(record.loc.dupe()),
                RecordErrorKind::RecordInvalidName { loc, .. }
                | RecordErrorKind::RecordInvalidNew { loc, .. }
                | RecordErrorKind::RecordDeclarationInvalidSyntax { loc, .. } => Some(loc.dupe()),
            },

            Self::EUndocumentedFeature { loc } => Some(loc.dupe()),
            Self::EDevOnlyRefinedLocInfo(box EDevOnlyRefinedLocInfoData {
                refined_loc, ..
            }) => Some(refined_loc.dupe()),
            Self::EDevOnlyInvalidatedRefinementInfo(
                box EDevOnlyInvalidatedRefinementInfoData { read_loc, .. },
            ) => Some(read_loc.dupe()),

            Self::EUnableToSpread(box EUnableToSpreadData { .. })
            | Self::ECannotSpreadInterface(box ECannotSpreadInterfaceData { .. })
            | Self::ECannotSpreadIndexerOnRight(box ECannotSpreadIndexerOnRightData { .. })
            | Self::EInexactMayOverwriteIndexer(box EInexactMayOverwriteIndexerData { .. })
            | Self::EFunctionCallExtraArg { .. }
            | Self::ENotAReactComponent(..)
            | Self::EIncompatibleTypesWithUseOp(..)
            | Self::EInvariantSubtypingWithUseOp(..)
            | Self::EEnumError(EnumErrorKind::EnumIncompatible(
                box EIncompatibleTypesWithUseOpData { .. },
            ))
            | Self::EInvalidObjectKit(box EInvalidObjectKitData { .. })
            | Self::EIncompatibleWithExact { .. }
            | Self::EFunctionIncompatibleWithIndexer { .. }
            | Self::EUnionSpeculationFailed(..)
            | Self::ETupleUnsafeWrite { .. }
            | Self::EROArrayWrite { .. }
            | Self::ETupleElementNotReadable(box ETupleElementNotReadableData { .. })
            | Self::ETupleElementNotWritable(box ETupleElementNotWritableData { .. })
            | Self::ETupleElementPolarityMismatch(box ETupleElementPolarityMismatchData {
                ..
            })
            | Self::ETupleOutOfBounds(box ETupleOutOfBoundsData { .. })
            | Self::ETupleNonIntegerIndex(box ETupleNonIntegerIndexData { .. })
            | Self::ENonLitArrayToTuple { .. }
            | Self::ETupleArityMismatch(box ETupleArityMismatchData { .. })
            | Self::EPrivateLookupFailed { .. }
            | Self::EPropPolarityMismatch(box EPropPolarityMismatchData { .. })
            | Self::EPropNotReadable(box EPropNotReadableData { .. })
            | Self::EPropNotWritable(box EPropNotWritableData { .. })
            | Self::EPropNotFoundInLookup(box EPropNotFoundInLookupData { .. })
            | Self::EPropNotFoundInSubtyping(box EPropNotFoundInSubtypingData { .. })
            | Self::EConstructSignatureMissingInSubtyping(
                box EConstructSignatureMissingInSubtypingData { .. },
            )
            | Self::EPropsNotFoundInSubtyping(box EPropsNotFoundInSubtypingData { .. })
            | Self::EPropsNotFoundInInvariantSubtyping(..)
            | Self::EPropsExtraAgainstExactObject(box EPropsExtraAgainstExactObjectData {
                ..
            })
            | Self::EIndexerCheckFailed(box EIndexerCheckFailedData { .. })
            | Self::EExpectedBooleanLit(box EExpectedBooleanLitData { .. })
            | Self::EExpectedNumberLit(box EExpectedNumberLitData { .. })
            | Self::EExpectedStringLit(box EExpectedStringLitData { .. })
            | Self::EExpectedBigIntLit(box EExpectedBigIntLitData { .. })
            | Self::EIncompatibleProp(box EIncompatiblePropData { .. })
            | Self::EIncompatibleType(box EIncompatibleTypeData { .. })
            | Self::EIncompatibleSpeculation(..)
            | Self::EHookIncompatible(box EHookIncompatibleData { .. })
            | Self::EHookUniqueIncompatible(box EHookUniqueIncompatibleData { .. })
            | Self::EImplicitInstantiationUnderconstrainedError(
                box EImplicitInstantiationUnderconstrainedErrorData { .. },
            )
            | Self::EClassToObject(box EClassToObjectData { .. })
            | Self::EPrimitiveAsInterface(box EPrimitiveAsInterfaceData { .. })
            | Self::ETypeGuardFuncIncompatibility { .. }
            | Self::ETypeGuardIndexMismatch { .. }
            | Self::ETypeGuardImpliesMismatch { .. }
            | Self::ETypeParamConstIncompatibility(box ETypeParamConstIncompatibilityData {
                ..
            }) => None,
        }
    }

    pub fn kind_of_msg(&self) -> ErrorKind {
        use ErrorKind::*;
        use LintKind::*;

        match self {
            // Lint errors
            ErrorMessage::EUntypedTypeImport(box (_, _)) => LintError(UntypedTypeImport),
            ErrorMessage::EUntypedImport(box (_, _)) => LintError(UntypedImport),
            ErrorMessage::ENonstrictImport(_) => LintError(NonstrictImport),
            ErrorMessage::EInternalType(_, _) => LintError(InternalType),
            ErrorMessage::EUnclearType(_) => LintError(UnclearType),
            ErrorMessage::EUnsafeGettersSetters(_) => LintError(UnsafeGettersSetters),
            ErrorMessage::EUnsafeObjectAssign(_) => LintError(UnsafeObjectAssign),
            ErrorMessage::ESketchyNullLint(box ESketchyNullLintData { kind, .. }) => {
                LintError(SketchyNull(*kind))
            }
            ErrorMessage::ESketchyNumberLint(kind, _) => LintError(SketchyNumber(*kind)),
            ErrorMessage::EUnnecessaryOptionalChain(box (_, _)) => {
                LintError(UnnecessaryOptionalChain)
            }
            ErrorMessage::EUnnecessaryInvariant(box EUnnecessaryInvariantData { .. }) => {
                LintError(UnnecessaryInvariant)
            }
            ErrorMessage::EAmbiguousObjectType(_) => LintError(AmbiguousObjectType),
            ErrorMessage::EEnumError(EnumErrorKind::EnumNotAllChecked(
                box EnumNotAllCheckedData {
                    default_case_loc: Some(_),
                    ..
                },
            )) => LintError(RequireExplicitEnumSwitchCases),
            ErrorMessage::EMatchError(MatchErrorKind::MatchNonExplicitEnumCheck(
                box MatchNonExplicitEnumCheckData { .. },
            )) => LintError(RequireExplicitEnumChecks),
            ErrorMessage::EUninitializedInstanceProperty(_, _) => {
                LintError(UninitializedInstanceProperty)
            }
            ErrorMessage::EBadDefaultImportAccess(box (_, _)) => LintError(DefaultImportAccess),
            ErrorMessage::EBadDefaultImportDestructuring(_) => LintError(DefaultImportAccess),
            ErrorMessage::EInvalidImportStarUse(box (_, _)) => LintError(InvalidImportStarUse),
            ErrorMessage::EInvalidThisArg(box EInvalidThisArgData { .. }) => {
                LintError(InvalidThisArg)
            }
            ErrorMessage::ENonConstVarExport(box (_, _)) => LintError(NonConstVarExport),
            ErrorMessage::EThisInExportedFunction(_) => LintError(ThisInExportedFunction),
            ErrorMessage::EMixedImportAndRequire(box (_, _)) => LintError(MixedImportAndRequire),
            ErrorMessage::EExportRenamedDefault(box EExportRenamedDefaultData { .. }) => {
                LintError(ExportRenamedDefault)
            }
            ErrorMessage::EUnusedPromise { .. } => LintError(UnusedPromise),
            ErrorMessage::EReactIntrinsicOverlap(box EReactIntrinsicOverlapData { .. }) => {
                LintError(ReactIntrinsicOverlap)
            }
            ErrorMessage::ENestedComponent(_) => LintError(NestedComponent),
            ErrorMessage::ENestedHook(_) => LintError(NestedHook),
            ErrorMessage::ESignatureBindingValidation(BindingValidation::ModuleOverride {
                ..
            })
            | ErrorMessage::ESignatureBindingValidation(BindingValidation::NameOverride {
                ..
            }) => LintError(LibdefOverride),

            // Infer warnings
            ErrorMessage::EBadExportPosition(_) | ErrorMessage::EBadExportContext(box (_, _)) => {
                InferWarning(InferWarningKind::ExportKind)
            }
            ErrorMessage::EEnumError(
                EnumErrorKind::EnumsNotEnabled(_) | EnumErrorKind::EnumConstNotSupported(_),
            )
            | ErrorMessage::EIndeterminateModuleType(_)
            | ErrorMessage::EUnreachable(_)
            | ErrorMessage::EInvalidTypeof(box (_, _)) => InferWarning(InferWarningKind::OtherKind),

            // Other error kinds
            ErrorMessage::EInternal(box (_, _)) => InternalError,
            ErrorMessage::ERecursionLimit(_) => RecursionLimitError,
            ErrorMessage::EDuplicateModuleProvider(box EDuplicateModuleProviderData { .. }) => {
                DuplicateProviderError
            }
            ErrorMessage::EParseError(box (_, _)) => ParseError,
            ErrorMessage::EDocblockError(box (_, _)) | ErrorMessage::ELintSetting(box (_, _)) => {
                PseudoParseError
            }

            // Default case: all other error messages are InferError
            _ => InferError,
        }
    }
}

pub fn polarity_explanation(pol: Polarity, pol2: Polarity) -> &'static str {
    match (pol, pol2) {
        (Polarity::Positive, _) => "read-only",
        (Polarity::Negative, _) => "write-only",
        (Polarity::Neutral, Polarity::Negative) => "readable",
        (Polarity::Neutral, Polarity::Positive) => "writable",
        (Polarity::Neutral, Polarity::Neutral) => panic!("unreachable"),
    }
}

pub fn mk_prop_message<L>(prop: Option<&str>) -> Vec<MessageFeature<L>> {
    match prop {
        None => vec![text(
            "an index signature declaring the expected key / value type",
        )],
        Some("$call") => {
            vec![text(
                "a call signature declaring the expected parameter / return type",
            )]
        }
        Some(prop) => vec![text("property "), code(prop)],
    }
}

pub fn string_of_internal_error(error: &InternalError) -> FlowSmolStr {
    match error {
        InternalError::ReadOfUnreachedTvar(k) => format!(
            "read of {:?} entry which has not been prepared for typechecking",
            k
        )
        .into(),
        InternalError::ForcedReadOfUnderResolutionTvar(k) => format!(
            "forced read of {:?} entry from component is not yet FullyResolved",
            k
        )
        .into(),
        InternalError::MethodNotAFunction => "expected function type".into(),
        InternalError::OptionalMethod => "optional methods are not supported".into(),
        InternalError::PropertyDescriptorPropertyCannotBeRead => {
            "unexpected property in properties object".into()
        }
        InternalError::ForInLHS => "unexpected LHS in for...in".into(),
        InternalError::ForOfLHS => "unexpected LHS in for...of".into(),
        InternalError::PropRefComputedOpen => {
            "unexpected open computed property element type".into()
        }
        InternalError::PropRefComputedLiteral => {
            "unexpected literal computed property element type".into()
        }
        InternalError::RestParameterNotIdentifierPattern => {
            "unexpected rest parameter, expected an identifier pattern".into()
        }
        InternalError::InterfaceTypeSpread => "unexpected spread property in interface".into(),
        InternalError::DebugThrow => "debug throw".into(),
        InternalError::ParseJobException(exc) => format!("uncaught exception: {}", exc).into(),
        InternalError::CheckTimeout(s) => format!("check job timed out after {} seconds", s).into(),
        InternalError::UnexpectedAnnotationInference(s) => {
            format!("unexpected {} in annotation inference", s).into()
        }
        InternalError::UnexpectedInlineInterfaceType => {
            "expected freshly-built inline interface to be InstanceT or ThisInstanceT".into()
        }
        InternalError::MissingSwitchExhaustiveCheck => "missing exhaustive check entry".into(),
        InternalError::MissingEnvRead(l) => format!("missing env entry for read at {:?}", l).into(),
        InternalError::MissingEnvWrite(loc) => {
            format!("expected env entry for write location{:?}", loc).into()
        }
        InternalError::EnvInvariant(failure) => match failure {
            EnvInvariantFailure::NameDefOrderingFailure {
                all,
                roots,
                missing_roots,
            } => {
                let all_str = all
                    .iter()
                    .map(|l| format!("{:?}", l))
                    .collect::<Vec<_>>()
                    .join(",");
                let roots_str = roots
                    .iter()
                    .map(|l| format!("{:?}", l))
                    .collect::<Vec<_>>()
                    .join(",");
                let missing_roots_str = missing_roots
                    .iter()
                    .map(|l| format!("{:?}", l))
                    .collect::<Vec<_>>()
                    .join(",");
                format!(
                    "Please report this error to the Flow team: Env_api tarjan failure, all: {{ {} }} roots: {{ {} }} missing_roots: {{ {} }}",
                    all_str, roots_str, missing_roots_str
                )
                .into()
            }
            EnvInvariantFailure::Impossible(s) => format!(
                "Internal state should be impossible, please report this to the Flow team: {}",
                s
            )
            .into(),
            EnvInvariantFailure::ASTStructureOverride(s) => format!(
                "AST visitor issue, please report this to the Flow team: {}",
                s
            )
            .into(),
            EnvInvariantFailure::NameDefGraphMismatch => {
                "EnvMap.find missed, please report this to the Flow team".into()
            }
            EnvInvariantFailure::MissingEnvEntry(x) => format!(
                "Did not find {} in name_resolver environment, please report this to the Flow team",
                x
            )
            .into(),
        },
        InternalError::ImplicitInstantiationInvariant(s) => format!(
            "Implicit instantiation issue, please report this to the Flow team: {}",
            s
        )
        .into(),
        InternalError::UnresolvedTvarInConditionalType => {
            "unresolved tvar in conditional type outside implicit instantiation".into()
        }
        InternalError::MethodBivariantInvariant(s) => format!(
            "Method bivariant subtyping issue, please report this to the Flow team: {}",
            s
        )
        .into(),
        InternalError::WorkerCanceled => "check job was canceled".into(),
    }
}

pub fn type_casting_examples() -> (&'static str, &'static str) {
    let example_as = "<expr> as <type>";
    let example_colon = "(<expr>: <type>)";
    (example_as, example_colon)
}

/// Friendly messages are created differently based on the specific error they come from.
/// We collect the ingredients here and pass them to make_error_printable.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct IncompatibleTypeUseData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub loc: L,
    pub upper_kind: UpperKind<L>,
    pub lower_loc: L,
    pub lower_kind: Option<LowerKind>,
    pub lower_desc: ErrorTypeDesc<L>,
    pub use_op: VirtualUseOp<L>,
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
pub struct SpeculationData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub loc: L,
    pub use_op: VirtualUseOp<L>,
    pub branches: Vec<ErrorMessage<L>>,
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
pub struct IncompatibleSubtypingData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub reason_lower: VirtualReason<L>,
    pub reason_upper: VirtualReason<L>,
    pub use_op: VirtualUseOp<L>,
    pub explanation: Option<Explanation<L>>,
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
pub struct IncompatibleInvariantSubtypingData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub sub_component: Option<SubComponentOfInvariantSubtypingError>,
    pub lower_loc: L,
    pub upper_loc: L,
    pub lower_desc: ErrorTypeDesc<L>,
    pub upper_desc: ErrorTypeDesc<L>,
    pub use_op: VirtualUseOp<L>,
    pub explanation: Option<Explanation<L>>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PrivatePropMissingInLookupData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub loc: L,
    pub object: MessageTypeReferenceData<L>,
    pub prop: FlowSmolStr,
    pub use_op: VirtualUseOp<L>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PropMissingInSubtypingData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub prop: Option<FlowSmolStr>,
    pub suggestion: Option<FlowSmolStr>,
    pub loc: L,
    pub lower: MessageTypeReferenceData<L>,
    pub upper: MessageTypeReferenceData<L>,
    pub use_op: VirtualUseOp<L>,
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
pub struct PropsMissingInSubtypingData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub props: Vec1<FlowSmolStr>,
    pub loc: L,
    pub lower: MessageTypeReferenceData<L>,
    pub upper: MessageTypeReferenceData<L>,
    pub use_op: VirtualUseOp<L>,
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
pub struct PropsMissingInInvariantSubtypingData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub props: Vec1<FlowSmolStr>,
    pub loc: L,
    pub lower_obj_loc: L,
    pub upper_obj_loc: L,
    pub lower: MessageTypeReferenceData<L>,
    pub upper: MessageTypeReferenceData<L>,
    pub use_op: VirtualUseOp<L>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PropsExtraAgainstExactObjectData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub props: Vec1<FlowSmolStr>,
    pub loc: L,
    pub lower: MessageTypeReferenceData<L>,
    pub upper: MessageTypeReferenceData<L>,
    pub upper_is_record: bool,
    pub use_op: VirtualUseOp<L>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct IndexerCheckFailedData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub prop: FlowSmolStr,
    pub lower: MessageTypeReferenceData<L>,
    pub upper: MessageTypeReferenceData<L>,
    pub indexer: MessageTypeReferenceData<L>,
    pub use_op: VirtualUseOp<L>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UseOpData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub loc: L,
    pub message: Message<L>,
    pub use_op: VirtualUseOp<L>,
    pub explanation: Option<Explanation<L>>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct IncompatibleTypesWithExampleData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub loc: L,
    pub lower_loc: L,
    pub upper_loc: L,
    pub lower_desc: ErrorTypeDesc<L>,
    pub upper_desc: ErrorTypeDesc<L>,
    pub use_op: VirtualUseOp<L>,
    pub explanation: Option<Explanation<L>>,
    pub example: Box<ErrorMessage<L>>,
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
pub struct PropPolarityMismatchData<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    pub loc: L,
    pub lower: PropPolarityMismatchObject<MessageTypeReferenceData<L>, L>,
    pub upper: PropPolarityMismatchObject<MessageTypeReferenceData<L>, L>,
    pub props: Vec1<(Option<FlowSmolStr>, Polarity, Polarity)>,
    pub use_op: VirtualUseOp<L>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum FriendlyMessageRecipe<L: Dupe + PartialOrd + Ord + PartialEq + Eq> {
    IncompatibleTypeUse(Box<IncompatibleTypeUseData<L>>),
    Speculation(Box<SpeculationData<L>>),
    IncompatibleSubtyping(Box<IncompatibleSubtypingData<L>>),
    IncompatibleInvariantSubtyping(Box<IncompatibleInvariantSubtypingData<L>>),
    PrivatePropMissingInLookup(Box<PrivatePropMissingInLookupData<L>>),
    PropMissingInSubtyping(Box<PropMissingInSubtypingData<L>>),
    PropsMissingInSubtyping(Box<PropsMissingInSubtypingData<L>>),
    PropsMissingInInvariantSubtyping(Box<PropsMissingInInvariantSubtypingData<L>>),
    PropsExtraAgainstExactObject(Box<PropsExtraAgainstExactObjectData<L>>),
    IndexerCheckFailed(Box<IndexerCheckFailedData<L>>),
    Normal(Message<L>),
    UseOp(Box<UseOpData<L>>),
    IncompatibleTypesWithExample(Box<IncompatibleTypesWithExampleData<L>>),
    PropPolarityMismatch(Box<PropPolarityMismatchData<L>>),
}

pub(crate) fn expect_type_desc<L: Dupe>(type_or_desc: TypeOrTypeDesc<L>) -> ErrorTypeDesc<L> {
    match type_or_desc {
        TypeOrTypeDesc::Type(_) => {
            panic!("At this point, we should no longer have TypeOrTypeDesc::Type")
        }
        TypeOrTypeDesc::TypeDesc(desc) => desc,
    }
}

fn expect_error_type_reference<L: Dupe + PartialOrd + Ord + PartialEq + Eq>(
    type_reference: ErrorTypeReferenceData<L>,
) -> MessageTypeReferenceData<L> {
    MessageTypeReferenceData {
        loc: type_reference.reference_loc,
        desc: expect_type_desc(type_reference.type_desc),
    }
}

impl<L: Dupe + PartialEq + Eq + PartialOrd + Ord> ErrorMessage<L> {
    /// Transform an ErrorMessage into a FriendlyMessageRecipe.
    ///
    /// This function consumes the ErrorMessage because OCaml callers never use
    /// the message after calling this function - they pattern match on the result.
    pub fn friendly_message_of_msg(self) -> FriendlyMessageRecipe<L> {
        use FriendlyMessageRecipe::*;

        match self {
            ErrorMessage::EIncompatibleType(box EIncompatibleTypeData {
                lower_kind,
                lower_def_loc,
                lower_desc,
                upper:
                    IncompatibleUpperData {
                        loc,
                        kind: upper_kind,
                    },
                use_op,
                ..
            }) => IncompatibleTypeUse(Box::new(IncompatibleTypeUseData {
                loc,
                upper_kind,
                lower_loc: lower_def_loc,
                lower_kind,
                lower_desc: expect_type_desc(lower_desc),
                use_op: use_op.unwrap_or(VirtualUseOp::Op(Arc::new(VirtualRootUseOp::UnknownUse))),
            })),

            ErrorMessage::EIncompatibleSpeculation(box EIncompatibleSpeculationData {
                loc,
                use_op,
                branches,
            }) => Speculation(Box::new(SpeculationData {
                loc,
                use_op: use_op.unwrap_or(VirtualUseOp::Op(Arc::new(VirtualRootUseOp::UnknownUse))),
                branches,
            })),

            ErrorMessage::EExportValueAsType(box (_, export_name)) => Normal(
                Message::MessageExportValueAsType(export_name.display_smol_str()),
            ),

            ErrorMessage::EImportValueAsType(box (_, export_name)) => {
                Normal(Message::MessageImportValueAsType(export_name))
            }

            ErrorMessage::EImportTypeAsTypeof(box (_, export_name)) => {
                Normal(Message::MessageImportTypeAsTypeof(export_name))
            }

            ErrorMessage::EImportTypeAsValue(box (_, export_name)) => {
                Normal(Message::MessageImportTypeAsValue(export_name))
            }

            ErrorMessage::EInvalidInfer { .. } => Normal(Message::MessageInvalidInferType),

            ErrorMessage::EInvalidExtends(box (loc, expression)) => Normal(
                Message::MessageCannotUseAsSuperClass(ExpressionReferenceData {
                    loc,
                    kind: ExpressionReferenceKind::Code(expression),
                }),
            ),

            ErrorMessage::EInvalidReactCreateElement(box EInvalidReactCreateElementData {
                react,
                ..
            }) => Normal(Message::MessageInvalidReactCreateElement(Box::new(react))),

            ErrorMessage::EInvalidThisArg(box EInvalidThisArgData {
                loc: _,
                name,
                receiver_expression,
                kind,
            }) => match kind {
                InvalidThisArgKind::MissingReceiver => {
                    Normal(Message::MessageInvalidThisArgMissingReceiver {
                        name,
                        callee_expression: receiver_expression,
                    })
                }
                InvalidThisArgKind::ReceiverMismatch => {
                    Normal(Message::MessageInvalidThisArgReceiverMismatch {
                        name,
                        receiver_expression,
                    })
                }
            },

            ErrorMessage::EAnyValueUsedAsType { reference, value } => {
                Normal(Message::MessageAnyValueUsedAsType {
                    reference,
                    value: expect_error_type_reference(value),
                })
            }

            ErrorMessage::EValueUsedAsType { reference, value } => {
                Normal(Message::MessageValueUsedAsType {
                    reference,
                    value: expect_error_type_reference(value),
                })
            }

            ErrorMessage::EExpectedStringLit(box EExpectedStringLitData {
                lower,
                upper,
                use_op,
            }) => ErrorMessage::EIncompatibleTypesWithUseOp(Box::new(
                EIncompatibleTypesWithUseOpData {
                    lower_loc: lower.loc.dupe(),
                    lower_def_loc: lower.reference_loc,
                    upper_loc: upper.loc.dupe(),
                    upper_def_loc: upper.reference_loc,
                    lower_desc: lower.type_desc,
                    upper_desc: upper.type_desc,
                    use_op,
                    explanation: None,
                    example: None,
                    branches: vec![],
                },
            ))
            .friendly_message_of_msg(),

            ErrorMessage::EExpectedNumberLit(box EExpectedNumberLitData {
                lower,
                upper,
                use_op,
            }) => ErrorMessage::EIncompatibleTypesWithUseOp(Box::new(
                EIncompatibleTypesWithUseOpData {
                    lower_loc: lower.loc.dupe(),
                    lower_def_loc: lower.reference_loc,
                    upper_loc: upper.loc.dupe(),
                    upper_def_loc: upper.reference_loc,
                    lower_desc: lower.type_desc,
                    upper_desc: upper.type_desc,
                    use_op,
                    explanation: None,
                    example: None,
                    branches: vec![],
                },
            ))
            .friendly_message_of_msg(),

            ErrorMessage::EExpectedBooleanLit(box EExpectedBooleanLitData {
                lower,
                upper,
                use_op,
            }) => ErrorMessage::EIncompatibleTypesWithUseOp(Box::new(
                EIncompatibleTypesWithUseOpData {
                    lower_loc: lower.loc.dupe(),
                    lower_def_loc: lower.reference_loc,
                    upper_loc: upper.loc.dupe(),
                    upper_def_loc: upper.reference_loc,
                    lower_desc: lower.type_desc,
                    upper_desc: upper.type_desc,
                    use_op,
                    explanation: None,
                    example: None,
                    branches: vec![],
                },
            ))
            .friendly_message_of_msg(),

            ErrorMessage::EExpectedBigIntLit(box EExpectedBigIntLitData {
                lower,
                upper,
                use_op,
            }) => ErrorMessage::EIncompatibleTypesWithUseOp(Box::new(
                EIncompatibleTypesWithUseOpData {
                    lower_loc: lower.loc.dupe(),
                    lower_def_loc: lower.reference_loc,
                    upper_loc: upper.loc.dupe(),
                    upper_def_loc: upper.reference_loc,
                    lower_desc: lower.type_desc,
                    upper_desc: upper.type_desc,
                    use_op,
                    explanation: None,
                    example: None,
                    branches: vec![],
                },
            ))
            .friendly_message_of_msg(),

            ErrorMessage::EPropNotFoundInLookup(box EPropNotFoundInLookupData {
                prop_name,
                object,
                prop_loc,
                use_op,
                suggestion,
            }) => UseOp(Box::new(UseOpData {
                loc: prop_loc,
                message: Message::MessagePropMissingWithPrintedType {
                    lower: Box::new(MessageTypeReferenceData {
                        loc: object.reference_loc,
                        desc: expect_type_desc(object.type_desc),
                    }),
                    upper: None,
                    prop: prop_name.as_ref().map(|n| n.display_smol_str()),
                    suggestion,
                },
                use_op,
                explanation: None,
            })),

            ErrorMessage::EPropNotFoundInSubtyping(box EPropNotFoundInSubtypingData {
                prop_name,
                suggestion,
                lower,
                upper,
                use_op,
            }) => PropMissingInSubtyping(Box::new(PropMissingInSubtypingData {
                prop: prop_name.as_ref().map(|n| n.display_smol_str()),
                suggestion,
                loc: lower.loc,
                lower: MessageTypeReferenceData {
                    loc: lower.reference_loc,
                    desc: expect_type_desc(lower.type_desc),
                },
                upper: MessageTypeReferenceData {
                    loc: upper.reference_loc,
                    desc: expect_type_desc(upper.type_desc),
                },
                use_op,
            })),

            ErrorMessage::EConstructSignatureMissingInSubtyping(
                box EConstructSignatureMissingInSubtypingData {
                    lower,
                    upper,
                    use_op,
                },
            ) => UseOp(Box::new(UseOpData {
                loc: lower.loc.dupe(),
                message: Message::MessageConstructSignatureMissing(Box::new(
                    MessageConstructSignatureMissingData {
                        lower: MessageTypeReferenceData {
                            loc: lower.reference_loc,
                            desc: expect_type_desc(lower.type_desc),
                        },
                        upper: MessageTypeReferenceData {
                            loc: upper.reference_loc,
                            desc: expect_type_desc(upper.type_desc),
                        },
                    },
                )),
                use_op,
                explanation: None,
            })),

            ErrorMessage::EPropsNotFoundInSubtyping(box EPropsNotFoundInSubtypingData {
                prop_names,
                lower,
                upper,
                use_op,
            }) => PropsMissingInSubtyping(Box::new(PropsMissingInSubtypingData {
                props: Vec1::try_from_vec(
                    prop_names.iter().map(|n| n.display_smol_str()).collect(),
                )
                .expect("prop_names is non-empty Vec1"),
                loc: lower.loc,
                lower: MessageTypeReferenceData {
                    loc: lower.reference_loc,
                    desc: expect_type_desc(lower.type_desc),
                },
                upper: MessageTypeReferenceData {
                    loc: upper.reference_loc,
                    desc: expect_type_desc(upper.type_desc),
                },
                use_op,
            })),

            ErrorMessage::EPropsExtraAgainstExactObject(
                box EPropsExtraAgainstExactObjectData {
                    prop_names,
                    lower,
                    upper,
                    upper_is_record,
                    use_op,
                },
            ) => {
                let loc = lower.loc.dupe();
                PropsExtraAgainstExactObject(Box::new(PropsExtraAgainstExactObjectData {
                    props: Vec1::try_from_vec(
                        prop_names.iter().map(|n| n.display_smol_str()).collect(),
                    )
                    .expect("prop_names is non-empty Vec1"),
                    loc,
                    lower: MessageTypeReferenceData {
                        loc: lower.reference_loc,
                        desc: expect_type_desc(lower.type_desc),
                    },
                    upper: MessageTypeReferenceData {
                        loc: upper.reference_loc,
                        desc: expect_type_desc(upper.type_desc),
                    },
                    upper_is_record,
                    use_op,
                }))
            }

            ErrorMessage::EIndexerCheckFailed(box EIndexerCheckFailedData {
                prop_name,
                lower,
                upper,
                indexer_loc,
                indexer_desc,
                use_op,
            }) => IndexerCheckFailed(Box::new(IndexerCheckFailedData {
                prop: prop_name.display_smol_str(),
                lower: MessageTypeReferenceData {
                    loc: lower.reference_loc,
                    desc: expect_type_desc(lower.type_desc),
                },
                upper: MessageTypeReferenceData {
                    loc: upper.reference_loc,
                    desc: expect_type_desc(upper.type_desc),
                },
                indexer: MessageTypeReferenceData {
                    loc: indexer_loc,
                    desc: expect_type_desc(indexer_desc),
                },
                use_op,
            })),

            ErrorMessage::EPropNotReadable(box EPropNotReadableData {
                prop_loc,
                prop_name,
                use_op,
            }) => UseOp(Box::new(UseOpData {
                loc: prop_loc,
                message: Message::MessagePropNotReadable(prop_name),
                use_op,
                explanation: None,
            })),

            ErrorMessage::EPropNotWritable(box EPropNotWritableData {
                prop_loc,
                prop_name,
                use_op,
            }) => UseOp(Box::new(UseOpData {
                loc: prop_loc,
                message: Message::MessagePropNotWritable(prop_name),
                use_op,
                explanation: None,
            })),

            ErrorMessage::EPropPolarityMismatch(box EPropPolarityMismatchData {
                lower,
                upper,
                props,
                use_op,
            }) => {
                let loc = match &lower {
                    PropPolarityMismatchObject::Type(t) => t.loc.dupe(),
                    PropPolarityMismatchObject::Class(class) => class.loc.dupe(),
                };
                let message_object = |object: PropPolarityMismatchObject<
                    ErrorTypeReferenceWithLocData<L>,
                    L,
                >| match object {
                    PropPolarityMismatchObject::Type(t) => {
                        PropPolarityMismatchObject::Type(MessageTypeReferenceData {
                            loc: t.reference_loc,
                            desc: expect_type_desc(t.type_desc),
                        })
                    }
                    PropPolarityMismatchObject::Class(class) => {
                        PropPolarityMismatchObject::Class(class)
                    }
                };
                PropPolarityMismatch(Box::new(PropPolarityMismatchData {
                    loc,
                    lower: message_object(lower),
                    upper: message_object(upper),
                    props: props
                        .iter()
                        .map(|(name, (lower, upper))| {
                            (name.as_ref().map(|n| n.display_smol_str()), *lower, *upper)
                        })
                        .collect::<Vec<_>>()
                        .try_into()
                        .expect("props is non-empty Vec1"),
                    use_op,
                }))
            }

            ErrorMessage::EUnionSpeculationFailed(box EUnionSpeculationFailedData {
                use_op,
                loc,
                branches,
            }) => Speculation(Box::new(SpeculationData {
                loc,
                use_op,
                branches,
            })),

            ErrorMessage::EUnsupportedExact(box EUnsupportedExactData {
                value_loc,
                value_desc,
                ..
            }) => Normal(Message::MessageCannotCreateExactType(Box::new(
                MessageTypeReferenceData {
                    loc: value_loc,
                    desc: expect_type_desc(value_desc),
                },
            ))),

            ErrorMessage::EInvalidExactObjectIntersection(
                box EInvalidExactObjectIntersectionData {
                    prop,
                    exact_object_loc,
                    exact_object_desc,
                    ..
                },
            ) => Normal(Message::MessageInvalidExactObjectIntersection {
                prop: prop.display_smol_str(),
                exact_object: MessageTypeReferenceData {
                    loc: exact_object_loc,
                    desc: expect_type_desc(exact_object_desc),
                },
            }),

            ErrorMessage::EUnexpectedThisType(_) => Normal(Message::MessageUnexpectedUseOfThisType),

            ErrorMessage::EExportsAnnot(_) => Normal(Message::MessageCannotUseDollarExports),

            ErrorMessage::EUseArrayLiteral(_) => Normal(Message::MessageShouldUseArrayLiteral),

            ErrorMessage::ERecursionLimit(_) => Normal(Message::MessageRecursionLimitExceeded),

            ErrorMessage::EEnumError(EnumErrorKind::EnumsNotEnabled(_)) => {
                Normal(Message::MessageEnumsNotEnabled)
            }

            ErrorMessage::EEnumError(EnumErrorKind::EnumConstNotSupported(_)) => {
                Normal(Message::MessageEnumConstNotSupported)
            }

            ErrorMessage::EIndeterminateModuleType(_) => {
                Normal(Message::MessageCannotDetermineModuleType)
            }

            ErrorMessage::EBadExportPosition(_) => Normal(Message::MessageNonToplevelExport),

            ErrorMessage::EBadDefaultImportDestructuring(_) => {
                Normal(Message::MessageCannotUseDefaultImportWithDestrucuturing)
            }

            ErrorMessage::EThisInExportedFunction(_) => {
                Normal(Message::MessageThisInExportedFunction)
            }

            ErrorMessage::EUnreachable(_) => Normal(Message::MessageUnreachableCode),

            ErrorMessage::EInvalidLHSInAssignment(_) => {
                Normal(Message::MessageCannotAssignToInvalidLHS)
            }

            ErrorMessage::EIncompatibleTypesWithUseOp(box EIncompatibleTypesWithUseOpData {
                lower_loc,
                lower_def_loc,
                upper_loc,
                upper_def_loc,
                lower_desc,
                upper_desc,
                use_op,
                explanation,
                example,
                branches,
            }) => {
                if !branches.is_empty() {
                    Speculation(Box::new(SpeculationData {
                        loc: upper_loc,
                        use_op,
                        branches,
                    }))
                } else {
                    match example {
                        Some(example) => IncompatibleTypesWithExample(Box::new(
                            IncompatibleTypesWithExampleData {
                                loc: lower_loc.dupe(),
                                lower_loc: lower_def_loc,
                                upper_loc: upper_def_loc,
                                lower_desc: expect_type_desc(lower_desc),
                                upper_desc: expect_type_desc(upper_desc),
                                use_op,
                                explanation,
                                example,
                            },
                        )),
                        None => {
                            let lower_desc = expect_type_desc(lower_desc);
                            let upper_desc = expect_type_desc(upper_desc);
                            if let (
                                ErrorTypeDesc::ReasonDesc(lower_desc),
                                ErrorTypeDesc::ReasonDesc(upper_desc),
                            ) = (&lower_desc, &upper_desc)
                                && (lower_desc.is_explanatory() || upper_desc.is_explanatory())
                            {
                                return IncompatibleSubtyping(Box::new(
                                    IncompatibleSubtypingData {
                                        reason_lower: VirtualReason::new_with(
                                            Some(lower_desc.clone()),
                                            lower_loc,
                                            Some(lower_def_loc),
                                            None,
                                        ),
                                        reason_upper: VirtualReason::new_with(
                                            Some(upper_desc.clone()),
                                            upper_loc,
                                            Some(upper_def_loc),
                                            None,
                                        ),
                                        use_op,
                                        explanation,
                                    },
                                ));
                            }
                            UseOp(Box::new(UseOpData {
                                loc: lower_loc,
                                message: Message::MessageIncompatibleGeneralWithPrintedTypes(
                                    Box::new(MessageIncompatibleGeneralWithPrintedTypesData {
                                        lower_loc: lower_def_loc,
                                        upper_loc: upper_def_loc,
                                        lower_desc,
                                        upper_desc,
                                    }),
                                ),
                                use_op,
                                explanation,
                            }))
                        }
                    }
                }
            }

            ErrorMessage::EUnsupportedSetProto(_) => {
                Normal(Message::MessageCannotMutateThisPrototype)
            }

            ErrorMessage::EAmbiguousObjectType(_) => Normal(Message::MessageAmbiguousObjectType),

            ErrorMessage::ENonstrictImport(_) => Normal(Message::MessageNonStrictImport),

            ErrorMessage::EUnclearType(_) => Normal(Message::MessageUnclearType),

            ErrorMessage::EDeprecatedBool(_) => Normal(Message::MessageDeprecatedBool),

            ErrorMessage::EUnsafeGettersSetters(_) => Normal(Message::MessageUnsafeGetterSetter),

            ErrorMessage::EUnsafeObjectAssign(_) => Normal(Message::MessageUnsafeObjectAssign),

            ErrorMessage::EUnusedSuppression(_) => Normal(Message::MessageUnusedSuppression),

            ErrorMessage::ECodelessSuppression(_) => Normal(Message::MessageSuppressionMissingCode),

            ErrorMessage::EUnnecessaryDeclareTypeOnlyExport(_) => {
                Normal(Message::MessageUnnecessaryDeclareTypeOnlyExport)
            }

            ErrorMessage::EInvalidConstructor(box EInvalidConstructorData {
                value_loc,
                value_desc,
                ..
            }) => Normal(Message::MessageCannotUseAsConstructor(Box::new(
                MessageTypeReferenceData {
                    loc: value_loc,
                    desc: expect_type_desc(value_desc),
                },
            ))),

            ErrorMessage::EInvalidPrototype(box EInvalidPrototypeData {
                prototype_loc,
                prototype_desc,
                ..
            }) => Normal(Message::MessageCannotUseAsPrototype(Box::new(
                MessageTypeReferenceData {
                    loc: prototype_loc,
                    desc: expect_type_desc(prototype_desc),
                },
            ))),

            ErrorMessage::EUnnecessaryOptionalChain(box (_, lhs_expression)) => {
                Normal(Message::MessageUnnecessaryOptionalChain { lhs_expression })
            }

            ErrorMessage::EUnnecessaryInvariant(box EUnnecessaryInvariantData {
                callee,
                condition,
                condition_kind,
                ..
            }) => Normal(Message::MessageUnnecessaryInvariant {
                callee,
                condition: Box::new(expect_error_type_reference(condition)),
                condition_kind,
            }),

            ErrorMessage::EArithmeticOperand(box EArithmeticOperandData { operand, .. }) => {
                Normal(Message::MessageCannotPerformArithOnNonNumbersOrBigInt(
                    Box::new(expect_error_type_reference(operand)),
                ))
            }

            ErrorMessage::EBinaryInLHS(box EBinaryInLHSData {
                lhs_loc, lhs_desc, ..
            }) => Normal(Message::MessageCannotUseInOperatorDueToBadLHS(Box::new(
                MessageTypeReferenceData {
                    loc: lhs_loc,
                    desc: expect_type_desc(lhs_desc),
                },
            ))),

            ErrorMessage::EBinaryInRHS(box EBinaryInRHSData {
                rhs_loc, rhs_desc, ..
            }) => Normal(Message::MessageCannotUseInOperatorDueToBadRHS(Box::new(
                MessageTypeReferenceData {
                    loc: rhs_loc,
                    desc: expect_type_desc(rhs_desc),
                },
            ))),

            ErrorMessage::EForInRHS(box EForInRHSData {
                rhs_loc, rhs_desc, ..
            }) => Normal(Message::MessageCannotIterateWithForIn(Box::new(
                MessageTypeReferenceData {
                    loc: rhs_loc,
                    desc: expect_type_desc(rhs_desc),
                },
            ))),

            ErrorMessage::EInstanceofRHS(box EInstanceofRHSData {
                rhs_loc, rhs_desc, ..
            }) => Normal(Message::MessageCannotUseInstanceOfOperatorDueToBadRHS(
                Box::new(MessageTypeReferenceData {
                    loc: rhs_loc,
                    desc: expect_type_desc(rhs_desc),
                }),
            )),

            ErrorMessage::EEnumError(EnumErrorKind::EnumIncompatible(data)) => {
                ErrorMessage::EIncompatibleTypesWithUseOp(data).friendly_message_of_msg()
            }

            ErrorMessage::EMalformedCode(_) => Normal(Message::MessageSuppressionMalformedCode),

            ErrorMessage::EComponentCase(_) => Normal(Message::MessageComponentNonUpperCase),

            ErrorMessage::EDeclareComponentInvalidParam { kind, .. } => {
                Normal(Message::MessageDeclareComponentInvalidParam(kind))
            }

            ErrorMessage::EComponentMissingReturn(component) => {
                Normal(Message::MessageComponentMissingReturn(component))
            }
            ErrorMessage::EComponentMissingBody(_) => Normal(Message::MessageComponentMissingBody),
            ErrorMessage::EComponentBodyInAmbientContext(_) => {
                Normal(Message::MessageComponentBodyInAmbientContext)
            }

            ErrorMessage::ENestedComponent(_) => Normal(Message::MessageCannotNestComponents),

            ErrorMessage::ENestedHook(_) => Normal(Message::MessageCannotNestHook),

            ErrorMessage::EEmptyArrayNoProvider { .. } => {
                Normal(Message::MessageCannotDetermineEmptyArrayLiteralType)
            }

            ErrorMessage::EHookNaming(_) => Normal(Message::MessageInvalidHookNaming),

            ErrorMessage::EUnusedPromise { async_: true, .. } => {
                Normal(Message::MessageUnusedPromiseInAsyncScope)
            }

            ErrorMessage::EUnusedPromise { async_: false, .. } => {
                Normal(Message::MessageUnusedPromiseInSyncScope)
            }

            ErrorMessage::EInvalidComponentRestParam(_) => {
                Normal(Message::MessageInvalidComponentRestParam)
            }

            ErrorMessage::EMatchError(MatchErrorKind::MatchInvalidGuardedWildcard(_)) => {
                Normal(Message::MessageMatchInvalidGuardedWildcard)
            }

            ErrorMessage::EMatchError(MatchErrorKind::MatchStatementInvalidBody { .. }) => {
                Normal(Message::MessageMatchStatementInvalidBody)
            }

            ErrorMessage::EMatchError(MatchErrorKind::MatchInvalidWildcardSyntax(_)) => {
                Normal(Message::MessageMatchInvalidWildcardSyntax)
            }

            ErrorMessage::EMatchError(MatchErrorKind::MatchInvalidInstancePattern(_)) => {
                Normal(Message::MessageMatchInvalidInstancePattern)
            }

            ErrorMessage::EMatchError(MatchErrorKind::MatchInvalidUnaryZero { .. }) => {
                Normal(Message::MessageMatchInvalidUnaryZero)
            }

            ErrorMessage::EMatchError(MatchErrorKind::MatchInvalidUnaryPlusBigInt { .. }) => {
                Normal(Message::MessageMatchInvalidUnaryPlusBigInt)
            }

            ErrorMessage::EMatchError(MatchErrorKind::MatchBindingInOrPattern { .. }) => {
                Normal(Message::MessageMatchBindingInOrPattern)
            }

            ErrorMessage::EMatchError(MatchErrorKind::MatchInvalidAsPattern { .. }) => {
                Normal(Message::MessageMatchInvalidAsPattern)
            }

            ErrorMessage::EUndocumentedFeature { .. } => {
                Normal(Message::MessageUndocumentedFeature)
            }

            ErrorMessage::EFunctionCallExtraArg(box EFunctionCallExtraArgData {
                loc,
                function_reference,
                param_count,
                use_op,
            }) => UseOp(Box::new(UseOpData {
                loc,
                message: Message::MessageCannotCallFunctionWithExtraArg {
                    function_reference,
                    param_count,
                },
                explanation: None,
                use_op,
            })),
            ErrorMessage::EExponentialSpread(box EExponentialSpreadData {
                spread_loc,
                operand1,
                operand2,
            }) => {
                let message_type_ref =
                    |t: ErrorTypeReferenceWithLocData<L>| MessageTypeReferenceData {
                        loc: t.reference_loc,
                        desc: expect_type_desc(t.type_desc),
                    };
                Normal(Message::MessageExponentialSpread(Box::new(
                    MessageExponentialSpreadData {
                        spread_loc,
                        operand1: map_exponential_spread_operand_group(message_type_ref, operand1),
                        operand2: map_exponential_spread_operand_group(message_type_ref, operand2),
                    },
                )))
            }
            ErrorMessage::EEnumError(EnumErrorKind::EnumInvalidMemberAccess(
                box EnumInvalidMemberAccessData {
                    member: EnumMemberAccess::Named { name, suggestion },
                    member_loc: _,
                    enum_,
                },
            )) => Normal(Message::MessageCannotAccessEnumMember(Box::new(
                MessageCannotAccessEnumMemberData {
                    member_name: name,
                    suggestion,
                    enum_,
                },
            ))),
            ErrorMessage::EEnumError(EnumErrorKind::EnumInvalidMemberAccess(
                box EnumInvalidMemberAccessData {
                    member: EnumMemberAccess::Computed(key),
                    member_loc: _,
                    enum_,
                },
            )) => Normal(Message::MessageCannotAccessEnumWithComputedProp {
                description: expect_type_desc(key.type_desc),
                enum_,
            }),
            ErrorMessage::EEnumError(EnumErrorKind::EnumModification(
                box EnumModificationData { enum_, .. },
            )) => Normal(Message::MessageCannotChangeEnumMember(enum_)),
            ErrorMessage::EEnumError(EnumErrorKind::EnumMemberDuplicateValue(
                box EnumMemberDuplicateValueData {
                    prev_use_loc,
                    enum_,
                    ..
                },
            )) => Normal(Message::MessageDuplicateEnumMember {
                prev_use_loc,
                enum_,
            }),
            ErrorMessage::EEnumError(EnumErrorKind::EnumNotIterable(box EnumNotIterableData {
                enum_,
            })) => Normal(Message::MessageCannotIterateEnum(enum_)),
            ErrorMessage::EEnumError(EnumErrorKind::EnumNotIterableForIn(
                box EnumNotIterableForInData { enum_, .. },
            )) => Normal(Message::MessageCannotIterateEnumForIn(enum_)),
            ErrorMessage::EEnumError(EnumErrorKind::EnumMemberAlreadyChecked(
                box EnumMemberAlreadyCheckedData {
                    prev_check_loc,
                    enum_,
                    member_name,
                    ..
                },
            )) => Normal(Message::MessageAlreadyExhaustivelyCheckOneEnumMember(
                Box::new(MessageAlreadyExhaustivelyCheckOneEnumMemberData {
                    prev_check_loc,
                    enum_,
                    member_name,
                }),
            )),
            ErrorMessage::EEnumError(EnumErrorKind::EnumAllMembersAlreadyChecked(
                box EnumAllMembersAlreadyCheckedData { enum_, .. },
            )) => Normal(Message::MessageAlreadyExhaustivelyCheckAllEnumMembers { enum_ }),
            ErrorMessage::EEnumError(EnumErrorKind::EnumNotAllChecked(
                box EnumNotAllCheckedData {
                    description_name,
                    enum_,
                    left_to_check,
                    default_case_loc,
                    ..
                },
            )) => Normal(Message::MessageIncompleteExhausiveCheckEnum(Box::new(
                MessageIncompleteExhausiveCheckEnumData {
                    description_name,
                    enum_,
                    left_to_check,
                    default_case_loc,
                },
            ))),
            ErrorMessage::EEnumError(EnumErrorKind::EnumUnknownNotChecked(
                box EnumUnknownNotCheckedData {
                    description_name,
                    enum_,
                    ..
                },
            )) => Normal(Message::MessageCannotExhaustivelyCheckEnumWithUnknowns(
                Box::new(MessageCannotExhaustivelyCheckEnumWithUnknownsData {
                    description_name,
                    enum_,
                }),
            )),
            ErrorMessage::EEnumError(EnumErrorKind::EnumInvalidCheck(
                box EnumInvalidCheckData {
                    enum_,
                    example_member,
                    from_match,
                    ..
                },
            )) => Normal(Message::MessageInvalidEnumMemberCheck(Box::new(
                MessageInvalidEnumMemberCheckData {
                    enum_,
                    example_member,
                    from_match,
                },
            ))),
            ErrorMessage::EEnumError(EnumErrorKind::EnumMemberUsedAsType(
                box EnumMemberUsedAsTypeData {
                    description_name,
                    enum_,
                    ..
                },
            )) => Normal(Message::MessageCannotUseEnumMemberUsedAsType(Box::new(
                MessageCannotUseEnumMemberUsedAsTypeData {
                    description_name,
                    enum_,
                },
            ))),
            ErrorMessage::EEnumError(EnumErrorKind::EnumInvalidAbstractUse(
                box EnumInvalidAbstractUseData {
                    description_name,
                    enum_loc,
                    enum_name,
                    ..
                },
            )) => Normal(Message::MessageCannotExhaustivelyCheckAbstractEnums(
                Box::new(MessageCannotExhaustivelyCheckAbstractEnumsData {
                    description_name,
                    enum_loc,
                    enum_name,
                }),
            )),
            ErrorMessage::EEnumError(EnumErrorKind::EnumInvalidMemberName(
                box EnumInvalidMemberNameData {
                    enum_, member_name, ..
                },
            )) => Normal(Message::MessageInvalidEnumMemberName { member_name, enum_ }),
            ErrorMessage::EEnumError(EnumErrorKind::EnumNonIdentifierMemberName(
                box EnumNonIdentifierMemberNameData {
                    enum_, member_name, ..
                },
            )) => Normal(Message::MessageEnumNonIdentifierMemberName { member_name, enum_ }),
            ErrorMessage::EEnumError(EnumErrorKind::EnumDuplicateMemberName(
                box EnumDuplicateMemberNameData {
                    prev_use_loc,
                    enum_,
                    member_name,
                    ..
                },
            )) => Normal(Message::MessageEnumDuplicateMemberName(Box::new(
                MessageEnumDuplicateMemberNameData {
                    member_name,
                    prev_use_loc,
                    enum_,
                },
            ))),
            ErrorMessage::EEnumError(EnumErrorKind::EnumInconsistentMemberValues(
                box EnumInconsistentMemberValuesData { enum_, .. },
            )) => Normal(Message::MessageEnumInconsistentMemberValues { enum_ }),
            ErrorMessage::EEnumError(EnumErrorKind::EnumInvalidMemberInitializer(
                box EnumInvalidMemberInitializerData {
                    enum_,
                    explicit_type,
                    member_name,
                    ..
                },
            )) => Normal(Message::MessageEnumInvalidMemberInitializer(Box::new(
                MessageEnumInvalidMemberInitializerData {
                    member_name,
                    explicit_type,
                    enum_,
                },
            ))),
            ErrorMessage::EEnumError(EnumErrorKind::EnumBooleanMemberNotInitialized(
                box EnumBooleanMemberNotInitializedData {
                    enum_, member_name, ..
                },
            )) => Normal(Message::MessageEnumBooleanMemberNotInitialized { member_name, enum_ }),
            ErrorMessage::EEnumError(EnumErrorKind::EnumNumberMemberNotInitialized(
                box EnumNumberMemberNotInitializedData {
                    enum_, member_name, ..
                },
            )) => Normal(Message::MessageEnumNumberMemberNotInitialized { member_name, enum_ }),
            ErrorMessage::EEnumError(EnumErrorKind::EnumBigIntMemberNotInitialized(
                box EnumBigIntMemberNotInitializedData {
                    enum_, member_name, ..
                },
            )) => Normal(Message::MessageEnumBigIntMemberNotInitialized { member_name, enum_ }),
            ErrorMessage::EEnumError(EnumErrorKind::EnumStringMemberInconsistentlyInitialized(
                box EnumStringMemberInconsistentlyInitializedData { enum_, .. },
            )) => Normal(Message::MessageEnumStringMemberInconsistentlyInitialized { enum_ }),
            ErrorMessage::EEnumError(EnumErrorKind::TSEnumInvalidMember(
                box TSEnumInvalidMemberData {
                    enum_,
                    member_name,
                    kind,
                    ..
                },
            )) => Normal(Message::MessageTSEnumInvalidMember {
                member_name,
                enum_,
                kind,
            }),
            ErrorMessage::EEnumError(EnumErrorKind::TSEnumInvalidSyntax(
                box TSEnumInvalidSyntaxData { enum_, kind, .. },
            )) => Normal(Message::MessageTSEnumInvalidSyntax { enum_, kind }),

            ErrorMessage::EDuplicateClassMember(box EDuplicateClassMemberData {
                name,
                is_static,
                class_kind,
                ..
            }) => Normal(Message::MessageDuplicateClassMember {
                name,
                static_: is_static,
                class_kind,
            }),

            ErrorMessage::EInvalidDeclaration(box EInvalidDeclarationData {
                declaration_loc,
                name,
                null_write: None,
                possible_generic_escape_locs,
            }) if possible_generic_escape_locs.is_empty() => Normal(
                Message::MessageVariableNeverInitAssignedAnnotated(NamedReferenceData {
                    loc: declaration_loc,
                    name,
                }),
            ),
            ErrorMessage::EInvalidDeclaration(box EInvalidDeclarationData {
                declaration_loc,
                name,
                null_write: None,
                possible_generic_escape_locs,
            }) => Normal(
                Message::MessageShouldAnnotateVariableOnlyInitializedInGenericContext {
                    declaration: NamedReferenceData {
                        loc: declaration_loc,
                        name,
                    },
                    possible_generic_escape_locs,
                },
            ),
            ErrorMessage::EInvalidDeclaration(box EInvalidDeclarationData {
                declaration_loc,
                name,
                null_write: Some(null_write),
                possible_generic_escape_locs,
            }) if possible_generic_escape_locs.is_empty() => {
                let null_loc = if null_write.initialized {
                    None
                } else {
                    Some(null_write.null_loc)
                };
                Normal(Message::MessageVariableOnlyAssignedByNull(Box::new(
                    MessageVariableOnlyAssignedByNullData {
                        declaration: NamedReferenceData {
                            loc: declaration_loc,
                            name,
                        },
                        null_loc,
                    },
                )))
            }
            ErrorMessage::EInvalidDeclaration(box EInvalidDeclarationData {
                declaration_loc,
                name,
                null_write: Some(null_write),
                possible_generic_escape_locs,
            }) => Normal(Message::MessageShouldAnnotateVariableUsedInGenericContext(
                Box::new(MessageShouldAnnotateVariableUsedInGenericContextData {
                    declaration: NamedReferenceData {
                        loc: declaration_loc,
                        name,
                    },
                    null_loc: null_write.null_loc,
                    initialized: null_write.initialized,
                    possible_generic_escape_locs,
                }),
            )),

            ErrorMessage::EAnnotationInference(box EAnnotationInferenceData {
                loc: _,
                operation,
                operation_loc,
                target_loc,
                target_desc,
            }) => Normal(Message::MessageCannotUseTypeForAnnotationInference(
                Box::new(MessageCannotUseTypeForAnnotationInferenceData {
                    operation,
                    operation_loc,
                    target_loc,
                    target_desc,
                }),
            )),
            ErrorMessage::ETrivialRecursiveDefinition(_) => {
                Normal(Message::MessageInvalidTrivialRecursiveDefinition)
            }
            ErrorMessage::ETrivialRecursiveTypeParameter(box (_, name)) => {
                Normal(Message::MessageInvalidTrivialRecursiveTypeParameter(name))
            }
            ErrorMessage::EClassTypeParamInStatic(box (_, name)) => {
                Normal(Message::MessageClassTypeParamInStatic(name))
            }
            ErrorMessage::ERecursiveDefinition(box ERecursiveDefinitionData {
                definition,
                recursion,
                annot_locs,
                ..
            }) => Normal(Message::MessageDefinitionInvalidRecursive(Box::new(
                MessageDefinitionInvalidRecursiveData {
                    definition: definition.kind,
                    recursion,
                    annot_locs,
                },
            ))),
            ErrorMessage::EDefinitionCycle(dependencies) => {
                Normal(Message::MessageDefinitionCycle(dependencies))
            }
            ErrorMessage::EReferenceInAnnotation(box (_, name, loc)) => {
                Normal(Message::MessageInvalidSelfReferencingTypeAnnotation(
                    Box::new(MessageInvalidSelfReferencingTypeAnnotationData { name, loc }),
                ))
            }

            ErrorMessage::EBigIntRShift3(box operand) => {
                Normal(Message::MessageCannotPerformBigIntRShift3(Box::new(
                    expect_error_type_reference(operand),
                )))
            }
            ErrorMessage::EBigIntNumCoerce(box operand) => {
                Normal(Message::MessageCannotPerformBigIntUnaryPlus(Box::new(
                    expect_error_type_reference(operand),
                )))
            }

            ErrorMessage::EInvalidCatchParameterAnnotation {
                ts_utility_syntax, ..
            } => Normal(Message::MessageInvalidCatchParameterAnnotation { ts_utility_syntax }),

            ErrorMessage::EInvalidMappedType { kind, .. } => {
                let msg = match kind {
                    InvalidMappedTypeErrorKind::InterfaceOrDeclaredClass => {
                        Message::MessageInvalidMappedTypeInInterfaceOrDeclaredClass
                    }
                    InvalidMappedTypeErrorKind::ExtraProperties => {
                        Message::MessageInvalidMappedTypeWithExtraProps
                    }
                    InvalidMappedTypeErrorKind::ExplicitExactOrInexact => {
                        Message::MessageInvalidMappedTypeWithExactOrInexact
                    }
                    InvalidMappedTypeErrorKind::VarianceOnArrayInput => {
                        Message::MessageInvalidMappedTypeWithVarianceOnArrayInput
                    }
                };
                Normal(msg)
            }

            ErrorMessage::EInvalidTemplateLiteralType { kind, .. } => {
                let msg = match kind {
                    InvalidTemplateLiteralTypeErrorKind::TooComplex => {
                        Message::MessageInvalidTemplateLiteralTypeComplexity
                    }
                    InvalidTemplateLiteralTypeErrorKind::InvalidPlaceholderType => {
                        Message::MessageInvalidTemplateLiteralTypePlaceholder
                    }
                };
                Normal(msg)
            }

            ErrorMessage::EDuplicateComponentProp(box EDuplicateComponentPropData {
                spread,
                duplicates,
            }) => Normal(Message::MessageRedeclareComponentProp(Box::new(
                MessageRedeclareComponentPropData {
                    duplicates,
                    spread_loc: spread,
                },
            ))),
            ErrorMessage::EKeySpreadProp(box EKeySpreadPropData {
                spread: spread_loc,
                loc: key_loc,
            }) => Normal(Message::MessageInvalidKeyPropertyInSpread(Box::new(
                MessageInvalidKeyPropertyInSpreadData {
                    spread_loc,
                    key_loc,
                },
            ))),

            ErrorMessage::EInvalidRendersTypeArgument(box EInvalidRendersTypeArgumentData {
                renders_variant,
                invalid_render_type_kind,
                invalid_types,
                ..
            }) => {
                let to_message_type_ref =
                    |type_ref: ErrorTypeReferenceData<L>| MessageTypeReferenceData {
                        loc: type_ref.loc,
                        desc: expect_type_desc(type_ref.type_desc),
                    };
                Normal(Message::MessageInvalidRendersTypeArgument(Box::new(
                    MessageInvalidRendersTypeArgumentData {
                        renders_variant,
                        invalid_render_type_kind: map_invalid_render_type_kind(
                            to_message_type_ref,
                            invalid_render_type_kind,
                        ),
                        invalid_types: Vec1::try_from_vec(
                            invalid_types.into_iter().map(to_message_type_ref).collect(),
                        )
                        .unwrap(),
                    },
                )))
            }

            ErrorMessage::EInvalidTypeCastSyntax { .. } => {
                Normal(Message::MessageInvalidTypeCastingSyntax)
            }

            ErrorMessage::ECannotCallReactComponent { component } => Normal(
                Message::MessageCannotCallReactComponent(MessageTypeReferenceData {
                    loc: component.reference_loc,
                    desc: expect_type_desc(component.type_desc),
                }),
            ),

            ErrorMessage::EMatchError(MatchErrorKind::MatchNotExhaustive(
                box MatchNotExhaustiveData { examples, .. },
            )) => Normal(Message::MessageMatchNotExhaustive {
                examples: examples
                    .into_iter()
                    .map(|(pattern, references)| {
                        (
                            pattern,
                            references
                                .into_iter()
                                .map(|reference| match reference {
                                    MatchExampleReference::Type(reference) => {
                                        MatchExampleReference::Type(MessageTypeReferenceData {
                                            loc: reference.reference_loc,
                                            desc: expect_type_desc(reference.type_desc),
                                        })
                                    }
                                    MatchExampleReference::EnumMember {
                                        loc,
                                        member_name,
                                        enum_name,
                                    } => MatchExampleReference::EnumMember {
                                        loc,
                                        member_name,
                                        enum_name,
                                    },
                                    MatchExampleReference::EnumUnknownMembers {
                                        loc,
                                        enum_name,
                                    } => {
                                        MatchExampleReference::EnumUnknownMembers { loc, enum_name }
                                    }
                                })
                                .collect(),
                        )
                    })
                    .collect(),
            }),
            ErrorMessage::EMatchError(MatchErrorKind::MatchUnusedPattern(
                box MatchUnusedPatternData {
                    pattern,
                    already_seen,
                },
            )) => Normal(Message::MessageMatchUnnecessaryPattern {
                pattern,
                already_seen,
            }),
            ErrorMessage::EMatchError(MatchErrorKind::MatchNonExhaustiveObjectPattern(
                box MatchNonExhaustiveObjectPatternData {
                    rest,
                    missing_props,
                    pattern_kind,
                    ..
                },
            )) => Normal(Message::MessageMatchNonExhaustiveObjectPattern(Box::new(
                MessageMatchNonExhaustiveObjectPatternData {
                    rest: rest.map(expect_error_type_reference),
                    missing_props: missing_props.to_vec(),
                    pattern_kind,
                },
            ))),
            ErrorMessage::EMatchError(MatchErrorKind::MatchNonExplicitEnumCheck(
                box MatchNonExplicitEnumCheckData {
                    wildcard_loc,
                    unchecked_members,
                    ..
                },
            )) => Normal(Message::MessageMatchNonExplicitEnumCheck(Box::new(
                MessageMatchNonExplicitEnumCheckData {
                    wildcard_loc,
                    unchecked_members: unchecked_members.to_vec(),
                },
            ))),
            ErrorMessage::EMatchError(MatchErrorKind::MatchInvalidIdentOrMemberPattern(
                box MatchInvalidIdentOrMemberPatternData {
                    type_loc,
                    type_desc,
                    ..
                },
            )) => Normal(Message::MessageMatchInvalidIdentOrMemberPattern {
                type_: Box::new(MessageTypeReferenceData {
                    loc: type_loc,
                    desc: expect_type_desc(type_desc),
                }),
            }),
            ErrorMessage::EMatchError(MatchErrorKind::MatchInvalidBindingKind { kind, .. }) => {
                Normal(Message::MessageMatchInvalidBindingKind { kind })
            }
            ErrorMessage::EMatchError(MatchErrorKind::MatchInvalidObjectPropertyLiteral {
                pattern_kind,
                ..
            }) => Normal(Message::MessageMatchInvalidObjectPropertyLiteral { pattern_kind }),
            ErrorMessage::EMatchError(MatchErrorKind::MatchDuplicateObjectProperty(
                box MatchDuplicateObjectPropertyData {
                    name, pattern_kind, ..
                },
            )) => Normal(Message::MessageMatchDuplicateObjectProperty { name, pattern_kind }),
            ErrorMessage::EMatchError(MatchErrorKind::MatchInvalidPatternReference(
                box MatchInvalidPatternReferenceData {
                    binding_loc, name, ..
                },
            )) => Normal(Message::MessageMatchInvalidPatternReference {
                binding: NamedReferenceData {
                    loc: binding_loc,
                    name,
                },
            }),
            ErrorMessage::EMatchError(MatchErrorKind::MatchInvalidObjectShorthand(
                box MatchInvalidObjectShorthandData {
                    name, pattern_kind, ..
                },
            )) => Normal(Message::MessageMatchInvalidObjectShorthand { name, pattern_kind }),
            ErrorMessage::EMatchError(MatchErrorKind::MatchInvalidCaseSyntax(
                box MatchInvalidCaseSyntaxData { kind, .. },
            )) => Normal(Message::MessageMatchInvalidCaseSyntax(kind)),

            ErrorMessage::ERecordError(record_error) => match record_error {
                RecordErrorKind::RecordBannedTypeUtil { op_loc, op, record } => {
                    Normal(Message::MessageRecordBannedTypeUtil {
                        op_loc,
                        op,
                        record: MessageTypeReferenceData {
                            loc: record.reference_loc,
                            desc: expect_type_desc(record.type_desc),
                        },
                    })
                }
                RecordErrorKind::RecordInvalidName { name, .. } => {
                    Normal(Message::MessageRecordInvalidName { name })
                }
                RecordErrorKind::RecordInvalidNew { record_name, .. } => {
                    Normal(Message::MessageRecordInvalidNew { record_name })
                }
                RecordErrorKind::RecordDeclarationInvalidSyntax { kind, .. } => {
                    Normal(Message::MessageRecordDeclarationInvalidSyntax(kind))
                }
            },

            ErrorMessage::EIllegalAssertOperator(box EIllegalAssertOperatorData {
                obj,
                specialized,
                ..
            }) => Normal(Message::MessageIllegalAssertOperator {
                obj: Box::new(obj),
                specialized,
            }),

            ErrorMessage::EReferenceInDefault(box (def_loc, name, ref_loc)) => {
                Normal(Message::MessageInvalidSelfReferencingDefault(Box::new(
                    MessageInvalidSelfReferencingDefaultData {
                        name,
                        def_loc,
                        ref_loc,
                    },
                )))
            }

            ErrorMessage::EIncompatibleProp(box EIncompatiblePropData {
                prop,
                prop_loc,
                object,
                special,
                use_op,
            }) => {
                let lower = Box::new(MessageTypeReferenceData {
                    loc: object.reference_loc,
                    desc: expect_type_desc(object.type_desc),
                });
                let message = match prop {
                    None if matches!(
                        special,
                        Some(LowerKind::PossiblyNull | LowerKind::PossiblyVoid)
                    ) =>
                    {
                        Message::MessageLowerDoesNotHavePropertiesWithPrintedType(lower)
                    }
                    prop => Message::MessagePropMissingWithPrintedType {
                        lower,
                        upper: None,
                        prop: prop.map(|n| n.display_smol_str()),
                        suggestion: None,
                    },
                };
                UseOp(Box::new(UseOpData {
                    loc: prop_loc,
                    message,
                    use_op: use_op
                        .unwrap_or(VirtualUseOp::Op(Arc::new(VirtualRootUseOp::UnknownUse))),
                    explanation: None,
                }))
            }

            ErrorMessage::EDevOnlyRefinedLocInfo(box EDevOnlyRefinedLocInfoData {
                refining_locs,
                ..
            }) => Normal(Message::MessageDevOnlyRefinedLocInfo { refining_locs }),

            ErrorMessage::EDevOnlyInvalidatedRefinementInfo(
                box EDevOnlyInvalidatedRefinementInfoData {
                    invalidation_info, ..
                },
            ) => Normal(Message::MessageDevOnlyInvalidatedRefinementInfo(
                invalidation_info,
            )),

            ErrorMessage::ENoDefaultExport(box (_, module_name, suggestion)) => Normal(
                Message::MessageNoDefaultExport(Box::new(MessageNoDefaultExportData {
                    module_name: module_name.into_inner(),
                    suggestion,
                })),
            ),

            ErrorMessage::EOnlyDefaultExport(box (_, module_name, export_name)) => Normal(
                Message::MessageOnlyDefaultExport(Box::new(MessageOnlyDefaultExportData {
                    module_name: module_name.into_inner(),
                    export_name,
                })),
            ),

            ErrorMessage::ENoNamedExport(box (_, module_name, export_name, suggestion)) => Normal(
                Message::MessageNoNamedExport(Box::new(MessageNoNamedExportData {
                    module_name: module_name.into_inner(),
                    export_name,
                    suggestion,
                })),
            ),

            ErrorMessage::EMissingTypeArgs(box EMissingTypeArgsData {
                type_,
                arity_loc,
                min_arity,
                max_arity,
                ..
            }) => Normal(Message::MessageCannotUseTypeWithoutAnyTypeArgs {
                type_: MessageTypeReferenceData {
                    loc: arity_loc,
                    desc: expect_type_desc(type_.type_desc),
                },
                min_arity,
                max_arity,
            }),

            ErrorMessage::ETooManyTypeArgs(box ETooManyTypeArgsData {
                reason_tapp,
                arity_loc,
                maximum_arity,
            }) => Normal(Message::MessageCannotUseTypeWithTooManyTypeArgs {
                reason_arity: VirtualReason::new(reason_tapp.desc, arity_loc),
                n: maximum_arity,
            }),

            ErrorMessage::ETooFewTypeArgs(box ETooFewTypeArgsData {
                reason_tapp,
                arity_loc,
                minimum_arity,
            }) => Normal(Message::MessageCannotUseTypeWithTooFewTypeArgs {
                reason_arity: VirtualReason::new(reason_tapp.desc, arity_loc),
                n: minimum_arity,
            }),

            ErrorMessage::EConstantCondition(box EConstantConditionData {
                is_truthy,
                warning,
                constant_condition_kind,
                ..
            }) => Normal(Message::MessageConstantCondition {
                is_truthy,
                warning,
                constant_condition_kind,
            }),

            ErrorMessage::ETypeParamArity(_, n) => {
                if n == 0 {
                    Normal(Message::MessageCannotApplyNonPolymorphicType)
                } else {
                    Normal(Message::MessageCannotUseTypeWithoutExactlyNTypeArgs(n))
                }
            }

            ErrorMessage::ECallTypeArity(box ECallTypeArityData {
                is_new,
                callee,
                expected_arity,
                ..
            }) => Normal(Message::MessageCannotUseNonPolymorphicTypeWithTypeArgs {
                is_new,
                callee: Box::new(match callee {
                    CallTypeArityCallee::Type(callee) => {
                        CallTypeArityCallee::Type(expect_error_type_reference(callee))
                    }
                    CallTypeArityCallee::Builtin(name) => CallTypeArityCallee::Builtin(name),
                    CallTypeArityCallee::Function => CallTypeArityCallee::Function,
                }),
                expected_arity,
            }),

            ErrorMessage::EPolarityMismatch(box EPolarityMismatchData {
                type_param_loc,
                name,
                expected_polarity,
                actual_polarity,
                ..
            }) => Normal(Message::MessageCannotUseTypeDueToPolarityMismatch {
                type_param: NamedReferenceData {
                    loc: type_param_loc,
                    name,
                },
                expected_polarity,
                actual_polarity,
            }),

            ErrorMessage::EBuiltinNameLookupFailed(box EBuiltinNameLookupFailedData {
                name,
                ..
            }) => Normal(Message::MessageCannotResolveBuiltinName(name)),

            ErrorMessage::EUmdGlobalValueInModule(box EUmdGlobalValueInModuleData {
                name, ..
            }) => Normal(Message::MessageUmdGlobalValueInModule(name)),

            ErrorMessage::EBuiltinModuleLookupFailed(box EBuiltinModuleLookupFailedData {
                name,
                potential_generator,
                ..
            }) => Normal(Message::MessageCannotResolveBuiltinModule(Box::new(
                MessageCannotResolveBuiltinModuleData {
                    name,
                    potential_generator,
                },
            ))),

            ErrorMessage::EExpectedModuleLookupFailed(box EExpectedModuleLookupFailedData {
                name,
                expected_module_purpose,
                ..
            }) => Normal(Message::MessageCannotResolveExpectedModule {
                name,
                expected_module_purpose,
            }),

            ErrorMessage::ECannotImportGlobalLibdef(box ECannotImportGlobalLibdefData {
                module_name,
                libdef_name,
                ..
            }) => Normal(Message::MessageCannotImportGlobalLibdef(Box::new(
                MessageCannotImportGlobalLibdefData {
                    module_name,
                    libdef_name,
                },
            ))),

            ErrorMessage::EPrivateLookupFailed(box EPrivateLookupFailedData {
                loc,
                object,
                prop_name,
                use_op,
            }) => PrivatePropMissingInLookup(Box::new(PrivatePropMissingInLookupData {
                loc,
                object: MessageTypeReferenceData {
                    loc: object.reference_loc,
                    desc: expect_type_desc(object.type_desc),
                },
                prop: format!("#{prop_name}").into(),
                use_op,
            })),

            ErrorMessage::EPlatformSpecificImplementationModuleLookupFailed(
                box EPlatformSpecificImplementationModuleLookupFailedData { name, .. },
            ) => Normal(Message::MessagePlatformSpecificImplementationModuleLookupFailed(name)),

            ErrorMessage::EComparison(box EComparisonData {
                r1,
                r2,
                strict_comparison_opt,
                ..
            }) => {
                let to_message_type_ref =
                    |type_ref: ErrorTypeReferenceWithLocData<L>| MessageTypeReferenceData {
                        loc: type_ref.reference_loc,
                        desc: expect_type_desc(type_ref.type_desc),
                    };
                Normal(Message::MessageCannotCompare(Box::new(
                    MessageCannotCompareData {
                        lower: to_message_type_ref(r1),
                        upper: to_message_type_ref(r2),
                        strict_comparison_opt: strict_comparison_opt.map(|info| {
                            map_strict_comparison_info(info, |loc| loc, to_message_type_ref)
                        }),
                    },
                )))
            }

            ErrorMessage::ENonStrictEqualityComparison(box ENonStrictEqualityComparisonData {
                lower,
                upper,
            }) => Normal(Message::MessageCannotCompareNonStrict {
                lower: MessageTypeReferenceData {
                    loc: lower.reference_loc,
                    desc: expect_type_desc(lower.type_desc),
                },
                upper: MessageTypeReferenceData {
                    loc: upper.reference_loc,
                    desc: expect_type_desc(upper.type_desc),
                },
            }),

            ErrorMessage::ETupleArityMismatch(box ETupleArityMismatchData {
                use_op,
                lower,
                lower_arity,
                lower_inexact,
                lower_rest,
                upper,
                upper_arity,
                upper_inexact,
                upper_rest,
                unify,
            }) => {
                let loc = lower.loc.dupe();
                UseOp(Box::new(UseOpData {
                    loc,
                    message: Message::MessageIncompatibleTupleArity(Box::new(
                        MessageIncompatibleTupleArityData {
                            lower: MessageTypeReferenceData {
                                loc: lower.reference_loc,
                                desc: expect_type_desc(lower.type_desc),
                            },
                            lower_arity,
                            lower_inexact,
                            lower_rest,
                            upper: MessageTypeReferenceData {
                                loc: upper.reference_loc,
                                desc: expect_type_desc(upper.type_desc),
                            },
                            upper_arity,
                            upper_inexact,
                            upper_rest,
                            unify,
                        },
                    )),
                    use_op,
                    explanation: None,
                }))
            }

            ErrorMessage::ETupleRequiredAfterOptional(box ETupleRequiredAfterOptionalData {
                tuple_loc,
                required,
                optional,
            }) => Normal(Message::MessageInvalidTupleRequiredAfterOptional {
                tuple_loc,
                required,
                optional,
            }),

            ErrorMessage::ETupleInvalidTypeSpread(box ETupleInvalidTypeSpreadData {
                argument,
                ..
            }) => Normal(Message::MessageInvalidTupleTypeSpread(Box::new(
                MessageTypeReferenceData {
                    loc: argument.reference_loc,
                    desc: expect_type_desc(argument.type_desc),
                },
            ))),

            ErrorMessage::ETupleElementAfterInexactSpread(_) => {
                Normal(Message::MessageTupleElementAfterInexactSpread)
            }
            ErrorMessage::ETupleElementAfterRestSpread(_) => {
                Normal(Message::MessageTupleElementAfterRestSpread)
            }

            ErrorMessage::ENonLitArrayToTuple((lower, upper), use_op) => {
                let loc = lower.loc.dupe();
                UseOp(Box::new(UseOpData {
                    loc,
                    message: Message::MessageIncompatibleNonLiteralArrayToTuple {
                        lower: MessageTypeReferenceData {
                            loc: lower.reference_loc,
                            desc: expect_type_desc(lower.type_desc),
                        },
                        upper: MessageTypeReferenceData {
                            loc: upper.reference_loc,
                            desc: expect_type_desc(upper.type_desc),
                        },
                    },
                    use_op,
                    explanation: None,
                }))
            }

            ErrorMessage::ETupleOutOfBounds(box ETupleOutOfBoundsData {
                loc,
                tuple,
                inexact,
                length,
                index,
                use_op,
            }) => UseOp(Box::new(UseOpData {
                loc,
                message: Message::MessageTupleIndexOutOfBound(Box::new(
                    MessageTupleIndexOutOfBoundData {
                        tuple: MessageTypeReferenceData {
                            loc: tuple.reference_loc,
                            desc: expect_type_desc(tuple.type_desc),
                        },
                        inexact,
                        length,
                        index,
                    },
                )),
                use_op,
                explanation: None,
            })),

            ErrorMessage::ETupleNonIntegerIndex(box ETupleNonIntegerIndexData {
                loc,
                index_def_loc,
                index,
                use_op,
            }) => UseOp(Box::new(UseOpData {
                loc,
                message: Message::MessageTupleNonIntegerIndex(Box::new(
                    MessageTupleNonIntegerIndexData {
                        index_def_loc,
                        index,
                    },
                )),
                use_op,
                explanation: None,
            })),

            ErrorMessage::ETupleUnsafeWrite { loc, use_op } => UseOp(Box::new(UseOpData {
                loc,
                message: Message::MessageTupleNonStaticallyKnownIndex,
                use_op,
                explanation: None,
            })),

            ErrorMessage::ETupleElementNotReadable(box ETupleElementNotReadableData {
                loc,
                index_def_loc,
                index,
                name,
                use_op,
            }) => UseOp(Box::new(UseOpData {
                loc,
                message: Message::MessageTupleElementNotReadable(Box::new(
                    MessageTupleElementNotReadableData {
                        index_def_loc,
                        index,
                        name,
                    },
                )),
                use_op,
                explanation: None,
            })),

            ErrorMessage::ETupleElementNotWritable(box ETupleElementNotWritableData {
                loc,
                index_def_loc,
                index,
                name,
                use_op,
            }) => UseOp(Box::new(UseOpData {
                loc,
                message: Message::MessageTupleElementNotWritable(Box::new(
                    MessageTupleElementNotWritableData {
                        index_def_loc,
                        index,
                        name,
                    },
                )),
                use_op,
                explanation: None,
            })),

            ErrorMessage::ETupleElementPolarityMismatch(
                box ETupleElementPolarityMismatchData {
                    index,
                    lower,
                    polarity_lower,
                    upper,
                    polarity_upper,
                    use_op,
                },
            ) => {
                let loc = lower.loc.dupe();
                UseOp(Box::new(UseOpData {
                    loc,
                    message: Message::MessageTuplePolarityMismatch {
                        index,
                        lower: MessageTypeReferenceData {
                            loc: lower.reference_loc,
                            desc: expect_type_desc(lower.type_desc),
                        },
                        upper: MessageTypeReferenceData {
                            loc: upper.reference_loc,
                            desc: expect_type_desc(upper.type_desc),
                        },
                        polarity_lower,
                        polarity_upper,
                    },
                    use_op,
                    explanation: None,
                }))
            }

            ErrorMessage::EROArrayWrite(loc, use_op) => UseOp(Box::new(UseOpData {
                loc,
                message: Message::MessageReadonlyArraysCannotBeWrittenTo,
                use_op,
                explanation: None,
            })),

            ErrorMessage::EIncompatibleWithExact((lower, upper), use_op, kind) => {
                let loc = lower.loc.dupe();
                UseOp(Box::new(UseOpData {
                    loc,
                    message: Message::MessageIncompatibleWithExact {
                        kind,
                        lower: MessageTypeReferenceData {
                            loc: lower.reference_loc,
                            desc: expect_type_desc(lower.type_desc),
                        },
                        upper: MessageTypeReferenceData {
                            loc: upper.reference_loc,
                            desc: expect_type_desc(upper.type_desc),
                        },
                    },
                    use_op,
                    explanation: None,
                }))
            }

            ErrorMessage::EFunctionIncompatibleWithIndexer((lower, upper), use_op) => {
                let loc = lower.loc.dupe();
                UseOp(Box::new(UseOpData {
                    loc,
                    message: Message::MessageIncompatibleWithIndexed {
                        lower: MessageTypeReferenceData {
                            loc: lower.reference_loc,
                            desc: expect_type_desc(lower.type_desc),
                        },
                        upper: MessageTypeReferenceData {
                            loc: upper.reference_loc,
                            desc: expect_type_desc(upper.type_desc),
                        },
                    },
                    use_op,
                    explanation: None,
                }))
            }

            ErrorMessage::EUnsupportedKeyInObject {
                key_error_kind,
                obj_kind,
                ..
            } => Normal(Message::MessageUnsupportedKeyInObject {
                key_error_kind,
                obj_kind,
            }),

            ErrorMessage::EUnsupportedComputedKeyInClass(_) => {
                Normal(Message::MessageUnsupportedComputedKeyInClass)
            }

            ErrorMessage::EAmbiguousNumericKeyWithVariance(_) => {
                Normal(Message::MessageAmbiguousNumericKeyWithVariance)
            }

            ErrorMessage::ETypeGuardFuncIncompatibility {
                use_op,
                lower,
                upper,
            } => {
                let loc = match &lower {
                    TypeGuardFuncReference::Type(lower) => lower.loc.dupe(),
                    TypeGuardFuncReference::TypeGuard(lower) => lower.loc.dupe(),
                };
                let message_reference = |reference: TypeGuardFuncReference<
                    L,
                    ErrorTypeReferenceWithLocData<L>,
                >| match reference {
                    TypeGuardFuncReference::Type(reference) => {
                        TypeGuardFuncReference::Type(MessageTypeReferenceData {
                            loc: reference.reference_loc,
                            desc: expect_type_desc(reference.type_desc),
                        })
                    }
                    TypeGuardFuncReference::TypeGuard(reference) => {
                        TypeGuardFuncReference::TypeGuard(reference)
                    }
                };
                UseOp(Box::new(UseOpData {
                    loc,
                    message: Message::MessageIncompatibleNonTypeGuardToTypeGuard {
                        lower: message_reference(lower),
                        upper: message_reference(upper),
                    },
                    use_op,
                    explanation: None,
                }))
            }

            ErrorMessage::ETypeGuardInvalidParameter(box ETypeGuardInvalidParameterData {
                type_guard,
                binding_loc,
                binding_kind,
            }) => Normal(Message::MessageCannotReferenceTypeGuardParameter {
                type_guard,
                binding_loc,
                binding_kind,
            }),

            ErrorMessage::ETypeGuardIndexMismatch {
                use_op,
                parameters: (lower, upper),
            } => {
                let loc = lower.loc.dupe();
                UseOp(Box::new(UseOpData {
                    loc,
                    message: Message::MessageTypeGuardIndexMismatch { lower, upper },
                    use_op,
                    explanation: None,
                }))
            }

            ErrorMessage::ETypeGuardImpliesMismatch {
                use_op,
                lower,
                upper,
            } => {
                let loc = lower.loc.dupe();
                UseOp(Box::new(UseOpData {
                    loc,
                    message: Message::MessageTypeGuardImpliesMismatch { lower, upper },
                    use_op,
                    explanation: None,
                }))
            }

            ErrorMessage::ETypeGuardParamUnbound(type_guard) => {
                Normal(Message::MessageInvalidTypeGuardParamUnbound(type_guard))
            }

            ErrorMessage::ETypeGuardThisParam(loc) => {
                Normal(Message::MessageInvalidTypeGuardThisParam(loc))
            }

            ErrorMessage::ETypeGuardFunctionInvalidWrites(
                box ETypeGuardFunctionInvalidWritesData {
                    type_guard,
                    write_locs,
                    ..
                },
            ) => Normal(Message::MessageInvalidTypeGuardFunctionWritten {
                type_guard,
                write_locs,
            }),

            ErrorMessage::ENegativeTypeGuardConsistency(
                box ENegativeTypeGuardConsistencyData {
                    return_reason,
                    type_,
                },
            ) => Normal(Message::MessageNegativeTypeGuardConsistency {
                return_desc: return_reason.desc,
                type_: Box::new(expect_error_type_reference(type_)),
            }),

            ErrorMessage::EAssertionFunctionFalsyAtExit(
                box EAssertionFunctionFalsyAtExitData {
                    param,
                    falsy,
                    implicit_return,
                    ..
                },
            ) => Normal(Message::MessageAssertionFunctionFalsyAtExit {
                param,
                falsy: Box::new(expect_error_type_reference(falsy)),
                implicit_return,
            }),

            ErrorMessage::EUnrecognizedAssertionCall(box EUnrecognizedAssertionCallData {
                callee,
                ..
            }) => Normal(Message::MessageUnrecognizedAssertionCall(callee)),

            ErrorMessage::ETypeParamConstIncompatibility(
                box ETypeParamConstIncompatibilityData {
                    use_op,
                    lower,
                    upper,
                },
            ) => {
                let loc = lower.loc.dupe();
                UseOp(Box::new(UseOpData {
                    loc,
                    message: Message::MessageIncompatiblETypeParamConstIncompatibility {
                        lower,
                        upper,
                    },
                    use_op,
                    explanation: None,
                }))
            }

            ErrorMessage::ETypeParamConstInvalidPosition(box (loc, name)) => Normal(
                Message::MessageTypeParamConstInvalidPosition(NamedReferenceData { loc, name }),
            ),

            ErrorMessage::ETypeGuardFunctionParamHavoced(
                box ETypeGuardFunctionParamHavocedData {
                    type_guard,
                    param_loc,
                    is_this,
                    call_locs,
                },
            ) => {
                let TypeGuardParameterData { name, .. } = type_guard;
                let (type_guard, param_kind) = if is_this {
                    (TypeGuardReferenceKind::This, TypeGuardReferenceKind::This)
                } else {
                    (
                        TypeGuardReferenceKind::TypeGuardParameter(name.dupe()),
                        TypeGuardReferenceKind::Parameter(Some(name)),
                    )
                };
                Normal(Message::MessageCannotUseTypeGuardWithFunctionParamHavoced(
                    Box::new(MessageCannotUseTypeGuardWithFunctionParamHavocedData {
                        type_guard,
                        param: TypeGuardReferenceData {
                            loc: param_loc,
                            kind: param_kind,
                        },
                        call_locs,
                    }),
                ))
            }

            ErrorMessage::ETypeGuardIncompatibleWithFunctionKind(
                box ETypeGuardIncompatibleWithFunctionKindData { kind, .. },
            ) => Normal(Message::MessageInvalidTypeGuardFunctionKind(kind)),

            ErrorMessage::EInternal(box (_, internal_error)) => {
                let msg = format!(
                    "Internal error: {}",
                    string_of_internal_error(&internal_error)
                );
                Normal(Message::MessagePlainTextReservedForInternalErrorOnly(
                    msg.into(),
                ))
            }

            ErrorMessage::EUnsupportedSyntax(box (_, unsupported_syntax)) => {
                Normal(Message::MessageUnsupportedSyntax(unsupported_syntax))
            }

            ErrorMessage::EMissingThisAnnotation(_) => {
                Normal(Message::MessageMissingThisAnnotation)
            }

            ErrorMessage::EMissingParameterAnnotation { hint_available, .. } => {
                if hint_available {
                    Normal(Message::MessageMissingParameterAnnotationDueToContextualTypingFailure)
                } else {
                    Normal(Message::MessageMissingParameterAnnotation)
                }
            }

            ErrorMessage::EMissingLocalAnnotation {
                loc: _,
                target,
                hint_available,
                from_generic_function,
            } => match target {
                MissingLocalAnnotationTarget::Property(name) => {
                    if hint_available {
                        Normal(Message::MessageMissingAnnotationDueToContextualTypingFailure(name))
                    } else if from_generic_function {
                        Normal(Message::MessageMissingAnnotationForGenericFunction(name))
                    } else {
                        Normal(Message::MessageMissingAnnotation(name))
                    }
                }
                MissingLocalAnnotationTarget::Return => {
                    if from_generic_function {
                        Normal(Message::MessageMissingReturnAnnotationForGenericFunction)
                    } else {
                        Normal(Message::MessageMissingReturnAnnotation)
                    }
                }
            },

            ErrorMessage::EBindingError(box (binding_error, _, x, entry_loc)) => {
                let binding = NamedReferenceData {
                    loc: entry_loc,
                    name: x.display_smol_str(),
                };

                let msg = match binding_error {
                    BindingError::ENameAlreadyBound => {
                        Message::MessageCannotDeclareAlreadyBoundName(binding)
                    }
                    BindingError::ENameAlreadyBoundInCoreJs => {
                        Message::MessageCannotDeclareAlreadyBoundNameInCoreJs(binding)
                    }
                    BindingError::EVarRedeclaration => Message::MessageCannotRedeclareVar(binding),
                    BindingError::EReferencedBeforeDeclaration => {
                        Message::MessageCannotUseBeforeDeclaration(binding)
                    }
                    BindingError::EReferencedThisSuperBeforeSuperCall => {
                        Message::MessageCannotUseThisSuperBeforeSuperCall(binding)
                    }
                    BindingError::ETypeInValuePosition {
                        imported,
                        type_only_namespace,
                        name,
                    } => {
                        let imported_name = if imported { Some(name) } else { None };
                        Message::MessageCannotUseTypeInValuePosition(Box::new(
                            MessageCannotUseTypeInValuePositionData {
                                binding,
                                type_only_namespace,
                                imported_name,
                            },
                        ))
                    }
                    BindingError::EConstReassigned | BindingError::EConstParamReassigned => {
                        Message::MessageCannotReassignConstant(binding)
                    }
                    BindingError::EImportReassigned => {
                        Message::MessageCannotReassignImport(binding)
                    }
                    BindingError::EEnumReassigned => Message::MessageCannotReassignEnum(binding),
                    BindingError::EReservedKeyword { keyword } => {
                        Message::MessageCannotDeclareReservedType { keyword, binding }
                    }
                };
                Normal(msg)
            }

            ErrorMessage::EUninitializedInstanceProperty(_, err) => {
                Normal(Message::MessageUninitializedInstanceProperty(err))
            }

            ErrorMessage::EUninitializedProperty(box (_, name, kind)) => {
                Normal(Message::MessageUninitializedProperty { name, kind })
            }

            ErrorMessage::EBadExportContext(box (name, _)) => Normal(
                Message::MessageCannotUseExportInNonLegalToplevelContext(name),
            ),

            ErrorMessage::EBadDefaultImportAccess(box (_, import_star_loc)) => {
                Normal(Message::MessageInvalidImportStarUse(import_star_loc))
            }

            ErrorMessage::EInvalidImportStarUse(box (_, import_star_loc)) => {
                Normal(Message::MessageCannotUseImportStar(import_star_loc))
            }

            ErrorMessage::ENonConstVarExport(box (_, declaration)) => {
                Normal(Message::MessageNonConstVarExport(
                    declaration.map(|(loc, name)| NamedReferenceData { loc, name }),
                ))
            }

            ErrorMessage::EMixedImportAndRequire(box (_, import_loc)) => {
                Normal(Message::MessageCannotUseMixedImportAndRequire(import_loc))
            }

            ErrorMessage::EUnsupportedVarianceAnnotation(box (_, kind)) => {
                Normal(Message::MessageUnsupportedVarianceAnnotation(kind))
            }

            ErrorMessage::EExportRenamedDefault(box EExportRenamedDefaultData {
                name,
                is_reexport,
                ..
            }) => Normal(Message::MessageCannotExportRenamedDefault(Box::new(
                MessageCannotExportRenamedDefaultData { name, is_reexport },
            ))),

            ErrorMessage::ECannotDelete(box ECannotDeleteData { expression, .. }) => Normal(
                Message::MessageCannotDelete(Box::new(expect_error_type_reference(expression))),
            ),

            ErrorMessage::ESignatureBindingValidation(sve) => {
                use flow_type_sig::signature_error::BindingValidation as BV;
                let msg = match sve {
                    BV::ModuleOverride {
                        name,
                        existing_binding_loc,
                        ..
                    } => Message::MessageBadLibdefModuleOverride(NamedReferenceData {
                        loc: existing_binding_loc,
                        name,
                    }),
                    BV::NameOverride {
                        name,
                        existing_binding_loc,
                        ..
                    } => Message::MessageBadLibdefNameOverride(NamedReferenceData {
                        loc: existing_binding_loc,
                        name,
                    }),
                    BV::NamespacedNameAlreadyBound {
                        name,
                        existing_binding_loc,
                        ..
                    } => Message::MessageCannotDeclareAlreadyBoundNameInNamespace(
                        NamedReferenceData {
                            loc: existing_binding_loc,
                            name,
                        },
                    ),
                    BV::InterfaceMergePropertyConflict {
                        name,
                        current_binding_loc,
                        ..
                    } => Message::MessageInterfaceMergePropertyConflict(NamedReferenceData {
                        loc: current_binding_loc,
                        name,
                    }),
                    BV::InterfaceMergeTparamMismatch {
                        name,
                        current_binding_loc,
                        ..
                    } => Message::MessageInterfaceMergeTparamMismatch(NamedReferenceData {
                        loc: current_binding_loc,
                        name,
                    }),
                };
                Normal(msg)
            }

            ErrorMessage::ESignatureVerification(sve) => {
                Normal(Message::MessageCannotBuildTypedInterface(sve))
            }

            ErrorMessage::EInvalidObjectKit(box EInvalidObjectKitData {
                loc,
                value,
                use_op,
                requirement,
            }) => UseOp(Box::new(UseOpData {
                loc,
                message: Message::MessageLowerIsNotWithPrintedType {
                    lower: Box::new(expect_error_type_reference(value)),
                    requirement,
                },
                explanation: None,
                use_op,
            })),

            ErrorMessage::EInvalidTypeof(box (_, typename)) => {
                Normal(Message::MessageInvalidGenericRef(typename))
            }

            ErrorMessage::EObjectComputedPropertyAccess(
                box EObjectComputedPropertyAccessData {
                    object,
                    property,
                    kind,
                },
            ) => Normal(Message::MessageCannotAccessObjectWithComputedProp {
                object: MessageTypeReferenceData {
                    loc: object.reference_loc,
                    desc: expect_type_desc(object.type_desc),
                },
                property: MessageTypeReferenceData {
                    loc: property.reference_loc,
                    desc: expect_type_desc(property.type_desc),
                },
                kind,
            }),

            ErrorMessage::EObjectComputedPropertyAssign(box (property, Some(key), kind)) => Normal(
                Message::MessageCannotAssignToObjectWithComputedPropWithKey {
                    property: MessageTypeReferenceData {
                        loc: property.reference_loc,
                        desc: expect_type_desc(property.type_desc),
                    },
                    key,
                    kind,
                },
            ),

            ErrorMessage::EObjectComputedPropertyAssign(box (property, None, _)) => {
                Normal(Message::MessageCannotAssignToObjectWithComputedProp(
                    Box::new(MessageTypeReferenceData {
                        loc: property.reference_loc,
                        desc: expect_type_desc(property.type_desc),
                    }),
                ))
            }

            ErrorMessage::EObjectComputedPropertyPotentialOverwrite(
                box EObjectComputedPropertyPotentialOverwriteData {
                    key_loc,
                    overwritten_locs,
                },
            ) => Normal(
                Message::MessageCannotAddComputedPropertyDueToPotentialOverwrite(Box::new(
                    MessageCannotAddComputedPropertyDueToPotentialOverwriteData {
                        key_loc,
                        overwritten_locs,
                    },
                )),
            ),

            ErrorMessage::EUnsupportedImplements(type_) => Normal(
                Message::MessageCannotImplementNonInterface(Box::new(MessageTypeReferenceData {
                    loc: type_.loc,
                    desc: expect_type_desc(type_.type_desc),
                })),
            ),

            ErrorMessage::ENotAReactComponent(box ENotAReactComponentData {
                component,
                use_op,
            }) => {
                let loc = component.loc.dupe();
                UseOp(Box::new(UseOpData {
                    loc,
                    message: Message::MessageLowerIsNotReactComponent(Box::new(
                        MessageTypeReferenceData {
                            loc: component.reference_loc,
                            desc: expect_type_desc(component.type_desc),
                        },
                    )),
                    use_op,
                    explanation: None,
                }))
            }

            ErrorMessage::EReactRefInRender {
                usage,
                kind,
                in_hook,
            } => {
                use crate::error_message::RefInRenderKind;
                match kind {
                    RefInRenderKind::Argument => {
                        Normal(Message::MessageCannotPassReactRefAsArgument { usage, in_hook })
                    }
                    RefInRenderKind::Access => {
                        Normal(Message::MessageCannotAccessReactRefInRender { usage, in_hook })
                    }
                }
            }

            ErrorMessage::EDuplicateModuleProvider(box EDuplicateModuleProviderData {
                module_name,
                provider,
                conflict,
            }) => Normal(Message::MessageDuplicateModuleProvider(Box::new(
                MessageDuplicateModuleProviderData {
                    module_name,
                    provider,
                    conflict,
                },
            ))),

            ErrorMessage::EParseError(box (_, parse_error)) => {
                Normal(Message::MessageParseError(parse_error))
            }

            ErrorMessage::EDocblockError(box (_, err)) => {
                Normal(Message::MessageDocblockError(err))
            }

            ErrorMessage::EUntypedTypeImport(box (_, module_name)) => Normal(
                Message::MessageUntypedTypeImport(module_name.display().to_string().into()),
            ),

            ErrorMessage::EUntypedImport(box (_, module_name)) => Normal(
                Message::MessageUntypedImport(module_name.display().to_string().into()),
            ),

            ErrorMessage::EInternalType(_, kind) => Normal(Message::MessageInternalType(kind)),

            ErrorMessage::EIncorrectTypeWithReplacement(
                box EIncorrectTypeWithReplacementData { kind, .. },
            ) => Normal(Message::MessageIncorrectType(kind)),

            ErrorMessage::ELintSetting(box (_, kind)) => {
                Normal(Message::MessageInvalidLintSettings(kind))
            }

            ErrorMessage::ESketchyNullLint(box ESketchyNullLintData {
                kind,
                falsy_loc,
                null_loc,
                ..
            }) => Normal(Message::MessageSketchyNullCheck(Box::new(
                MessageSketchyNullCheckData {
                    kind,
                    falsy_loc,
                    null_loc,
                },
            ))),

            ErrorMessage::ESketchyNumberLint(_, value) => {
                Normal(Message::MessageSketchyNumber(MessageTypeReferenceData {
                    loc: value.reference_loc,
                    desc: expect_type_desc(value.type_desc),
                }))
            }

            ErrorMessage::EPrimitiveAsInterface(box EPrimitiveAsInterfaceData {
                use_op,
                lower,
                upper,
                kind,
            }) => {
                let loc = lower.loc.dupe();
                UseOp(Box::new(UseOpData {
                    loc,
                    message: Message::MessageCannotUsePrimitiveAsInterface {
                        lower: MessageTypeReferenceData {
                            loc: lower.reference_loc,
                            desc: expect_type_desc(lower.type_desc),
                        },
                        upper: MessageTypeReferenceData {
                            loc: upper.reference_loc,
                            desc: expect_type_desc(upper.type_desc),
                        },
                        kind,
                    },
                    explanation: None,
                    use_op,
                }))
            }

            ErrorMessage::ECannotSpreadInterface(box ECannotSpreadInterfaceData {
                spread_loc,
                interface,
                use_op,
            }) => {
                let loc = spread_loc.dupe();
                UseOp(Box::new(UseOpData {
                    loc,
                    message: Message::MessageCannotSpreadInterface {
                        spread_loc,
                        interface: MessageTypeReferenceData {
                            loc: interface.reference_loc,
                            desc: expect_type_desc(interface.type_desc),
                        },
                    },
                    explanation: None,
                    use_op,
                }))
            }

            ErrorMessage::ECannotSpreadIndexerOnRight(box ECannotSpreadIndexerOnRightData {
                spread_loc,
                object,
                key_loc,
                key_desc,
                use_op,
            }) => {
                let loc = spread_loc.dupe();
                UseOp(Box::new(UseOpData {
                    loc,
                    message: Message::MessageCannotSpreadDueToPotentialOverwrite {
                        spread_loc,
                        object: Box::new(MessageTypeReferenceData {
                            loc: object.reference_loc,
                            desc: expect_type_desc(object.type_desc),
                        }),
                        key: Box::new(MessageTypeReferenceData {
                            loc: key_loc,
                            desc: expect_type_desc(key_desc),
                        }),
                    },
                    explanation: None,
                    use_op,
                }))
            }

            ErrorMessage::EUnableToSpread(box EUnableToSpreadData {
                spread_loc,
                object1,
                object2,
                propname,
                error_kind,
                use_op,
            }) => {
                let loc = spread_loc.dupe();
                UseOp(Box::new(UseOpData {
                    loc,
                    message: Message::MessageCannotSpreadGeneral(Box::new(
                        MessageCannotSpreadGeneralData {
                            spread_loc,
                            object1: MessageTypeReferenceData {
                                loc: object1.reference_loc,
                                desc: expect_type_desc(object1.type_desc),
                            },
                            object2: MessageTypeReferenceData {
                                loc: object2.reference_loc,
                                desc: expect_type_desc(object2.type_desc),
                            },
                            propname,
                            error_kind,
                        },
                    )),
                    explanation: None,
                    use_op,
                }))
            }

            ErrorMessage::EInexactMayOverwriteIndexer(box EInexactMayOverwriteIndexerData {
                spread_loc,
                key_loc,
                key_desc,
                value_loc,
                value_desc,
                object2,
                use_op,
            }) => {
                let loc = spread_loc.dupe();
                UseOp(Box::new(UseOpData {
                    loc,
                    message: Message::MessageCannotSpreadInexactMayOverwriteIndexer(Box::new(
                        MessageCannotSpreadInexactMayOverwriteIndexerData {
                            spread_loc,
                            key: Box::new(MessageTypeReferenceData {
                                loc: key_loc,
                                desc: expect_type_desc(key_desc),
                            }),
                            value: Box::new(MessageTypeReferenceData {
                                loc: value_loc,
                                desc: expect_type_desc(value_desc),
                            }),
                            object2: Box::new(MessageTypeReferenceData {
                                loc: object2.reference_loc,
                                desc: expect_type_desc(object2.type_desc),
                            }),
                        },
                    )),
                    explanation: None,
                    use_op,
                }))
            }

            ErrorMessage::EEnumError(EnumErrorKind::EnumInvalidObjectUtilType(
                box EnumInvalidObjectUtilTypeData {
                    operation_name,
                    enum_,
                    ..
                },
            )) => Normal(Message::MessageCannotInstantiateObjectUtilTypeWithEnum(
                Box::new(MessageCannotInstantiateObjectUtilTypeWithEnumData {
                    operation_name,
                    enum_,
                }),
            )),

            ErrorMessage::EEnumError(EnumErrorKind::EnumInvalidObjectFunction(
                box EnumInvalidObjectFunctionData {
                    operation_loc,
                    function_name,
                    enum_,
                },
            )) => Normal(Message::MessageCannotCallObjectFunctionOnEnum {
                function: NamedReferenceData {
                    loc: operation_loc,
                    name: function_name,
                },
                enum_,
            }),

            ErrorMessage::EAssignConstLikeBinding(box EAssignConstLikeBindingData {
                definition_loc,
                name,
                binding_kind,
                ..
            }) => Normal(Message::MessageCannotReassignConstantLikeBinding {
                definition: NamedReferenceData {
                    loc: definition_loc,
                    name,
                },
                binding_kind,
            }),

            ErrorMessage::EObjectThisSuperReference(box EObjectThisSuperReferenceData {
                method_loc,
                method_name,
                kind,
                ..
            }) => {
                let converted_kind = match kind {
                    ThisFinderKind::This => crate::intermediate_error_types::ThisFinderKind::This,
                    ThisFinderKind::Super => crate::intermediate_error_types::ThisFinderKind::Super,
                };
                Normal(Message::MessageThisSuperInObject {
                    method_loc,
                    method_name,
                    kind: converted_kind,
                })
            }

            ErrorMessage::EComponentThisReference(box EComponentThisReferenceData {
                component_loc,
                ..
            }) => Normal(Message::MessageThisInComponent(component_loc)),

            ErrorMessage::EImplicitInstantiationUnderconstrainedError(
                box EImplicitInstantiationUnderconstrainedErrorData {
                    call,
                    type_param_loc,
                    type_param_name,
                    use_op,
                    ..
                },
            ) => {
                let loc = call.loc.dupe();
                UseOp(Box::new(UseOpData {
                    use_op,
                    message: Message::MessageUnderconstrainedImplicitInstantiaton {
                        call,
                        type_param: NamedReferenceData {
                            loc: type_param_loc,
                            name: type_param_name,
                        },
                    },
                    loc,
                    explanation: None,
                }))
            }

            ErrorMessage::EClassToObject(box EClassToObjectData {
                lower,
                upper,
                use_op,
                kind,
            }) => {
                let loc = lower.loc.dupe();
                UseOp(Box::new(UseOpData {
                    loc,
                    message: Message::MessageIncompatibleClassToObject {
                        lower: MessageTypeReferenceData {
                            loc: lower.reference_loc,
                            desc: expect_type_desc(lower.type_desc),
                        },
                        upper: MessageTypeReferenceData {
                            loc: upper.reference_loc,
                            desc: expect_type_desc(upper.type_desc),
                        },
                        kind,
                    },
                    explanation: None,
                    use_op,
                }))
            }

            ErrorMessage::EHookIncompatible(box EHookIncompatibleData {
                use_op,
                lower,
                upper,
                lower_is_hook,
                hook_is_annot,
            }) => {
                let loc = lower.loc.dupe();
                UseOp(Box::new(UseOpData {
                    loc,
                    message: Message::MessageIncompatibleReactHooksWithNonReactHook {
                        lower: MessageTypeReferenceData {
                            loc: lower.reference_loc,
                            desc: expect_type_desc(lower.type_desc),
                        },
                        upper: MessageTypeReferenceData {
                            loc: upper.reference_loc,
                            desc: expect_type_desc(upper.type_desc),
                        },
                        lower_is_hook,
                        hook_is_annot,
                    },
                    use_op,
                    explanation: Some(
                        Explanation::ExplanationReactHookIncompatibleWithNormalFunctions,
                    ),
                }))
            }

            ErrorMessage::EHookUniqueIncompatible(box EHookUniqueIncompatibleData {
                use_op,
                lower,
                upper,
            }) => {
                let loc = lower.loc.dupe();
                UseOp(Box::new(UseOpData {
                    loc,
                    message: Message::MessageIncompatibleReactHooksDueToUniqueness {
                        lower: MessageTypeReferenceData {
                            loc: lower.reference_loc,
                            desc: expect_type_desc(lower.type_desc),
                        },
                        upper: MessageTypeReferenceData {
                            loc: upper.reference_loc,
                            desc: expect_type_desc(upper.type_desc),
                        },
                    },
                    use_op,
                    explanation: Some(Explanation::ExplanationReactHookIncompatibleWithEachOther),
                }))
            }

            ErrorMessage::EHookRuleViolation(box EHookRuleViolationData {
                callee_loc,
                hook_rule,
                ..
            }) => {
                use crate::error_message::HookRule;
                match hook_rule {
                    HookRule::ConditionalHook => {
                        Normal(Message::MessageCannotCallReactHookConditionally(callee_loc))
                    }
                    HookRule::HookHasIllegalName => {
                        Normal(Message::MessageCannotCallReactHookWithIllegalName(callee_loc))
                    }
                    HookRule::MaybeHook { hooks, non_hooks } => {
                        Normal(Message::MessageCannotCallMaybeReactHook(Box::new(MessageCannotCallMaybeReactHookData {
                            callee_loc,
                            hooks,
                            non_hooks,
                        })))
                    }
                    HookRule::NotHookSyntaxHook => {
                        Normal(Message::MessageCannotCallNonHookSyntaxHook(callee_loc))
                    }
                    HookRule::HookDefinitelyNotInComponentOrHook => {
                        Normal(Message::MessageCannotCallReactHookInDefinitelyNonComponentOrHook(
                            callee_loc,
                        ))
                    }
                    HookRule::HookNotInComponentSyntaxComponentOrHookSyntaxHook => {
                        Normal(Message::MessageCannotCallReactHookInNonComponentSyntaxComponentOrHookSyntaxHook(
                            callee_loc,
                        ))
                    }
                    HookRule::HookInUnknownContext => {
                        Normal(Message::MessageCannotCallReactHookInUnknownContext(
                            callee_loc,
                        ))
                    }
                }
            }

            ErrorMessage::EInvalidGraphQL(box (_, err)) => {
                Normal(Message::MessageInvalidGraphQL(err))
            }

            ErrorMessage::EReactIntrinsicOverlap(box EReactIntrinsicOverlapData {
                use_,
                def,
                type_,
                mixed,
            }) => Normal(Message::MessageReactIntrinsicOverlap(Box::new(
                MessageReactIntrinsicOverlapData {
                    use_,
                    def,
                    type_,
                    mixed,
                },
            ))),

            ErrorMessage::EInvalidBinaryArith(box EInvalidBinaryArithData {
                left,
                right,
                kind,
                ..
            }) => Normal(Message::MessageCannotPerformBinaryArith {
                kind,
                left: Box::new(expect_error_type_reference(left)),
                right: Box::new(expect_error_type_reference(right)),
            }),

            ErrorMessage::EMissingPlatformSupportWithAvailablePlatforms(
                box EMissingPlatformSupportWithAvailablePlatformsData {
                    available_platforms,
                    required_platforms,
                    ..
                },
            ) => Normal(
                Message::MessageMissingPlatformSupportWithAvailablePlatforms(Box::new(
                    MessageMissingPlatformSupportWithAvailablePlatformsData {
                        available_platforms: available_platforms.clone(),
                        required_platforms: required_platforms.clone(),
                    },
                )),
            ),

            ErrorMessage::EUnionPartialOptimizationNonUniqueKey(
                box EUnionPartialOptimizationNonUniqueKeyData {
                    non_unique_keys, ..
                },
            ) => Normal(Message::MessageCannotOptimizeUnionDueToNonUniqueKeys(
                non_unique_keys
                    .into_iter()
                    .map(|(name, inner_map)| {
                        (
                            name,
                            inner_map
                                .into_iter()
                                .map(|(enum_key, type_references)| {
                                    (
                                        enum_key,
                                        type_references.mapped(|type_reference| type_reference.loc),
                                    )
                                })
                                .collect(),
                        )
                    })
                    .collect(),
            )),

            ErrorMessage::EUnionOptimization(box EUnionOptimizationData { kind, .. }) => {
                Normal(Message::MessageCannotOptimizeUnionInternally(kind))
            }

            ErrorMessage::EUnionOptimizationOnNonUnion(box EUnionOptimizationOnNonUnionData {
                arg_loc,
                arg_desc,
                ..
            }) => Normal(Message::MessageInvalidUseOfFlowEnforceOptimized(Box::new(
                MessageTypeReferenceData {
                    loc: arg_loc,
                    desc: expect_type_desc(arg_desc),
                },
            ))),

            ErrorMessage::EInvariantSubtypingWithUseOp(box EInvariantSubtypingWithUseOpData {
                sub_component,
                lower_loc,
                upper_loc,
                lower_desc,
                upper_desc,
                use_op,
                explanation,
            }) => {
                let explanation = explanation.as_ref().map(|expl| {
                    use crate::intermediate_error_types::ExplanationWithLazyParts;
                    match expl {
                        ExplanationWithLazyParts::LazyExplanationInvariantSubtypingDueToMutableArray {
                            lower_array_loc,
                            upper_array_loc,
                            lower_array_desc,
                            upper_array_desc,
                            upper_array_reference_loc,
                        } => Explanation::ExplanationInvariantSubtypingDueToMutableArray(Box::new(ExplanationInvariantSubtypingDueToMutableArrayData {
                            lower_array_loc: lower_array_loc.dupe(),
                            upper_array_loc: upper_array_loc.dupe(),
                            lower_array_desc: expect_type_desc(lower_array_desc.clone()),
                            upper_array_desc: expect_type_desc(upper_array_desc.clone()),
                            upper_array_reference_loc: upper_array_reference_loc.dupe(),
                        })),
                        ExplanationWithLazyParts::LazyExplanationInvariantSubtypingDueToMutableProperty {
                            lower_obj_loc,
                            upper_obj_loc,
                            lower_obj_desc,
                            upper_obj_desc,
                            upper_object_loc,
                            property_name,
                        } => Explanation::ExplanationInvariantSubtypingDueToMutableProperty(Box::new(ExplanationInvariantSubtypingDueToMutablePropertyData {
                            lower_obj_loc: lower_obj_loc.dupe(),
                            upper_obj_loc: upper_obj_loc.dupe(),
                            lower_obj_desc: expect_type_desc(lower_obj_desc.clone()),
                            upper_obj_desc: expect_type_desc(upper_obj_desc.clone()),
                            upper_object_loc: upper_object_loc.dupe(),
                            property_name: property_name.dupe(),
                        })),
                        ExplanationWithLazyParts::LazyExplanationInvariantSubtypingDueToMutableProperties {
                            lower_obj_loc,
                            upper_obj_loc,
                            lower_obj_desc,
                            upper_obj_desc,
                            upper_object_loc,
                            properties,
                        } => Explanation::ExplanationInvariantSubtypingDueToMutableProperties(Box::new(ExplanationInvariantSubtypingDueToMutablePropertiesData {
                            lower_obj_loc: lower_obj_loc.dupe(),
                            upper_obj_loc: upper_obj_loc.dupe(),
                            lower_obj_desc: expect_type_desc(lower_obj_desc.clone()),
                            upper_obj_desc: expect_type_desc(upper_obj_desc.clone()),
                            upper_object_loc: upper_object_loc.dupe(),
                            properties: properties.clone(),
                        })),
                    }
                });
                IncompatibleInvariantSubtyping(Box::new(IncompatibleInvariantSubtypingData {
                    sub_component,
                    lower_loc,
                    upper_loc,
                    lower_desc: expect_type_desc(lower_desc),
                    upper_desc: expect_type_desc(upper_desc),
                    use_op,
                    explanation,
                }))
            }

            ErrorMessage::EPropsNotFoundInInvariantSubtyping(
                box EPropsNotFoundInInvariantSubtypingData {
                    prop_names,
                    lower,
                    upper,
                    use_op,
                },
            ) => PropsMissingInInvariantSubtyping(Box::new(PropsMissingInInvariantSubtypingData {
                props: Vec1::try_from_vec(
                    prop_names.iter().map(|n| n.display_smol_str()).collect(),
                )
                .expect("prop_names is Vec1"),
                loc: lower.loc,
                lower_obj_loc: lower.definition_loc,
                upper_obj_loc: upper.definition_loc,
                lower: MessageTypeReferenceData {
                    loc: lower.reference_loc,
                    desc: expect_type_desc(lower.type_desc),
                },
                upper: MessageTypeReferenceData {
                    loc: upper.reference_loc,
                    desc: expect_type_desc(upper.type_desc),
                },
                use_op,
            })),

            ErrorMessage::ETSSyntax(box ETSSyntaxData { kind, .. }) => {
                use crate::intermediate_error_types::Message;
                let msg = match kind {
                    TSSyntaxKind::TSNever => Message::MessageTSNeverType,
                    TSSyntaxKind::TSUndefined => Message::MessageTSUndefinedType,
                    TSSyntaxKind::TSInOutVariance => Message::MessageTSVarianceInOut,
                    TSSyntaxKind::TSReadonlyType(Some(arg_kind)) => match arg_kind {
                        ReadonlyTypeKind::Tuple => Message::MessageTSReadonlyOperatorOnTuple,
                        ReadonlyTypeKind::Array => Message::MessageTSReadonlyOperatorOnArray,
                    },
                    TSSyntaxKind::TSReadonlyType(None) => Message::MessageTSReadonlyType,
                    TSSyntaxKind::TSClassAccessibility(kind) => {
                        Message::MessageTSClassAccessibility(kind)
                    }
                    TSSyntaxKind::TSParameterProperty => Message::MessageTSParameterProperty,
                    TSSyntaxKind::AbstractClass => Message::MessageAbstractClass,
                    TSSyntaxKind::AbstractMethod => Message::MessageAbstractMethod,
                    TSSyntaxKind::DeprecatedTypeParamColon => {
                        Message::MessageDeprecatedTypeParamColonBound
                    }
                };
                Normal(msg)
            }

            ErrorMessage::EAbstractClass(box EAbstractClassData { kind, .. }) => {
                use crate::intermediate_error_types::Message;
                Normal(Message::MessageAbstract(kind))
            }

            ErrorMessage::EOverride(box EOverrideData { kind, .. }) => {
                use crate::intermediate_error_types::Message;
                Normal(Message::MessageOverride(kind))
            }

            ErrorMessage::EVarianceKeyword(box EVarianceKeywordData { kind, .. }) => {
                use crate::intermediate_error_types::Message;
                use crate::intermediate_error_types::VarianceSigilKind;
                let msg = match kind {
                    VarianceKeywordKind::Plus(parent) => {
                        Message::MessageDeprecatedVarianceSigil(VarianceSigilKind::Plus(parent))
                    }
                    VarianceKeywordKind::Minus(parent) => {
                        Message::MessageDeprecatedVarianceSigil(VarianceSigilKind::Minus(parent))
                    }
                };
                Normal(msg)
            }
        }
    }
}

impl<L: Dupe + PartialEq + Eq + PartialOrd + Ord> ErrorMessage<L> {
    pub fn defered_in_speculation(&self) -> bool {
        match self {
            Self::EUntypedTypeImport(box (_, _))
            | Self::EUntypedImport(box (_, _))
            | Self::ENonstrictImport(_)
            | Self::EUnclearType(_)
            | Self::EDeprecatedBool(_)
            | Self::EInternalType(_, _)
            | Self::EUnsafeGettersSetters(_)
            | Self::EUnsafeObjectAssign(_)
            | Self::ESketchyNullLint(box ESketchyNullLintData { .. })
            | Self::ESketchyNumberLint(_, _)
            | Self::EUnnecessaryOptionalChain(box (_, _))
            | Self::EUnnecessaryInvariant(box EUnnecessaryInvariantData { .. })
            | Self::EUnnecessaryDeclareTypeOnlyExport(_)
            | Self::EAmbiguousObjectType(_)
            | Self::EInvalidThisArg(box EInvalidThisArgData { .. })
            | Self::EMatchError(MatchErrorKind::MatchNonExplicitEnumCheck(
                box MatchNonExplicitEnumCheckData { .. },
            ))
            | Self::EUninitializedInstanceProperty(_, _)
            | Self::EUninitializedProperty(_)
            | Self::ETrivialRecursiveDefinition(_)
            | Self::ETrivialRecursiveTypeParameter(_)
            | Self::EAnyValueUsedAsType { .. }
            | Self::EValueUsedAsType { .. }
            | Self::EUnusedPromise { .. }
            | Self::EImplicitInstantiationUnderconstrainedError(
                box EImplicitInstantiationUnderconstrainedErrorData { .. },
            )
            | Self::EDuplicateComponentProp(box EDuplicateComponentPropData { .. })
            | Self::EConstantCondition(box EConstantConditionData { .. })
            | Self::EBuiltinNameLookupFailed(box EBuiltinNameLookupFailedData { .. }) => true,

            Self::EBindingError(box (BindingError::EReferencedBeforeDeclaration, _, _, _)) => true,

            Self::EEnumError(EnumErrorKind::EnumNotAllChecked(box EnumNotAllCheckedData {
                default_case_loc: Some(_),
                ..
            })) => true,

            Self::EPropNotReadable(box EPropNotReadableData { use_op, .. }) => {
                matches!(
                    use_op,
                    VirtualUseOp::Frame(frame, _) if matches!(frame.as_ref(), VirtualFrameUseOp::ReactDeepReadOnly(..))
                )
            }

            _ => false,
        }
    }

    fn react_rule_of_use_op(
        use_op: &Option<VirtualUseOp<L>>,
        default: ErrorCode,
    ) -> Option<ErrorCode> {
        fn code_of_frame<L: Dupe + PartialEq + Eq + PartialOrd + Ord>(
            acc: Option<ErrorCode>,
            frame: &VirtualFrameUseOp<L>,
        ) -> Option<ErrorCode> {
            use flow_common_errors::error_codes::ErrorCode::*;
            match frame {
                VirtualFrameUseOp::ReactDeepReadOnly(box (_, DroType::HookReturn)) => {
                    Some(ReactRuleHookMutation)
                }
                VirtualFrameUseOp::ReactDeepReadOnly(box (
                    _,
                    DroType::Props | DroType::HookArg,
                )) => Some(ReactRuleUnsafeMutation),
                _ => acc,
            }
        }

        let result = use_op
            .as_ref()
            .map(|op| fold_virtual_use_op(op, |_| None, &code_of_frame));
        result.flatten().or(Some(default))
    }

    fn error_code_of_use_op(
        use_op: &Option<VirtualUseOp<L>>,
        default: ErrorCode,
    ) -> Option<ErrorCode> {
        fn code_of_root<L: Dupe + PartialEq + Eq + PartialOrd + Ord>(
            root: &VirtualRootUseOp<L>,
        ) -> Option<ErrorCode> {
            use flow_common_errors::error_codes::ErrorCode::*;
            match root {
                VirtualRootUseOp::Cast { .. }
                | VirtualRootUseOp::ClassExtendsCheck(..)
                | VirtualRootUseOp::FunCall(..)
                | VirtualRootUseOp::FunCallMethod(..)
                | VirtualRootUseOp::FunReturnStatement { .. }
                | VirtualRootUseOp::FunImplicitReturn(..) => Some(IncompatibleType),
                VirtualRootUseOp::TypeGuardIncompatibility(..)
                | VirtualRootUseOp::PositiveTypeGuardConsistency(..)
                | VirtualRootUseOp::AssertionFunctionConsistency(..) => Some(IncompatibleTypeGuard),
                _ => None,
            }
        }

        fn code_of_frame<L: Dupe + PartialEq + Eq + PartialOrd + Ord>(
            acc: Option<ErrorCode>,
            frame: &VirtualFrameUseOp<L>,
        ) -> Option<ErrorCode> {
            use flow_common_errors::error_codes::ErrorCode::*;
            match (&acc, frame) {
                (Some(_), _) => acc,
                (_, VirtualFrameUseOp::TypeArgCompatibility(..))
                | (_, VirtualFrameUseOp::TypeParamBound { .. }) => Some(IncompatibleType),
                (None, _) => None,
            }
        }

        let result = use_op
            .as_ref()
            .map(|op| fold_virtual_use_op(op, code_of_root, &code_of_frame));
        result.flatten().or(Some(default))
    }

    fn error_code_of_upper_kind(upper_kind: &UpperKind<L>) -> Option<ErrorCode> {
        use flow_common_errors::error_codes::ErrorCode::*;
        match upper_kind {
            UpperKind::IncompatibleCallT => Some(NotAFunction),
            UpperKind::IncompatibleObjAssignFromTSpread | UpperKind::IncompatibleArrRestT => {
                Some(NotAnArray)
            }
            UpperKind::IncompatibleObjAssignFromT
            | UpperKind::IncompatibleObjRestT
            | UpperKind::IncompatibleGetKeysT
            | UpperKind::IncompatibleGetValuesT => Some(NotAnObject),
            UpperKind::IncompatibleMixinT | UpperKind::IncompatibleThisSpecializeT => {
                Some(NotAClass)
            }
            _ => Some(IncompatibleUse),
        }
    }

    /// Maps error messages to their corresponding error codes
    pub fn error_code_of_message(&self) -> Option<ErrorCode> {
        use BindingError::*;
        use flow_common_errors::error_codes::ErrorCode::*;

        match self {
            ErrorMessage::EArithmeticOperand { .. } => Some(UnsafeArith),

            ErrorMessage::EInvalidBinaryArith(box EInvalidBinaryArithData { kind, .. }) => {
                match kind.1 {
                    flow_typing_type::type_::arith_kind::ArithKindInner::Plus => {
                        Some(UnsafeAddition)
                    }
                    _ => Some(UnsafeArith),
                }
            }

            ErrorMessage::EAssignConstLikeBinding(box EAssignConstLikeBindingData { .. }) => {
                Some(CannotReassignConstLike)
            }
            ErrorMessage::EBadExportContext(box (_, _)) => Some(InvalidExport),
            ErrorMessage::EBadExportPosition(_) => Some(InvalidExport),
            ErrorMessage::EBadDefaultImportAccess { .. } => Some(DefaultImportAccess),
            ErrorMessage::EBadDefaultImportDestructuring(_) => Some(DefaultImportAccess),
            ErrorMessage::EInvalidImportStarUse { .. } => Some(InvalidImportStarUse),
            ErrorMessage::EBinaryInLHS { .. } => Some(InvalidInLhs),
            ErrorMessage::EBinaryInRHS { .. } => Some(InvalidInRhs),

            ErrorMessage::EBindingError(box (binding_error, _, _, _)) => match binding_error {
                ENameAlreadyBound => Some(NameAlreadyBound),
                ENameAlreadyBoundInCoreJs => Some(NameAlreadyBound),
                EVarRedeclaration => Some(NameAlreadyBound),
                EReferencedBeforeDeclaration => Some(ReferenceBeforeDeclaration),
                EReferencedThisSuperBeforeSuperCall => Some(ReferenceBeforeDeclaration),
                ETypeInValuePosition { .. } => Some(TypeAsValue),
                EConstReassigned | EConstParamReassigned => Some(ReassignConst),
                EImportReassigned => Some(ReassignImport),
                EEnumReassigned => Some(ReassignEnum),
                EReservedKeyword { .. } => Some(ReservedKeyword),
            },

            ErrorMessage::EBuiltinNameLookupFailed(box EBuiltinNameLookupFailedData { .. }) => {
                Some(CannotResolveName)
            }
            ErrorMessage::EUmdGlobalValueInModule(box EUmdGlobalValueInModuleData { .. }) => {
                Some(UmdGlobalValueAccess)
            }
            ErrorMessage::EBuiltinModuleLookupFailed(box EBuiltinModuleLookupFailedData {
                ..
            }) => Some(CannotResolveModule),
            ErrorMessage::EExpectedModuleLookupFailed(box EExpectedModuleLookupFailedData {
                ..
            }) => Some(CannotResolveModule),
            ErrorMessage::ECannotImportGlobalLibdef(box ECannotImportGlobalLibdefData {
                ..
            }) => Some(CannotResolveModule),
            ErrorMessage::EPlatformSpecificImplementationModuleLookupFailed(
                box EPlatformSpecificImplementationModuleLookupFailedData { .. },
            ) => Some(CannotResolveModule),
            ErrorMessage::ECallTypeArity(box ECallTypeArityData { .. }) => {
                Some(NonpolymorphicTypeArg)
            }
            ErrorMessage::ECannotDelete { .. } => Some(CannotDelete),
            ErrorMessage::ECannotSpreadIndexerOnRight(box ECannotSpreadIndexerOnRightData {
                ..
            }) => Some(CannotSpreadIndexer),
            ErrorMessage::ECannotSpreadInterface(box ECannotSpreadInterfaceData { .. }) => {
                Some(CannotSpreadInterface)
            }
            ErrorMessage::ECodelessSuppression(_) => None,
            ErrorMessage::ENonStrictEqualityComparison { .. }
            | ErrorMessage::EComparison(box EComparisonData { .. }) => Some(InvalidCompare),

            ErrorMessage::EDevOnlyRefinedLocInfo(box EDevOnlyRefinedLocInfoData { .. }) => None,
            ErrorMessage::EDevOnlyInvalidatedRefinementInfo(
                box EDevOnlyInvalidatedRefinementInfoData { .. },
            ) => None,
            ErrorMessage::EIncorrectTypeWithReplacement(
                box EIncorrectTypeWithReplacementData { kind, .. },
            ) => match kind.error_type_of_kind() {
                IncorrectTypeErrorType::DeprecatedUtility => Some(DeprecatedUtility),
                IncorrectTypeErrorType::TSType => Some(UnsupportedSyntax),
            },
            ErrorMessage::EDocblockError(box (_, err)) => match err {
                DocblockError::MultipleFlowAttributes => Some(DuplicateFlowDecl),
                DocblockError::InvalidFlowMode(_) => Some(InvalidFlowModeDecl),
                DocblockError::MultipleJSXAttributes => Some(DuplicateJsxDecl),
                DocblockError::InvalidJSXAttribute(_) => Some(InvalidJsxDecl),
                DocblockError::MultipleJSXRuntimeAttributes => Some(DuplicateJsxRuntimeDecl),
                DocblockError::InvalidJSXRuntimeAttribute => Some(InvalidJsxRuntimeDecl),
                DocblockError::InvalidSupportsPlatform(_) => Some(InvalidSupportsPlatformDecl),
                DocblockError::DisallowedSupportsPlatform => Some(InvalidSupportsPlatformDecl),
            },
            ErrorMessage::EDuplicateModuleProvider(box EDuplicateModuleProviderData { .. }) => {
                Some(DuplicateModule)
            }
            ErrorMessage::EEnumError(EnumErrorKind::EnumAllMembersAlreadyChecked(
                box EnumAllMembersAlreadyCheckedData { .. },
            )) => Some(InvalidExhaustiveCheck),
            ErrorMessage::EEnumError(EnumErrorKind::EnumInvalidAbstractUse(
                box EnumInvalidAbstractUseData { .. },
            )) => Some(InvalidExhaustiveCheck),
            ErrorMessage::EEnumError(EnumErrorKind::EnumInvalidMemberName(
                box EnumInvalidMemberNameData { .. },
            )) => Some(InvalidEnumMemberName),
            ErrorMessage::EEnumError(EnumErrorKind::EnumNonIdentifierMemberName(
                box EnumNonIdentifierMemberNameData { .. },
            )) => Some(InvalidEnumMemberName),
            ErrorMessage::EEnumError(EnumErrorKind::EnumInvalidCheck(
                box EnumInvalidCheckData { from_match, .. },
            )) => {
                if *from_match {
                    Some(MatchInvalidPattern)
                } else {
                    Some(InvalidExhaustiveCheck)
                }
            }
            ErrorMessage::EEnumError(EnumErrorKind::EnumInvalidMemberAccess(
                box EnumInvalidMemberAccessData { .. },
            )) => Some(InvalidEnumAccess),
            ErrorMessage::EEnumError(EnumErrorKind::EnumInvalidObjectUtilType(
                box EnumInvalidObjectUtilTypeData { .. },
            )) => Some(NotAnObject),
            ErrorMessage::EEnumError(EnumErrorKind::EnumInvalidObjectFunction(
                box EnumInvalidObjectFunctionData { .. },
            )) => Some(NotAnObject),
            ErrorMessage::EEnumError(
                EnumErrorKind::EnumNotIterable(_) | EnumErrorKind::EnumNotIterableForIn(_),
            ) => Some(NotIterable),
            ErrorMessage::EEnumError(EnumErrorKind::EnumMemberAlreadyChecked(
                box EnumMemberAlreadyCheckedData { .. },
            )) => Some(InvalidExhaustiveCheck),
            ErrorMessage::EEnumError(EnumErrorKind::EnumMemberDuplicateValue(
                box EnumMemberDuplicateValueData { .. },
            )) => Some(DuplicateEnumInit),
            ErrorMessage::EEnumError(EnumErrorKind::EnumMemberUsedAsType(
                box EnumMemberUsedAsTypeData { .. },
            )) => Some(EnumValueAsType),
            ErrorMessage::EEnumError(EnumErrorKind::EnumModification(
                box EnumModificationData { .. },
            )) => Some(CannotWriteEnum),
            ErrorMessage::EEnumError(EnumErrorKind::EnumNotAllChecked(
                box EnumNotAllCheckedData {
                    default_case_loc: None,
                    ..
                },
            )) => Some(InvalidExhaustiveCheck),
            ErrorMessage::EEnumError(EnumErrorKind::EnumNotAllChecked(
                box EnumNotAllCheckedData {
                    default_case_loc: Some(_),
                    ..
                },
            )) => Some(RequireExplicitEnumSwitchCases),
            ErrorMessage::EEnumError(EnumErrorKind::EnumUnknownNotChecked(
                box EnumUnknownNotCheckedData { .. },
            )) => Some(InvalidExhaustiveCheck),
            ErrorMessage::EExpectedBooleanLit(box EExpectedBooleanLitData { use_op, .. }) => {
                Self::error_code_of_use_op(&Some(use_op.dupe()), IncompatibleType)
            }
            ErrorMessage::EExpectedNumberLit(box EExpectedNumberLitData { use_op, .. }) => {
                Self::error_code_of_use_op(&Some(use_op.dupe()), IncompatibleType)
            }
            ErrorMessage::EExpectedStringLit(box EExpectedStringLitData { use_op, .. }) => {
                Self::error_code_of_use_op(&Some(use_op.dupe()), IncompatibleType)
            }
            ErrorMessage::EExpectedBigIntLit(box EExpectedBigIntLitData { use_op, .. }) => {
                Self::error_code_of_use_op(&Some(use_op.dupe()), IncompatibleType)
            }
            ErrorMessage::EEnumError(EnumErrorKind::EnumsNotEnabled { .. }) => {
                Some(UnsupportedSyntax)
            }
            ErrorMessage::EEnumError(EnumErrorKind::EnumConstNotSupported { .. }) => {
                Some(UnsupportedSyntax)
            }
            ErrorMessage::EEnumError(EnumErrorKind::EnumDuplicateMemberName(
                box EnumDuplicateMemberNameData { .. },
            )) => Some(InvalidEnum),
            ErrorMessage::EEnumError(EnumErrorKind::EnumInconsistentMemberValues(
                box EnumInconsistentMemberValuesData { .. },
            )) => Some(InvalidEnum),
            ErrorMessage::EEnumError(EnumErrorKind::EnumInvalidMemberInitializer(
                box EnumInvalidMemberInitializerData { .. },
            )) => Some(InvalidEnum),
            ErrorMessage::EEnumError(EnumErrorKind::EnumBooleanMemberNotInitialized(
                box EnumBooleanMemberNotInitializedData { .. },
            )) => Some(InvalidEnum),
            ErrorMessage::EEnumError(EnumErrorKind::EnumNumberMemberNotInitialized(
                box EnumNumberMemberNotInitializedData { .. },
            )) => Some(InvalidEnum),
            ErrorMessage::EEnumError(EnumErrorKind::EnumBigIntMemberNotInitialized(
                box EnumBigIntMemberNotInitializedData { .. },
            )) => Some(InvalidEnum),
            ErrorMessage::EEnumError(EnumErrorKind::EnumStringMemberInconsistentlyInitialized(
                box EnumStringMemberInconsistentlyInitializedData { .. },
            )) => Some(InvalidEnum),
            ErrorMessage::EEnumError(EnumErrorKind::TSEnumInvalidMember(
                box TSEnumInvalidMemberData { .. },
            )) => Some(InvalidEnum),
            ErrorMessage::EEnumError(EnumErrorKind::TSEnumInvalidSyntax(
                box TSEnumInvalidSyntaxData { .. },
            )) => Some(InvalidEnum),
            ErrorMessage::EExponentialSpread(box EExponentialSpreadData { .. }) => {
                Some(ExponentialSpread)
            }
            ErrorMessage::EExportsAnnot { .. } => Some(InvalidExportsTypeArg),
            ErrorMessage::EExportValueAsType(box (_, _)) => Some(ExportValueAsType),
            ErrorMessage::EForInRHS { .. } => Some(InvalidInRhs),
            ErrorMessage::EInstanceofRHS { .. } => Some(InvalidInRhs),
            ErrorMessage::EFunctionCallExtraArg { .. } => Some(ExtraArg),
            ErrorMessage::ETypeGuardFuncIncompatibility { .. }
            | ErrorMessage::ETypeGuardInvalidParameter(box ETypeGuardInvalidParameterData {
                ..
            })
            | ErrorMessage::ETypeGuardIndexMismatch { .. }
            | ErrorMessage::ETypeGuardImpliesMismatch { .. }
            | ErrorMessage::ETypeGuardParamUnbound { .. }
            | ErrorMessage::ETypeGuardThisParam { .. }
            | ErrorMessage::ETypeGuardFunctionInvalidWrites(
                box ETypeGuardFunctionInvalidWritesData { .. },
            )
            | ErrorMessage::ETypeGuardFunctionParamHavoced(
                box ETypeGuardFunctionParamHavocedData { .. },
            )
            | ErrorMessage::ETypeGuardIncompatibleWithFunctionKind(
                box ETypeGuardIncompatibleWithFunctionKindData { .. },
            ) => Some(FunctionPredicate),
            ErrorMessage::ENegativeTypeGuardConsistency(
                box ENegativeTypeGuardConsistencyData { .. },
            )
            | ErrorMessage::EAssertionFunctionFalsyAtExit(
                box EAssertionFunctionFalsyAtExitData { .. },
            ) => Some(IncompatibleTypeGuard),
            ErrorMessage::EUnrecognizedAssertionCall(box EUnrecognizedAssertionCallData {
                ..
            }) => Some(UnrecognizedAssertionCall),
            ErrorMessage::ETypeParamConstIncompatibility(
                box ETypeParamConstIncompatibilityData { .. },
            )
            | ErrorMessage::ETypeParamConstInvalidPosition { .. } => Some(TypeParamConstCode),
            ErrorMessage::EImportTypeAsTypeof(box (_, _)) => Some(InvalidImportType),
            ErrorMessage::EImportTypeAsValue(box (_, _)) => Some(ImportTypeAsValue),
            ErrorMessage::EImportValueAsType(box (_, _)) => Some(ImportValueAsType),
            ErrorMessage::EIncompatibleType(box EIncompatibleTypeData {
                upper:
                    IncompatibleUpperData {
                        kind: upper_kind, ..
                    },
                ..
            }) => Self::error_code_of_upper_kind(upper_kind),
            ErrorMessage::EIncompatibleSpeculation(box EIncompatibleSpeculationData {
                use_op: Some(use_op),
                ..
            }) => Self::error_code_of_use_op(&Some(use_op.dupe()), IncompatibleUse),
            ErrorMessage::EIncompatibleSpeculation(..) => Some(IncompatibleUse),
            ErrorMessage::EIncompatibleProp(box EIncompatiblePropData {
                use_op: Some(use_op),
                ..
            }) => Self::error_code_of_use_op(&Some(use_op.dupe()), IncompatibleType),
            ErrorMessage::EIncompatibleProp(box EIncompatiblePropData { use_op: None, .. }) => {
                Some(IncompatibleType)
            }
            ErrorMessage::EIncompatibleWithExact(_, _, ExactnessErrorKind::UnexpectedInexact) => {
                Some(IncompatibleExact)
            }
            ErrorMessage::EIncompatibleWithExact(_, _, ExactnessErrorKind::UnexpectedIndexer) => {
                Some(IncompatibleIndexer)
            }
            ErrorMessage::EFunctionIncompatibleWithIndexer { .. } => {
                Some(IncompatibleFunctionIndexer)
            }
            ErrorMessage::EEnumError(EnumErrorKind::EnumIncompatible(
                box EIncompatibleTypesWithUseOpData { use_op, .. },
            ))
            | ErrorMessage::EIncompatibleTypesWithUseOp(box EIncompatibleTypesWithUseOpData {
                use_op,
                ..
            }) => Self::error_code_of_use_op(&Some(use_op.dupe()), IncompatibleType),
            ErrorMessage::EInvariantSubtypingWithUseOp(box EInvariantSubtypingWithUseOpData {
                use_op,
                ..
            }) => Self::error_code_of_use_op(&Some(use_op.dupe()), IncompatibleType),
            ErrorMessage::EIndeterminateModuleType { .. } => Some(ModuleTypeConflict),
            ErrorMessage::EInexactMayOverwriteIndexer(box EInexactMayOverwriteIndexerData {
                ..
            }) => Some(CannotSpreadInexact),
            ErrorMessage::EInternal(box (_, _)) => None,
            ErrorMessage::EInvalidConstructor { .. } => Some(InvalidConstructor),
            ErrorMessage::EInvalidLHSInAssignment { .. } => Some(InvalidLhs),
            ErrorMessage::EInvalidObjectKit(box EInvalidObjectKitData { .. }) => Some(NotAnObject),
            ErrorMessage::EInvalidPrototype { .. } => Some(NotAnObject),
            ErrorMessage::EInvalidReactCreateElement(box EInvalidReactCreateElementData {
                ..
            }) => Some(InvalidReactCreateElement),
            ErrorMessage::EInvalidTypeof { .. } => Some(IllegalTypeof),
            ErrorMessage::EInvalidInfer { .. } => Some(InvalidInfer),
            ErrorMessage::EConstantCondition(box EConstantConditionData { .. }) => {
                Some(ConstantCondition)
            }
            ErrorMessage::EInvalidExtends { .. } => Some(InvalidExtends),
            ErrorMessage::ELintSetting { .. } => Some(LintSetting),
            ErrorMessage::EMissingLocalAnnotation { .. } => Some(MissingLocalAnnot),
            ErrorMessage::EMissingParameterAnnotation { .. } => Some(MissingLocalAnnot),
            ErrorMessage::EMissingThisAnnotation(_) => Some(MissingThisAnnot),
            ErrorMessage::EMissingTypeArgs(box EMissingTypeArgsData { .. }) => Some(MissingTypeArg),
            ErrorMessage::EMixedImportAndRequire { .. } => Some(MixedImportAndRequire),
            ErrorMessage::EUnsupportedVarianceAnnotation { .. } => {
                Some(UnsupportedVarianceAnnotation)
            }
            ErrorMessage::ENoDefaultExport(box (_, _, _)) => Some(MissingExport),
            ErrorMessage::ENoNamedExport(box (_, _, _, _)) => Some(MissingExport),
            ErrorMessage::ENonConstVarExport { .. } => Some(NonConstVarExport),
            ErrorMessage::ENonLitArrayToTuple { .. } => Some(InvalidTupleArity),
            ErrorMessage::ENotAReactComponent(..) => Some(NotAComponent),
            ErrorMessage::EObjectComputedPropertyAccess(
                box EObjectComputedPropertyAccessData { .. },
            ) => Some(InvalidComputedProp),
            ErrorMessage::EObjectComputedPropertyAssign { .. } => Some(InvalidComputedProp),
            ErrorMessage::EObjectComputedPropertyPotentialOverwrite(
                box EObjectComputedPropertyPotentialOverwriteData { .. },
            ) => Some(InvalidComputedProp),
            ErrorMessage::EOnlyDefaultExport(box (_, _, _)) => Some(MissingExport),
            ErrorMessage::EParseError(box (_, _)) => None,
            ErrorMessage::EPolarityMismatch(box EPolarityMismatchData { .. }) => {
                Some(IncompatibleVariance)
            }
            ErrorMessage::EPrimitiveAsInterface(box EPrimitiveAsInterfaceData { .. }) => {
                Some(IncompatibleType)
            }
            ErrorMessage::EPrivateLookupFailed { .. } => Some(PropMissing),
            ErrorMessage::EPropNotFoundInLookup(box EPropNotFoundInLookupData {
                use_op, ..
            }) => {
                let default = match use_op {
                    VirtualUseOp::Op(root)
                        if matches!(root.as_ref(), VirtualRootUseOp::GetExport(_)) =>
                    {
                        MissingExport
                    }
                    _ => PropMissing,
                };
                Self::react_rule_of_use_op(&Some(use_op.dupe()), default)
            }
            ErrorMessage::EPropNotFoundInSubtyping(box EPropNotFoundInSubtypingData {
                use_op,
                ..
            })
            | ErrorMessage::EConstructSignatureMissingInSubtyping(
                box EConstructSignatureMissingInSubtypingData { use_op, .. },
            )
            | ErrorMessage::EPropsNotFoundInSubtyping(box EPropsNotFoundInSubtypingData {
                use_op,
                ..
            })
            | ErrorMessage::EPropsExtraAgainstExactObject(
                box EPropsExtraAgainstExactObjectData { use_op, .. },
            ) => Self::react_rule_of_use_op(&Some(use_op.dupe()), IncompatibleType),
            ErrorMessage::EPropsNotFoundInInvariantSubtyping(
                box EPropsNotFoundInInvariantSubtypingData { use_op, .. },
            ) => Self::react_rule_of_use_op(&Some(use_op.dupe()), IncompatibleType),
            ErrorMessage::EIndexerCheckFailed(box EIndexerCheckFailedData { use_op, .. }) => {
                Self::react_rule_of_use_op(&Some(use_op.dupe()), IncompatibleType)
            }
            ErrorMessage::EPropNotReadable(box EPropNotReadableData { use_op, .. }) => {
                Self::react_rule_of_use_op(&Some(use_op.dupe()), CannotRead)
            }
            ErrorMessage::EPropNotWritable(box EPropNotWritableData { use_op, .. }) => {
                Self::react_rule_of_use_op(&Some(use_op.dupe()), CannotWrite)
            }
            ErrorMessage::EPropPolarityMismatch(box EPropPolarityMismatchData { .. }) => {
                Some(IncompatibleVariance)
            }
            ErrorMessage::EReactRefInRender { .. } => Some(ReactRuleRef),
            ErrorMessage::ERecursionLimit(_) => None,
            ErrorMessage::EROArrayWrite(_, use_op) => {
                Self::react_rule_of_use_op(&Some(use_op.dupe()), CannotWrite)
            }
            ErrorMessage::ESignatureBindingValidation(
                flow_type_sig::signature_error::BindingValidation::ModuleOverride { .. },
            )
            | ErrorMessage::ESignatureBindingValidation(
                flow_type_sig::signature_error::BindingValidation::NameOverride { .. },
            ) => Some(LibdefOverride),
            ErrorMessage::ESignatureBindingValidation(
                flow_type_sig::signature_error::BindingValidation::InterfaceMergePropertyConflict {
                    ..
                },
            )
            | ErrorMessage::ESignatureBindingValidation(
                flow_type_sig::signature_error::BindingValidation::InterfaceMergeTparamMismatch {
                    ..
                },
            ) => Some(LibdefOverride),
            ErrorMessage::ESignatureBindingValidation(
                flow_type_sig::signature_error::BindingValidation::NamespacedNameAlreadyBound {
                    ..
                },
            ) => Some(SignatureVerificationFailure),
            ErrorMessage::ESignatureVerification { .. } => Some(SignatureVerificationFailure),
            ErrorMessage::EThisInExportedFunction { .. } => Some(ThisInExportedFunction),
            ErrorMessage::EExportRenamedDefault(box EExportRenamedDefaultData { .. }) => {
                Some(ExportRenamedDefault)
            }
            ErrorMessage::ETooFewTypeArgs(box ETooFewTypeArgsData { .. }) => Some(MissingTypeArg),
            ErrorMessage::ETooManyTypeArgs(box ETooManyTypeArgsData { .. }) => Some(ExtraTypeArg),
            ErrorMessage::ETupleArityMismatch(box ETupleArityMismatchData { .. }) => {
                Some(InvalidTupleArity)
            }
            ErrorMessage::ETupleRequiredAfterOptional(box ETupleRequiredAfterOptionalData {
                ..
            }) => Some(TupleRequiredAfterOptional),
            ErrorMessage::ETupleInvalidTypeSpread(box ETupleInvalidTypeSpreadData { .. }) => {
                Some(TupleInvalidTypeSpread)
            }
            ErrorMessage::ETupleElementAfterInexactSpread { .. } => {
                Some(ElementAfterInexactTupleSpread)
            }
            ErrorMessage::ETupleElementAfterRestSpread { .. } => Some(ElementAfterTupleRestSpread),
            ErrorMessage::ETupleElementNotReadable(box ETupleElementNotReadableData { .. }) => {
                Some(CannotRead)
            }
            ErrorMessage::ETupleElementNotWritable(box ETupleElementNotWritableData { .. }) => {
                Some(CannotWrite)
            }
            ErrorMessage::ETupleElementPolarityMismatch(
                box ETupleElementPolarityMismatchData { .. },
            ) => Some(IncompatibleVariance),
            ErrorMessage::ETupleNonIntegerIndex(box ETupleNonIntegerIndexData { .. }) => {
                Some(InvalidTupleIndex)
            }
            ErrorMessage::ETupleOutOfBounds(box ETupleOutOfBoundsData { .. }) => {
                Some(InvalidTupleIndex)
            }
            ErrorMessage::ETupleUnsafeWrite { .. } => Some(InvalidTupleIndex),
            ErrorMessage::ETypeParamArity(_, _) => Some(NonpolymorphicTypeApp),
            ErrorMessage::EUnableToSpread(box EUnableToSpreadData { error_kind, .. }) => {
                match error_kind {
                    ExactnessErrorKind::UnexpectedInexact => Some(CannotSpreadInexact),
                    ExactnessErrorKind::UnexpectedIndexer => Some(CannotSpreadIndexer),
                }
            }
            ErrorMessage::EUnexpectedThisType { .. } => Some(IllegalThis),
            ErrorMessage::EUnionSpeculationFailed(box EUnionSpeculationFailedData {
                use_op,
                ..
            }) => Self::error_code_of_use_op(&Some(use_op.dupe()), IncompatibleType),
            ErrorMessage::EUnreachable { .. } => Some(UnreachableCode),
            ErrorMessage::EUnsupportedExact(box EUnsupportedExactData { .. }) => Some(InvalidExact),
            ErrorMessage::EInvalidExactObjectIntersection(_) => Some(InvalidExact),
            ErrorMessage::EUnsupportedImplements(_) => Some(CannotImplement),
            ErrorMessage::EUnsupportedKeyInObject { .. } => Some(IllegalKey),
            ErrorMessage::EUnsupportedComputedKeyInClass(_) => Some(IllegalKey),
            ErrorMessage::EAmbiguousNumericKeyWithVariance { .. } => Some(IllegalKey),
            ErrorMessage::EUnsupportedSetProto { .. } => Some(CannotWrite),
            ErrorMessage::EUnsupportedSyntax(box (_, _)) => Some(UnsupportedSyntax),
            ErrorMessage::EImplicitInstantiationUnderconstrainedError(
                box EImplicitInstantiationUnderconstrainedErrorData { .. },
            ) => Some(UnderconstrainedImplicitInstantiation),
            ErrorMessage::EObjectThisSuperReference { .. } => Some(ObjectThisReference),
            ErrorMessage::EComponentThisReference(box EComponentThisReferenceData { .. }) => {
                Some(ComponentThisReference)
            }
            ErrorMessage::EComponentCase { .. } => Some(ComponentCase),
            ErrorMessage::EDeclareComponentInvalidParam { .. } => Some(InvalidComponentProp),
            ErrorMessage::EComponentMissingReturn { .. } => Some(ComponentMissingReturn),
            ErrorMessage::EComponentMissingBody { .. } => Some(ComponentMissingBody),
            ErrorMessage::EComponentBodyInAmbientContext { .. } => {
                Some(ComponentBodyInAmbientContext)
            }
            ErrorMessage::EInvalidDeclaration(box EInvalidDeclarationData { .. }) => {
                Some(InvalidDeclaration)
            }
            ErrorMessage::EInvalidMappedType { .. } => Some(InvalidMappedType),
            ErrorMessage::EInvalidTemplateLiteralType { .. } => Some(InvalidTemplateLiteralType),
            ErrorMessage::EDuplicateComponentProp(box EDuplicateComponentPropData { .. }) => {
                Some(InvalidComponentProp)
            }
            ErrorMessage::EKeySpreadProp(box EKeySpreadPropData { .. }) => Some(InvalidSpreadProp),
            ErrorMessage::EInvalidComponentRestParam { .. } => Some(InvalidComponentProp),
            ErrorMessage::EMalformedCode { .. } | ErrorMessage::EUnusedSuppression { .. } => None,
            ErrorMessage::EUseArrayLiteral { .. } => Some(IllegalNewArray),
            ErrorMessage::EAnyValueUsedAsType { .. } | ErrorMessage::EValueUsedAsType { .. } => {
                Some(ValueAsType)
            }
            ErrorMessage::EClassToObject(box EClassToObjectData { .. }) => Some(ClassObject),
            ErrorMessage::EHookIncompatible(box EHookIncompatibleData { .. })
            | ErrorMessage::EHookUniqueIncompatible(box EHookUniqueIncompatibleData { .. }) => {
                Some(ReactRuleHookIncompatible)
            }
            ErrorMessage::EHookNaming { .. } => Some(ReactRuleHookNamingConvention),
            ErrorMessage::EHookRuleViolation(box EHookRuleViolationData {
                hook_rule: HookRule::ConditionalHook,
                ..
            }) => Some(ReactRuleHookConditional),
            ErrorMessage::EHookRuleViolation(box EHookRuleViolationData {
                hook_rule: HookRule::HookHasIllegalName,
                ..
            }) => Some(ReactRuleHookNamingConvention),
            ErrorMessage::EHookRuleViolation(box EHookRuleViolationData {
                hook_rule: HookRule::HookDefinitelyNotInComponentOrHook,
                ..
            }) => Some(ReactRuleHookDefinitelyNotInComponentOrHook),
            ErrorMessage::EHookRuleViolation(box EHookRuleViolationData {
                hook_rule: HookRule::MaybeHook { .. },
                ..
            }) => Some(ReactRuleHookMixedWithNonHoook),
            ErrorMessage::EHookRuleViolation(box EHookRuleViolationData {
                hook_rule: HookRule::NotHookSyntaxHook,
                ..
            }) => Some(ReactRuleHookNonHookSyntax),
            ErrorMessage::EHookRuleViolation(box EHookRuleViolationData {
                hook_rule:
                    HookRule::HookInUnknownContext
                    | HookRule::HookNotInComponentSyntaxComponentOrHookSyntaxHook,
                ..
            }) => Some(ReactRuleHook),
            ErrorMessage::EInvalidGraphQL { .. } => Some(InvalidGraphQL),
            ErrorMessage::EAnnotationInference { .. } => Some(InvalidExportedAnnotation),
            ErrorMessage::ETrivialRecursiveDefinition(_) => Some(RecursiveDefinition),
            ErrorMessage::ETrivialRecursiveTypeParameter(_) => Some(RecursiveDefinition),
            ErrorMessage::EClassTypeParamInStatic(_) => Some(CannotResolveName),
            ErrorMessage::EDefinitionCycle { .. } => Some(DefinitionCycle),
            ErrorMessage::ERecursiveDefinition(box ERecursiveDefinitionData { .. }) => {
                Some(RecursiveDefinition)
            }
            ErrorMessage::EReferenceInAnnotation { .. } => Some(RecursiveDefinition),
            ErrorMessage::EReferenceInDefault { .. } => Some(RecursiveDefinition),
            ErrorMessage::EDuplicateClassMember(box EDuplicateClassMemberData { .. }) => {
                Some(DuplicateClassMember)
            }
            ErrorMessage::EEmptyArrayNoProvider { .. } => Some(EmptyArrayNoAnnot),
            ErrorMessage::EBigIntRShift3 { .. } => Some(BigIntRShift3),
            ErrorMessage::EBigIntNumCoerce { .. } => Some(BigIntNumCoerce),
            ErrorMessage::EInvalidCatchParameterAnnotation { .. } => {
                Some(InvalidCatchParameterAnnotation)
            }
            ErrorMessage::EInvalidRendersTypeArgument(box EInvalidRendersTypeArgumentData {
                ..
            }) => Some(InvalidRendersTypeArgument),
            ErrorMessage::EUnnecessaryDeclareTypeOnlyExport { .. } => {
                Some(UnnecessaryDeclareTypeOnlyExport)
            }
            ErrorMessage::EDeprecatedBool { .. } => Some(DeprecatedType),
            ErrorMessage::EUntypedTypeImport { .. }
            | ErrorMessage::EUntypedImport { .. }
            | ErrorMessage::ENonstrictImport { .. }
            | ErrorMessage::EInternalType { .. }
            | ErrorMessage::EUnclearType { .. }
            | ErrorMessage::EUnsafeObjectAssign { .. }
            | ErrorMessage::EUnsafeGettersSetters { .. }
            | ErrorMessage::ESketchyNullLint(box ESketchyNullLintData { .. })
            | ErrorMessage::ESketchyNumberLint { .. }
            | ErrorMessage::EUnnecessaryOptionalChain { .. }
            | ErrorMessage::EUnnecessaryInvariant { .. }
            | ErrorMessage::EAmbiguousObjectType { .. }
            | ErrorMessage::EReactIntrinsicOverlap(box EReactIntrinsicOverlapData { .. })
            | ErrorMessage::EUninitializedInstanceProperty { .. }
            | ErrorMessage::ENestedComponent { .. }
            | ErrorMessage::ENestedHook { .. }
            | ErrorMessage::EInvalidThisArg(box EInvalidThisArgData { .. })
            | ErrorMessage::EUnusedPromise { .. } => match self.kind_of_msg() {
                ErrorKind::LintError(kind) => {
                    Some(flow_common_errors::error_codes::code_of_lint(&kind))
                }
                _ => None,
            },
            ErrorMessage::ETSSyntax(box ETSSyntaxData { .. }) => Some(UnsupportedSyntax),
            ErrorMessage::EUninitializedProperty(_) => Some(UninitializedProperty),
            ErrorMessage::EAbstractClass(box EAbstractClassData { .. }) => Some(InvalidAbstract),
            ErrorMessage::EOverride(box EOverrideData { .. }) => Some(InvalidOverride),
            ErrorMessage::EVarianceKeyword(box EVarianceKeywordData { .. }) => {
                Some(UnsupportedSyntax)
            }
            ErrorMessage::EInvalidTypeCastSyntax { .. } => Some(InvalidTypeCastSyntax),
            ErrorMessage::EMissingPlatformSupportWithAvailablePlatforms(
                box EMissingPlatformSupportWithAvailablePlatformsData { .. },
            ) => Some(MissingPlatformSupport),
            ErrorMessage::EUnionPartialOptimizationNonUniqueKey(..) => {
                Some(UnionPartiallyOptimizableNonUniqueKeys)
            }
            ErrorMessage::EUnionOptimization(box EUnionOptimizationData { .. }) => {
                Some(UnionUnoptimizable)
            }
            ErrorMessage::EUnionOptimizationOnNonUnion(box EUnionOptimizationOnNonUnionData {
                ..
            }) => Some(UnionUnoptimizable),
            ErrorMessage::ECannotCallReactComponent { .. } => Some(ReactRuleCallComponent),
            ErrorMessage::EMatchError(
                MatchErrorKind::MatchNotExhaustive(box MatchNotExhaustiveData { .. })
                | MatchErrorKind::MatchNonExhaustiveObjectPattern(
                    box MatchNonExhaustiveObjectPatternData { .. },
                )
                | MatchErrorKind::MatchInvalidGuardedWildcard(_),
            ) => Some(MatchNotExhaustive),
            ErrorMessage::EMatchError(MatchErrorKind::MatchNonExplicitEnumCheck(
                box MatchNonExplicitEnumCheckData { .. },
            )) => Some(RequireExplicitEnumChecks),
            ErrorMessage::EMatchError(MatchErrorKind::MatchUnusedPattern(
                box MatchUnusedPatternData { .. },
            )) => Some(MatchUnusedPattern),
            ErrorMessage::EMatchError(
                MatchErrorKind::MatchInvalidIdentOrMemberPattern(
                    box MatchInvalidIdentOrMemberPatternData { .. },
                )
                | MatchErrorKind::MatchInvalidBindingKind { .. }
                | MatchErrorKind::MatchInvalidObjectPropertyLiteral { .. }
                | MatchErrorKind::MatchInvalidUnaryZero { .. }
                | MatchErrorKind::MatchInvalidUnaryPlusBigInt { .. }
                | MatchErrorKind::MatchDuplicateObjectProperty(
                    box MatchDuplicateObjectPropertyData { .. },
                )
                | MatchErrorKind::MatchBindingInOrPattern { .. }
                | MatchErrorKind::MatchInvalidAsPattern { .. }
                | MatchErrorKind::MatchInvalidPatternReference(
                    box MatchInvalidPatternReferenceData { .. },
                )
                | MatchErrorKind::MatchInvalidObjectShorthand(box MatchInvalidObjectShorthandData {
                    ..
                })
                | MatchErrorKind::MatchInvalidInstancePattern(_),
            ) => Some(MatchInvalidPattern),
            ErrorMessage::EMatchError(MatchErrorKind::MatchStatementInvalidBody { .. }) => {
                Some(MatchStatementInvalidBody)
            }
            ErrorMessage::EMatchError(
                MatchErrorKind::MatchInvalidCaseSyntax(box MatchInvalidCaseSyntaxData { .. })
                | MatchErrorKind::MatchInvalidWildcardSyntax(_),
            ) => Some(UnsupportedSyntax),
            ErrorMessage::ERecordError(e) => match e {
                RecordErrorKind::RecordBannedTypeUtil { .. } => Some(RecordBannedTypeUtil),
                RecordErrorKind::RecordInvalidName { .. } => Some(RecordInvalidName),
                RecordErrorKind::RecordInvalidNew { .. } => Some(RecordInvalidNew),
                RecordErrorKind::RecordDeclarationInvalidSyntax { .. } => Some(RecordInvalidSyntax),
            },
            ErrorMessage::EUndocumentedFeature { .. } => Some(UndocumentedFeature),
            ErrorMessage::EIllegalAssertOperator(box EIllegalAssertOperatorData { .. }) => {
                Some(IllegalAssertOperator)
            }
        }
    }
}

/// Map location in SignatureError
fn map_signature_error<L, M, F>(f: &F, error: SignatureError<L>) -> SignatureError<M>
where
    F: Fn(L) -> M,
    L: Clone,
    M: Clone,
{
    use flow_type_sig::signature_error::SignatureError::*;

    match error {
        ExpectedAnnotation(loc, sort) => ExpectedAnnotation(f(loc), sort),
        UnexpectedObjectKey(loc, key_loc) => UnexpectedObjectKey(f(loc), f(key_loc)),
        UnexpectedArraySpread(loc, spread_loc) => UnexpectedArraySpread(f(loc), f(spread_loc)),
        UnexpectedArrayHole(loc) => UnexpectedArrayHole(f(loc)),
        EmptyArray(loc) => EmptyArray(f(loc)),
        EmptyObject(loc) => EmptyObject(f(loc)),
        UnexpectedExpression(loc, kind) => UnexpectedExpression(f(loc), kind),
    }
}

/// Map location in BindingValidation
fn map_binding_validation<L, M, F>(f: &F, validation: BindingValidation<L>) -> BindingValidation<M>
where
    F: Fn(L) -> M,
    L: Clone,
    M: Clone,
{
    use flow_type_sig::signature_error::BindingValidation::*;

    match validation {
        ModuleOverride {
            name,
            override_binding_loc,
            existing_binding_loc,
        } => ModuleOverride {
            name,
            override_binding_loc: f(override_binding_loc),
            existing_binding_loc: f(existing_binding_loc),
        },
        NameOverride {
            name,
            override_binding_loc,
            existing_binding_loc,
        } => NameOverride {
            name,
            override_binding_loc: f(override_binding_loc),
            existing_binding_loc: f(existing_binding_loc),
        },
        NamespacedNameAlreadyBound {
            name,
            invalid_binding_loc,
            existing_binding_loc,
        } => NamespacedNameAlreadyBound {
            name,
            invalid_binding_loc: f(invalid_binding_loc),
            existing_binding_loc: f(existing_binding_loc),
        },
        InterfaceMergePropertyConflict {
            name,
            current_binding_loc,
            existing_binding_loc,
        } => InterfaceMergePropertyConflict {
            name,
            current_binding_loc: f(current_binding_loc),
            existing_binding_loc: f(existing_binding_loc),
        },
        InterfaceMergeTparamMismatch {
            name,
            current_binding_loc,
            existing_binding_loc,
        } => InterfaceMergeTparamMismatch {
            name,
            current_binding_loc: f(current_binding_loc),
            existing_binding_loc: f(existing_binding_loc),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_timeout_preserves_three_digit_seconds() {
        let error = InternalError::check_timeout(Duration::from_millis(100_003));

        assert_eq!(
            string_of_internal_error(&error).to_string(),
            "check job timed out after 100.00 seconds"
        );
    }
}
