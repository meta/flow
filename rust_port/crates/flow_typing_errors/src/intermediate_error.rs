/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::collections::BTreeMap;
use std::sync::Arc;

use dupe::Dupe;
use flow_aloc::ALoc;
use flow_common::polarity::Polarity;
use flow_common::reason::Name;
use flow_common::reason::VirtualReason;
use flow_common::reason::VirtualReasonDesc;
use flow_common::reason::mk_reason;
use flow_common_errors::error_codes::ErrorCode;
use flow_common_errors::error_utils::ConcreteLocPrintableErrorSet;
use flow_common_errors::error_utils::ErrorKind;
use flow_common_errors::error_utils::PrintableError;
use flow_common_ty::ty::Decl;
use flow_common_ty::ty::Elt;
use flow_common_ty::ty::Ty;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_env_builder::name_def_types::DefinitionReferenceKind;
use flow_parser::file_key::FileKey;
use flow_parser::jsdoc;
use flow_parser::loc::LOC_NONE;
use flow_parser::loc::Loc;
use flow_typing_type::type_::ArrayElementCompatibilityData;
use flow_typing_type::type_::AssertionFunctionConsistencyData;
use flow_typing_type::type_::AssignVarDefaultData;
use flow_typing_type::type_::AssignVarInit;
use flow_typing_type::type_::AssignVarTarget;
use flow_typing_type::type_::ClassExtendsCheckData;
use flow_typing_type::type_::ClassImplementsCheckData;
use flow_typing_type::type_::ClassMethodDefinitionData;
use flow_typing_type::type_::ClassOwnProtoCheckData;
use flow_typing_type::type_::ComponentRestParamCompatibilityData;
use flow_typing_type::type_::ConformToCommonInterfaceData;
use flow_typing_type::type_::ConstrainedAssignmentData;
use flow_typing_type::type_::DefTInner;
use flow_typing_type::type_::DroType;
use flow_typing_type::type_::EvalMappedTypeData;
use flow_typing_type::type_::FunCallData;
use flow_typing_type::type_::FunCallMethodCallee;
use flow_typing_type::type_::FunCallMethodData;
use flow_typing_type::type_::FunImplicitReturnData;
use flow_typing_type::type_::FunMissingArgData;
use flow_typing_type::type_::FunParamData;
use flow_typing_type::type_::FunReturnData;
use flow_typing_type::type_::ImplicitInstantiationReferenceKind;
use flow_typing_type::type_::IndexedTypeAccessData;
use flow_typing_type::type_::IndexerKeyCompatibilityData;
use flow_typing_type::type_::JSXCreateElementData;
use flow_typing_type::type_::MappedTypeKeyCompatibilityData;
use flow_typing_type::type_::MergedDeclarationConflict;
use flow_typing_type::type_::MergedDeclarationData;
use flow_typing_type::type_::OpaqueTypeCustomErrorCompatibilityData;
use flow_typing_type::type_::PositiveTypeGuardConsistencyData;
use flow_typing_type::type_::PropertyCompatibilityData;
use flow_typing_type::type_::ReactCreateElementCallData;
use flow_typing_type::type_::RecordCreateData;
use flow_typing_type::type_::SetPropertyData;
use flow_typing_type::type_::StandaloneCallThisData;
use flow_typing_type::type_::SwitchRefinementCheckData;
use flow_typing_type::type_::TupleElementCompatibilityData;
use flow_typing_type::type_::TypeApplicationTarget;
use flow_typing_type::type_::TypeArgCompatibilityData;
use flow_typing_type::type_::TypeGuardIncompatibilityData;
use flow_typing_type::type_::TypeInner;
use flow_typing_type::type_::TypeofApplicationData;
use flow_typing_type::type_::UnionEnum;
use flow_typing_type::type_::UnionRepresentativeData;
use flow_typing_type::type_::VirtualFrameUseOp;
use flow_typing_type::type_::VirtualRootUseOp;
use flow_typing_type::type_::VirtualUseOp;
use flow_typing_type::type_::aconstraint::AnnotationInferenceOperation;
use flow_typing_type::type_::fold_virtual_use_op;
use flow_typing_type::type_::implicit_instantiation_reference_kind;
use flow_typing_type::type_::root_of_use_op;
use flow_typing_type::type_::type_or_type_desc::ErrorTypeDesc;
use flow_typing_type::type_::type_or_type_desc::TypeOrTypeDescT;
use vec1::Vec1;

use super::error_message::EnumErrorKind;
use super::error_message::ErrorMessage as FlowErrorMessage;
use super::error_message::util_use_op_of_msg;
use super::flow_error::ErrorSet;
use super::flow_error::FlowError;
use super::intermediate_error_types::AccessChainSegment;
use super::intermediate_error_types::AssignTargetDesc;
use super::intermediate_error_types::CalleeDesc;
use super::intermediate_error_types::ErrorMessage;
use super::intermediate_error_types::Explanation;
use super::intermediate_error_types::ExplanationAdditionalUnionMembersData;
use super::intermediate_error_types::ExplanationConstrainedAssignData;
use super::intermediate_error_types::ExplanationCustomErrorData;
use super::intermediate_error_types::ExplanationInvariantSubtypingDueToMutableArrayData;
use super::intermediate_error_types::ExplanationInvariantSubtypingDueToMutablePropertiesData;
use super::intermediate_error_types::ExplanationInvariantSubtypingDueToMutablePropertyData;
use super::intermediate_error_types::ExplanationPropertyMissingDueToNeutralOptionalPropertyData;
use super::intermediate_error_types::ExpressionFunctionKind;
use super::intermediate_error_types::ExpressionReferenceData;
use super::intermediate_error_types::ExpressionReferenceKind;
use super::intermediate_error_types::Frame as ErrorFrame;
use super::intermediate_error_types::FunctionReferenceData;
use super::intermediate_error_types::FunctionReferenceKind;
use super::intermediate_error_types::ImplicitInstantiationReferenceData;
use super::intermediate_error_types::IntermediateError;
use super::intermediate_error_types::LowerRequirement;
use super::intermediate_error_types::Message;
use super::intermediate_error_types::MessageAlreadyExhaustivelyCheckOneEnumMemberData;
use super::intermediate_error_types::MessageCannotAccessEnumMemberData;
use super::intermediate_error_types::MessageCannotAddComputedPropertyDueToPotentialOverwriteData;
use super::intermediate_error_types::MessageCannotCallMaybeReactHookData;
use super::intermediate_error_types::MessageCannotCompareData;
use super::intermediate_error_types::MessageCannotExhaustivelyCheckAbstractEnumsData;
use super::intermediate_error_types::MessageCannotExhaustivelyCheckEnumWithUnknownsData;
use super::intermediate_error_types::MessageCannotExportRenamedDefaultData;
use super::intermediate_error_types::MessageCannotImportGlobalLibdefData;
use super::intermediate_error_types::MessageCannotInstantiateObjectUtilTypeWithEnumData;
use super::intermediate_error_types::MessageCannotResolveBuiltinModuleData;
use super::intermediate_error_types::MessageCannotSpreadGeneralData;
use super::intermediate_error_types::MessageCannotSpreadInexactMayOverwriteIndexerData;
use super::intermediate_error_types::MessageCannotUseEnumMemberUsedAsTypeData;
use super::intermediate_error_types::MessageCannotUseTypeForAnnotationInferenceData;
use super::intermediate_error_types::MessageCannotUseTypeGuardWithFunctionParamHavocedData;
use super::intermediate_error_types::MessageCannotUseTypeInValuePositionData;
use super::intermediate_error_types::MessageConstructSignatureMissingData;
use super::intermediate_error_types::MessageDefinitionInvalidRecursiveData;
use super::intermediate_error_types::MessageDuplicateModuleProviderData;
use super::intermediate_error_types::MessageEnumDuplicateMemberNameData;
use super::intermediate_error_types::MessageEnumInvalidMemberInitializerData;
use super::intermediate_error_types::MessageExponentialSpreadData;
use super::intermediate_error_types::MessageIncompatibleDueToInvariantSubtypingData;
use super::intermediate_error_types::MessageIncompatibleGeneralWithPrintedTypesData;
use super::intermediate_error_types::MessageIncompatibleTupleArityData;
use super::intermediate_error_types::MessageIncompleteExhausiveCheckEnumData;
use super::intermediate_error_types::MessageIndexerCheckFailedData;
use super::intermediate_error_types::MessageInvalidEnumMemberCheckData;
use super::intermediate_error_types::MessageInvalidKeyMirrorArgumentData;
use super::intermediate_error_types::MessageInvalidKeyPropertyInSpreadData;
use super::intermediate_error_types::MessageInvalidRendersTypeArgumentData;
use super::intermediate_error_types::MessageInvalidSelfReferencingDefaultData;
use super::intermediate_error_types::MessageInvalidSelfReferencingTypeAnnotationData;
use super::intermediate_error_types::MessageMatchNonExhaustiveObjectPatternData;
use super::intermediate_error_types::MessageMatchNonExplicitEnumCheckData;
use super::intermediate_error_types::MessageMissingPlatformSupportWithAvailablePlatformsData;
use super::intermediate_error_types::MessageNoDefaultExportData;
use super::intermediate_error_types::MessageNoNamedExportData;
use super::intermediate_error_types::MessageOnlyDefaultExportData;
use super::intermediate_error_types::MessagePropExtraAgainstExactObjectData;
use super::intermediate_error_types::MessagePropPolarityMismatchData;
use super::intermediate_error_types::MessagePropsMissingWithPrintedTypeData;
use super::intermediate_error_types::MessageReactIntrinsicOverlapData;
use super::intermediate_error_types::MessageRedeclareComponentPropData;
use super::intermediate_error_types::MessageShouldAnnotateVariableUsedInGenericContextData;
use super::intermediate_error_types::MessageSketchyNullCheckData;
use super::intermediate_error_types::MessageTupleElementNotReadableData;
use super::intermediate_error_types::MessageTupleElementNotWritableData;
use super::intermediate_error_types::MessageTupleIndexOutOfBoundData;
use super::intermediate_error_types::MessageTupleNonIntegerIndexData;
use super::intermediate_error_types::MessageTypeReferenceData;
use super::intermediate_error_types::MessageVariableOnlyAssignedByNullData;
use super::intermediate_error_types::NamedReferenceData;
use super::intermediate_error_types::PropPolarityMismatchObject;
use super::intermediate_error_types::RootMessage;
use super::intermediate_error_types::StrictComparisonInfo;
use super::intermediate_error_types::SubComponentOfInvariantSubtypingError;
use super::intermediate_error_types::TupleElementReferenceData;
use super::intermediate_error_types::TypeGuardReferenceData;
use super::intermediate_error_types::TypeGuardReferenceKind;
use super::intermediate_error_types::UnnecessaryInvariantConditionKind;
use super::intermediate_error_types::UnrecognizedAssertionCallee;
use super::intermediate_error_types::ValueAsTypeReference;
use super::intermediate_error_types::ValueDesc;
use crate::error_message::CallTypeArityCallee;
use crate::error_message::EExpectedBigIntLitData;
use crate::error_message::EExpectedBooleanLitData;
use crate::error_message::EExpectedNumberLitData;
use crate::error_message::EExpectedStringLitData;
use crate::error_message::EIncompatiblePropData;
use crate::error_message::EIncompatibleTypesWithUseOpData;
use crate::error_message::EPropNotFoundInLookupData;
use crate::error_message::EnumReferenceData;
use crate::error_message::IncompatibleInvariantSubtypingData;
use crate::error_message::IncompatibleSubtypingData;
use crate::error_message::IncompatibleTypeUseData;
use crate::error_message::IncompatibleTypesWithExampleData;
use crate::error_message::IndexerCheckFailedData;
use crate::error_message::MatchExampleReference;
use crate::error_message::PrivatePropMissingInLookupData;
use crate::error_message::PropMissingInSubtypingData;
use crate::error_message::PropPolarityMismatchData;
use crate::error_message::PropsExtraAgainstExactObjectData;
use crate::error_message::PropsMissingInInvariantSubtypingData;
use crate::error_message::PropsMissingInSubtypingData;
use crate::error_message::SpeculationData;
use crate::error_message::TypeGuardBindingKind;
use crate::error_message::TypeGuardFuncReference;
use crate::error_message::TypeGuardParameterData;
use crate::error_message::UninitializedPropertyKind;
use crate::error_message::UseOpData;

/// Rank scores for signals of different strength on an x^2 scale so that
/// greater signals dominate lesser signals.
const REASON_SCORE: i32 = 100;
const FRAME_SCORE: i32 = REASON_SCORE * 2;
const TYPE_ARG_FRAME_SCORE: i32 = FRAME_SCORE * 2;
const TUPLE_ELEMENT_FRAME_SCORE: i32 = TYPE_ARG_FRAME_SCORE * 2;

/// Gets the score of a use_op. Used in score_of_msg.
///
/// Calculated by taking the count of all the frames.
fn score_of_use_op<L: Dupe + PartialEq + Eq + PartialOrd + Ord>(use_op: &VirtualUseOp<L>) -> i32 {
    fold_virtual_use_op(
        use_op,
        |root| -> i32 {
            match root {
                VirtualRootUseOp::Speculation(inner_use_op) => score_of_use_op(inner_use_op),
                _ => 0,
            }
        },
        &|acc: i32, frame: &VirtualFrameUseOp<L>| -> i32 {
            acc + match frame {
                // Later params that error get a higher score. This roughly represents how
                // much type-checking work Flow successfully completed before erroring.
                // Useful for basically only overloaded function error messages.
                //
                // The signal that this gives us is that we successfully type checked n
                // params in the call before erroring. If there was no error, Flow may
                // have gone to successfully check another m params. However, we will
                // never know that. n is our best approximation. It rewards errors near
                // the end of a call and punishes (slightly) errors near the beginning of
                // a call.
                //
                // This, however, turns out to be consistent with code style in modern
                // JavaScript. As an unspoken convention, more complex arguments usually
                // go last. For overloaded functions, the switching generally happens on
                // the first argument. The "tag". This gives us confidence that n on
                // FunParam is a good heuristic for the score.
                //
                // FunRestParam is FunParam, but at the end. So give it a larger score
                // then FunParam after adding n.
                //
                // We do _not_ add n to the score if this use_op was added to an implicit type parameter.
                VirtualFrameUseOp::FunParam(box FunParamData { n, .. }) => FRAME_SCORE + *n,
                VirtualFrameUseOp::FunRestParam { .. } => FRAME_SCORE + FRAME_SCORE - 1,
                // FunCompatibility is generally followed by another use_op. So let's not
                // count FunCompatibility.
                VirtualFrameUseOp::FunCompatibility { .. } => 0,
                VirtualFrameUseOp::StandaloneCallThis(..) => 0,
                // FunMissingArg means the error is *less* likely to be correct.
                VirtualFrameUseOp::FunMissingArg(..) => 0,
                // Higher signal then PropertyCompatibility, for example.
                VirtualFrameUseOp::TypeArgCompatibility(..) => TYPE_ARG_FRAME_SCORE,
                VirtualFrameUseOp::ArrayElementCompatibility(..) => TYPE_ARG_FRAME_SCORE,
                // Higher signal then TypeArgCompatibility.
                VirtualFrameUseOp::TupleElementCompatibility(..) => TUPLE_ELEMENT_FRAME_SCORE,
                // ImplicitTypeParam is an internal marker use_op that doesn't get
                // rendered in error messages. So it doesn't necessarily signal anything
                // about the user's intent.
                VirtualFrameUseOp::ImplicitTypeParam => 0,
                VirtualFrameUseOp::OpaqueTypeCustomErrorCompatibility(..) => 0,
                _ => FRAME_SCORE,
            }
        },
    )
}

/// Gets the score of an error message.
///
/// The score is an approximation of how close the user was to getting their code right.
/// A higher score means the user was closer than a lower score. A score of 0 means
/// we have no signal about how close the user was.
///
/// For example, consider the following two flows:
///
/// ```text
/// number ~> {p: string}
/// {p: number} ~> {p: string}
/// ```
///
/// Clearly, the user was closer to being correct with the second flow. So this
/// function should assign the `number ~> string` error a higher score than the
/// `number ~> object` error.
///
/// Now consider:
///
/// ```text
/// number ~> string
/// number ~> {p: string}
/// ```
///
/// This time we kept the lower bound the same and changed the upper bound. The
/// first flow is this time closer to the user's intent than the second flow.
/// So we give the `number ~> string` message a higher score than the
/// `number ~> object` message.
///
/// This scoring mechanism is useful for union and intersection error messages
/// where we want to approximate which branch the user meant to target with
/// their code. Branches with higher scores have a higher likelihood of being
/// the branch the user was targeting.
#[allow(dead_code)]
pub fn score_of_msg<L: Dupe + PartialEq + Eq + PartialOrd + Ord>(msg: &FlowErrorMessage<L>) -> i32 {
    #[derive(Clone, Copy)]
    enum TypeCategory {
        Nullish,
        Scalar,
        Array,
        Other,
    }

    fn type_category<L: Dupe>(type_or_desc: &TypeOrTypeDescT<L>) -> TypeCategory {
        match type_or_desc {
            TypeOrTypeDescT::Type(t) => match &**t {
                TypeInner::DefT(_, def_t) => match &**def_t {
                    DefTInner::NullT | DefTInner::VoidT => TypeCategory::Nullish,
                    DefTInner::NumGeneralT(_)
                    | DefTInner::StrGeneralT(_)
                    | DefTInner::BoolGeneralT
                    | DefTInner::BigIntGeneralT(_)
                    | DefTInner::SymbolT
                    | DefTInner::UniqueSymbolT(_)
                    | DefTInner::SingletonStrT { .. }
                    | DefTInner::NumericStrKeyT(_)
                    | DefTInner::SingletonNumT { .. }
                    | DefTInner::SingletonBoolT { .. }
                    | DefTInner::SingletonBigIntT { .. } => TypeCategory::Scalar,
                    DefTInner::ArrT(_) => TypeCategory::Array,
                    _ => TypeCategory::Other,
                },
                _ => TypeCategory::Other,
            },
            TypeOrTypeDescT::TypeDesc(ErrorTypeDesc::Normalized(Elt::Type(t))) => {
                match t.as_ref() {
                    Ty::Null | Ty::Void => TypeCategory::Nullish,
                    Ty::Symbol
                    | Ty::UniqueSymbol
                    | Ty::Num
                    | Ty::Str
                    | Ty::Bool
                    | Ty::BigInt
                    | Ty::NumLit(_)
                    | Ty::StrLit(_)
                    | Ty::BoolLit(_)
                    | Ty::BigIntLit(_) => TypeCategory::Scalar,
                    Ty::Arr(_) | Ty::Tup { .. } => TypeCategory::Array,
                    _ => TypeCategory::Other,
                }
            }
            TypeOrTypeDescT::TypeDesc(
                ErrorTypeDesc::Normalized(Elt::Decl(_))
                | ErrorTypeDesc::ReasonDesc(_)
                | ErrorTypeDesc::Unknown,
            ) => TypeCategory::Other,
        }
    }

    fn score_categories(lower: TypeCategory, upper: TypeCategory) -> i32 {
        match (lower, upper) {
            (TypeCategory::Nullish, TypeCategory::Nullish)
            | (TypeCategory::Scalar, TypeCategory::Scalar)
            | (TypeCategory::Array, TypeCategory::Array)
            | (TypeCategory::Other, TypeCategory::Other) => REASON_SCORE,
            (TypeCategory::Nullish, _) | (_, TypeCategory::Nullish) => 0,
            (TypeCategory::Scalar, _) | (_, TypeCategory::Scalar) => 1,
            (TypeCategory::Array, _) | (_, TypeCategory::Array) => 1,
        }
    }

    // Start by getting the score based off the use_op of our error message. If
    // the message does not have a use_op then we return 0. This score
    // contribution declares that greater complexity in the use is more likely to
    // cause a match.
    let score = util_use_op_of_msg(0, score_of_use_op, msg);

    // Special cases for messages which increment the score.
    // If a property doesn't exist, we still use a PropertyCompatibility use_op.
    // a missing prop does not increase the likelihood that the user was close to
    // the right types.
    let score = score
        + match msg {
            FlowErrorMessage::EIncompatibleProp(box EIncompatiblePropData {
                use_op: Some(use_op),
                ..
            }) => match use_op {
                VirtualUseOp::Frame(frame, _)
                    if matches!(frame.as_ref(), VirtualFrameUseOp::PropertyCompatibility(..)) =>
                {
                    -FRAME_SCORE
                }
                _ => 0,
            },
            FlowErrorMessage::EPropNotFoundInLookup(box EPropNotFoundInLookupData {
                use_op,
                ..
            }) => match use_op {
                VirtualUseOp::Frame(frame, _)
                    if matches!(frame.as_ref(), VirtualFrameUseOp::PropertyCompatibility(..)) =>
                {
                    -FRAME_SCORE
                }
                _ => 0,
            },
            _ => 0,
        };

    // If we have two incompatible types and both incompatible types are scalar or
    // both types are arrays then increment our score. This is based on the belief
    // that the solutions with the lowest possible complexity are closest to each
    // other. e.g. number ~> string. If one type is a scalar or array and the
    // other type is not then we decrement our score.
    score
        + match msg {
            FlowErrorMessage::EIncompatibleTypesWithUseOp(
                box EIncompatibleTypesWithUseOpData {
                    lower_desc,
                    upper_desc,
                    branches,
                    ..
                },
            ) => {
                if branches.is_empty() {
                    score_categories(type_category(lower_desc), type_category(upper_desc))
                } else {
                    REASON_SCORE
                }
            }
            FlowErrorMessage::EIncompatibleWithExact((lower, upper), _, _) => score_categories(
                type_category(&lower.type_desc),
                type_category(&upper.type_desc),
            ),
            _ => REASON_SCORE,
        }
}

/// Flips the lower and upper bounds of a frame use operation.
///
/// When an error occurs in a contravariant position (like function arguments),
/// we need to flip the lower and upper bounds to point to the correct location.
fn flip_frame<L: Dupe + PartialEq + Eq + PartialOrd + Ord>(
    frame: VirtualFrameUseOp<L>,
) -> VirtualFrameUseOp<L> {
    use VirtualFrameUseOp::*;

    match frame {
        ArrayElementCompatibility(box ArrayElementCompatibilityData {
            lower,
            upper,
            lower_t,
            upper_t,
        }) => ArrayElementCompatibility(Box::new(ArrayElementCompatibilityData {
            lower: upper,
            upper: lower,
            lower_t: upper_t,
            upper_t: lower_t,
        })),
        FunCompatibility { lower, upper } => FunCompatibility {
            lower: upper,
            upper: lower,
        },
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
            lower: upper,
            upper: lower,
            lower_t: upper_t,
            upper_t: lower_t,
        })),
        FunRestParam { lower, upper } => FunRestParam {
            lower: upper,
            upper: lower,
        },
        FunReturn(box FunReturnData {
            lower,
            upper,
            lower_t,
            upper_t,
        }) => FunReturn(Box::new(FunReturnData {
            lower: upper,
            upper: lower,
            lower_t: upper_t,
            upper_t: lower_t,
        })),
        IndexerKeyCompatibility(box IndexerKeyCompatibilityData {
            lower,
            upper,
            lower_t,
            upper_t,
        }) => IndexerKeyCompatibility(Box::new(IndexerKeyCompatibilityData {
            lower: upper,
            upper: lower,
            lower_t: upper_t,
            upper_t: lower_t,
        })),
        OpaqueTypeLowerBoundCompatibility { lower, upper } => OpaqueTypeLowerBoundCompatibility {
            lower: upper,
            upper: lower,
        },
        OpaqueTypeUpperBoundCompatibility { lower, upper } => OpaqueTypeUpperBoundCompatibility {
            lower: upper,
            upper: lower,
        },
        PropertyCompatibility(box PropertyCompatibilityData {
            prop,
            lower,
            upper,
            lower_t,
            upper_t,
        }) => PropertyCompatibility(Box::new(PropertyCompatibilityData {
            prop,
            lower: upper,
            upper: lower,
            lower_t: upper_t,
            upper_t: lower_t,
        })),
        ReactConfigCheck => ReactConfigCheck,
        TupleElementCompatibility(box TupleElementCompatibilityData {
            n,
            lower,
            upper,
            lower_t,
            upper_t,
        }) => TupleElementCompatibility(Box::new(TupleElementCompatibilityData {
            n,
            lower: upper,
            upper: lower,
            lower_t: upper_t,
            upper_t: lower_t,
        })),
        TypeArgCompatibility(box TypeArgCompatibilityData {
            name,
            targ,
            lower,
            upper,
            polarity,
        }) => TypeArgCompatibility(Box::new(TypeArgCompatibilityData {
            name,
            targ,
            lower: upper,
            upper: lower,
            polarity,
        })),
        EnumRepresentationTypeCompatibility { lower, upper } => {
            EnumRepresentationTypeCompatibility {
                lower: upper,
                upper: lower,
            }
        }
        TupleAssignment
        | StandaloneCallThis(..)
        | TypeParamBound { .. }
        | OpaqueTypeLowerBound { .. }
        | OpaqueTypeUpperBound { .. }
        | FunMissingArg(..)
        | ImplicitTypeParam
        | ReactGetConfig { .. }
        | OpaqueTypeCustomErrorCompatibility(..)
        | UnifyFlip
        | ConstrainedAssignment(..)
        | MappedTypeKeyCompatibility(..)
        | TypeGuardCompatibility
        | RendersCompatibility
        | ReactDeepReadOnly(..)
        | UnionRepresentative(..) => frame,
    }
}

#[allow(dead_code)]
pub fn post_process_errors(original_errors: ErrorSet) -> ErrorSet {
    use super::flow_error::error_of_msg;

    if original_errors.len() <= 1 {
        return original_errors;
    }

    ///  Unification produces two errors. One for both sides. For example,
    /// `{p: number} ~> {p: string}` errors on both `number ~> string` and
    /// `string ~> number`. Showing both errors to our user is often redundant.
    /// So we use this utility to flip the `string ~> number` case and produce an
    /// error identical to one we've produced before. These two errors will be
    /// deduped by the filter.
    fn dedupe_by_flip_loop(use_op: VirtualUseOp<ALoc>) -> (bool, VirtualUseOp<ALoc>) {
        match use_op {
            // Roots don't flip
            VirtualUseOp::Op(_) => (false, use_op),
            // Start flipping if we are on the reverse side of unification
            VirtualUseOp::Frame(frame, inner) => {
                if matches!(frame.as_ref(), VirtualFrameUseOp::UnifyFlip) {
                    let (flip, inner_op) = dedupe_by_flip_loop(inner.as_ref().clone());
                    (!flip, inner_op)
                } else {
                    // If we are in flip mode then flip our frame.
                    let (flip, inner_op) = dedupe_by_flip_loop(inner.as_ref().clone());
                    if flip {
                        let flipped_frame = flip_frame(frame.as_ref().clone());
                        (
                            true,
                            VirtualUseOp::Frame(Arc::new(flipped_frame), Arc::new(inner_op)),
                        )
                    } else {
                        (false, VirtualUseOp::Frame(frame, Arc::new(inner_op)))
                    }
                }
            }
        }
    }

    fn dedupe_by_flip<T>(
        lower: T,
        upper: T,
        use_op: VirtualUseOp<ALoc>,
    ) -> ((T, T), VirtualUseOp<ALoc>) {
        let (flip, use_op) = dedupe_by_flip_loop(use_op);
        if flip {
            ((upper, lower), use_op)
        } else {
            ((lower, upper), use_op)
        }
    }

    fn remove_unify_flip(use_op: VirtualUseOp<ALoc>) -> VirtualUseOp<ALoc> {
        match use_op {
            VirtualUseOp::Op(_) => use_op,
            VirtualUseOp::Frame(frame, inner) => {
                if matches!(frame.as_ref(), VirtualFrameUseOp::UnifyFlip) {
                    remove_unify_flip(inner.as_ref().clone())
                } else {
                    VirtualUseOp::Frame(frame, Arc::new(remove_unify_flip(inner.as_ref().clone())))
                }
            }
        }
    }

    // Clone for use in the closure - the filter will consume the original
    let errors_for_lookup = original_errors.dupe();

    let retain_error = |error: &FlowError<ALoc>| -> bool {
        let is_not_duplicate = |new_msg: FlowErrorMessage<ALoc>| -> bool {
            let e_prime = error_of_msg(error.source_file.dupe(), new_msg);
            !errors_for_lookup.mem(&e_prime)
        };

        match error.msg_of_error() {
            FlowErrorMessage::EExpectedStringLit(box EExpectedStringLitData {
                lower,
                upper,
                use_op,
            }) => {
                let ((lower_new, upper_new), use_op_new) =
                    dedupe_by_flip(lower.clone(), upper.clone(), use_op.clone());
                lower == &lower_new
                    || is_not_duplicate(FlowErrorMessage::EExpectedStringLit(Box::new(
                        EExpectedStringLitData {
                            lower: lower_new,
                            upper: upper_new,
                            use_op: use_op_new,
                        },
                    )))
            }
            FlowErrorMessage::EExpectedNumberLit(box EExpectedNumberLitData {
                lower,
                upper,
                use_op,
            }) => {
                let ((lower_new, upper_new), use_op_new) =
                    dedupe_by_flip(lower.clone(), upper.clone(), use_op.clone());
                lower == &lower_new
                    || is_not_duplicate(FlowErrorMessage::EExpectedNumberLit(Box::new(
                        EExpectedNumberLitData {
                            lower: lower_new,
                            upper: upper_new,
                            use_op: use_op_new,
                        },
                    )))
            }
            FlowErrorMessage::EExpectedBooleanLit(box EExpectedBooleanLitData {
                lower,
                upper,
                use_op,
            }) => {
                let ((lower_new, upper_new), use_op_new) =
                    dedupe_by_flip(lower.clone(), upper.clone(), use_op.clone());
                lower == &lower_new
                    || is_not_duplicate(FlowErrorMessage::EExpectedBooleanLit(Box::new(
                        EExpectedBooleanLitData {
                            lower: lower_new,
                            upper: upper_new,
                            use_op: use_op_new,
                        },
                    )))
            }
            FlowErrorMessage::EExpectedBigIntLit(box EExpectedBigIntLitData {
                lower,
                upper,
                use_op,
            }) => {
                let ((lower_new, upper_new), use_op_new) =
                    dedupe_by_flip(lower.clone(), upper.clone(), use_op.clone());
                lower == &lower_new
                    || is_not_duplicate(FlowErrorMessage::EExpectedBigIntLit(Box::new(
                        EExpectedBigIntLitData {
                            lower: lower_new,
                            upper: upper_new,
                            use_op: use_op_new,
                        },
                    )))
            }
            FlowErrorMessage::EIncompatibleTypesWithUseOp(
                box EIncompatibleTypesWithUseOpData {
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
                },
            ) => {
                let (
                    (
                        (lower_loc_new, lower_def_loc_new, lower_desc_new),
                        (upper_loc_new, upper_def_loc_new, upper_desc_new),
                    ),
                    use_op_new,
                ) = dedupe_by_flip(
                    (lower_loc.dupe(), lower_def_loc.dupe(), lower_desc.clone()),
                    (upper_loc.dupe(), upper_def_loc.dupe(), upper_desc.clone()),
                    use_op.clone(),
                );
                (lower_loc == &lower_loc_new && lower_def_loc == &lower_def_loc_new)
                    || is_not_duplicate(FlowErrorMessage::EIncompatibleTypesWithUseOp(Box::new(
                        EIncompatibleTypesWithUseOpData {
                            use_op: use_op_new,
                            lower_loc: lower_loc_new,
                            lower_def_loc: lower_def_loc_new,
                            upper_loc: upper_loc_new,
                            upper_def_loc: upper_def_loc_new,
                            lower_desc: lower_desc_new,
                            upper_desc: upper_desc_new,
                            explanation: explanation.clone(),
                            example: example.clone(),
                            branches: branches.clone(),
                        },
                    )))
            }
            FlowErrorMessage::EEnumError(EnumErrorKind::EnumIncompatible(
                box EIncompatibleTypesWithUseOpData {
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
                },
            )) => {
                let (
                    (
                        (lower_loc_new, lower_def_loc_new, lower_desc_new),
                        (upper_loc_new, upper_def_loc_new, upper_desc_new),
                    ),
                    use_op_new,
                ) = dedupe_by_flip(
                    (lower_loc.dupe(), lower_def_loc.dupe(), lower_desc.clone()),
                    (upper_loc.dupe(), upper_def_loc.dupe(), upper_desc.clone()),
                    use_op.clone(),
                );
                (lower_loc == &lower_loc_new && lower_def_loc == &lower_def_loc_new)
                    || is_not_duplicate(FlowErrorMessage::EEnumError(
                        EnumErrorKind::EnumIncompatible(Box::new(
                            EIncompatibleTypesWithUseOpData {
                                use_op: use_op_new,
                                lower_loc: lower_loc_new,
                                lower_def_loc: lower_def_loc_new,
                                upper_loc: upper_loc_new,
                                upper_def_loc: upper_def_loc_new,
                                lower_desc: lower_desc_new,
                                upper_desc: upper_desc_new,
                                explanation: explanation.clone(),
                                example: example.clone(),
                                branches: branches.clone(),
                            },
                        )),
                    ))
            }
            FlowErrorMessage::EPropNotFoundInLookup(box EPropNotFoundInLookupData {
                prop_name,
                object,
                prop_loc,
                use_op,
                suggestion,
            }) => {
                // PropNotFound error will always display the missing vs existing prop in the same order
                let use_op_new = remove_unify_flip(use_op.clone());
                use_op == &use_op_new
                    || is_not_duplicate(FlowErrorMessage::EPropNotFoundInLookup(Box::new(
                        EPropNotFoundInLookupData {
                            prop_name: prop_name.clone(),
                            object: object.clone(),
                            prop_loc: prop_loc.dupe(),
                            use_op: use_op_new,
                            suggestion: suggestion.clone(),
                        },
                    )))
            }
            _ => true,
        }
    };

    original_errors.filter(retain_error)
}

/// A use-op type that missed post-inference normalization has nothing to print.
fn printed_type_desc<L: Dupe>(type_desc: &TypeOrTypeDescT<L>) -> ErrorTypeDesc<L> {
    match type_desc {
        TypeOrTypeDescT::Type(_) => ErrorTypeDesc::Unknown,
        TypeOrTypeDescT::TypeDesc(desc) => desc.clone(),
    }
}

fn method_callee_desc<L: Dupe + PartialEq + Eq + PartialOrd + Ord>(
    fn_: &FunCallMethodCallee<L>,
) -> CalleeDesc<L> {
    match fn_ {
        FunCallMethodCallee::Reason(reason) => CalleeDesc::Reason(reason.desc.clone()),
        FunCallMethodCallee::Method(name) => CalleeDesc::Method(name.dupe()),
    }
}

/// References a printed type at the location `reason` would be referenced at.
fn printed_type_reference<L: Dupe>(
    reason: &VirtualReason<L>,
    type_desc: &TypeOrTypeDescT<L>,
) -> MessageTypeReferenceData<L> {
    MessageTypeReferenceData {
        loc: reason.annot_loc().unwrap_or(reason.def_loc()).dupe(),
        desc: printed_type_desc(type_desc),
    }
}

/// What a render type renders, so that "X does not render Y" doesn't read "renders Y".
fn rendered_type_desc<L: Dupe + PartialEq + Eq + PartialOrd + Ord + Clone + std::fmt::Debug>(
    desc: &ErrorTypeDesc<L>,
) -> ErrorTypeDesc<L> {
    match desc {
        ErrorTypeDesc::Normalized(Elt::Type(ty)) => match ty.as_ref() {
            Ty::Renders(inner, _) => ErrorTypeDesc::Normalized(Elt::Type(inner.dupe())),
            _ => desc.clone(),
        },
        ErrorTypeDesc::ReasonDesc(_)
        | ErrorTypeDesc::Normalized(Elt::Decl(_))
        | ErrorTypeDesc::Unknown => desc.clone(),
    }
}

fn mk_error<L: Dupe>(
    kind: ErrorKind,
    loc: Loc,
    error_code: Option<ErrorCode>,
    message: Message<L>,
    root: Option<(Loc, RootMessage<L>)>,
    frames: Option<Vec<ErrorFrame<L>>>,
    explanations: Option<Vec<Explanation<L>>>,
) -> IntermediateError<L> {
    IntermediateError {
        kind,
        loc,
        root,
        error_code,
        message: ErrorMessage::SingletonMessage {
            message,
            frames,
            explanations,
        },
        misplaced_source_file: None,
        unsuppressable: false,
    }
}

fn mk_speculation_error<L: Dupe + Clone>(
    kind: ErrorKind,
    loc: Loc,
    root: Option<(Loc, RootMessage<L>)>,
    frames: Vec<ErrorFrame<L>>,
    explanations: Vec<Explanation<L>>,
    error_code: Option<ErrorCode>,
    speculation_errors: Vec<(i32, IntermediateError<L>)>,
) -> IntermediateError<L>
where
    ErrorFrame<L>: Clone,
    Explanation<L>: Clone,
{
    // Flatten speculation branches using mutable refs + push/pop to avoid clones.
    // recursing and pop after, eliminating structural clones entirely.
    fn flatten_speculation_branches<L: Dupe + Clone>(
        acc_frames: &mut Vec<Vec<ErrorFrame<L>>>,
        acc_explanations: &mut Vec<Vec<Explanation<L>>>,
        acc: &mut Vec<(i32, IntermediateError<L>)>,
        branches: Vec<(i32, IntermediateError<L>)>,
    ) where
        ErrorFrame<L>: Clone,
        Explanation<L>: Clone,
    {
        for (score, error) in branches {
            match error.message {
                // If we have a speculation error with no root, flatten its branches
                // using push/pop pattern - no cloning needed
                ErrorMessage::SpeculationMessage {
                    branches: nested_branches,
                    frames: inner_frames,
                    explanations: inner_explanations,
                } if error.root.is_none() => {
                    acc_frames.push(inner_frames);
                    acc_explanations.push(inner_explanations);
                    flatten_speculation_branches(
                        acc_frames,
                        acc_explanations,
                        acc,
                        nested_branches,
                    );
                    acc_frames.pop();
                    acc_explanations.pop();
                }
                ErrorMessage::SpeculationMessage {
                    branches: nested_branches,
                    frames: inner_frames,
                    explanations: inner_explanations,
                } => {
                    // Collect accumulated frames by iterating + cloning elements,
                    // then chain with owned inner_frames (moved, not cloned)
                    let frames: Vec<ErrorFrame<L>> = acc_frames
                        .iter()
                        .flat_map(|v| v.iter().cloned())
                        .chain(inner_frames)
                        .collect();

                    let explanations: Vec<Explanation<L>> = acc_explanations
                        .iter()
                        .flat_map(|v| v.iter().cloned())
                        .chain(inner_explanations)
                        .collect();

                    let message = ErrorMessage::SpeculationMessage {
                        branches: nested_branches,
                        frames,
                        explanations,
                    };
                    acc.push((
                        score,
                        IntermediateError {
                            kind: error.kind,
                            loc: error.loc,
                            root: error.root,
                            error_code: error.error_code,
                            message,
                            misplaced_source_file: error.misplaced_source_file,
                            unsuppressable: error.unsuppressable,
                        },
                    ));
                }
                ErrorMessage::SingletonMessage {
                    message,
                    frames: inner_frames,
                    explanations: inner_explanations,
                } => {
                    let frames = match (inner_frames, acc_frames.is_empty()) {
                        (Some(inner), _) => Some(
                            acc_frames
                                .iter()
                                .flat_map(|v| v.iter().cloned())
                                .chain(inner)
                                .collect(),
                        ),
                        (None, false) => {
                            Some(acc_frames.iter().flat_map(|v| v.iter().cloned()).collect())
                        }
                        (None, true) => None,
                    };

                    let explanations = match (inner_explanations, acc_explanations.is_empty()) {
                        (Some(inner), _) => Some(
                            acc_explanations
                                .iter()
                                .flat_map(|v| v.iter().cloned())
                                .chain(inner)
                                .collect(),
                        ),
                        (None, false) => Some(
                            acc_explanations
                                .iter()
                                .flat_map(|v| v.iter().cloned())
                                .collect(),
                        ),
                        (None, true) => None,
                    };

                    let msg = ErrorMessage::SingletonMessage {
                        message,
                        frames,
                        explanations,
                    };
                    acc.push((
                        score,
                        IntermediateError {
                            kind: error.kind,
                            loc: error.loc,
                            root: error.root,
                            error_code: error.error_code,
                            message: msg,
                            misplaced_source_file: error.misplaced_source_file,
                            unsuppressable: error.unsuppressable,
                        },
                    ));
                }
                ErrorMessage::SingletonMessageWithExample {
                    message,
                    frames: inner_frames,
                    explanations: inner_explanations,
                    example,
                } => {
                    let frames = match (inner_frames, acc_frames.is_empty()) {
                        (Some(inner), _) => Some(
                            acc_frames
                                .iter()
                                .flat_map(|v| v.iter().cloned())
                                .chain(inner)
                                .collect(),
                        ),
                        (None, false) => {
                            Some(acc_frames.iter().flat_map(|v| v.iter().cloned()).collect())
                        }
                        (None, true) => None,
                    };

                    let explanations = match (inner_explanations, acc_explanations.is_empty()) {
                        (Some(inner), _) => Some(
                            acc_explanations
                                .iter()
                                .flat_map(|v| v.iter().cloned())
                                .chain(inner)
                                .collect(),
                        ),
                        (None, false) => Some(
                            acc_explanations
                                .iter()
                                .flat_map(|v| v.iter().cloned())
                                .collect(),
                        ),
                        (None, true) => None,
                    };

                    let msg = ErrorMessage::SingletonMessageWithExample {
                        message,
                        frames,
                        explanations,
                        example,
                    };
                    acc.push((
                        score,
                        IntermediateError {
                            kind: error.kind,
                            loc: error.loc,
                            root: error.root,
                            error_code: error.error_code,
                            message: msg,
                            misplaced_source_file: error.misplaced_source_file,
                            unsuppressable: error.unsuppressable,
                        },
                    ));
                }
            }
        }
    }

    let mut acc_frames = vec![];
    let mut acc_explanations = vec![];
    let mut branches = vec![];
    flatten_speculation_branches(
        &mut acc_frames,
        &mut acc_explanations,
        &mut branches,
        speculation_errors,
    );
    branches.sort_by_key(|(score, _)| *score);

    IntermediateError {
        kind,
        loc,
        root,
        error_code,
        message: ErrorMessage::SpeculationMessage {
            frames,
            explanations,
            branches,
        },
        misplaced_source_file: None,
        unsuppressable: false,
    }
}

/// Make intermediate errors from FlowError.
///
/// Returns IntermediateError with the same location type as input (ALoc).
/// The loc_of_aloc function is only used for computing the primary loc and for error cases.
pub fn make_intermediate_error<L, F, G>(
    loc_of_aloc: F,
    speculation: bool,
    error: &FlowError<L>,
    is_lib: G,
) -> IntermediateError<L>
where
    L: Dupe + PartialEq + Eq + PartialOrd + Ord + Clone + std::fmt::Debug,
    F: Fn(&L) -> Loc + Clone,
    G: Fn(&FileKey) -> bool + Clone,
{
    // In friendly error messages, we always want to point to a value as the primary location.
    // Normally, values are in the lower bound, but in contravariant positions this flips.
    fn flip_contravariant<
        L: Dupe + PartialEq + Eq + PartialOrd + Ord + Clone + std::fmt::Debug,
        T,
    >(
        lower: T,
        upper: T,
        use_op: VirtualUseOp<L>,
    ) -> ((T, T), VirtualUseOp<L>) {
        fn is_contravariant<
            L: Dupe + PartialEq + Eq + PartialOrd + Ord + Clone + std::fmt::Debug,
        >(
            frame: &VirtualFrameUseOp<L>,
            use_op: &VirtualUseOp<L>,
        ) -> (bool, bool) {
            use VirtualFrameUseOp::*;

            match (frame, use_op) {
                (FunParam(box FunParamData { .. }), VirtualUseOp::Frame(f, _))
                    if matches!(f.as_ref(), FunCompatibility { .. }) =>
                {
                    (true, true)
                }
                (FunRestParam { .. }, VirtualUseOp::Frame(f, _))
                    if matches!(f.as_ref(), FunCompatibility { .. }) =>
                {
                    (true, true)
                }
                (
                    ReactGetConfig {
                        polarity: Polarity::Negative,
                    },
                    _,
                ) => (true, false),
                (
                    TypeArgCompatibility(box TypeArgCompatibilityData {
                        polarity: Polarity::Negative,
                        ..
                    }),
                    _,
                ) => (true, false),
                _ => (false, false),
            }
        }

        fn is_contravariant_root<
            L: Dupe + PartialEq + Eq + PartialOrd + Ord + Clone + std::fmt::Debug,
        >(
            root: &VirtualRootUseOp<L>,
        ) -> bool {
            matches!(root, VirtualRootUseOp::FunImplicitReturn(..))
        }

        fn loop_flip<L: Dupe + PartialEq + Eq + PartialOrd + Ord + Clone + std::fmt::Debug>(
            use_op: VirtualUseOp<L>,
        ) -> (bool, VirtualUseOp<L>) {
            use VirtualUseOp::*;

            match use_op {
                Op(root_use_op) => {
                    let flip = is_contravariant_root(&root_use_op);
                    (flip, Op(root_use_op))
                }
                Frame(frame, inner_use_op) => {
                    let (flip, inner_use_op) = loop_flip(inner_use_op.as_ref().clone());
                    let (contravariant, flip_self_flag) = is_contravariant(&frame, &inner_use_op);

                    let flip = if contravariant { !flip } else { flip };
                    let flip_self = flip && (!contravariant || flip_self_flag);

                    let frame = if flip_self {
                        Arc::new(flip_frame((*frame).clone()))
                    } else {
                        frame
                    };

                    (flip, Frame(frame, Arc::new(inner_use_op)))
                }
            }
        }

        let (flip, use_op) = loop_flip(use_op);
        if flip {
            ((upper, lower), use_op)
        } else {
            ((lower, upper), use_op)
        }
    }

    fn mod_lower_reason_according_to_use_ops<
        L: Dupe + PartialEq + Eq + PartialOrd + Ord + Clone + std::fmt::Debug,
    >(
        lower: VirtualReason<L>,
        use_op: &VirtualUseOp<L>,
    ) -> VirtualReason<L> {
        use VirtualFrameUseOp::*;
        use VirtualUseOp::*;

        match use_op {
            Frame(frame_rc, inner_use_op) => match frame_rc.as_ref() {
                OpaqueTypeUpperBound {
                    opaque_t_reason, ..
                } => mod_lower_reason_according_to_use_ops(opaque_t_reason.dupe(), inner_use_op),
                _ => mod_lower_reason_according_to_use_ops(lower, inner_use_op),
            },
            Op(_) => lower,
        }
    }

    fn mod_upper_reason_according_to_use_ops<
        L: Dupe + PartialEq + Eq + PartialOrd + Ord + Clone + std::fmt::Debug,
    >(
        upper: VirtualReason<L>,
        use_op: &VirtualUseOp<L>,
    ) -> VirtualReason<L> {
        use VirtualFrameUseOp::*;
        use VirtualUseOp::*;

        match use_op {
            Frame(frame_rc, inner_use_op) => match frame_rc.as_ref() {
                OpaqueTypeLowerBound {
                    opaque_t_reason, ..
                } => mod_upper_reason_according_to_use_ops(opaque_t_reason.dupe(), inner_use_op),
                _ => mod_upper_reason_according_to_use_ops(upper, inner_use_op),
            },
            Op(_) => upper,
        }
    }

    /// Takes the smallest location where we found the error and a use_op which we will unwrap.
    /// Returns: (root with loc, custom error message, primary loc, frames, explanations)
    ///
    /// This is a massive function (~750 lines) with 13 nested helper functions.
    #[allow(unused_variables)]
    fn unwrap_use_ops<L: Dupe + PartialEq + Eq + PartialOrd + Ord + Clone + std::fmt::Debug>(
        loc_of_aloc: impl Fn(&L) -> Loc + Clone,
        speculation: bool,
    ) -> impl Fn(
        Loc,
        VirtualUseOp<L>,
    ) -> (
        Option<(Loc, RootMessage<L>)>,
        Option<Message<L>>,
        Loc,
        Vec<ErrorFrame<L>>,
        Vec<Explanation<L>>,
    ) {
        move |initial_loc: Loc, initial_use_op: VirtualUseOp<L>| {
            // We will optionally include lower and upper pair to provide a TS-like stacked errors.
            // We only do this if the upper bound has a nice name
            let opt_incompatibility_pair =
                |use_op: &VirtualUseOp<L>,
                 pair: &(VirtualReason<L>, VirtualReason<L>)|
                 -> Option<(VirtualReason<L>, VirtualReason<L>)> {
                    let (_lower, upper) = pair;

                    let is_named_type = |desc: &VirtualReasonDesc<L>| -> bool {
                        use flow_common::reason::VirtualReasonDesc::*;
                        matches!(
                            desc,
                            REnum { name: Some(_), .. } | RType(_) | RTypeAlias(box (_, Some(_), _))
                        )
                    };

                    if upper.desc.as_ref().is_some_and(is_named_type) {
                        match use_op {
                            VirtualUseOp::Op(root) => match root.as_ref() {
                                VirtualRootUseOp::AssignVar { var: Some(_), .. }
                                | VirtualRootUseOp::Cast { .. }
                                | VirtualRootUseOp::ClassExtendsCheck(..)
                                | VirtualRootUseOp::ClassImplementsCheck(..) => None,
                                _ => Some(pair.dupe()),
                            },
                            _ => Some(pair.dupe()),
                        }
                    } else {
                        None
                    }
                };

            let loc_of_prop_compatibility_reason =
                |loc: Loc, reason: &VirtualReason<L>, use_op: &VirtualUseOp<L>| -> Loc {
                    match use_op {
                        VirtualUseOp::Op(root) => match root.as_ref() {
                            VirtualRootUseOp::ClassExtendsCheck(..)
                            | VirtualRootUseOp::ClassImplementsCheck(..) => loc,
                            _ => loc_of_aloc(&reason.loc),
                        },
                        _ => loc_of_aloc(&reason.loc),
                    }
                };

            fn loop_to_form_access_chain_impl<
                L: Dupe + PartialEq + Eq + PartialOrd + Ord + Clone + std::fmt::Debug,
            >(
                lower_loc: Loc,
                use_op: VirtualUseOp<L>,
                loc_of_aloc: &impl Fn(&L) -> Loc,
                loc_of_prop_compatibility_reason: &impl Fn(
                    Loc,
                    &VirtualReason<L>,
                    &VirtualUseOp<L>,
                ) -> Loc,
                opt_incompatibility_pair: &impl Fn(
                    &VirtualUseOp<L>,
                    &(VirtualReason<L>, VirtualReason<L>),
                )
                    -> Option<(VirtualReason<L>, VirtualReason<L>)>,
            ) -> (Loc, Vec<AccessChainSegment>, VirtualUseOp<L>) {
                match use_op {
                    VirtualUseOp::Frame(frame_rc, inner) => {
                        match frame_rc.as_ref() {
                            VirtualFrameUseOp::PropertyCompatibility(
                                box PropertyCompatibilityData {
                                    prop: Some(prop),
                                    lower,
                                    upper,
                                    ..
                                },
                            ) if !prop.matches_str("$call")
                                && opt_incompatibility_pair(
                                    &inner,
                                    &(lower.dupe(), upper.dupe()),
                                )
                                .is_none() =>
                            {
                                let lower_loc_prime = loc_of_prop_compatibility_reason(
                                    lower_loc.dupe(),
                                    lower,
                                    &inner,
                                );

                                let lower_loc = if lower_loc_prime.contains(&lower_loc) {
                                    lower_loc
                                } else {
                                    lower_loc_prime
                                };

                                // Recurse to build the chain
                                let (final_loc, mut props, final_use_op) =
                                    loop_to_form_access_chain_impl(
                                        lower_loc,
                                        inner.as_ref().clone(),
                                        loc_of_aloc,
                                        loc_of_prop_compatibility_reason,
                                        opt_incompatibility_pair,
                                    );

                                props.push(AccessChainSegment::PropSegment(prop.dupe()));
                                (final_loc, props, final_use_op)
                            }

                            VirtualFrameUseOp::TupleElementCompatibility(
                                box TupleElementCompatibilityData {
                                    n, lower, upper, ..
                                },
                            ) if opt_incompatibility_pair(
                                &inner,
                                &(lower.dupe(), upper.dupe()),
                            )
                            .is_none() =>
                            {
                                let lower_loc_prime = loc_of_aloc(&lower.loc);

                                let lower_loc = if lower_loc_prime.contains(&lower_loc) {
                                    lower_loc
                                } else {
                                    lower_loc_prime
                                };

                                // Recurse to build the chain
                                let (final_loc, mut props, final_use_op) =
                                    loop_to_form_access_chain_impl(
                                        lower_loc,
                                        inner.as_ref().clone(),
                                        loc_of_aloc,
                                        loc_of_prop_compatibility_reason,
                                        opt_incompatibility_pair,
                                    );

                                props.push(AccessChainSegment::TupleIndexSegment(*n));
                                (final_loc, props, final_use_op)
                            }

                            _ => (lower_loc, vec![], VirtualUseOp::Frame(frame_rc, inner)),
                        }
                    }
                    _ => (lower_loc, vec![], use_op),
                }
            }

            let loop_to_form_access_chain =
                |lower_loc: Loc,
                 use_op: VirtualUseOp<L>|
                 -> (Loc, Vec<AccessChainSegment>, VirtualUseOp<L>) {
                    let (loc, mut props, use_op) = loop_to_form_access_chain_impl(
                        lower_loc,
                        use_op,
                        &loc_of_aloc,
                        &loc_of_prop_compatibility_reason,
                        &opt_incompatibility_pair,
                    );
                    props.reverse();
                    (loc, props, use_op)
                };

            let unknown_root = |loc: Loc,
                                frames: (Vec<ErrorFrame<L>>, Vec<Explanation<L>>),
                                custom_error_message: Option<Message<L>>|
             -> (
                Option<(Loc, RootMessage<L>)>,
                Option<Message<L>>,
                Loc,
                Vec<ErrorFrame<L>>,
                Vec<Explanation<L>>,
            ) {
                let (all_frames, explanations) = frames;
                (None, custom_error_message, loc, all_frames, explanations)
            };

            let root_with_loc_and_specific_loc =
                |loc: Loc,
                 frames: (Vec<ErrorFrame<L>>, Vec<Explanation<L>>),
                 root_loc: Loc,
                 specific_loc: Loc,
                 root_message: RootMessage<L>,
                 custom_error_message: Option<Message<L>>| {
                    let final_loc = if root_loc.contains(&loc) && root_loc != loc {
                        loc
                    } else {
                        specific_loc
                    };

                    let (all_frames, explanations) = frames;
                    (
                        Some((root_loc, root_message)),
                        custom_error_message,
                        final_loc,
                        all_frames,
                        explanations,
                    )
                };

            let root_with_specific_reason =
                |loc: Loc,
                 frames: (Vec<ErrorFrame<L>>, Vec<Explanation<L>>),
                 root_reason: &VirtualReason<L>,
                 specific_reason: &VirtualReason<L>,
                 root_message: RootMessage<L>,
                 custom_error_message: Option<Message<L>>| {
                    let root_loc = loc_of_aloc(&root_reason.loc);
                    let specific_loc = loc_of_aloc(&specific_reason.loc);
                    root_with_loc_and_specific_loc(
                        loc,
                        frames,
                        root_loc,
                        specific_loc,
                        root_message,
                        custom_error_message,
                    )
                };

            let root = |loc: Loc,
                        frames: (Vec<ErrorFrame<L>>, Vec<Explanation<L>>),
                        root_reason: &VirtualReason<L>,
                        root_message: RootMessage<L>,
                        custom_error_message: Option<Message<L>>| {
                let root_loc = loc_of_aloc(&root_reason.loc);
                root_with_loc_and_specific_loc(
                    loc,
                    frames,
                    root_loc.dupe(),
                    root_loc,
                    root_message,
                    custom_error_message,
                )
            };

            fn loop_impl<L: Dupe + PartialEq + Eq + PartialOrd + Ord + Clone + std::fmt::Debug>(
                loc: Loc,
                frames: (Vec<ErrorFrame<L>>, Vec<Explanation<L>>),
                use_op: VirtualUseOp<L>,
                custom_error_message: Option<Message<L>>,
                loc_of_aloc: &impl Fn(&L) -> Loc,
                loc_of_prop_compatibility_reason: &impl Fn(
                    Loc,
                    &VirtualReason<L>,
                    &VirtualUseOp<L>,
                ) -> Loc,
                loop_to_form_access_chain: &impl Fn(
                    Loc,
                    VirtualUseOp<L>,
                )
                    -> (Loc, Vec<AccessChainSegment>, VirtualUseOp<L>),
                opt_incompatibility_pair: &impl Fn(
                    &VirtualUseOp<L>,
                    &(VirtualReason<L>, VirtualReason<L>),
                )
                    -> Option<(VirtualReason<L>, VirtualReason<L>)>,
                speculation: bool,
            ) -> (
                Option<(Loc, RootMessage<L>)>,
                Option<Message<L>>,
                Loc,
                Vec<ErrorFrame<L>>,
                Vec<Explanation<L>>,
            ) {
                let root_at_loc = |loc: Loc,
                                   frames: (Vec<ErrorFrame<L>>, Vec<Explanation<L>>),
                                   root_loc: Loc,
                                   root_message: RootMessage<L>,
                                   custom_error_message: Option<Message<L>>|
                 -> (
                    Option<(Loc, RootMessage<L>)>,
                    Option<Message<L>>,
                    Loc,
                    Vec<ErrorFrame<L>>,
                    Vec<Explanation<L>>,
                ) {
                    let specific_loc = root_loc.dupe();

                    let final_loc = if root_loc.contains(&loc) && root_loc != loc {
                        loc
                    } else {
                        specific_loc
                    };

                    let (all_frames, explanations) = frames;
                    (
                        Some((root_loc, root_message)),
                        custom_error_message,
                        final_loc,
                        all_frames,
                        explanations,
                    )
                };
                let root = |loc: Loc,
                            frames: (Vec<ErrorFrame<L>>, Vec<Explanation<L>>),
                            root_reason: &VirtualReason<L>,
                            root_message: RootMessage<L>,
                            custom_error_message: Option<Message<L>>| {
                    root_at_loc(
                        loc,
                        frames,
                        loc_of_aloc(&root_reason.loc),
                        root_message,
                        custom_error_message,
                    )
                };

                match use_op {
                    VirtualUseOp::Op(root_op) => {
                        match root_op.as_ref() {
                            VirtualRootUseOp::UnknownUse => {
                                let (all_frames, explanations) = frames;
                                (None, custom_error_message, loc, all_frames, explanations)
                            }

                            VirtualRootUseOp::Speculation(_) if speculation => {
                                let (all_frames, explanations) = frames;
                                (None, custom_error_message, loc, all_frames, explanations)
                            }

                            VirtualRootUseOp::Speculation(inner_use_op) => loop_impl(
                                loc,
                                frames,
                                inner_use_op.as_ref().clone(),
                                custom_error_message,
                                loc_of_aloc,
                                loc_of_prop_compatibility_reason,
                                loop_to_form_access_chain,
                                opt_incompatibility_pair,
                                speculation,
                            ),

                            // Object operations
                            VirtualRootUseOp::ObjectAddComputedProperty { op } => root(
                                loc,
                                frames,
                                op,
                                RootMessage::RootCannotAddComputedProperty,
                                custom_error_message,
                            ),

                            VirtualRootUseOp::ObjectSpread { op } => root(
                                loc,
                                frames,
                                op,
                                RootMessage::RootCannotSpread(op.desc.clone()),
                                custom_error_message,
                            ),

                            VirtualRootUseOp::ObjectRest { op } => root(
                                loc,
                                frames,
                                op,
                                RootMessage::RootCannotGetRest(op.desc.clone()),
                                custom_error_message,
                            ),

                            VirtualRootUseOp::ObjectChain { op } => root(
                                loc,
                                frames,
                                op,
                                RootMessage::RootCannotCallObjectAssign(op.desc.clone()),
                                custom_error_message,
                            ),

                            VirtualRootUseOp::AssignVar { var, init } => {
                                let (init_loc, init) = match init {
                                    AssignVarInit::Reason(reason) => {
                                        (&reason.loc, ValueDesc::Reason(reason.desc.clone()))
                                    }
                                    AssignVarInit::Default(box AssignVarDefaultData {
                                        loc: default_loc,
                                        default,
                                    }) => {
                                        (default_loc, ValueDesc::Type(printed_type_desc(default)))
                                    }
                                };
                                root_at_loc(
                                    loc,
                                    frames,
                                    loc_of_aloc(init_loc),
                                    RootMessage::RootCannotAssign {
                                        init,
                                        target: var.as_deref().map(|target| match target {
                                            AssignVarTarget::Variable(reason) => {
                                                AssignTargetDesc::Variable(reason.desc.clone())
                                            }
                                            AssignVarTarget::Annotation(t)
                                            | AssignVarTarget::Pattern { annot: t, .. } => {
                                                AssignTargetDesc::Annotation(printed_type_desc(t))
                                            }
                                        }),
                                    },
                                    custom_error_message,
                                )
                            }

                            VirtualRootUseOp::DeleteVar { var } => root(
                                loc,
                                frames,
                                var,
                                RootMessage::RootCannotDelete(var.desc.clone()),
                                custom_error_message,
                            ),

                            VirtualRootUseOp::InitField { op, body } => root(
                                loc,
                                frames,
                                op,
                                RootMessage::RootCannotInitializeField {
                                    field: op.desc.clone(),
                                    body: body.desc.clone(),
                                },
                                custom_error_message,
                            ),

                            // Cast
                            VirtualRootUseOp::Cast { lower, upper } => root(
                                loc,
                                frames,
                                lower,
                                RootMessage::RootCannotCast {
                                    lower: lower.desc.clone(),
                                    upper: printed_type_desc(upper),
                                },
                                custom_error_message,
                            ),

                            VirtualRootUseOp::ClassExtendsCheck(box ClassExtendsCheckData {
                                extends_loc,
                                extends,
                                def_loc,
                                def,
                            }) => root_at_loc(
                                loc,
                                frames,
                                loc_of_aloc(def_loc),
                                RootMessage::RootCannotExtendClass {
                                    extends: MessageTypeReferenceData {
                                        loc: extends_loc.dupe(),
                                        desc: printed_type_desc(extends),
                                    },
                                    def: MessageTypeReferenceData {
                                        loc: def_loc.dupe(),
                                        desc: printed_type_desc(def),
                                    },
                                },
                                custom_error_message,
                            ),

                            VirtualRootUseOp::ClassMethodDefinition(
                                box ClassMethodDefinitionData {
                                    def_loc,
                                    method_name,
                                    name,
                                },
                            ) => root_at_loc(
                                loc,
                                frames,
                                loc_of_aloc(def_loc),
                                RootMessage::RootCannotDefineClassMethod {
                                    method_: NamedReferenceData {
                                        loc: def_loc.dupe(),
                                        name: method_name.dupe(),
                                    },
                                    name: name.desc.clone(),
                                },
                                custom_error_message,
                            ),

                            VirtualRootUseOp::ClassImplementsCheck(
                                box ClassImplementsCheckData {
                                    implements, def, ..
                                },
                            ) => root(
                                loc,
                                frames,
                                def,
                                RootMessage::RootCannotImplementClass {
                                    implements: implements.dupe(),
                                    def: def.desc.clone(),
                                },
                                custom_error_message,
                            ),

                            VirtualRootUseOp::ClassOwnProtoCheck(box ClassOwnProtoCheckData {
                                prop,
                                own_loc,
                                proto_loc,
                            }) => match (own_loc, proto_loc) {
                                (None, None) => {
                                    let (all_frames, explanations) = frames;
                                    (None, custom_error_message, loc, all_frames, explanations)
                                }
                                (Some(loc_val), None) => root_at_loc(
                                    loc_of_aloc(loc_val),
                                    frames,
                                    loc_of_aloc(loc_val),
                                    RootMessage::RootCannotShadowProtoProperty,
                                    custom_error_message,
                                ),
                                (None, Some(loc_val)) => root_at_loc(
                                    loc_of_aloc(loc_val),
                                    frames,
                                    loc_of_aloc(loc_val),
                                    RootMessage::RootCannotDefineShadowedProtoProperty,
                                    custom_error_message,
                                ),
                                (Some(own_loc_val), Some(proto_loc_val)) => root_at_loc(
                                    loc,
                                    frames,
                                    loc_of_aloc(own_loc_val),
                                    RootMessage::RootCannotShadowProto(NamedReferenceData {
                                        loc: proto_loc_val.dupe(),
                                        name: prop.display_smol_str(),
                                    }),
                                    custom_error_message,
                                ),
                            },

                            VirtualRootUseOp::Coercion { from, target } => root(
                                loc,
                                frames,
                                from,
                                RootMessage::RootCannotCoerce {
                                    from: from.desc.clone(),
                                    target: target.desc.clone(),
                                },
                                custom_error_message,
                            ),

                            VirtualRootUseOp::ConformToCommonInterface(
                                box ConformToCommonInterfaceData {
                                    self_sig_loc,
                                    self_module_loc,
                                    originate_from_import,
                                },
                            ) => {
                                let (all_frames, mut explanations) = frames;
                                explanations.push(Explanation::ExplanationMultiplatform);
                                let frames = (all_frames, explanations);

                                let root_loc = loc_of_aloc(self_module_loc);
                                let specific_loc = loc_of_aloc(self_sig_loc);

                                let final_loc = if root_loc.contains(&loc) && root_loc != loc {
                                    loc
                                } else {
                                    specific_loc
                                };

                                let (all_frames, explanations) = frames;
                                (
                                    Some((
                                        root_loc,
                                        RootMessage::RootCannotConformToCommonInterface {
                                            originate_from_import: *originate_from_import,
                                        },
                                    )),
                                    custom_error_message,
                                    final_loc,
                                    all_frames,
                                    explanations,
                                )
                            }

                            VirtualRootUseOp::MergedDeclaration(box MergedDeclarationData {
                                conflict,
                                first_decl_loc,
                                first_decl,
                                current_decl,
                            }) => root(
                                loc,
                                frames,
                                current_decl,
                                RootMessage::RootCannotMergeDeclaration {
                                    conflict: conflict.dupe(),
                                    first_decl: MessageTypeReferenceData {
                                        loc: first_decl_loc.dupe(),
                                        desc: printed_type_desc(first_decl),
                                    },
                                },
                                custom_error_message,
                            ),

                            VirtualRootUseOp::DeclareComponentRef { op } => root(
                                loc,
                                frames,
                                op,
                                RootMessage::RootCannotDeclareRef,
                                custom_error_message,
                            ),

                            VirtualRootUseOp::FunCall(box FunCallData { op, fn_, .. }) => {
                                let root_loc = loc_of_aloc(&op.loc);
                                let specific_loc = loc_of_aloc(&fn_.loc);

                                let final_loc = if root_loc.contains(&loc) && root_loc != loc {
                                    loc
                                } else {
                                    specific_loc
                                };

                                let (all_frames, explanations) = frames;
                                (
                                    Some((
                                        root_loc,
                                        RootMessage::RootCannotCall(CalleeDesc::Reason(
                                            fn_.desc.clone(),
                                        )),
                                    )),
                                    custom_error_message,
                                    final_loc,
                                    all_frames,
                                    explanations,
                                )
                            }

                            VirtualRootUseOp::FunCallMethod(box FunCallMethodData {
                                op,
                                fn_,
                                prop,
                                ..
                            }) => {
                                let root_loc = loc_of_aloc(&op.loc);
                                let specific_loc = loc_of_aloc(&prop.loc);

                                let final_loc = if root_loc.contains(&loc) && root_loc != loc {
                                    loc
                                } else {
                                    specific_loc
                                };

                                let (all_frames, explanations) = frames;
                                (
                                    Some((
                                        root_loc,
                                        RootMessage::RootCannotCall(method_callee_desc(fn_)),
                                    )),
                                    custom_error_message,
                                    final_loc,
                                    all_frames,
                                    explanations,
                                )
                            }

                            VirtualRootUseOp::RenderTypeInstantiation { render_type } => root(
                                loc,
                                frames,
                                render_type,
                                RootMessage::RootCannotInstantiateRenderType,
                                custom_error_message,
                            ),

                            VirtualRootUseOp::FunReturnStatement { value } => root(
                                loc,
                                frames,
                                value,
                                RootMessage::RootCannotReturn(value.desc.clone()),
                                custom_error_message,
                            ),

                            VirtualRootUseOp::FunImplicitReturn(box FunImplicitReturnData {
                                upper,
                                fn_,
                                type_guard: true,
                                ..
                            }) => root(
                                loc,
                                frames,
                                upper,
                                RootMessage::RootCannotDeclareTypeGuard {
                                    type_guard_loc: upper.loc.dupe(),
                                    fn_: fn_.dupe(),
                                },
                                custom_error_message,
                            ),

                            VirtualRootUseOp::FunImplicitReturn(box FunImplicitReturnData {
                                upper,
                                upper_t,
                                fn_,
                                ..
                            }) => root(
                                loc,
                                frames,
                                upper,
                                RootMessage::RootCannotExpectImplicitReturn {
                                    upper: printed_type_desc(upper_t),
                                    fn_: fn_.desc.clone(),
                                },
                                custom_error_message,
                            ),

                            VirtualRootUseOp::GeneratorYield { value } => root(
                                loc,
                                frames,
                                value,
                                RootMessage::RootCannotYield(value.desc.clone()),
                                custom_error_message,
                            ),

                            VirtualRootUseOp::GetExport(prop) => root(
                                loc,
                                frames,
                                prop,
                                RootMessage::RootCannotGetProp(prop.desc.clone()),
                                custom_error_message,
                            ),

                            VirtualRootUseOp::GetProperty(prop) => root(
                                loc,
                                frames,
                                prop,
                                RootMessage::RootCannotGetProp(prop.desc.clone()),
                                custom_error_message,
                            ),

                            VirtualRootUseOp::IndexedTypeAccess(box IndexedTypeAccessData {
                                index_loc,
                                object,
                                index,
                            }) => root_at_loc(
                                loc,
                                frames,
                                loc_of_aloc(index_loc),
                                RootMessage::RootCannotAccessIndex {
                                    index: printed_type_desc(index),
                                    object_: printed_type_desc(object),
                                },
                                custom_error_message,
                            ),

                            VirtualRootUseOp::InferBoundCompatibilityCheck { bound, infer } => {
                                root(
                                    loc,
                                    frames,
                                    bound,
                                    RootMessage::RootCannotUseInferTypeBound {
                                        infer: infer.desc.clone(),
                                    },
                                    custom_error_message,
                                )
                            }

                            VirtualRootUseOp::JSXCreateElement(box JSXCreateElementData {
                                op,
                                component,
                                ..
                            })
                            | VirtualRootUseOp::ReactCreateElementCall(
                                box ReactCreateElementCallData { op, component, .. },
                            ) => {
                                let root_loc = loc_of_aloc(&op.loc);
                                let specific_loc = loc_of_aloc(&component.loc);

                                let final_loc = if root_loc.contains(&loc) && root_loc != loc {
                                    loc
                                } else {
                                    specific_loc
                                };

                                let (all_frames, explanations) = frames;
                                (
                                    Some((
                                        root_loc,
                                        RootMessage::RootCannotCreateElement(
                                            component.desc.clone(),
                                        ),
                                    )),
                                    custom_error_message,
                                    final_loc,
                                    all_frames,
                                    explanations,
                                )
                            }

                            VirtualRootUseOp::ReactGetIntrinsic { literal } => root(
                                loc,
                                frames,
                                literal,
                                RootMessage::RootCannotCreateElement(literal.desc.clone()),
                                custom_error_message,
                            ),

                            VirtualRootUseOp::RecordCreate(box RecordCreateData {
                                op,
                                constructor,
                                ..
                            }) => {
                                let root_loc = loc_of_aloc(&op.loc);
                                let specific_loc = loc_of_aloc(&constructor.loc);

                                let final_loc = if root_loc.contains(&loc) && root_loc != loc {
                                    loc
                                } else {
                                    specific_loc
                                };

                                let (all_frames, explanations) = frames;
                                (
                                    Some((
                                        root_loc,
                                        RootMessage::RootCannotCreateRecord(
                                            constructor.desc.clone(),
                                        ),
                                    )),
                                    custom_error_message,
                                    final_loc,
                                    all_frames,
                                    explanations,
                                )
                            }

                            VirtualRootUseOp::TypeApplication {
                                type_: TypeApplicationTarget::Reason(type_),
                            } => root(
                                loc,
                                frames,
                                type_,
                                RootMessage::RootCannotInstantiateTypeApp(type_.desc.clone()),
                                custom_error_message,
                            ),

                            VirtualRootUseOp::TypeApplication {
                                type_:
                                    TypeApplicationTarget::Typeof(box TypeofApplicationData {
                                        reason,
                                        name,
                                    }),
                            } => root(
                                loc,
                                frames,
                                reason,
                                RootMessage::RootCannotInstantiateTypeof(name.dupe()),
                                custom_error_message,
                            ),

                            VirtualRootUseOp::MakeReadOnly {
                                loc: box readonly_loc,
                            } => root_at_loc(
                                loc,
                                frames,
                                loc_of_aloc(readonly_loc),
                                RootMessage::RootCannotMakeReadOnly,
                                custom_error_message,
                            ),

                            VirtualRootUseOp::SetProperty(box SetPropertyData {
                                prop,
                                value,
                                lhs,
                                ..
                            }) => {
                                let loc_reason = if loc_of_aloc(&lhs.loc).contains(&loc) {
                                    &lhs
                                } else {
                                    &value
                                };
                                root(
                                    loc,
                                    frames,
                                    loc_reason,
                                    RootMessage::RootCannotAssign {
                                        init: ValueDesc::Reason(value.desc.clone()),
                                        target: Some(AssignTargetDesc::Variable(prop.desc.clone())),
                                    },
                                    custom_error_message,
                                )
                            }

                            VirtualRootUseOp::UpdateProperty { prop, lhs } => root(
                                loc,
                                frames,
                                lhs,
                                RootMessage::RootCannotUpdate(prop.desc.clone()),
                                custom_error_message,
                            ),

                            VirtualRootUseOp::DeleteProperty { prop, lhs } => root(
                                loc,
                                frames,
                                lhs,
                                RootMessage::RootCannotDelete(prop.desc.clone()),
                                custom_error_message,
                            ),

                            VirtualRootUseOp::RefinementCheck { test, discriminant } => root(
                                loc,
                                frames,
                                test,
                                RootMessage::RootCannotCheckAgainst {
                                    test: test.desc.clone(),
                                    discriminant: discriminant.dupe(),
                                },
                                custom_error_message,
                            ),

                            VirtualRootUseOp::SwitchRefinementCheck(
                                box SwitchRefinementCheckData { test, discriminant },
                            ) => {
                                let root_loc = loc_of_aloc(test);
                                let specific_loc = root_loc.dupe();

                                let final_loc = if root_loc.contains(&loc) && root_loc != loc {
                                    loc
                                } else {
                                    specific_loc
                                };

                                let (all_frames, explanations) = frames;
                                (
                                    Some((
                                        root_loc,
                                        RootMessage::RootCannotCheckAgainstSwitchDiscriminant(
                                            discriminant.dupe(),
                                        ),
                                    )),
                                    custom_error_message,
                                    final_loc,
                                    all_frames,
                                    explanations,
                                )
                            }

                            VirtualRootUseOp::EvalMappedType(box EvalMappedTypeData {
                                mapped_type_loc,
                                mapped_type,
                            }) => root_at_loc(
                                loc,
                                frames,
                                loc_of_aloc(mapped_type_loc),
                                RootMessage::RootCannotInstantiateEval(MessageTypeReferenceData {
                                    loc: mapped_type_loc.dupe(),
                                    desc: printed_type_desc(mapped_type),
                                }),
                                custom_error_message,
                            ),

                            VirtualRootUseOp::TypeGuardIncompatibility(
                                box TypeGuardIncompatibilityData {
                                    guard_type_loc,
                                    guard_type_reference_loc,
                                    guard_type,
                                    param_name,
                                },
                            ) => root_at_loc(
                                loc,
                                frames,
                                loc_of_aloc(guard_type_loc),
                                RootMessage::RootCannotUseTypeGuard {
                                    guard_type: MessageTypeReferenceData {
                                        loc: guard_type_reference_loc.dupe(),
                                        desc: printed_type_desc(guard_type),
                                    },
                                    param_name: param_name.clone(),
                                },
                                custom_error_message,
                            ),

                            VirtualRootUseOp::ComponentRestParamCompatibility { .. } => {
                                let (all_frames, explanations) = frames;
                                (None, custom_error_message, loc, all_frames, explanations)
                            }

                            VirtualRootUseOp::PositiveTypeGuardConsistency(
                                box PositiveTypeGuardConsistencyData {
                                    return_reason,
                                    param_reason,
                                    guard_type_reference_loc,
                                    guard_type,
                                    is_return_false_statement,
                                    ..
                                },
                            ) => {
                                let (all_frames, mut explanations) = frames;
                                explanations.push(
                                    Explanation::ExplanationTypeGuardPositiveConsistency {
                                        return_: return_reason.dupe(),
                                        param: param_reason.dupe(),
                                        guard_type: MessageTypeReferenceData {
                                            loc: guard_type_reference_loc.dupe(),
                                            desc: printed_type_desc(guard_type),
                                        },
                                        is_return_false_statement: *is_return_false_statement,
                                    },
                                );
                                root(
                                    loc,
                                    (all_frames, explanations),
                                    return_reason,
                                    RootMessage::RootCannotReturn(return_reason.desc.clone()),
                                    custom_error_message,
                                )
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
                                let (all_frames, mut explanations) = frames;
                                explanations.push(
                                    Explanation::ExplanationAssertionFunctionConsistency {
                                        param: param_reason.dupe(),
                                        guard_type: MessageTypeReferenceData {
                                            loc: guard_type_reference_loc.dupe(),
                                            desc: printed_type_desc(guard_type),
                                        },
                                    },
                                );
                                root(
                                    loc,
                                    (all_frames, explanations),
                                    exit_reason,
                                    RootMessage::RootCannotReturnFromAssertionFunction {
                                        implicit_return: *implicit_return,
                                    },
                                    custom_error_message,
                                )
                            }
                        }
                    }

                    VirtualUseOp::Frame(frame_rc, inner_use_op)
                        if matches!(frame_rc.as_ref(), VirtualFrameUseOp::FunParam(..))
                            && matches!(
                                inner_use_op.as_ref(),
                                VirtualUseOp::Op(spec_rc) if matches!(
                                    spec_rc.as_ref(),
                                    VirtualRootUseOp::Speculation(spec2_rc) if matches!(
                                        spec2_rc.as_ref(),
                                        VirtualUseOp::Op(inner_rc) if matches!(
                                            inner_rc.as_ref(),
                                            VirtualRootUseOp::FunCall(..)
                                                | VirtualRootUseOp::FunCallMethod(..)
                                                | VirtualRootUseOp::JSXCreateElement(..)
                                        )
                                    )
                                )
                            ) =>
                    {
                        loop_impl(
                            loc,
                            frames,
                            inner_use_op.as_ref().clone(),
                            custom_error_message,
                            loc_of_aloc,
                            loc_of_prop_compatibility_reason,
                            loop_to_form_access_chain,
                            opt_incompatibility_pair,
                            speculation,
                        )
                    }

                    VirtualUseOp::Frame(ref frame_rc, ref inner_use_op)
                        if let VirtualFrameUseOp::FunParam(box FunParamData {
                            n,
                            name,
                            lower: lower_prime,
                            lower_t,
                            ..
                        }) = frame_rc.as_ref()
                            && let VirtualUseOp::Op(op_rc) = inner_use_op.as_ref()
                            && matches!(
                                op_rc.as_ref(),
                                VirtualRootUseOp::FunCall(..)
                                    | VirtualRootUseOp::FunCallMethod(..)
                                    | VirtualRootUseOp::JSXCreateElement(..)
                                    | VirtualRootUseOp::RecordCreate(..)
                            ) =>
                    {
                        let fun_param_root =
                            |args: &[VirtualReason<L>],
                             fn_: CalleeDesc<L>,
                             frames: (Vec<ErrorFrame<L>>, Vec<Explanation<L>>),
                             custom_error_message: Option<Message<L>>| {
                                // Get the actual argument at position n-1, or fallback to the
                                // type of lower_prime
                                let (lower, lower_desc) = if (*n as usize) <= args.len() {
                                    let arg = &args[(*n as usize) - 1];
                                    (arg, ValueDesc::Reason(arg.desc.clone()))
                                } else {
                                    (lower_prime, ValueDesc::Type(printed_type_desc(lower_t)))
                                };

                                let root_msg = match name {
                                    Some(name_str) => RootMessage::RootCannotCallWithNamedParam {
                                        fn_,
                                        lower: lower_desc,
                                        name: name_str.clone(),
                                    },
                                    None => RootMessage::RootCannotCallWithNthParam {
                                        fn_,
                                        lower: lower_desc,
                                        n: *n,
                                    },
                                };
                                root(loc.dupe(), frames, lower, root_msg, custom_error_message)
                            };
                        match op_rc.as_ref() {
                            VirtualRootUseOp::FunCall(box FunCallData { args, fn_, .. }) => {
                                fun_param_root(
                                    args,
                                    CalleeDesc::Reason(fn_.desc.clone()),
                                    frames,
                                    custom_error_message,
                                )
                            }
                            VirtualRootUseOp::FunCallMethod(box FunCallMethodData {
                                args,
                                fn_,
                                ..
                            }) => fun_param_root(
                                args,
                                method_callee_desc(fn_),
                                frames,
                                custom_error_message,
                            ),
                            VirtualRootUseOp::JSXCreateElement(box JSXCreateElementData {
                                op,
                                component,
                                ..
                            }) => {
                                let root_loc = loc_of_aloc(&op.loc);
                                let specific_loc = loc_of_aloc(&component.loc);

                                let final_loc = if root_loc.contains(&loc) && root_loc != loc {
                                    loc
                                } else {
                                    specific_loc
                                };

                                let (all_frames, explanations) = frames;
                                (
                                    Some((
                                        root_loc,
                                        RootMessage::RootCannotCreateElement(
                                            component.desc.clone(),
                                        ),
                                    )),
                                    custom_error_message,
                                    final_loc,
                                    all_frames,
                                    explanations,
                                )
                            }
                            VirtualRootUseOp::RecordCreate(box RecordCreateData {
                                op,
                                constructor,
                                ..
                            }) => {
                                let root_loc = loc_of_aloc(&op.loc);
                                let specific_loc = loc_of_aloc(&constructor.loc);

                                let final_loc = if root_loc.contains(&loc) && root_loc != loc {
                                    loc
                                } else {
                                    specific_loc
                                };

                                let (all_frames, explanations) = frames;
                                (
                                    Some((
                                        root_loc,
                                        RootMessage::RootCannotCreateRecord(
                                            constructor.desc.clone(),
                                        ),
                                    )),
                                    custom_error_message,
                                    final_loc,
                                    all_frames,
                                    explanations,
                                )
                            }
                            _ => {
                                let (all_frames, explanations) = frames;
                                (None, custom_error_message, loc, all_frames, explanations)
                            }
                        }
                    }

                    VirtualUseOp::Frame(frame, inner) => match frame.as_ref() {
                        VirtualFrameUseOp::StandaloneCallThis(box StandaloneCallThisData {
                            op,
                            fn_,
                            receiver_loc,
                            receiver: box receiver,
                        }) => {
                            let (all_frames, explanations) = frames;
                            (
                                Some((
                                    loc_of_aloc(&op.loc),
                                    RootMessage::RootCannotCallStandalone(fn_.desc.clone()),
                                )),
                                Some(Message::MessageStandaloneCallRequiresReceiver(
                                    MessageTypeReferenceData {
                                        loc: receiver_loc.dupe(),
                                        desc: printed_type_desc(receiver),
                                    },
                                )),
                                loc_of_aloc(&fn_.loc),
                                all_frames,
                                explanations,
                            )
                        }

                        VirtualFrameUseOp::FunRestParam { .. } => loop_impl(
                            loc,
                            frames,
                            inner.as_ref().clone(),
                            custom_error_message,
                            loc_of_aloc,
                            loc_of_prop_compatibility_reason,
                            loop_to_form_access_chain,
                            opt_incompatibility_pair,
                            speculation,
                        ),

                        VirtualFrameUseOp::UnifyFlip => {
                            if let VirtualUseOp::Frame(inner_frame, _) = inner.as_ref() {
                                if matches!(
                                    inner_frame.as_ref(),
                                    VirtualFrameUseOp::ArrayElementCompatibility(..)
                                ) {
                                    let (all_frames, mut explanations) = frames;
                                    explanations.push(Explanation::ExplanationArrayInvariantTyping);
                                    loop_impl(
                                        loc,
                                        (all_frames, explanations),
                                        inner.as_ref().clone(),
                                        custom_error_message,
                                        loc_of_aloc,
                                        loc_of_prop_compatibility_reason,
                                        loop_to_form_access_chain,
                                        opt_incompatibility_pair,
                                        speculation,
                                    )
                                } else if matches!(
                                    inner_frame.as_ref(),
                                    VirtualFrameUseOp::PropertyCompatibility(..)
                                ) {
                                    let (all_frames, mut explanations) = frames;
                                    explanations
                                        .push(Explanation::ExplanationPropertyInvariantTyping);
                                    loop_impl(
                                        loc,
                                        (all_frames, explanations),
                                        inner.as_ref().clone(),
                                        custom_error_message,
                                        loc_of_aloc,
                                        loc_of_prop_compatibility_reason,
                                        loop_to_form_access_chain,
                                        opt_incompatibility_pair,
                                        speculation,
                                    )
                                } else {
                                    loop_impl(
                                        loc,
                                        frames,
                                        inner.as_ref().clone(),
                                        custom_error_message,
                                        loc_of_aloc,
                                        loc_of_prop_compatibility_reason,
                                        loop_to_form_access_chain,
                                        opt_incompatibility_pair,
                                        speculation,
                                    )
                                }
                            } else {
                                loop_impl(
                                    loc,
                                    frames,
                                    inner.as_ref().clone(),
                                    custom_error_message,
                                    loc_of_aloc,
                                    loc_of_prop_compatibility_reason,
                                    loop_to_form_access_chain,
                                    opt_incompatibility_pair,
                                    speculation,
                                )
                            }
                        }

                        VirtualFrameUseOp::OpaqueTypeLowerBound { .. }
                        | VirtualFrameUseOp::OpaqueTypeUpperBound { .. }
                        | VirtualFrameUseOp::FunMissingArg(..)
                        | VirtualFrameUseOp::ImplicitTypeParam
                        | VirtualFrameUseOp::ReactConfigCheck
                        | VirtualFrameUseOp::ReactGetConfig { .. }
                        | VirtualFrameUseOp::MappedTypeKeyCompatibility(..)
                        | VirtualFrameUseOp::TupleAssignment
                        | VirtualFrameUseOp::RendersCompatibility => loop_impl(
                            loc,
                            frames,
                            inner.as_ref().clone(),
                            custom_error_message,
                            loc_of_aloc,
                            loc_of_prop_compatibility_reason,
                            loop_to_form_access_chain,
                            opt_incompatibility_pair,
                            speculation,
                        ),

                        VirtualFrameUseOp::ReactDeepReadOnly(box (_, DroType::DebugAnnot)) => {
                            loop_impl(
                                loc,
                                frames,
                                inner.as_ref().clone(),
                                custom_error_message,
                                loc_of_aloc,
                                loc_of_prop_compatibility_reason,
                                loop_to_form_access_chain,
                                opt_incompatibility_pair,
                                speculation,
                            )
                        }

                        VirtualFrameUseOp::FunReturn(box FunReturnData {
                            lower,
                            upper,
                            lower_t,
                            upper_t,
                        }) => {
                            let (mut all_frames, explanations) = frames;
                            let incompatibility_pair = opt_incompatibility_pair(
                                inner.as_ref(),
                                &(lower.dupe(), upper.dupe()),
                            )
                            .map(|(lower, upper)| {
                                (
                                    printed_type_reference(&lower, lower_t),
                                    printed_type_reference(&upper, upper_t),
                                )
                            });
                            all_frames.push(ErrorFrame::FrameReturnValue {
                                incompatibility_pair,
                            });
                            loop_impl(
                                loc,
                                (all_frames, explanations),
                                inner.as_ref().clone(),
                                custom_error_message,
                                loc_of_aloc,
                                loc_of_prop_compatibility_reason,
                                loop_to_form_access_chain,
                                opt_incompatibility_pair,
                                speculation,
                            )
                        }

                        VirtualFrameUseOp::TypeParamBound { name } => {
                            let (mut all_frames, explanations) = frames;
                            all_frames.push(ErrorFrame::FrameTypeParameterBound(
                                name.string_of_subst_name().dupe(),
                            ));
                            loop_impl(
                                loc,
                                (all_frames, explanations),
                                inner.as_ref().clone(),
                                custom_error_message,
                                loc_of_aloc,
                                loc_of_prop_compatibility_reason,
                                loop_to_form_access_chain,
                                opt_incompatibility_pair,
                                speculation,
                            )
                        }

                        VirtualFrameUseOp::TypeGuardCompatibility => {
                            let (mut all_frames, explanations) = frames;
                            all_frames.push(ErrorFrame::FrameTypePredicate);
                            loop_impl(
                                loc,
                                (all_frames, explanations),
                                inner.as_ref().clone(),
                                custom_error_message,
                                loc_of_aloc,
                                loc_of_prop_compatibility_reason,
                                loop_to_form_access_chain,
                                opt_incompatibility_pair,
                                speculation,
                            )
                        }

                        VirtualFrameUseOp::UnionRepresentative(box UnionRepresentativeData {
                            loc: union_loc,
                            reference_loc,
                            union,
                        }) => {
                            let frame_loc = loc_of_aloc(union_loc);
                            let final_loc = if frame_loc.contains(&loc) {
                                loc
                            } else {
                                frame_loc
                            };
                            let (mut all_frames, explanations) = frames;
                            all_frames.push(ErrorFrame::FrameUnionRepresentative(
                                MessageTypeReferenceData {
                                    loc: reference_loc.dupe(),
                                    desc: printed_type_desc(union),
                                },
                            ));
                            loop_impl(
                                final_loc,
                                (all_frames, explanations),
                                inner.as_ref().clone(),
                                custom_error_message,
                                loc_of_aloc,
                                loc_of_prop_compatibility_reason,
                                loop_to_form_access_chain,
                                opt_incompatibility_pair,
                                speculation,
                            )
                        }

                        VirtualFrameUseOp::ReactDeepReadOnly(box (props_loc, DroType::Props)) => {
                            let (all_frames, mut explanations) = frames;
                            explanations.push(
                                Explanation::ExplanationReactComponentPropsDeepReadOnly(
                                    props_loc.dupe(),
                                ),
                            );
                            loop_impl(
                                loc,
                                (all_frames, explanations),
                                inner.as_ref().clone(),
                                custom_error_message,
                                loc_of_aloc,
                                loc_of_prop_compatibility_reason,
                                loop_to_form_access_chain,
                                opt_incompatibility_pair,
                                speculation,
                            )
                        }

                        VirtualFrameUseOp::ReactDeepReadOnly(box (props_loc, DroType::HookArg)) => {
                            let (all_frames, mut explanations) = frames;
                            explanations.push(Explanation::ExplanationReactHookArgsDeepReadOnly(
                                props_loc.dupe(),
                            ));
                            loop_impl(
                                loc,
                                (all_frames, explanations),
                                inner.as_ref().clone(),
                                custom_error_message,
                                loc_of_aloc,
                                loc_of_prop_compatibility_reason,
                                loop_to_form_access_chain,
                                opt_incompatibility_pair,
                                speculation,
                            )
                        }

                        VirtualFrameUseOp::ReactDeepReadOnly(box (
                            hook_loc,
                            DroType::HookReturn,
                        )) => {
                            let (all_frames, mut explanations) = frames;
                            explanations.push(Explanation::ExplanationReactHookReturnDeepReadOnly(
                                hook_loc.dupe(),
                            ));
                            loop_impl(
                                loc,
                                (all_frames, explanations),
                                inner.as_ref().clone(),
                                custom_error_message,
                                loc_of_aloc,
                                loc_of_prop_compatibility_reason,
                                loop_to_form_access_chain,
                                opt_incompatibility_pair,
                                speculation,
                            )
                        }

                        VirtualFrameUseOp::ConstrainedAssignment(
                            box ConstrainedAssignmentData {
                                name,
                                declaration,
                                providers,
                            },
                        ) => {
                            let (all_frames, mut explanations) = frames;
                            explanations.push(Explanation::ExplanationConstrainedAssign(Box::new(
                                ExplanationConstrainedAssignData {
                                    name: name.clone(),
                                    declaration: declaration.dupe(),
                                    providers: providers.clone(),
                                },
                            )));
                            loop_impl(
                                loc,
                                (all_frames, explanations),
                                inner.as_ref().clone(),
                                custom_error_message,
                                loc_of_aloc,
                                loc_of_prop_compatibility_reason,
                                loop_to_form_access_chain,
                                opt_incompatibility_pair,
                                speculation,
                            )
                        }

                        VirtualFrameUseOp::FunCompatibility { lower, .. } => {
                            let frame_loc = loc_of_aloc(&lower.loc);
                            let final_loc = if frame_loc.contains(&loc) {
                                loc
                            } else {
                                frame_loc
                            };
                            loop_impl(
                                final_loc,
                                frames,
                                inner.as_ref().clone(),
                                custom_error_message,
                                loc_of_aloc,
                                loc_of_prop_compatibility_reason,
                                loop_to_form_access_chain,
                                opt_incompatibility_pair,
                                speculation,
                            )
                        }

                        VirtualFrameUseOp::ArrayElementCompatibility(
                            box ArrayElementCompatibilityData {
                                lower,
                                upper,
                                lower_t,
                                upper_t,
                            },
                        ) => {
                            let frame_loc = loc_of_aloc(&lower.loc);
                            let final_loc = if frame_loc.contains(&loc) {
                                loc
                            } else {
                                frame_loc
                            };

                            let (mut all_frames, explanations) = frames;
                            let incompatibility_pair = opt_incompatibility_pair(
                                inner.as_ref(),
                                &(lower.dupe(), upper.dupe()),
                            )
                            .map(|(lower, upper)| {
                                (
                                    printed_type_reference(&lower, lower_t),
                                    printed_type_reference(&upper, upper_t),
                                )
                            });
                            all_frames.push(ErrorFrame::FrameArrayElement {
                                incompatibility_pair,
                            });
                            loop_impl(
                                final_loc,
                                (all_frames, explanations),
                                inner.as_ref().clone(),
                                custom_error_message,
                                loc_of_aloc,
                                loc_of_prop_compatibility_reason,
                                loop_to_form_access_chain,
                                opt_incompatibility_pair,
                                speculation,
                            )
                        }

                        VirtualFrameUseOp::IndexerKeyCompatibility(
                            box IndexerKeyCompatibilityData {
                                lower,
                                upper,
                                lower_t,
                                ..
                            },
                        ) => {
                            let frame_loc = loc_of_aloc(&lower.loc);
                            let final_loc = if frame_loc.contains(&loc) {
                                loc
                            } else {
                                frame_loc
                            };

                            let (mut all_frames, explanations) = frames;
                            let incompatibility_pair = opt_incompatibility_pair(
                                inner.as_ref(),
                                &(lower.dupe(), upper.dupe()),
                            )
                            .map(|(lower, upper)| (printed_type_reference(&lower, lower_t), upper));
                            all_frames.push(ErrorFrame::FrameIndexerPropertyKey {
                                incompatibility_pair,
                            });
                            loop_impl(
                                final_loc,
                                (all_frames, explanations),
                                inner.as_ref().clone(),
                                custom_error_message,
                                loc_of_aloc,
                                loc_of_prop_compatibility_reason,
                                loop_to_form_access_chain,
                                opt_incompatibility_pair,
                                speculation,
                            )
                        }

                        VirtualFrameUseOp::TypeArgCompatibility(box TypeArgCompatibilityData {
                            targ,
                            lower,
                            ..
                        }) => {
                            let frame_loc = loc_of_aloc(&lower.loc);
                            let final_loc = if frame_loc.contains(&loc) {
                                loc
                            } else {
                                frame_loc
                            };

                            let (mut all_frames, explanations) = frames;
                            all_frames.push(ErrorFrame::FrameTypeArgument(targ.dupe()));
                            loop_impl(
                                final_loc,
                                (all_frames, explanations),
                                inner.as_ref().clone(),
                                custom_error_message,
                                loc_of_aloc,
                                loc_of_prop_compatibility_reason,
                                loop_to_form_access_chain,
                                opt_incompatibility_pair,
                                speculation,
                            )
                        }

                        VirtualFrameUseOp::EnumRepresentationTypeCompatibility {
                            lower, ..
                        } => {
                            let frame_loc = loc_of_aloc(&lower.loc);
                            let final_loc = if frame_loc.contains(&loc) {
                                loc
                            } else {
                                frame_loc
                            };

                            let (mut all_frames, explanations) = frames;
                            all_frames.push(ErrorFrame::FrameEnumRepresentationType);
                            loop_impl(
                                final_loc,
                                (all_frames, explanations),
                                inner.as_ref().clone(),
                                custom_error_message,
                                loc_of_aloc,
                                loc_of_prop_compatibility_reason,
                                loop_to_form_access_chain,
                                opt_incompatibility_pair,
                                speculation,
                            )
                        }

                        VirtualFrameUseOp::FunParam(box FunParamData {
                            n,
                            lower,
                            upper,
                            name,
                            lower_t,
                            upper_t,
                        }) if matches!(
                            inner.as_ref(),
                            VirtualUseOp::Frame(f_rc, _) if matches!(
                                f_rc.as_ref(),
                                VirtualFrameUseOp::FunCompatibility { .. }
                            )
                        ) =>
                        {
                            let frame_loc = loc_of_aloc(&lower.loc);
                            let final_loc = if frame_loc.contains(&loc) {
                                loc
                            } else {
                                frame_loc
                            };

                            let (mut all_frames, explanations) = frames;
                            let incompatibility_pair = opt_incompatibility_pair(
                                inner.as_ref(),
                                &(lower.dupe(), upper.dupe()),
                            )
                            .map(|(lower, upper)| {
                                (
                                    printed_type_reference(&lower, lower_t),
                                    printed_type_reference(&upper, upper_t),
                                )
                            });

                            let frame = if name.as_deref() == Some("this") {
                                ErrorFrame::FrameFunThisParam {
                                    incompatibility_pair,
                                }
                            } else {
                                ErrorFrame::FrameFunNthParam {
                                    n: *n,
                                    incompatibility_pair,
                                }
                            };

                            all_frames.push(frame);
                            loop_impl(
                                final_loc,
                                (all_frames, explanations),
                                inner.as_ref().clone(),
                                custom_error_message,
                                loc_of_aloc,
                                loc_of_prop_compatibility_reason,
                                loop_to_form_access_chain,
                                opt_incompatibility_pair,
                                speculation,
                            )
                        }

                        VirtualFrameUseOp::FunParam(box FunParamData {
                            n,
                            lower,
                            upper,
                            name,
                            lower_t,
                            upper_t,
                        }) => {
                            let frame_loc = loc_of_aloc(&lower.loc);
                            let final_loc = if frame_loc.contains(&loc) {
                                loc
                            } else {
                                frame_loc
                            };

                            let (mut all_frames, explanations) = frames;
                            let incompatibility_pair = opt_incompatibility_pair(
                                inner.as_ref(),
                                &(lower.dupe(), upper.dupe()),
                            )
                            .map(|(lower, upper)| {
                                (
                                    printed_type_reference(&lower, lower_t),
                                    printed_type_reference(&upper, upper_t),
                                )
                            });

                            let frame = if name.as_deref() == Some("this") {
                                ErrorFrame::FrameFunThisArgument {
                                    incompatibility_pair,
                                }
                            } else {
                                ErrorFrame::FrameFunNthArgument {
                                    n: *n,
                                    incompatibility_pair,
                                }
                            };

                            all_frames.push(frame);
                            loop_impl(
                                final_loc,
                                (all_frames, explanations),
                                inner.as_ref().clone(),
                                custom_error_message,
                                loc_of_aloc,
                                loc_of_prop_compatibility_reason,
                                loop_to_form_access_chain,
                                opt_incompatibility_pair,
                                speculation,
                            )
                        }

                        VirtualFrameUseOp::PropertyCompatibility(
                            box PropertyCompatibilityData {
                                prop: None,
                                lower,
                                upper,
                                lower_t,
                                ..
                            },
                        ) => {
                            let frame_loc = loc_of_aloc(&lower.loc);
                            let final_loc = if frame_loc.contains(&loc) {
                                loc
                            } else {
                                frame_loc
                            };

                            let (mut all_frames, explanations) = frames;
                            let incompatibility_pair = opt_incompatibility_pair(
                                inner.as_ref(),
                                &(lower.dupe(), upper.dupe()),
                            )
                            .map(|(lower, upper)| (printed_type_reference(&lower, lower_t), upper));

                            all_frames.push(ErrorFrame::FrameIndexerProperty {
                                incompatibility_pair,
                            });
                            loop_impl(
                                final_loc,
                                (all_frames, explanations),
                                inner.as_ref().clone(),
                                custom_error_message,
                                loc_of_aloc,
                                loc_of_prop_compatibility_reason,
                                loop_to_form_access_chain,
                                opt_incompatibility_pair,
                                speculation,
                            )
                        }

                        VirtualFrameUseOp::PropertyCompatibility(
                            box PropertyCompatibilityData {
                                prop: Some(name),
                                lower,
                                upper,
                                lower_t,
                                ..
                            },
                        ) if name.matches_str("$call") => {
                            let frame_loc = loc_of_aloc(&lower.loc);
                            let final_loc = if frame_loc.contains(&loc) {
                                loc
                            } else {
                                frame_loc
                            };

                            let (mut all_frames, explanations) = frames;
                            let incompatibility_pair = opt_incompatibility_pair(
                                inner.as_ref(),
                                &(lower.dupe(), upper.dupe()),
                            )
                            .map(|(lower, upper)| (printed_type_reference(&lower, lower_t), upper));

                            all_frames.push(ErrorFrame::FrameCallableSignature {
                                incompatibility_pair,
                            });
                            loop_impl(
                                final_loc,
                                (all_frames, explanations),
                                inner.as_ref().clone(),
                                custom_error_message,
                                loc_of_aloc,
                                loc_of_prop_compatibility_reason,
                                loop_to_form_access_chain,
                                opt_incompatibility_pair,
                                speculation,
                            )
                        }

                        VirtualFrameUseOp::OpaqueTypeLowerBoundCompatibility { lower, .. }
                        | VirtualFrameUseOp::OpaqueTypeUpperBoundCompatibility { lower, .. } => {
                            let frame_loc = loc_of_aloc(&lower.loc);
                            let final_loc = if frame_loc.contains(&loc) {
                                loc
                            } else {
                                frame_loc
                            };

                            let (mut all_frames, explanations) = frames;
                            all_frames.push(ErrorFrame::FrameAnonymous);
                            loop_impl(
                                final_loc,
                                (all_frames, explanations),
                                inner.as_ref().clone(),
                                custom_error_message,
                                loc_of_aloc,
                                loc_of_prop_compatibility_reason,
                                loop_to_form_access_chain,
                                opt_incompatibility_pair,
                                speculation,
                            )
                        }

                        VirtualFrameUseOp::OpaqueTypeCustomErrorCompatibility(
                            box OpaqueTypeCustomErrorCompatibilityData {
                                lower,
                                upper,
                                lower_t,
                                upper_t,
                                name,
                                custom_error_loc,
                            },
                        ) => {
                            let lower_desc = match lower_t {
                                TypeOrTypeDescT::Type(_) => ErrorTypeDesc::Unknown,
                                TypeOrTypeDescT::TypeDesc(d) => d.clone(),
                            };
                            let upper_desc = match upper_t {
                                TypeOrTypeDescT::Type(_) => ErrorTypeDesc::Unknown,
                                TypeOrTypeDescT::TypeDesc(d) => d.clone(),
                            };

                            // Keep as Message since we're in the generic context
                            let custom_error_message =
                                Some(Message::MessageIncompatibleGeneralWithPrintedTypes(
                                    Box::new(MessageIncompatibleGeneralWithPrintedTypesData {
                                        lower_loc: lower.loc.dupe(),
                                        upper_loc: upper.loc.dupe(),
                                        lower_desc,
                                        upper_desc,
                                    }),
                                ));

                            // Keep as Explanation
                            let frames = (
                                vec![],
                                vec![Explanation::ExplanationCustomError(Box::new(
                                    ExplanationCustomErrorData {
                                        name: name.clone(),
                                        custom_error_loc: custom_error_loc.dupe(),
                                    },
                                ))],
                            );

                            let loc = loc_of_aloc(&lower.loc);
                            let frame_loc = loc_of_aloc(&lower.loc);
                            let final_loc = if frame_loc.contains(&loc) {
                                loc
                            } else {
                                frame_loc
                            };

                            loop_impl(
                                final_loc,
                                frames,
                                inner.as_ref().clone(),
                                custom_error_message,
                                loc_of_aloc,
                                loc_of_prop_compatibility_reason,
                                loop_to_form_access_chain,
                                opt_incompatibility_pair,
                                speculation,
                            )
                        }

                        VirtualFrameUseOp::PropertyCompatibility(
                            box PropertyCompatibilityData {
                                prop: Some(prop),
                                lower,
                                upper,
                                lower_t,
                                ..
                            },
                        ) => {
                            let lower_loc =
                                loc_of_prop_compatibility_reason(loc.dupe(), lower, &inner);

                            let (lower_loc, props, use_op) =
                                loop_to_form_access_chain(lower_loc, inner.as_ref().clone());

                            let frame_loc = lower_loc;
                            let final_loc = if frame_loc.contains(&loc) {
                                loc
                            } else {
                                frame_loc.dupe()
                            };

                            let (mut all_frames, explanations) = frames;
                            let incompatibility_pair =
                                opt_incompatibility_pair(&use_op, &(lower.dupe(), upper.dupe()))
                                    .map(|(lower, upper)| {
                                        (printed_type_reference(&lower, lower_t), upper)
                                    });

                            let mut chain_vec = vec![AccessChainSegment::PropSegment(prop.dupe())];
                            chain_vec.extend(props);
                            let chain =
                                Vec1::try_from_vec(chain_vec).expect("chain must be non-empty");

                            all_frames.push(ErrorFrame::FrameAccessChain {
                                chain,
                                incompatibility_pair,
                            });

                            loop_impl(
                                final_loc,
                                (all_frames, explanations),
                                use_op,
                                custom_error_message,
                                loc_of_aloc,
                                loc_of_prop_compatibility_reason,
                                loop_to_form_access_chain,
                                opt_incompatibility_pair,
                                speculation,
                            )
                        }

                        VirtualFrameUseOp::TupleElementCompatibility(
                            box TupleElementCompatibilityData {
                                n,
                                lower,
                                upper,
                                lower_t,
                                ..
                            },
                        ) => {
                            let lower_loc = loc_of_aloc(&lower.loc);

                            let (lower_loc, props, use_op) =
                                loop_to_form_access_chain(lower_loc, inner.as_ref().clone());

                            let frame_loc = lower_loc;
                            let final_loc = if frame_loc.contains(&loc) {
                                loc
                            } else {
                                frame_loc.dupe()
                            };

                            let (mut all_frames, explanations) = frames;
                            let incompatibility_pair =
                                opt_incompatibility_pair(&use_op, &(lower.dupe(), upper.dupe()))
                                    .map(|(lower, upper)| {
                                        (printed_type_reference(&lower, lower_t), upper)
                                    });

                            let mut chain_vec = vec![AccessChainSegment::TupleIndexSegment(*n)];
                            chain_vec.extend(props);
                            let chain =
                                Vec1::try_from_vec(chain_vec).expect("chain must be non-empty");

                            all_frames.push(ErrorFrame::FrameAccessChain {
                                chain,
                                incompatibility_pair,
                            });

                            loop_impl(
                                final_loc,
                                (all_frames, explanations),
                                use_op,
                                custom_error_message,
                                loc_of_aloc,
                                loc_of_prop_compatibility_reason,
                                loop_to_form_access_chain,
                                opt_incompatibility_pair,
                                speculation,
                            )
                        }
                    },
                }
            }

            let (root, msg, loc, mut frames, mut explanations) = loop_impl(
                initial_loc,
                (vec![], vec![]),
                initial_use_op,
                None,
                &loc_of_aloc,
                &loc_of_prop_compatibility_reason,
                &loop_to_form_access_chain,
                &opt_incompatibility_pair,
                speculation,
            );
            frames.reverse();
            explanations.reverse();
            (root, msg, loc, frames, explanations)
        }
    }

    let loc = error.loc_of_error();
    let msg = error.msg_of_error();
    let source_file = error.source_file();
    let kind = msg.kind_of_msg();

    let mk_use_op_error = |loc: Loc,
                           use_op: VirtualUseOp<L>,
                           explanation: Option<Explanation<L>>,
                           message: Message<L>|
     -> IntermediateError<L> {
        let unwrapper = unwrap_use_ops(&loc_of_aloc, speculation);
        let (root, custom_error_msg, loc, frames, explanations) = unwrapper(loc, use_op);

        let code = error.code_of_error();
        let explanations = {
            let mut v = Vec::with_capacity(explanations.len() + explanation.is_some() as usize);
            if let Some(exp) = explanation {
                v.push(exp);
            }
            v.extend(explanations);
            Some(v)
        };
        let message = match custom_error_msg {
            Some(ref custom) => custom.clone(),
            None => message,
        };
        mk_error(kind, loc, code, message, root, Some(frames), explanations)
    };

    let mk_use_op_error_reason = |reason: &VirtualReason<L>,
                                  use_op: VirtualUseOp<L>,
                                  explanation: Option<Explanation<L>>,
                                  message: Message<L>|
     -> IntermediateError<L> {
        mk_use_op_error(loc_of_aloc(&reason.loc), use_op, explanation, message)
    };

    let mk_no_frame_or_explanation_error = |loc: &L, message: Message<L>| -> IntermediateError<L> {
        let loc = loc_of_aloc(loc);
        let code = error.code_of_error();
        mk_error(kind, loc, code, message, None, Some(vec![]), Some(vec![]))
    };

    let mk_prop_missing_in_subtyping_error = |prop: Option<FlowSmolStr>,
                                              suggestion: Option<FlowSmolStr>,
                                              loc: L,
                                              lower: MessageTypeReferenceData<L>,
                                              upper: MessageTypeReferenceData<L>,
                                              use_op: VirtualUseOp<L>|
     -> IntermediateError<L> {
        mk_use_op_error(
            loc_of_aloc(&loc),
            use_op,
            None,
            Message::MessagePropMissingWithPrintedType {
                lower: Box::new(lower),
                upper: Some(Box::new(upper)),
                prop,
                suggestion,
            },
        )
    };

    let mk_props_missing_in_subtyping_error = |props: Vec1<FlowSmolStr>,
                                               loc: L,
                                               lower: MessageTypeReferenceData<L>,
                                               upper: MessageTypeReferenceData<L>,
                                               use_op: VirtualUseOp<L>|
     -> IntermediateError<L> {
        mk_use_op_error(
            loc_of_aloc(&loc),
            use_op,
            None,
            Message::MessagePropsMissingWithPrintedType(Box::new(
                MessagePropsMissingWithPrintedTypeData {
                    lower,
                    upper,
                    props,
                },
            )),
        )
    };

    let mk_prop_polarity_mismatch_error =
        |loc: L,
         lower: PropPolarityMismatchObject<MessageTypeReferenceData<L>, L>,
         upper: PropPolarityMismatchObject<MessageTypeReferenceData<L>, L>,
         props: Vec1<(Option<FlowSmolStr>, Polarity, Polarity)>,
         use_op: VirtualUseOp<L>|
         -> IntermediateError<L> {
            let use_op = match &use_op {
                VirtualUseOp::Frame(frame_rc, inner_use_op) => match frame_rc.as_ref() {
                    VirtualFrameUseOp::PropertyCompatibility(box PropertyCompatibilityData {
                        prop,
                        ..
                    }) => {
                        // `props` is already rendered, so this matches on display
                        // spelling: two unnamed symbol keys both render `[symbol]`
                        // and compare equal here. That only picks which use_op
                        // frame labels the message, never which property is
                        // reported, so the collision is cosmetic.
                        let prop_str: Option<FlowSmolStr> =
                            prop.as_ref().map(|n| n.display_smol_str());
                        if props.iter().any(|(p, _, _)| p == &prop_str) {
                            inner_use_op.dupe().as_ref().clone()
                        } else {
                            use_op
                        }
                    }
                    _ => use_op,
                },
                _ => use_op,
            };

            mk_use_op_error(
                loc_of_aloc(&loc),
                use_op,
                None,
                Message::MessagePropPolarityMismatch(Box::new(MessagePropPolarityMismatchData {
                    lower,
                    upper,
                    props,
                })),
            )
        };

    let mk_incompatible_invariant_subtyping_error =
        |additional_explanation: Option<Explanation<L>>,
         sub_component: Option<SubComponentOfInvariantSubtypingError>,
         lower_loc: L,
         upper_loc: L,
         lower_desc: ErrorTypeDesc<L>,
         upper_desc: ErrorTypeDesc<L>,
         use_op: VirtualUseOp<L>|
         -> IntermediateError<L> {
            mk_use_op_error(
                loc_of_aloc(&lower_loc),
                use_op,
                additional_explanation,
                Message::MessageIncompatibleDueToInvariantSubtyping(Box::new(
                    MessageIncompatibleDueToInvariantSubtypingData {
                        sub_component,
                        lower_loc,
                        upper_loc,
                        lower_desc,
                        upper_desc,
                    },
                )),
            )
        };

    let mk_props_missing_in_invariant_subtyping_error = |props: Vec1<FlowSmolStr>,
                                                         loc: L,
                                                         lower_obj_loc: L,
                                                         upper_obj_loc: L,
                                                         lower: MessageTypeReferenceData<L>,
                                                         upper: MessageTypeReferenceData<L>,
                                                         use_op: VirtualUseOp<L>|
     -> IntermediateError<L> {
        let props_plural = props.len() > 1;
        let explanation = Some(
            Explanation::ExplanationPropertyMissingDueToNeutralOptionalProperty(Box::new(
                ExplanationPropertyMissingDueToNeutralOptionalPropertyData {
                    props_plural,
                    lower_obj_loc,
                    upper_obj_loc,
                    lower_obj_desc: lower.desc.clone(),
                    upper_obj_desc: upper.desc.clone(),
                    upper_object_loc: upper.loc.dupe(),
                },
            )),
        );
        mk_use_op_error(
            loc_of_aloc(&loc),
            use_op,
            explanation,
            Message::MessagePropsMissingWithPrintedType(Box::new(
                MessagePropsMissingWithPrintedTypeData {
                    lower,
                    upper,
                    props,
                },
            )),
        )
    };

    let mk_use_op_speculation_error = |loc: L,
                                       use_op: VirtualUseOp<L>,
                                       branches: Vec<FlowErrorMessage<L>>|
     -> IntermediateError<L> {
        use super::flow_error::error_of_msg;

        if let VirtualUseOp::Frame(frame, inner_use_op) = &use_op {
            if let VirtualFrameUseOp::MappedTypeKeyCompatibility(
                box MappedTypeKeyCompatibilityData {
                    source_type_loc,
                    source_type,
                    mapped_type_loc,
                    mapped_type,
                },
            ) = frame.as_ref()
            {
                return mk_use_op_error(
                    loc_of_aloc(source_type_loc),
                    inner_use_op.as_ref().clone(),
                    None,
                    Message::MessageIncompatibleMappedTypeKey {
                        source_type: MessageTypeReferenceData {
                            loc: source_type_loc.dupe(),
                            desc: printed_type_desc(source_type),
                        },
                        mapped_type: MessageTypeReferenceData {
                            loc: mapped_type_loc.dupe(),
                            desc: printed_type_desc(mapped_type),
                        },
                    },
                );
            }
        }

        let unwrapper = unwrap_use_ops(&loc_of_aloc, speculation);
        let (root, custom_error_msg, unwrapped_loc, frames, explanations) =
            unwrapper(loc_of_aloc(&loc), use_op);

        let error_code = error.code_of_error();

        match custom_error_msg {
            Some(custom_msg) => mk_error(
                kind,
                unwrapped_loc,
                error_code,
                custom_msg,
                root,
                Some(frames),
                Some(explanations),
            ),
            None => {
                let speculation_errors: Vec<(i32, IntermediateError<L>)> = branches
                    .into_iter()
                    .map(|msg| {
                        let score = score_of_msg(&msg);
                        let branch_error = error_of_msg(source_file.dupe(), msg);
                        let intermediate = make_intermediate_error(
                            loc_of_aloc.clone(),
                            true,
                            &branch_error,
                            is_lib.clone(),
                        );
                        (score, intermediate)
                    })
                    .collect();

                mk_speculation_error(
                    kind,
                    unwrapped_loc,
                    root,
                    frames,
                    explanations,
                    error_code,
                    speculation_errors,
                )
            }
        }
    };

    let missing_argument_message =
        |def: MessageTypeReferenceData<L>,
         op: &VirtualReason<L>,
         inner_use_op: &VirtualUseOp<L>| {
            if let VirtualUseOp::Op(inner_root) = inner_use_op
                && matches!(
                    inner_root.as_ref(),
                    VirtualRootUseOp::FunCall(..) | VirtualRootUseOp::FunCallMethod(..)
                )
            {
                Message::MessageFunctionRequiresAnotherArgument { def, from: None }
            } else if op.desc(false).is_none()
                && let Some(kind) = implicit_instantiation_reference_kind(inner_use_op)
            {
                Message::MessageFunctionRequiresAnotherArgumentFromCall {
                    def,
                    call: ImplicitInstantiationReferenceData {
                        loc: op.loc().dupe(),
                        kind: kind.dupe(),
                    },
                }
            } else {
                Message::MessageFunctionRequiresAnotherArgument {
                    def,
                    from: Some(op.dupe()),
                }
            }
        };

    // An error between two incompatible types. A "lower" type and an "upper" type.
    let mk_incompatible_error = |additional_explanation: Option<Explanation<L>>,
                                 lower: VirtualReason<L>,
                                 upper: VirtualReason<L>,
                                 use_op: VirtualUseOp<L>|
     -> IntermediateError<L> {
        // Flip contravariant positions
        let ((lower, upper), use_op) = flip_contravariant(lower, upper, use_op);

        // Local helper to create error with use_op context
        let make_error = |reason: &VirtualReason<L>, message: Message<L>| -> IntermediateError<L> {
            mk_use_op_error_reason(
                reason,
                use_op.clone(),
                additional_explanation.clone(),
                message,
            )
        };

        // Modify lower and upper according to use_ops
        let lower = mod_lower_reason_according_to_use_ops(lower, &use_op);
        let upper = mod_upper_reason_according_to_use_ops(upper, &use_op);

        match &use_op {
            VirtualUseOp::Op(root) => match root.as_ref() {
                VirtualRootUseOp::Coercion { from, .. } => mk_use_op_error_reason(
                    from,
                    use_op.clone(),
                    additional_explanation.clone(),
                    Message::MessageShouldNotBeCoerced(lower.dupe()),
                ),
                _ => {
                    let root_use_op = root_of_use_op(&use_op);
                    match root_use_op {
                        VirtualRootUseOp::ComponentRestParamCompatibility(
                            box ComponentRestParamCompatibilityData {
                                rest_param_loc,
                                rest_param,
                            },
                        ) => mk_no_frame_or_explanation_error(
                            rest_param_loc,
                            Message::MessageIncompatibleComponentRestParam(
                                MessageTypeReferenceData {
                                    loc: rest_param_loc.dupe(),
                                    desc: printed_type_desc(rest_param),
                                },
                            ),
                        ),
                        _ => make_error(
                            &lower,
                            Message::MessageIncompatibleGeneral {
                                lower: lower.dupe(),
                                upper: upper.dupe(),
                            },
                        ),
                    }
                }
            },

            VirtualUseOp::Frame(frame, inner_use_op) => match frame.as_ref() {
                VirtualFrameUseOp::FunMissingArg(box FunMissingArgData {
                    def, op, def_t, ..
                }) => {
                    let message = missing_argument_message(
                        printed_type_reference(def, def_t),
                        op,
                        inner_use_op,
                    );
                    mk_use_op_error_reason(
                        op,
                        use_op.clone(),
                        additional_explanation.clone(),
                        message,
                    )
                }

                VirtualFrameUseOp::RendersCompatibility => make_error(
                    &lower,
                    Message::MessageDoesNotRender {
                        lower: lower.dupe(),
                        upper: upper.dupe(),
                    },
                ),

                VirtualFrameUseOp::UnionRepresentative(box UnionRepresentativeData {
                    loc: union_loc,
                    reference_loc,
                    union,
                }) => mk_use_op_error(
                    loc_of_aloc(union_loc),
                    inner_use_op.as_ref().clone(),
                    additional_explanation.clone(),
                    Message::MessageIncompatibleWithUnionRepresentative {
                        union: MessageTypeReferenceData {
                            loc: reference_loc.dupe(),
                            desc: printed_type_desc(union),
                        },
                        lower: lower.dupe(),
                        upper: upper.dupe(),
                    },
                ),

                _ => {
                    let root_use_op = root_of_use_op(&use_op);
                    match root_use_op {
                        VirtualRootUseOp::ComponentRestParamCompatibility(
                            box ComponentRestParamCompatibilityData {
                                rest_param_loc,
                                rest_param,
                            },
                        ) => mk_no_frame_or_explanation_error(
                            rest_param_loc,
                            Message::MessageIncompatibleComponentRestParam(
                                MessageTypeReferenceData {
                                    loc: rest_param_loc.dupe(),
                                    desc: printed_type_desc(rest_param),
                                },
                            ),
                        ),
                        _ => make_error(
                            &lower,
                            Message::MessageIncompatibleGeneral {
                                lower: lower.dupe(),
                                upper: upper.dupe(),
                            },
                        ),
                    }
                }
            },
        }
    };

    // An error between two incompatible types. The use_kind specifies the kind of incompatibility.
    // Unlike mk_incompatible_error, this handles specific use type incompatibilities.
    let mk_incompatible_use_error = |use_loc: Loc,
                                     use_kind: super::error_message::UpperKind<L>,
                                     lower_loc: L,
                                     lower_kind: Option<super::error_message::LowerKind>,
                                     lower_desc: ErrorTypeDesc<L>,
                                     use_op: VirtualUseOp<L>|
     -> IntermediateError<L> {
        use super::error_message::LowerKind;
        use super::error_message::UpperKind;

        let lower_type_ref = MessageTypeReferenceData {
            loc: lower_loc,
            desc: lower_desc,
        };
        let lower_is_not = |requirement| Message::MessageLowerIsNotWithPrintedType {
            lower: Box::new(lower_type_ref.clone()),
            requirement,
        };
        let prop_missing = |prop: Option<FlowSmolStr>| match prop {
            None if matches!(
                lower_kind,
                Some(LowerKind::PossiblyNull | LowerKind::PossiblyVoid)
            ) =>
            {
                Message::MessageLowerDoesNotHavePropertiesWithPrintedType(Box::new(
                    lower_type_ref.clone(),
                ))
            }
            prop => Message::MessagePropMissingWithPrintedType {
                lower: Box::new(lower_type_ref.clone()),
                upper: None,
                prop,
                suggestion: None,
            },
        };

        match use_kind {
            UpperKind::IncompatibleElemTOfArrT => mk_use_op_error(
                use_loc,
                use_op,
                None,
                lower_is_not(LowerRequirement::ArrayIndex),
            ),

            UpperKind::IncompatibleGetPrivatePropT | UpperKind::IncompatibleSetPrivatePropT => {
                mk_use_op_error(
                    use_loc,
                    use_op,
                    None,
                    lower_is_not(LowerRequirement::ClassWithPrivateProperties),
                )
            }

            UpperKind::IncompatibleMixedCallT => mk_use_op_error(
                use_loc,
                use_op,
                None,
                Message::MessageUnknownParameterTypes(lower_type_ref.loc.dupe()),
            ),

            UpperKind::IncompatibleCallT => mk_use_op_error(
                use_loc,
                use_op,
                None,
                lower_is_not(LowerRequirement::Function),
            ),

            UpperKind::IncompatibleObjAssignFromTSpread | UpperKind::IncompatibleArrRestT => {
                mk_use_op_error(use_loc, use_op, None, lower_is_not(LowerRequirement::Array))
            }

            UpperKind::IncompatibleObjAssignFromT
            | UpperKind::IncompatibleObjRestT
            | UpperKind::IncompatibleGetKeysT
            | UpperKind::IncompatibleGetValuesT => mk_use_op_error(
                use_loc,
                use_op,
                None,
                lower_is_not(LowerRequirement::Object),
            ),

            UpperKind::IncompatibleMapTypeTObject => mk_use_op_error(
                use_loc,
                use_op,
                None,
                Message::MessageInvalidKeyMirrorArgument(Box::new(
                    MessageInvalidKeyMirrorArgumentData {
                        lower: lower_type_ref.clone(),
                    },
                )),
            ),

            UpperKind::IncompatibleMixinT | UpperKind::IncompatibleThisSpecializeT => {
                mk_use_op_error(use_loc, use_op, None, lower_is_not(LowerRequirement::Class))
            }

            UpperKind::IncompatibleSpecializeT | UpperKind::IncompatibleVarianceCheckT => {
                mk_use_op_error(
                    use_loc,
                    use_op,
                    None,
                    lower_is_not(LowerRequirement::PolymorphicType),
                )
            }

            UpperKind::IncompatibleSuperT => mk_use_op_error(
                use_loc,
                use_op,
                None,
                lower_is_not(LowerRequirement::Inheritable),
            ),

            UpperKind::IncompatibleGetPropT(prop_loc, prop)
            | UpperKind::IncompatibleSetPropT(prop_loc, prop)
            | UpperKind::IncompatibleHasOwnPropT(prop_loc, prop)
            | UpperKind::IncompatibleMethodT(prop_loc, prop) => mk_use_op_error(
                loc_of_aloc(&prop_loc),
                use_op,
                None,
                prop_missing(prop.map(|p| p.display_smol_str())),
            ),

            UpperKind::IncompatibleGetElemT(prop_loc)
            | UpperKind::IncompatibleSetElemT(prop_loc)
            | UpperKind::IncompatibleCallElemT(prop_loc) => {
                mk_use_op_error(loc_of_aloc(&prop_loc), use_op, None, prop_missing(None))
            }

            UpperKind::IncompatibleGetStaticsT => mk_use_op_error(
                use_loc,
                use_op,
                None,
                lower_is_not(LowerRequirement::InstanceType),
            ),

            UpperKind::IncompatibleUnclassified(ctor) => mk_use_op_error(
                use_loc,
                use_op,
                None,
                Message::MessageLowerIsNotSupportedByUnclassifiedUseWithPrintedType {
                    lower: Box::new(lower_type_ref),
                    ctor: ctor.to_string().into(),
                },
            ),
        }
    };

    use super::error_message::FriendlyMessageRecipe;

    let recipe = msg.clone().friendly_message_of_msg();
    let mut intermediate_error = match (loc.as_ref(), recipe) {
        (Some(loc), FriendlyMessageRecipe::Normal(message)) => mk_error(
            kind,
            loc_of_aloc(loc),
            error.code_of_error(),
            message,
            None,
            None,
            None,
        ),

        (
            None,
            FriendlyMessageRecipe::UseOp(box UseOpData {
                loc: primary_loc,
                message:
                    Message::MessageIncompatibleGeneralWithPrintedTypes(
                        box MessageIncompatibleGeneralWithPrintedTypesData {
                            lower_loc,
                            upper_loc,
                            lower_desc,
                            upper_desc,
                        },
                    ),
                use_op,
                explanation,
            }),
        ) => {
            if let VirtualRootUseOp::ComponentRestParamCompatibility(
                box ComponentRestParamCompatibilityData {
                    rest_param_loc,
                    rest_param,
                },
            ) = root_of_use_op(&use_op)
            {
                mk_no_frame_or_explanation_error(
                    rest_param_loc,
                    Message::MessageIncompatibleComponentRestParam(MessageTypeReferenceData {
                        loc: rest_param_loc.dupe(),
                        desc: printed_type_desc(rest_param),
                    }),
                )
            } else if let VirtualUseOp::Frame(frame, inner_use_op) = &use_op
                && let VirtualFrameUseOp::FunMissingArg(box FunMissingArgData {
                    def,
                    op,
                    def_t,
                    ..
                }) = frame.as_ref()
            {
                let message =
                    missing_argument_message(printed_type_reference(def, def_t), op, inner_use_op);
                mk_use_op_error_reason(op, use_op.clone(), explanation, message)
            } else {
                let (((lower_loc, lower_desc), (upper_loc, upper_desc)), use_op) =
                    flip_contravariant((lower_loc, lower_desc), (upper_loc, upper_desc), use_op);
                let data = Box::new(MessageIncompatibleGeneralWithPrintedTypesData {
                    lower_loc,
                    upper_loc,
                    lower_desc,
                    upper_desc,
                });
                let message = match &use_op {
                    VirtualUseOp::Frame(frame, _)
                        if matches!(frame.as_ref(), VirtualFrameUseOp::RendersCompatibility) =>
                    {
                        Message::MessageDoesNotRenderWithPrintedTypes(data)
                    }
                    _ => Message::MessageIncompatibleGeneralWithPrintedTypes(data),
                };
                mk_use_op_error(loc_of_aloc(&primary_loc), use_op, explanation, message)
            }
        }

        (
            None,
            FriendlyMessageRecipe::IncompatibleTypesWithExample(
                box IncompatibleTypesWithExampleData {
                    loc,
                    lower_loc,
                    upper_loc,
                    lower_desc,
                    upper_desc,
                    use_op,
                    explanation,
                    example,
                },
            ),
        ) => {
            use super::flow_error::error_of_msg;

            let mut outer = mk_use_op_error(
                loc_of_aloc(&loc),
                use_op,
                explanation,
                Message::MessageIncompatibleGeneralWithPrintedTypes(Box::new(
                    MessageIncompatibleGeneralWithPrintedTypesData {
                        lower_loc,
                        upper_loc,
                        lower_desc,
                        upper_desc,
                    },
                )),
            );
            let example_error = error_of_msg(source_file.dupe(), *example);
            let example =
                make_intermediate_error(loc_of_aloc.clone(), true, &example_error, is_lib.clone());
            outer.message = match outer.message {
                ErrorMessage::SingletonMessage {
                    message,
                    frames,
                    explanations,
                } => ErrorMessage::SingletonMessageWithExample {
                    message,
                    frames,
                    explanations,
                    example: Box::new(example),
                },
                message => message,
            };
            outer
        }

        (
            None,
            FriendlyMessageRecipe::UseOp(box UseOpData {
                loc,
                message,
                use_op,
                explanation,
            }),
        ) => mk_use_op_error(loc_of_aloc(&loc), use_op, explanation, message),

        (
            None,
            FriendlyMessageRecipe::PrivatePropMissingInLookup(box PrivatePropMissingInLookupData {
                loc,
                object,
                prop,
                use_op,
            }),
        ) => mk_use_op_error(
            loc_of_aloc(&loc),
            use_op,
            None,
            Message::MessagePrivatePropMissing { object, prop },
        ),

        (
            None,
            FriendlyMessageRecipe::IndexerCheckFailed(box IndexerCheckFailedData {
                prop,
                lower,
                upper,
                indexer,
                use_op,
            }),
        ) => mk_use_op_error(
            loc_of_aloc(&lower.loc),
            use_op,
            None,
            Message::MessageIndexerCheckFailed(Box::new(MessageIndexerCheckFailedData {
                prop,
                lower,
                upper,
                indexer,
            })),
        ),

        (
            None,
            FriendlyMessageRecipe::PropMissingInSubtyping(box PropMissingInSubtypingData {
                prop,
                suggestion,
                loc,
                lower,
                upper,
                use_op,
            }),
        ) => mk_prop_missing_in_subtyping_error(
            prop.dupe(),
            suggestion.dupe(),
            loc,
            lower,
            upper,
            use_op,
        ),

        (
            None,
            FriendlyMessageRecipe::PropsMissingInSubtyping(box PropsMissingInSubtypingData {
                props,
                loc,
                lower,
                upper,
                use_op,
            }),
        ) => mk_props_missing_in_subtyping_error(
            Vec1::try_from_vec(props.iter().map(|s| FlowSmolStr::new(s.as_str())).collect())
                .unwrap(),
            loc,
            lower,
            upper,
            use_op,
        ),

        (
            None,
            FriendlyMessageRecipe::PropsMissingInInvariantSubtyping(
                box PropsMissingInInvariantSubtypingData {
                    props,
                    loc,
                    lower_obj_loc,
                    upper_obj_loc,
                    lower,
                    upper,
                    use_op,
                },
            ),
        ) => mk_props_missing_in_invariant_subtyping_error(
            Vec1::try_from_vec(props.to_vec()).unwrap(),
            loc,
            lower_obj_loc,
            upper_obj_loc,
            lower,
            upper,
            use_op,
        ),

        (
            None,
            FriendlyMessageRecipe::PropsExtraAgainstExactObject(
                box PropsExtraAgainstExactObjectData {
                    props,
                    loc,
                    lower,
                    upper,
                    upper_is_record,
                    use_op,
                },
            ),
        ) => mk_use_op_error(
            loc_of_aloc(&loc),
            use_op,
            None,
            Message::MessagePropExtraAgainstExactObject(Box::new(
                MessagePropExtraAgainstExactObjectData {
                    lower,
                    upper,
                    upper_is_record,
                    props: Vec1::try_from_vec(
                        props.iter().map(|s| FlowSmolStr::new(s.as_str())).collect(),
                    )
                    .unwrap(),
                },
            )),
        ),

        (
            None,
            FriendlyMessageRecipe::PropPolarityMismatch(box PropPolarityMismatchData {
                loc,
                lower,
                upper,
                props,
                use_op,
            }),
        ) => mk_prop_polarity_mismatch_error(
            loc,
            lower,
            upper,
            Vec1::try_from_vec(
                props
                    .iter()
                    .map(|(name, p1, p2)| (name.dupe(), *p1, *p2))
                    .collect(),
            )
            .unwrap(),
            use_op,
        ),

        (
            None,
            FriendlyMessageRecipe::IncompatibleTypeUse(box IncompatibleTypeUseData {
                loc,
                upper_kind,
                lower_loc,
                lower_kind,
                lower_desc,
                use_op,
            }),
        ) => mk_incompatible_use_error(
            loc_of_aloc(&loc),
            upper_kind,
            lower_loc,
            lower_kind,
            lower_desc,
            use_op,
        ),

        (
            None,
            FriendlyMessageRecipe::IncompatibleSubtyping(box IncompatibleSubtypingData {
                reason_lower,
                reason_upper,
                use_op,
                explanation,
            }),
        ) => mk_incompatible_error(explanation, reason_lower, reason_upper, use_op),

        (
            None,
            FriendlyMessageRecipe::IncompatibleInvariantSubtyping(
                box IncompatibleInvariantSubtypingData {
                    sub_component,
                    lower_loc,
                    upper_loc,
                    lower_desc,
                    upper_desc,
                    use_op,
                    explanation,
                },
            ),
        ) => mk_incompatible_invariant_subtyping_error(
            explanation,
            sub_component,
            lower_loc,
            upper_loc,
            lower_desc,
            upper_desc,
            use_op,
        ),

        (
            None,
            FriendlyMessageRecipe::Speculation(box SpeculationData {
                loc,
                use_op,
                branches,
            }),
        ) => mk_use_op_speculation_error(loc, use_op, branches),

        (None, FriendlyMessageRecipe::Normal(_)) | (Some(_), _) => {
            panic!("ImproperlyFormattedError: {:?}", msg)
        }
    };

    intermediate_error.misplaced_source_file = match intermediate_error.loc.source.as_ref() {
        Some(file) if !speculation && !is_lib(source_file) && file != source_file => {
            Some(source_file.dupe())
        }
        _ => None,
    };

    intermediate_error
}

fn annotation_inference_operation_description(
    operation: &AnnotationInferenceOperation,
) -> FlowSmolStr {
    use AnnotationInferenceOperation::*;

    match operation {
        ImportsAndExports => "imports and exports".into(),
        CommonJSExports => "CommonJS exports".into(),
        Inspection => "type inspection".into(),
        ImportTypeof(name) => format!("`import typeof` of `{}`", name).into(),
        Export(name) => format!("export `{}`", name.display_smol_str()).into(),
        ClassExtends => "class extends".into(),
        ClassMixins => "class mixins".into(),
        TypeApplication => "type application".into(),
        ThisSpecialization => "`this` specialization".into(),
        TypeReference => "type reference".into(),
        Property(Some(name)) => format!("property `{}`", name.display_smol_str()).into(),
        Property(None) | ComputedProperty => "computed property".into(),
        Enum => "enum".into(),
        Statics => "statics".into(),
        Exact => "exact".into(),
        ReactCheckComponentConfig => "React component config check".into(),
        ReadOnly => "readonly".into(),
        Partial => "partial".into(),
        Required => "required".into(),
        Spread => "spread".into(),
        Rest => "rest".into(),
        ReactConfig => "React config".into(),
        Object => "object".into(),
        MappedType => "mapped type".into(),
        Prototype => "prototype".into(),
        Mixins => "mixins".into(),
        Arithmetic(operator) => format!("arithmetic operator `{}`", operator).into(),
        UnaryPlus => "unary plus".into(),
        UnaryMinus => "unary minus".into(),
        BitwiseNot => "bitwise not".into(),
        Update => "update".into(),
        UnaryNot => "unary not".into(),
        ObjectKeyMirror => "`$KeyMirror`".into(),
        DeepReadOnly => "deep readonly".into(),
        KeySet => "key set".into(),
        ToString => "string conversion".into(),
        Values => "values".into(),
        ArrayRest => "array rest".into(),
    }
}

#[allow(dead_code)]
pub fn to_printable_error<L, F, G>(
    loc_of_aloc: F,
    get_ast: G,
    strip_root: Option<&std::path::Path>,
    intermediate_error: IntermediateError<L>,
) -> PrintableError<Loc>
where
    L: Dupe + PartialEq + Eq + PartialOrd + Ord + Clone + std::fmt::Debug,
    F: Fn(&L) -> Loc + Clone,
    G: Fn(&FileKey) -> Option<Arc<flow_parser::ast::Program<Loc, Loc>>>,
{
    use flow_common_errors::error_utils::RootKind;
    use flow_common_errors::error_utils::friendly;

    let text = |s: &str| -> friendly::MessageFeature<Loc> { friendly::text(s) };

    let code = |s: &str| -> friendly::MessageFeature<Loc> { friendly::code(s) };

    let ref_ = |r: &VirtualReason<L>| -> friendly::MessageFeature<Loc> {
        friendly::ref_map(&loc_of_aloc, r)
    };

    let msg_export = |prefix: &str,
                      export_name: &str|
     -> (friendly::MessageFeature<Loc>, friendly::MessageFeature<Loc>) {
        if export_name == "default" {
            (text(""), text("the default export"))
        } else {
            (text(prefix), code(export_name))
        }
    };

    fn decl_description(decl: &Decl<ALoc>) -> String {
        match decl {
            Decl::ClassDecl(box (name, _)) => format!("class `{}`", name.sym_name),
            Decl::InterfaceDecl(box (name, _)) => format!("interface `{}`", name.sym_name),
            Decl::EnumDecl(box flow_common_ty::ty::DeclEnumDeclData { name, .. }) => {
                format!("enum `{}`", name.sym_name)
            }
            Decl::NominalComponentDecl(box flow_common_ty::ty::DeclNominalComponentDeclData {
                name,
                ..
            }) => format!("component {}", name.sym_name),
            Decl::TypeAliasDecl(box flow_common_ty::ty::DeclTypeAliasDeclData { name, .. }) => {
                format!("`{}`", name.sym_name)
            }
            Decl::VariableDecl(_)
            | Decl::RecordDecl(_)
            | Decl::NamespaceDecl(_)
            | Decl::ModuleDecl(_) => flow_common::string_utils::code_span(
                &flow_common_ty::ty_printer::string_of_decl_single_line(
                    decl,
                    &flow_common_ty::ty_printer::PrinterOptions::default(),
                ),
            ),
        }
    }

    let ref_of_type_desc = |loc: &L,
                            type_desc: &ErrorTypeDesc<L>|
     -> friendly::MessageFeature<Loc> {
        match type_desc {
            ErrorTypeDesc::Normalized(Elt::Decl(decl)) => {
                friendly::hardcoded_string_desc_ref(&decl_description(decl), loc_of_aloc(loc))
            }
            ErrorTypeDesc::Normalized(Elt::Type(ty)) => {
                let ty = flow_common_ty::ty_utils::simplify_type(true, None, ty.dupe());
                let ty_str = flow_common_ty::ty_printer::string_of_t_single_line(
                    &ty,
                    &flow_common_ty::ty_printer::PrinterOptions::default(),
                );
                let ty_str = if ty_str.len() > 100 {
                    format!("{}...", flow_common::string_utils::truncate(100, &ty_str))
                } else {
                    ty_str
                };
                let desc = flow_common::string_utils::code_span(&ty_str);
                friendly::hardcoded_string_desc_ref(&desc, loc_of_aloc(loc))
            }
            ErrorTypeDesc::ReasonDesc(desc) => {
                use flow_common::reason::mk_reason;
                let reason = mk_reason(desc.clone(), loc.dupe());
                friendly::ref_map(&loc_of_aloc, &reason)
            }
            ErrorTypeDesc::Unknown => friendly::hardcoded_string_desc_ref("`?`", loc_of_aloc(loc)),
        }
    };

    let ref_of_type = |type_: &MessageTypeReferenceData<L>| match &type_.desc {
        ErrorTypeDesc::Unknown | ErrorTypeDesc::ReasonDesc(_) => {
            friendly::hardcoded_string_desc_ref("this type", loc_of_aloc(&type_.loc))
        }
        desc => ref_of_type_desc(&type_.loc, desc),
    };

    let desc_of_type_desc = |type_desc: &ErrorTypeDesc<L>| match type_desc {
        ErrorTypeDesc::Normalized(Elt::Decl(decl)) => friendly::MessageFeature::Inline(
            friendly::message_inlines_of_string(&decl_description(decl)),
        ),
        ErrorTypeDesc::Normalized(Elt::Type(ty)) => {
            let ty = flow_common_ty::ty_utils::simplify_type(true, None, ty.dupe());
            let ty_str = flow_common_ty::ty_printer::string_of_t_single_line(
                &ty,
                &flow_common_ty::ty_printer::PrinterOptions::default(),
            );
            code(&ty_str)
        }
        ErrorTypeDesc::ReasonDesc(desc) => {
            friendly::desc_of_reason_desc(Some(&desc.map_locs(&loc_of_aloc)))
        }
        ErrorTypeDesc::Unknown => code("?"),
    };

    let desc_of_value_desc = |value: &ValueDesc<L>| match value {
        ValueDesc::Reason(desc) => friendly::desc_of_reason_desc(desc.as_ref()),
        ValueDesc::Type(desc) => desc_of_type_desc(desc),
    };

    let desc_of_callee_desc = |fn_: &CalleeDesc<L>| match fn_ {
        CalleeDesc::Reason(desc) => friendly::desc_of_reason_desc(desc.as_ref()),
        CalleeDesc::Method(name) => friendly::MessageFeature::Inline(
            friendly::message_inlines_of_string(&format!("method `{name}`")),
        ),
    };

    let desc_of_value_as_type_reference =
        |reference: Option<&ValueAsTypeReference>, value: &MessageTypeReferenceData<L>| {
            match reference {
                Some(ValueAsTypeReference::Name(name)) => code(name.as_str()),
                Some(ValueAsTypeReference::ImportType(module)) => {
                    code(&format!("import(\"{}\")", module.as_str()))
                }
                None => desc_of_type_desc(&value.desc),
            }
        };

    let explanation_to_friendly_msgs = |explanation: &Explanation<L>| -> friendly::Message<Loc> {
        use super::intermediate_error_types::Explanation::*;

        match explanation {
            ExplanationAbstractEnumCasting => friendly::Message(vec![
                text("You can explicitly cast your enum value to its representation type using "),
                code("<expr>.valueOf()"),
            ]),
            ExplanationArrayInvariantTyping => friendly::Message(vec![
                text("Arrays are invariantly typed. See "),
                text(
                    "https://flow.org/en/docs/faq/#why-cant-i-pass-an-arraystring-to-a-function-that-takes-an-arraystring-number",
                ),
            ]),
            ExplanationFunctionsWithStaticsToObject => friendly::Message(vec![text(
                "Functions without statics are not compatible with objects",
            )]),
            ExplanationMultiplatform => friendly::Message(vec![
                text("Read the docs on Flow's multi-platform support for more information: "),
                text("https://flow.org/en/docs/react/multiplatform"),
            ]),
            ExplanationPropertyInvariantTyping => friendly::Message(vec![
                text("This property is invariantly typed. See "),
                text(
                    "https://flow.org/en/docs/faq/#why-cant-i-pass-a-string-to-a-function-that-takes-a-string-number",
                ),
            ]),
            ExplanationReactComponentPropsDeepReadOnly(props_loc) => friendly::Message(vec![
                text("React "),
                friendly::hardcoded_string_desc_ref("component properties", loc_of_aloc(props_loc)),
                text(" and their nested props and elements cannot be written to. "),
                text("(https://react.dev/reference/rules/components-and-hooks-must-be-pure#props)"),
            ]),
            ExplanationReactHookArgsDeepReadOnly(props_loc) => friendly::Message(vec![
                text("React "),
                friendly::hardcoded_string_desc_ref("hook arguments", loc_of_aloc(props_loc)),
                text(" and their nested elements cannot be written to. "),
                text(
                    "(https://react.dev/reference/rules/components-and-hooks-must-be-pure#return-values-and-arguments-to-hooks-are-immutable)",
                ),
            ]),
            ExplanationReactHookIncompatibleWithEachOther => friendly::Message(vec![text(
                "Different React hooks are not compatible with each other, because hooks cannot be called conditionally",
            )]),
            ExplanationReactHookIncompatibleWithNormalFunctions => friendly::Message(vec![text(
                "React hooks and other functions are not compatible with each other, because hooks cannot be called conditionally",
            )]),
            ExplanationReactHookReturnDeepReadOnly(hook_loc) => friendly::Message(vec![
                text("The return value of a "),
                friendly::hardcoded_string_desc_ref("React hook", loc_of_aloc(hook_loc)),
                text(" cannot be written to. "),
                text(
                    "(https://react.dev/reference/rules/components-and-hooks-must-be-pure#return-values-and-arguments-to-hooks-are-immutable)",
                ),
            ]),
            ExplanationTypeGuardPositiveConsistency {
                return_,
                param,
                guard_type,
                is_return_false_statement,
            } => {
                let mut features = vec![
                    text("The type of "),
                    ref_(param),
                    text(" at the return expression "),
                    ref_(return_),
                    text(" needs to be compatible with the guard type "),
                    ref_of_type_desc(&guard_type.loc, &guard_type.desc),
                    text("."),
                    text(" See 1. in "),
                    text(
                        "https://flow.org/en/docs/types/type-guards/#toc-consistency-checks-of-type-guard-functions",
                    ),
                ];
                if *is_return_false_statement {
                    features.push(text(". Consider replacing the body of this predicate "));
                    features.push(text("function with a single conditional expression"));
                }
                friendly::Message(features)
            }
            ExplanationAssertionFunctionConsistency { param, guard_type } => {
                friendly::Message(vec![
                    text("The type of "),
                    ref_(param),
                    text(" needs to be compatible with the asserted type "),
                    ref_of_type_desc(&guard_type.loc, &guard_type.desc),
                    text(" wherever the function returns normally. Throw an error or call "),
                    text("another assertion function on every path where it is not"),
                ])
            }
            ExplanationConstrainedAssign(data) => {
                let ExplanationConstrainedAssignData {
                    name,
                    declaration,
                    providers,
                } = data.as_ref();

                let assignments = if providers.is_empty() {
                    vec![text("one of its initial assignments")]
                } else if providers.len() == 1 && providers[0] == *declaration {
                    vec![
                        text("its "),
                        friendly::hardcoded_string_desc_ref(
                            "initializer",
                            loc_of_aloc(declaration),
                        ),
                    ]
                } else if providers.len() == 1 {
                    vec![
                        text("its "),
                        friendly::hardcoded_string_desc_ref(
                            "initial assignment",
                            loc_of_aloc(&providers[0]),
                        ),
                    ]
                } else {
                    let mut features = vec![text("one of its initial assignments")];
                    for (i, provider) in providers.iter().enumerate() {
                        if i > 0 {
                            features.push(text(","));
                        }
                        features.push(friendly::no_desc_ref(&loc_of_aloc(provider)));
                    }
                    features
                };

                let mut features = vec![
                    text("All writes to "),
                    code(name),
                    text(" must be compatible with the type of "),
                ];
                features.extend(assignments);
                features.push(text(". Add an annotation to "));
                features.push(friendly::hardcoded_string_desc_ref(
                    &format!("`{name}`"),
                    loc_of_aloc(declaration),
                ));
                features.push(text(" if a different type is desired"));
                friendly::Message(features)
            }
            ExplanationConcreteEnumCasting {
                representation_type,
            } => {
                let example = format!("<expr> as {}", representation_type);
                friendly::Message(vec![
                    text("You can explicitly cast your enum value to a "),
                    text(representation_type),
                    text(" using "),
                    code(&example),
                ])
            }
            ExplanationCustomError(data) => {
                let ExplanationCustomErrorData {
                    name,
                    custom_error_loc,
                } = data.as_ref();
                let loc = loc_of_aloc(custom_error_loc);
                let custom_docs = loc
                    .source
                    .as_ref()
                    .and_then(&get_ast)
                    .and_then(|program| {
                        let all_comments = &program.all_comments;
                        all_comments.iter().find_map(|comment| {
                            if loc == comment.loc
                                && comment.kind == flow_parser::ast::CommentKind::Block
                            {
                                jsdoc::parse(&comment.text)
                            } else {
                                None
                            }
                        })
                    })
                    .and_then(|jsdoc_val| {
                        let description = jsdoc_val
                            .description()
                            .as_deref()
                            .map(|s| s.trim().to_string());
                        let example = jsdoc_val
                            .unrecognized_tags()
                            .0
                            .iter()
                            .find(|(tag_name, _)| tag_name == "example")
                            .and_then(|(_, desc)| desc.as_deref())
                            .map(|s| s.trim().to_string());
                        match (description, example) {
                            (None, None) => None,
                            (Some(d), None) => Some(friendly::Message(vec![
                                text("Description of "),
                                code(name),
                                text(": "),
                                text(&d),
                            ])),
                            (None, Some(e)) => Some(friendly::Message(vec![
                                text("Example for "),
                                code(name),
                                text(": "),
                                code(&e),
                            ])),
                            (Some(d), Some(e)) => Some(friendly::Message(vec![
                                text("Description of "),
                                code(name),
                                text(": "),
                                text(&d),
                                text(". Example for "),
                                code(name),
                                text(": "),
                                code(&e),
                            ])),
                        }
                    });
                match custom_docs {
                    Some(m) => m,
                    None => friendly::Message(vec![
                        text("See "),
                        friendly::hardcoded_string_desc_ref(
                            "relevant docs",
                            loc_of_aloc(custom_error_loc),
                        ),
                    ]),
                }
            }
            ExplanationAdditionalUnionMembers(data) => {
                use super::error_message::expect_type_desc;
                let ExplanationAdditionalUnionMembersData {
                    left_loc,
                    left_desc,
                    right_loc,
                    right_desc,
                    members,
                    extra_number,
                } = data.as_ref();
                let mut features = vec![
                    text("Type "),
                    ref_of_type_desc(left_loc, &expect_type_desc(left_desc.clone())),
                    text(" includes members "),
                ];
                for (i, member) in members.iter().enumerate() {
                    if i > 0 {
                        features.push(text(", "));
                    }
                    features.push(code(member));
                }
                if *extra_number > 0 {
                    features.push(text(&format!(" and {} more", extra_number)));
                }
                features.push(text(" that are not included in type "));
                features.push(ref_of_type_desc(
                    right_loc,
                    &expect_type_desc(right_desc.clone()),
                ));
                friendly::Message(features)
            }
            ExplanationObjectLiteralNeedsRecordSyntax {
                record_name,
                obj_loc,
            } => friendly::Message(vec![
                text("Fix by adding the record name "),
                code(record_name),
                text(" before the "),
                friendly::hardcoded_string_desc_ref("object literal", loc_of_aloc(obj_loc)),
                text(", e.g. "),
                code(&format!("{} {{...}}", record_name)),
            ]),
            ExplanationStringCasingMustBeCanonical { kind_name } => friendly::Message(vec![
                text("The string literal must already be in "),
                code(kind_name),
                text(" canonical form to be a member of this type"),
            ]),
            ExplanationPropertyMissingDueToNeutralOptionalProperty(data) => {
                let ExplanationPropertyMissingDueToNeutralOptionalPropertyData {
                    props_plural,
                    lower_obj_loc,
                    upper_obj_loc,
                    lower_obj_desc,
                    upper_obj_desc,
                    upper_object_loc,
                } = data.as_ref();
                use flow_common::reason::VirtualReasonDesc::*;

                let prefix = if *props_plural {
                    vec![
                        text("These optional properties of "),
                        ref_of_type_desc(upper_object_loc, upper_obj_desc),
                        text(" are"),
                    ]
                } else {
                    vec![
                        text("This optional property of "),
                        ref_of_type_desc(upper_object_loc, upper_obj_desc),
                        text(" is"),
                    ]
                };

                let fix_suggestion: Vec<friendly::MessageFeature<Loc>> =
                    match (lower_obj_desc, upper_obj_desc) {
                        // lower is literal, upper is type
                        (
                            ErrorTypeDesc::ReasonDesc(lower_desc),
                            ErrorTypeDesc::Normalized(Elt::Type(_)),
                        ) if matches!(lower_desc, RObjectLit | RArrayLit) => {
                            vec![
                                text("annotate "),
                                friendly::ref_map(
                                    &loc_of_aloc,
                                    &mk_reason(lower_desc.clone(), lower_obj_loc.dupe()),
                                ),
                                text(" with "),
                                ref_of_type_desc(upper_obj_loc, upper_obj_desc),
                            ]
                        }
                        // upper is literal, lower is type
                        (
                            ErrorTypeDesc::Normalized(Elt::Type(_)),
                            ErrorTypeDesc::ReasonDesc(upper_desc),
                        ) if matches!(upper_desc, RObjectLit | RArrayLit) => {
                            vec![
                                text("annotate "),
                                friendly::ref_map(
                                    &loc_of_aloc,
                                    &mk_reason(upper_desc.clone(), upper_obj_loc.dupe()),
                                ),
                                text(" with "),
                                ref_of_type_desc(lower_obj_loc, lower_obj_desc),
                            ]
                        }
                        _ => vec![
                            text("make "),
                            ref_of_type_desc(lower_obj_loc, lower_obj_desc),
                            text(" and "),
                            ref_of_type_desc(upper_obj_loc, upper_obj_desc),
                            text(" exactly the same"),
                        ],
                    };

                let mut features = prefix;
                features.push(text(" invariantly typed. To fix,\n- Either "));
                features.extend(fix_suggestion);
                features.push(text("\n- Or make "));
                features.push(ref_of_type_desc(upper_object_loc, upper_obj_desc));
                features.push(text(" readonly. See "));
                features.push(text(
                    "https://flow.org/en/docs/faq/#why-cant-i-pass-a-string-to-a-function-that-takes-a-string-number",
                ));
                friendly::Message(features)
            }
            ExplanationInvariantSubtypingDueToMutableArray(data) => {
                let ExplanationInvariantSubtypingDueToMutableArrayData {
                    lower_array_loc,
                    upper_array_loc,
                    lower_array_desc,
                    upper_array_desc,
                    upper_array_reference_loc,
                } = data.as_ref();
                use flow_common::reason::VirtualReasonDesc::*;

                let fix_suggestion: Vec<friendly::MessageFeature<Loc>> =
                    match (lower_array_desc, upper_array_desc) {
                        (
                            ErrorTypeDesc::ReasonDesc(lower_desc),
                            ErrorTypeDesc::Normalized(Elt::Type(_)),
                        ) if matches!(lower_desc, RObjectLit | RArrayLit) => {
                            vec![
                                text("annotate "),
                                friendly::ref_map(
                                    &loc_of_aloc,
                                    &mk_reason(lower_desc.clone(), lower_array_loc.dupe()),
                                ),
                                text(" with "),
                                ref_of_type_desc(upper_array_loc, upper_array_desc),
                            ]
                        }
                        (
                            ErrorTypeDesc::Normalized(Elt::Type(_)),
                            ErrorTypeDesc::ReasonDesc(upper_desc),
                        ) if matches!(upper_desc, RObjectLit | RArrayLit) => {
                            vec![
                                text("annotate "),
                                friendly::ref_map(
                                    &loc_of_aloc,
                                    &mk_reason(upper_desc.clone(), upper_array_loc.dupe()),
                                ),
                                text(" with "),
                                ref_of_type_desc(lower_array_loc, lower_array_desc),
                            ]
                        }
                        _ => vec![
                            text("make "),
                            ref_of_type_desc(lower_array_loc, lower_array_desc),
                            text(" and "),
                            ref_of_type_desc(upper_array_loc, upper_array_desc),
                            text(" exactly the same"),
                        ],
                    };

                let mut features = vec![
                    text(
                        "The above-mentioned two types must be the same because arrays are invariantly typed. ",
                    ),
                    text("To fix the error,\n- Either "),
                ];
                features.extend(fix_suggestion);
                features.push(text("\n- Or make "));
                features.push(ref_of_type_desc(
                    upper_array_reference_loc,
                    upper_array_desc,
                ));
                features.push(text(" a "));
                features.push(code("ReadonlyArray"));
                features.push(text(".\nSee "));
                features.push(text(
                    "https://flow.org/en/docs/faq/#why-cant-i-pass-an-arraystring-to-a-function-that-takes-an-arraystring-number",
                ));
                friendly::Message(features)
            }
            ExplanationInvariantSubtypingDueToMutableProperty(data) => {
                let ExplanationInvariantSubtypingDueToMutablePropertyData {
                    lower_obj_loc,
                    upper_obj_loc,
                    lower_obj_desc,
                    upper_obj_desc,
                    upper_object_loc,
                    property_name,
                } = data.as_ref();
                use flow_common::reason::VirtualReasonDesc::*;

                let prop: Vec<friendly::MessageFeature<Loc>> = match property_name {
                    None => vec![text("the indexer")],
                    Some(name) => vec![text("property "), code(name)],
                };

                let fix_suggestion: Vec<friendly::MessageFeature<Loc>> =
                    match (lower_obj_desc, upper_obj_desc) {
                        (
                            ErrorTypeDesc::ReasonDesc(lower_desc),
                            ErrorTypeDesc::Normalized(Elt::Type(_)),
                        ) if matches!(lower_desc, RObjectLit | RArrayLit) => {
                            vec![
                                text("annotate "),
                                friendly::ref_map(
                                    &loc_of_aloc,
                                    &mk_reason(lower_desc.clone(), lower_obj_loc.dupe()),
                                ),
                                text(" with "),
                                ref_of_type_desc(upper_obj_loc, upper_obj_desc),
                            ]
                        }
                        (
                            ErrorTypeDesc::Normalized(Elt::Type(_)),
                            ErrorTypeDesc::ReasonDesc(upper_desc),
                        ) if matches!(upper_desc, RObjectLit | RArrayLit) => {
                            vec![
                                text("annotate "),
                                friendly::ref_map(
                                    &loc_of_aloc,
                                    &mk_reason(upper_desc.clone(), upper_obj_loc.dupe()),
                                ),
                                text(" with "),
                                ref_of_type_desc(lower_obj_loc, lower_obj_desc),
                            ]
                        }
                        _ => vec![
                            text("make "),
                            ref_of_type_desc(lower_obj_loc, lower_obj_desc),
                            text(" and "),
                            ref_of_type_desc(upper_obj_loc, upper_obj_desc),
                            text(" exactly the same"),
                        ],
                    };

                let mut features = vec![text(
                    "The above-mentioned two types must be the same because ",
                )];
                features.extend(prop.clone());
                features.push(text(" is invariantly typed. To fix the error,\n- Either "));
                features.extend(fix_suggestion);
                features.push(text("\n- Or make "));
                features.extend(prop);
                features.push(text(" in "));
                features.push(ref_of_type_desc(upper_object_loc, upper_obj_desc));
                features.push(text(" readonly. See "));
                features.push(text(
                    "https://flow.org/en/docs/faq/#why-cant-i-pass-a-string-to-a-function-that-takes-a-string-number",
                ));
                friendly::Message(features)
            }
            ExplanationInvariantSubtypingDueToMutableProperties(data) => {
                let ExplanationInvariantSubtypingDueToMutablePropertiesData {
                    lower_obj_loc,
                    upper_obj_loc,
                    lower_obj_desc,
                    upper_obj_desc,
                    upper_object_loc,
                    properties,
                } = data.as_ref();
                use flow_common::reason::VirtualReasonDesc::*;

                let prop_codes: Vec<friendly::Message<Loc>> = properties
                    .iter()
                    .map(|prop| friendly::Message(vec![code(&prop.display_smol_str())]))
                    .collect();
                let mut props_msg: Vec<friendly::MessageFeature<Loc>> = vec![text("properties ")];
                props_msg.extend(friendly::conjunction_concat(prop_codes, "and", None).0);

                let fix_suggestion: Vec<friendly::MessageFeature<Loc>> =
                    match (lower_obj_desc, upper_obj_desc) {
                        (
                            ErrorTypeDesc::ReasonDesc(lower_desc),
                            ErrorTypeDesc::Normalized(Elt::Type(_)),
                        ) if matches!(lower_desc, RObjectLit | RArrayLit) => {
                            vec![
                                text("annotate "),
                                friendly::ref_map(
                                    &loc_of_aloc,
                                    &mk_reason(lower_desc.clone(), lower_obj_loc.dupe()),
                                ),
                                text(" with "),
                                ref_of_type_desc(upper_obj_loc, upper_obj_desc),
                            ]
                        }
                        (
                            ErrorTypeDesc::Normalized(Elt::Type(_)),
                            ErrorTypeDesc::ReasonDesc(upper_desc),
                        ) if matches!(upper_desc, RObjectLit | RArrayLit) => {
                            vec![
                                text("annotate "),
                                friendly::ref_map(
                                    &loc_of_aloc,
                                    &mk_reason(upper_desc.clone(), upper_obj_loc.dupe()),
                                ),
                                text(" with "),
                                ref_of_type_desc(lower_obj_loc, lower_obj_desc),
                            ]
                        }
                        _ => vec![
                            text("make "),
                            ref_of_type_desc(lower_obj_loc, lower_obj_desc),
                            text(" and "),
                            ref_of_type_desc(upper_obj_loc, upper_obj_desc),
                            text(" exactly the same"),
                        ],
                    };

                let mut features = vec![text(
                    "The above-mentioned two types must be the same because ",
                )];
                features.extend(props_msg.clone());
                features.push(text(" are invariantly typed. To fix the error,\n- Either "));
                features.extend(fix_suggestion);
                features.push(text("\n- Or make "));
                features.extend(props_msg);
                features.push(text(" in "));
                features.push(ref_of_type_desc(upper_object_loc, upper_obj_desc));
                features.push(text(" readonly. See "));
                features.push(text(
                    "https://flow.org/en/docs/faq/#why-cant-i-pass-a-string-to-a-function-that-takes-a-string-number",
                ));
                friendly::Message(features)
            }
        }
    };

    let frame_to_friendly_msgs = |include_incompatibility_pair: bool,
                                  frame: &ErrorFrame<L>|
     -> (Option<friendly::Message<Loc>>, friendly::Message<Loc>) {
        use super::intermediate_error_types::Frame::*;

        // Helper to convert incompatibility_pair to message if include_incompatibility_pair is true
        let map_typed_incompatibility = |pair: &Option<(
            MessageTypeReferenceData<L>,
            MessageTypeReferenceData<L>,
        )>|
         -> Option<friendly::Message<Loc>> {
            if include_incompatibility_pair {
                pair.as_ref().map(|(l, u)| {
                    friendly::Message(vec![
                        ref_of_type_desc(&l.loc, &l.desc),
                        text(" is incompatible with "),
                        ref_of_type_desc(&u.loc, &u.desc),
                    ])
                })
            } else {
                None
            }
        };
        // The upper of a pair is always named, so it is referenced by its name.
        let map_lower_typed_incompatibility = |pair: &Option<(
            MessageTypeReferenceData<L>,
            VirtualReason<L>,
        )>|
         -> Option<friendly::Message<Loc>> {
            if include_incompatibility_pair {
                pair.as_ref().map(|(l, u)| {
                    friendly::Message(vec![
                        ref_of_type_desc(&l.loc, &l.desc),
                        text(" is incompatible with "),
                        ref_(u),
                    ])
                })
            } else {
                None
            }
        };

        match frame {
            FrameAnonymous => (None, friendly::Message(vec![])),
            FrameAccessChain {
                chain,
                incompatibility_pair,
            } => {
                let first = chain.first();
                let kind = match first {
                    AccessChainSegment::PropSegment(_) => "property ",
                    AccessChainSegment::TupleIndexSegment(_) => "index ",
                };
                let chain_str: String =
                    chain
                        .iter()
                        .rev()
                        .enumerate()
                        .fold(String::new(), |acc, (i, segment)| match segment {
                            AccessChainSegment::PropSegment(prop) => {
                                if i == 0 {
                                    prop.display_smol_str().to_string()
                                } else {
                                    format!("{}.{}", acc, prop)
                                }
                            }
                            AccessChainSegment::TupleIndexSegment(n) => {
                                if i == 0 {
                                    n.to_string()
                                } else {
                                    format!("{}[{}]", acc, n)
                                }
                            }
                        });
                (
                    map_lower_typed_incompatibility(incompatibility_pair),
                    friendly::Message(vec![text(kind), code(&chain_str)]),
                )
            }
            FrameArrayElement {
                incompatibility_pair,
            } => (
                map_typed_incompatibility(incompatibility_pair),
                friendly::Message(vec![text("array element")]),
            ),
            FrameCallableSignature {
                incompatibility_pair,
            } => (
                map_lower_typed_incompatibility(incompatibility_pair),
                friendly::Message(vec![text("the callable signature")]),
            ),
            FrameEnumRepresentationType => (
                None,
                friendly::Message(vec![text("the enum's representation type")]),
            ),
            FrameFunNthArgument {
                n,
                incompatibility_pair,
            } => (
                map_typed_incompatibility(incompatibility_pair),
                friendly::Message(vec![text("the "), text(&ordinal(*n)), text(" argument")]),
            ),
            FrameFunThisArgument {
                incompatibility_pair,
            } => (
                map_typed_incompatibility(incompatibility_pair),
                friendly::Message(vec![text("the "), code("this"), text(" argument")]),
            ),
            FrameFunNthParam {
                n,
                incompatibility_pair,
            } => (
                map_typed_incompatibility(incompatibility_pair),
                friendly::Message(vec![text("the "), text(&ordinal(*n)), text(" parameter")]),
            ),
            FrameFunThisParam {
                incompatibility_pair,
            } => (
                map_typed_incompatibility(incompatibility_pair),
                friendly::Message(vec![text("the "), code("this"), text(" parameter")]),
            ),
            FrameIndexerProperty {
                incompatibility_pair,
            } => (
                map_lower_typed_incompatibility(incompatibility_pair),
                friendly::Message(vec![text("the indexer property")]),
            ),
            FrameIndexerPropertyKey {
                incompatibility_pair,
            } => (
                map_lower_typed_incompatibility(incompatibility_pair),
                friendly::Message(vec![text("the indexer property's key")]),
            ),
            FrameTypeArgument(targ) => (
                None,
                friendly::Message(vec![text("type argument "), ref_(targ)]),
            ),
            FrameTypeParameterBound(name) => (
                None,
                friendly::Message(vec![text("type argument "), code(name)]),
            ),
            FrameTypePredicate => (None, friendly::Message(vec![text("the type predicate")])),
            FrameReturnValue {
                incompatibility_pair,
            } => (
                map_typed_incompatibility(incompatibility_pair),
                friendly::Message(vec![text("the return value")]),
            ),
            FrameUnionRepresentative(union) => (
                None,
                friendly::Message(vec![
                    text("at least one member of "),
                    ref_of_type_desc(&union.loc, &union.desc),
                ]),
            ),
        }
    };

    let hardcoded_string_desc_ref = |s: &str, loc: &L| -> friendly::MessageFeature<Loc> {
        friendly::hardcoded_string_desc_ref(s, loc_of_aloc(loc))
    };

    let type_guard_description = |kind: &TypeGuardReferenceKind| match kind {
        TypeGuardReferenceKind::TypeGuard => "type guard".to_string(),
        TypeGuardReferenceKind::Parameter(Some(name)) => format!("`{name}`"),
        TypeGuardReferenceKind::Parameter(None) => "parameter".to_string(),
        TypeGuardReferenceKind::TypeGuardParameter(name) => {
            format!("type guard parameter `{name}`")
        }
        TypeGuardReferenceKind::This => "this".to_string(),
    };

    let type_guard_reference = |reference: &TypeGuardReferenceData<L>| {
        hardcoded_string_desc_ref(&type_guard_description(&reference.kind), &reference.loc)
    };

    let type_guard_parameter_reference = |parameter: &TypeGuardParameterData<L>| {
        hardcoded_string_desc_ref(
            &format!("type guard parameter `{}`", parameter.name),
            &parameter.loc,
        )
    };

    let render_named_reference = |reference: &NamedReferenceData<L>| {
        hardcoded_string_desc_ref(&format!("`{}`", reference.name), &reference.loc)
    };

    let null_literal_reference = |loc: &L| {
        let null = vec![friendly::MessageInline::Code("null".to_string())];
        let loc = loc_of_aloc(loc);
        if loc == LOC_NONE {
            friendly::MessageFeature::Inline(null)
        } else {
            friendly::MessageFeature::Reference(null, loc)
        }
    };

    let render_enum_reference = |reference: &EnumReferenceData<L>| match &reference.name {
        Some(name) => hardcoded_string_desc_ref(&format!("enum `{}`", name), &reference.loc),
        None => hardcoded_string_desc_ref("enum", &reference.loc),
    };

    let render_enum_value_reference = |reference: &EnumReferenceData<L>| match &reference.name {
        Some(name) => hardcoded_string_desc_ref(&format!("`{}`", name), &reference.loc),
        None => hardcoded_string_desc_ref("enum", &reference.loc),
    };

    fn definition_description(kind: &DefinitionReferenceKind) -> String {
        match kind {
            DefinitionReferenceKind::Name(name) => format!("`{name}`"),
            DefinitionReferenceKind::Function { async_, generator } => {
                let prefix = match (async_, generator) {
                    (true, true) => "async generator ",
                    (true, false) => "async ",
                    (false, true) => "generator ",
                    (false, false) => "",
                };
                format!("{prefix}function")
            }
            DefinitionReferenceKind::Component(name) => format!("component {name}"),
            DefinitionReferenceKind::Enum(name) => format!("enum `{name}`"),
            DefinitionReferenceKind::Interface => "interface type".to_string(),
            DefinitionReferenceKind::This => "this".to_string(),
            DefinitionReferenceKind::Destructuring => "destructuring".to_string(),
            DefinitionReferenceKind::Match => "match".to_string(),
            DefinitionReferenceKind::Other => "definition".to_string(),
        }
    }

    let render_expression_reference = |reference: &ExpressionReferenceData<L>| {
        let description = match &reference.kind {
            ExpressionReferenceKind::Code(code) => flow_common::string_utils::code_span(code),
            ExpressionReferenceKind::ObjectLiteral => "object literal".to_string(),
            ExpressionReferenceKind::ArrayLiteral => "array literal".to_string(),
            ExpressionReferenceKind::Function(kind) => match kind {
                ExpressionFunctionKind::Normal => "function".to_string(),
                ExpressionFunctionKind::Async => "async function".to_string(),
                ExpressionFunctionKind::Generator => "generator function".to_string(),
                ExpressionFunctionKind::AsyncGenerator => "async generator function".to_string(),
                ExpressionFunctionKind::Unknown => "unknown function".to_string(),
            },
            ExpressionReferenceKind::EmptyString => "empty string".to_string(),
            ExpressionReferenceKind::TemplateString => "template string".to_string(),
            ExpressionReferenceKind::Expression => "expression".to_string(),
        };
        hardcoded_string_desc_ref(&description, &reference.loc)
    };

    let render_function_reference = |reference: &FunctionReferenceData<L>| {
        let description = match reference.kind {
            FunctionReferenceKind::Function => "function",
            FunctionReferenceKind::FunctionType => "function type",
            FunctionReferenceKind::DefaultConstructor => "default constructor",
        };
        hardcoded_string_desc_ref(description, &reference.loc)
    };

    let render_implicit_instantiation_reference =
        |reference: &ImplicitInstantiationReferenceData<L>| {
            let description = match &reference.kind {
                ImplicitInstantiationReferenceKind::Call(Some(name)) => {
                    format!("call of {}", flow_common::string_utils::code_span(name))
                }
                ImplicitInstantiationReferenceKind::Call(None) => "function call".to_string(),
                ImplicitInstantiationReferenceKind::Method(Some(name)) => {
                    format!("call of method `{name}`")
                }
                ImplicitInstantiationReferenceKind::Method(None) => {
                    "call of computed property".to_string()
                }
                ImplicitInstantiationReferenceKind::Constructor(Some(name)) => {
                    format!("new {}", flow_common::string_utils::code_span(name))
                }
                ImplicitInstantiationReferenceKind::Constructor(None) => {
                    "constructor call".to_string()
                }
                ImplicitInstantiationReferenceKind::ReactElement(Some(name)) => {
                    format!("`{name}` element")
                }
                ImplicitInstantiationReferenceKind::ReactElement(None) => {
                    "React element".to_string()
                }
                ImplicitInstantiationReferenceKind::JSXFunction(name) => {
                    format!("`{name}(...)`")
                }
                ImplicitInstantiationReferenceKind::SuperCall => "call of super".to_string(),
            };
            hardcoded_string_desc_ref(&description, &reference.loc)
        };

    fn ordinal(n: i32) -> String {
        match n {
            1 => "first".to_string(),
            2 => "second".to_string(),
            3 => "third".to_string(),
            4 => "fourth".to_string(),
            5 => "fifth".to_string(),
            6 => "sixth".to_string(),
            7 => "seventh".to_string(),
            8 => "eighth".to_string(),
            9 => "ninth".to_string(),
            n => {
                let s = n.to_string();
                let bytes = s.as_bytes();
                let len = bytes.len();
                let tens = if len >= 2 {
                    (bytes[len - 2] - b'0') as i32
                } else {
                    0
                };
                let ones = (bytes[len - 1] - b'0') as i32;
                let suffix = if tens == 1 {
                    "th"
                } else {
                    match ones {
                        1 => "st",
                        2 => "nd",
                        3 => "rd",
                        _ => "th",
                    }
                };
                format!("{}{}", n, suffix)
            }
        }
    }

    let root_msg_to_root_kind_and_friendly_msgs =
        |root_msg: &RootMessage<L>| -> (RootKind, friendly::Message<Loc>) {
            match root_msg {
                RootMessage::RootCannotAccessIndex { index, object_ } => (
                    RootKind::OperationRoot,
                    friendly::Message(vec![
                        text("Cannot access "),
                        desc_of_type_desc(index),
                        text(" on "),
                        desc_of_type_desc(object_),
                    ]),
                ),
                RootMessage::RootCannotAddComputedProperty => (
                    RootKind::OperationRoot,
                    friendly::Message(vec![text("Cannot add computed property")]),
                ),
                RootMessage::RootCannotAssign { init, target } => match target {
                    None => (
                        RootKind::OperationRoot,
                        friendly::Message(vec![
                            text("Cannot assign "),
                            desc_of_value_desc(init),
                            text(" to variable"),
                        ]),
                    ),
                    Some(target) => (
                        RootKind::OperationRoot,
                        friendly::Message(vec![
                            text("Cannot assign "),
                            desc_of_value_desc(init),
                            text(" to "),
                            match target {
                                AssignTargetDesc::Variable(desc) => {
                                    friendly::desc_of_reason_desc(desc.as_ref())
                                }
                                AssignTargetDesc::Annotation(desc) => desc_of_type_desc(desc),
                            },
                        ]),
                    ),
                },
                RootMessage::RootCannotCall(fn_) => (
                    RootKind::OperationRoot,
                    friendly::Message(vec![text("Cannot call "), desc_of_callee_desc(fn_)]),
                ),
                RootMessage::RootCannotCallStandalone(fn_) => (
                    RootKind::OperationRoot,
                    friendly::Message(vec![
                        text("Cannot call "),
                        friendly::desc_of_reason_desc(fn_.as_ref()),
                        text(" as a standalone function"),
                    ]),
                ),
                RootMessage::RootCannotCallWithNamedParam { fn_, lower, name } => (
                    RootKind::OperationRoot,
                    friendly::Message(vec![
                        text("Cannot call "),
                        desc_of_callee_desc(fn_),
                        text(" with "),
                        desc_of_value_desc(lower),
                        text(" bound to "),
                        code(name),
                    ]),
                ),
                RootMessage::RootCannotCallWithNthParam { fn_, lower, n } => (
                    RootKind::OperationRoot,
                    friendly::Message(vec![
                        text("Cannot call "),
                        desc_of_callee_desc(fn_),
                        text(" with "),
                        desc_of_value_desc(lower),
                        text(" bound to "),
                        text(&format!("the {} parameter", ordinal(*n))),
                    ]),
                ),
                RootMessage::RootCannotCallObjectAssign(op) => (
                    RootKind::OperationRoot,
                    friendly::Message(vec![
                        text("Incorrect arguments passed to "),
                        friendly::desc_of_reason_desc(op.as_ref()),
                    ]),
                ),
                RootMessage::RootCannotCast { lower, upper } => (
                    RootKind::OperationRoot,
                    friendly::Message(vec![
                        text("Cannot cast "),
                        friendly::desc_of_reason_desc(lower.as_ref()),
                        text(" to "),
                        desc_of_type_desc(upper),
                    ]),
                ),
                RootMessage::RootCannotCheckAgainst { test, discriminant } => (
                    RootKind::OperationRoot,
                    friendly::Message(vec![
                        text("Invalid check of "),
                        friendly::desc_of_reason_desc(test.as_ref()),
                        text(" against "),
                        ref_(discriminant),
                    ]),
                ),
                RootMessage::RootCannotCheckAgainstSwitchDiscriminant(discriminant_loc) => (
                    RootKind::OperationRoot,
                    friendly::Message(vec![
                        text("Invalid check of case test against "),
                        hardcoded_string_desc_ref("switch discriminant", discriminant_loc),
                    ]),
                ),
                RootMessage::RootCannotCoerce { from, target } => (
                    RootKind::OperationRoot,
                    friendly::Message(vec![
                        text("Cannot coerce "),
                        friendly::desc_of_reason_desc(from.as_ref()),
                        text(" to "),
                        friendly::desc_of_reason_desc(target.as_ref()),
                    ]),
                ),
                RootMessage::RootCannotConformToCommonInterface {
                    originate_from_import,
                } => {
                    if *originate_from_import {
                        (
                            RootKind::OperationRoot,
                            friendly::Message(vec![text(
                                "The import resolves to a forked module that has implementations of conflicting types",
                            )]),
                        )
                    } else {
                        (
                            RootKind::OperationRoot,
                            friendly::Message(vec![text(
                                "Cannot conform to common interface module",
                            )]),
                        )
                    }
                }
                RootMessage::RootCannotMergeDeclaration {
                    conflict,
                    first_decl,
                } => (
                    RootKind::OperationRoot,
                    friendly::Message(vec![
                        text("Cannot merge this interface declaration with previous declaration "),
                        ref_of_type(first_decl),
                        text(match conflict {
                            MergedDeclarationConflict::Property => {
                                " because of conflicting property types"
                            }
                            MergedDeclarationConflict::TypeParamDefault => {
                                " because of conflicting type parameter defaults"
                            }
                        }),
                    ]),
                ),
                RootMessage::RootCannotCreateElement(component) => (
                    RootKind::OperationRoot,
                    friendly::Message(vec![
                        text("Cannot create "),
                        friendly::desc_of_reason_desc(component.as_ref()),
                        text(" element"),
                    ]),
                ),
                RootMessage::RootCannotCreateRecord(constructor) => (
                    RootKind::OperationRoot,
                    friendly::Message(vec![
                        text("Cannot create record "),
                        friendly::desc_of_reason_desc(constructor.as_ref()),
                    ]),
                ),
                RootMessage::RootCannotDeclareRef => (
                    RootKind::ShortExplanationRoot,
                    friendly::Message(vec![
                        text("The "),
                        code("ref"),
                        text(" parameter must be a subtype of "),
                        code("React.RefSetter"),
                    ]),
                ),
                RootMessage::RootCannotDeclareTypeGuard {
                    type_guard_loc,
                    fn_,
                } => (
                    RootKind::OperationRoot,
                    friendly::Message(vec![
                        text("Cannot declare a "),
                        hardcoded_string_desc_ref("type guard", type_guard_loc),
                        text(" for "),
                        ref_(fn_),
                    ]),
                ),
                RootMessage::RootCannotDefineClassMethod { method_, name } => (
                    RootKind::OperationRoot,
                    friendly::Message(vec![
                        text("Cannot define "),
                        hardcoded_string_desc_ref(
                            &format!("method `{}`", method_.name),
                            &method_.loc,
                        ),
                        text(" on "),
                        friendly::desc_of_reason_desc(name.as_ref()),
                    ]),
                ),
                RootMessage::RootCannotDefineShadowedProtoProperty => (
                    RootKind::OperationRoot,
                    friendly::Message(vec![text("Cannot define shadowed proto property")]),
                ),
                RootMessage::RootCannotDelete(prop) => (
                    RootKind::OperationRoot,
                    friendly::Message(vec![
                        text("Cannot delete "),
                        friendly::desc_of_reason_desc(prop.as_ref()),
                    ]),
                ),
                RootMessage::RootCannotExpectImplicitReturn { upper, fn_ } => (
                    RootKind::OperationRoot,
                    friendly::Message(vec![
                        text("Cannot expect "),
                        desc_of_type_desc(upper),
                        text(" as the return type of "),
                        friendly::desc_of_reason_desc(fn_.as_ref()),
                    ]),
                ),
                RootMessage::RootCannotExtendClass { extends, def } => (
                    RootKind::OperationRoot,
                    friendly::Message(vec![
                        text("Cannot extend "),
                        ref_of_type(extends),
                        text(" with "),
                        ref_of_type(def),
                    ]),
                ),
                RootMessage::RootCannotGetProp(prop) => (
                    RootKind::OperationRoot,
                    friendly::Message(vec![
                        text("Cannot get "),
                        friendly::desc_of_reason_desc(prop.as_ref()),
                    ]),
                ),
                RootMessage::RootCannotGetRest(op) => (
                    RootKind::OperationRoot,
                    friendly::Message(vec![
                        text("Cannot get rest of "),
                        friendly::desc_of_reason_desc(op.as_ref()),
                    ]),
                ),
                RootMessage::RootCannotImplementClass { implements, def } => (
                    RootKind::OperationRoot,
                    friendly::Message(vec![
                        text("Cannot implement "),
                        ref_(implements),
                        text(" with "),
                        friendly::desc_of_reason_desc(def.as_ref()),
                    ]),
                ),
                RootMessage::RootCannotInitializeField { field, body } => (
                    RootKind::OperationRoot,
                    friendly::Message(vec![
                        text("Cannot initialize "),
                        friendly::desc_of_reason_desc(field.as_ref()),
                        text(" with "),
                        friendly::desc_of_reason_desc(body.as_ref()),
                    ]),
                ),
                RootMessage::RootCannotInstantiateEval(type_) => (
                    RootKind::OperationRoot,
                    friendly::Message(vec![text("Cannot instantiate "), ref_of_type(type_)]),
                ),
                RootMessage::RootCannotInstantiateTypeApp(type_) => (
                    RootKind::OperationRoot,
                    friendly::Message(vec![
                        text("Cannot instantiate "),
                        friendly::desc_of_reason_desc(type_.as_ref()),
                    ]),
                ),
                RootMessage::RootCannotInstantiateTypeof(name) => (
                    RootKind::OperationRoot,
                    friendly::Message(vec![
                        text("Cannot instantiate "),
                        friendly::MessageFeature::Inline(friendly::message_inlines_of_string(
                            &format!("`typeof {name}`"),
                        )),
                    ]),
                ),
                RootMessage::RootCannotMakeReadOnly => (
                    RootKind::OperationRoot,
                    friendly::Message(vec![text("Cannot make this type read-only")]),
                ),
                RootMessage::RootCannotInstantiateRenderType => (
                    RootKind::ShortExplanationRoot,
                    friendly::Message(vec![
                        text("Render types must be a subtype of "),
                        code("React.Node"),
                        text(" or a reference to a component-syntax component"),
                    ]),
                ),
                RootMessage::RootCannotReturn(value) => (
                    RootKind::OperationRoot,
                    friendly::Message(vec![
                        text("Cannot return "),
                        friendly::desc_of_reason_desc(value.as_ref()),
                    ]),
                ),
                RootMessage::RootCannotReturnFromAssertionFunction { implicit_return } => (
                    RootKind::OperationRoot,
                    friendly::Message(vec![text(if *implicit_return {
                        "Cannot return from assertion function at the end of its body"
                    } else {
                        "Cannot return from assertion function"
                    })]),
                ),
                RootMessage::RootCannotShadowProto(proto) => (
                    RootKind::OperationRoot,
                    friendly::Message(vec![
                        text("Cannot shadow proto "),
                        hardcoded_string_desc_ref(
                            &format!("property `{}`", proto.name),
                            &proto.loc,
                        ),
                    ]),
                ),
                RootMessage::RootCannotShadowProtoProperty => (
                    RootKind::OperationRoot,
                    friendly::Message(vec![text("Cannot shadow proto property")]),
                ),
                RootMessage::RootCannotSpread(op) => (
                    RootKind::OperationRoot,
                    friendly::Message(vec![
                        text("Cannot spread "),
                        friendly::desc_of_reason_desc(op.as_ref()),
                    ]),
                ),
                RootMessage::RootCannotUpdate(prop) => (
                    RootKind::OperationRoot,
                    friendly::Message(vec![
                        text("Cannot update "),
                        friendly::desc_of_reason_desc(prop.as_ref()),
                    ]),
                ),
                RootMessage::RootCannotUseInferTypeBound { infer } => (
                    RootKind::ShortExplanationRoot,
                    friendly::Message(vec![
                        text("The infer type "),
                        friendly::desc_of_reason_desc(infer.as_ref()),
                        text(" must have consistent bounds"),
                    ]),
                ),
                RootMessage::RootCannotUseTypeGuard {
                    guard_type,
                    param_name,
                } => (
                    RootKind::ShortExplanationRoot,
                    friendly::Message(vec![
                        text("The type predicate "),
                        ref_of_type_desc(&guard_type.loc, &guard_type.desc),
                        text(" needs to be compatible with parameter `"),
                        text(param_name),
                        text("`'s type"),
                    ]),
                ),
                RootMessage::RootCannotYield(value) => (
                    RootKind::OperationRoot,
                    friendly::Message(vec![
                        text("Cannot yield "),
                        friendly::desc_of_reason_desc(value.as_ref()),
                    ]),
                ),
            }
        };

    let msg_to_friendly_msgs = |message: &Message<L>| -> friendly::Message<Loc> {
        use super::intermediate_error_types::Message::*;

        match message {
            MessagePlainTextReservedForInternalErrorOnly(s) => friendly::Message(vec![text(s)]),
            MessageAlreadyExhaustivelyCheckOneEnumMember(
                box MessageAlreadyExhaustivelyCheckOneEnumMemberData {
                    prev_check_loc,
                    enum_,
                    member_name,
                },
            ) => friendly::Message(vec![
                text("Invalid exhaustive check: "),
                text("case checks for enum member "),
                code(member_name),
                text(" of "),
                render_named_reference(enum_),
                text(", but member "),
                code(member_name),
                text(" was already checked at "),
                friendly::hardcoded_string_desc_ref("case", loc_of_aloc(prev_check_loc)),
                text("."),
            ]),
            MessageAlreadyExhaustivelyCheckAllEnumMembers { enum_ } => {
                friendly::Message(vec![
                    text("Invalid exhaustive check: "),
                    text("default case checks for additional enum members of "),
                    render_named_reference(enum_),
                    text(", but all of its members have already been checked."),
                ])
            }
            MessageAmbiguousNumericKeyWithVariance => friendly::Message(vec![
                text("Cannot mix number literal keys and variance annotations, "),
                text("as the variance annotation could be interpreted as negating or "),
                text("making positive the number literal. "),
                text("Consider using a string literal key name to disambiguate."),
            ]),
            MessageAmbiguousObjectType => friendly::Message(vec![
                text("Please write this object type as explicitly exact (use "),
                code("{|"),
                text(" and "),
                code("|}"),
                text(" instead of "),
                code("{"),
                text(" and "),
                code("}"),
                text(") or as explicitly inexact (add "),
                code("..."),
                text(" to the end of the list of properties)."),
            ]),
            MessageAnyValueUsedAsType { reference, value } => friendly::Message(vec![
                text("Cannot use "),
                desc_of_value_as_type_reference(reference.as_ref(), value),
                text(" as a type because it is an "),
                code("any"),
                text("-typed value. "),
                text("Type "),
                desc_of_value_as_type_reference(reference.as_ref(), value),
                text(" properly, so it is no longer "),
                code("any"),
                text("-typed, to use it as an annotation."),
            ]),
            MessageBadLibdefModuleOverride(x) => friendly::Message(vec![
                text("This module declaration overrides an existing module "),
                render_named_reference(x),
                text(". Overriding in library definitions can lead to surprising behaviors."),
            ]),
            MessageBadLibdefNameOverride(x) => friendly::Message(vec![
                text("This name declaration overrides an existing binding "),
                render_named_reference(x),
                text(". Overriding in library definitions can lead to surprising behaviors."),
            ]),
            MessageCannotAccessEnumMember(box MessageCannotAccessEnumMemberData {
                member_name,
                suggestion,
                enum_,
            }) => {
                let name = member_name.display_smol_str();
                let mut features = vec![
                    text("Cannot access "),
                    code(&name),
                    text(" because "),
                    code(&name),
                    text(" is not a member of "),
                    render_enum_reference(enum_),
                    text("."),
                ];
                if let Some(sugg) = suggestion {
                    features.extend(vec![
                        text(" Did you mean the member "),
                        code(sugg),
                        text("?"),
                    ]);
                }
                friendly::Message(features)
            }
            MessageCannotAccessEnumWithComputedProp { description, enum_ } => {
                let mut features = vec![text("Cannot access ")];
                match description {
                    ErrorTypeDesc::Normalized(Elt::Type(ty)) => match ty.as_ref() {
                        Ty::StrLit(value) => {
                            features.extend(vec![text("string literal "), code(value.as_str())])
                        }
                        Ty::NumLit(value) => {
                            features.extend(vec![text("number literal "), code(value)])
                        }
                        Ty::BoolLit(value) => features.extend(vec![
                            text("boolean literal "),
                            code(if *value { "true" } else { "false" }),
                        ]),
                        Ty::BigIntLit(value) => {
                            features.extend(vec![text("bigint literal "), code(value)])
                        }
                        Ty::Str => features.push(text("string")),
                        Ty::Num => features.push(text("number")),
                        Ty::Bool => features.push(text("boolean")),
                        Ty::BigInt => features.push(text("bigint")),
                        Ty::Symbol => features.push(text("symbol")),
                        Ty::UniqueSymbol => features.push(text("unique symbol")),
                        Ty::Void => features.push(text("undefined")),
                        Ty::Null => features.push(text("null")),
                        _ => features.push(desc_of_type_desc(description)),
                    },
                    _ => features.push(desc_of_type_desc(description)),
                }
                features.extend(vec![
                    text(" on "),
                    render_enum_reference(enum_),
                    text(" because computed access is not allowed on enums."),
                ]);
                friendly::Message(features)
            }
            MessageCannotApplyNonPolymorphicType => friendly::Message(vec![text(
                "Cannot apply type because it is not a polymorphic type.",
            )]),
            MessageCannotAssignToInvalidLHS => friendly::Message(vec![text(
                "Invalid left-hand side in assignment expression.",
            )]),
            MessageCannotAccessReactRefInRender { usage, in_hook } => {
                let context = if *in_hook {
                    text("within hooks")
                } else {
                    text("during render")
                };
                friendly::Message(vec![
                    text("Cannot read "),
                    code("current"),
                    text(" from "),
                    render_expression_reference(usage),
                    text(" because "),
                    code("ref"),
                    text(" values may not be read "),
                    context,
                    text(". (https://react.dev/reference/react/useRef)."),
                ])
            }
            MessageCannotAssignToObjectWithComputedProp(property) => friendly::Message(vec![
                text("Cannot use "),
                ref_of_type_desc(&property.loc, &property.desc),
                text(" to assign a computed property."),
                text(" Computed properties may only be numeric or string literal values."),
                text(
                    " See https://flow.org/en/docs/types/literals/ for more information on literal types.",
                ),
            ]),
            MessageIncompatibleWithExact { kind, lower, upper } => {
                let object_kind = match kind {
                    super::intermediate_error_types::ExactnessErrorKind::UnexpectedIndexer => {
                        "indexed "
                    }
                    super::intermediate_error_types::ExactnessErrorKind::UnexpectedInexact => {
                        "inexact "
                    }
                };
                friendly::Message(vec![
                    text(object_kind),
                    ref_of_type_desc(&lower.loc, &lower.desc),
                    text(" is incompatible with exact "),
                    ref_of_type_desc(&upper.loc, &upper.desc),
                ])
            }
            MessageIncompatibleGeneral { lower, upper } => friendly::Message(vec![
                ref_(lower),
                text(" is incompatible with "),
                ref_(upper),
            ]),
            MessageStandaloneCallRequiresReceiver(receiver) => friendly::Message(vec![
                text("its "),
                code("this"),
                text(" parameter requires "),
                ref_of_type(receiver),
                text(
                    " as the receiver. Call it as a method, or bind it to its receiver before calling it",
                ),
            ]),
            MessageIncompatibleWithUnionRepresentative {
                union,
                lower,
                upper,
            } => friendly::Message(vec![
                text("at least one member of "),
                ref_of_type_desc(&union.loc, &union.desc),
                text(", e.g. "),
                ref_(lower),
                text(", is incompatible with "),
                ref_(upper),
            ]),
            MessageIncompatibleReactHooksWithNonReactHook {
                lower,
                upper,
                lower_is_hook,
                hook_is_annot,
            } => {
                let hook_wording = if *hook_is_annot {
                    text("hook type annotation")
                } else {
                    text("hook")
                };
                let (lower_parts, upper_parts) = if *lower_is_hook {
                    (
                        vec![
                            ref_of_type_desc(&lower.loc, &lower.desc),
                            text(" is a React "),
                            hook_wording,
                        ],
                        vec![
                            ref_of_type_desc(&upper.loc, &upper.desc),
                            text(" is not a hook"),
                        ],
                    )
                } else {
                    (
                        vec![
                            ref_of_type_desc(&lower.loc, &lower.desc),
                            text(" is not a React hook"),
                        ],
                        vec![
                            ref_of_type_desc(&upper.loc, &upper.desc),
                            text(" is a "),
                            hook_wording,
                        ],
                    )
                };
                let mut parts = lower_parts;
                parts.push(text(" but "));
                parts.extend(upper_parts);
                friendly::Message(parts)
            }
            MessageInvalidKeyMirrorArgument(
                box MessageInvalidKeyMirrorArgumentData { lower },
            ) => friendly::Message(vec![
                ref_of_type_desc(&lower.loc, &lower.desc),
                text(" is not a valid argument of "),
                code("$KeyMirror"),
            ]),
            MessageCannotAccessObjectWithComputedProp {
                object,
                property,
                kind,
            } => {
                use super::intermediate_error_types::InvalidObjKey;
                let suffix: Vec<friendly::MessageFeature<Loc>> = match kind {
                    InvalidObjKey::Other
                    | InvalidObjKey::NullOrVoid
                    | InvalidObjKey::ComputedNotLiteral
                    | InvalidObjKey::ComputedTypeName => vec![],
                    InvalidObjKey::NumberNonInt => {
                        vec![text(" Only integer-like number literals are allowed.")]
                    }
                    InvalidObjKey::NumberTooLarge => vec![
                        text(" Number literals must not be larger than "),
                        code("Number.MAX_SAFE_INTEGER"),
                        text("."),
                    ],
                    InvalidObjKey::NumberTooSmall => vec![
                        text(" Number literals must not be smaller than "),
                        code("Number.MIN_SAFE_INTEGER"),
                        text("."),
                    ],
                };
                let mut features = vec![
                    text("Cannot access "),
                    ref_of_type_desc(&object.loc, &object.desc),
                    text(" with computed property using "),
                    match kind {
                        InvalidObjKey::NullOrVoid => {
                            hardcoded_string_desc_ref("null or undefined", &property.loc)
                        }
                        _ => ref_of_type_desc(&property.loc, &property.desc),
                    },
                    text("."),
                ];
                features.extend(suffix);
                friendly::Message(features)
            }
            MessageCannotAddComputedPropertyDueToPotentialOverwrite(
                box MessageCannotAddComputedPropertyDueToPotentialOverwriteData {
                    key_loc,
                    overwritten_locs,
                },
            ) => {
                let mut features = vec![
                    text("Cannot add computed property because the indexer"),
                    friendly::no_desc_ref(&loc_of_aloc(key_loc)),
                    text(" may overwrite properties with explicit keys"),
                ];
                for loc in overwritten_locs {
                    features.push(friendly::no_desc_ref(&loc_of_aloc(loc)));
                }
                features.push(text(" in a way that Flow cannot track."));
                friendly::Message(features)
            }
            MessageCannotBuildTypedInterface(sve) => {
                use flow_type_sig::signature_error::SignatureError;
                let features = match sve {
                    SignatureError::ExpectedAnnotation(_, sort) => {
                        vec![text(&format!("Missing type annotation at {}:", sort))]
                    }
                    SignatureError::UnexpectedObjectKey(_, _) => {
                        vec![text("Expected simple key in object:")]
                    }
                    SignatureError::UnexpectedArraySpread(_, _) => {
                        vec![text("Unexpected spread in array:")]
                    }
                    SignatureError::UnexpectedArrayHole(_) => {
                        vec![text("Unexpected array hole:")]
                    }
                    SignatureError::EmptyArray(_) => vec![
                        text("Cannot determine the element type of an empty array. "),
                        text(
                            "Please provide an annotation, e.g., by adding a type cast around this expression.",
                        ),
                    ],
                    SignatureError::EmptyObject(_) => vec![
                        text(
                            "Cannot determine types of initialized properties of an empty object. ",
                        ),
                        text(
                            "Please provide an annotation, e.g., by adding a type cast around this expression.",
                        ),
                    ],
                    SignatureError::UnexpectedExpression(_, esort) => vec![
                        text(&format!(
                            "Cannot determine the type of this {}. ",
                            esort.as_str()
                        )),
                        text(
                            "Please provide an annotation, e.g., by adding a type cast around this expression.",
                        ),
                    ],
                };
                let mut result = vec![
                    text("Cannot build a typed interface for this module. "),
                    text("You should annotate the exports of this module with types. "),
                ];
                result.extend(features);
                friendly::Message(result)
            }
            MessageCannotCallMaybeReactHook(box MessageCannotCallMaybeReactHookData {
                callee_loc,
                hooks,
                non_hooks,
            }) => {
                let hook_blame = if hooks.is_empty() {
                    vec![text("React hook")]
                } else {
                    vec![friendly::hardcoded_string_desc_ref(
                        "React hook",
                        loc_of_aloc(&hooks[0]),
                    )]
                };
                let non_hook_blame = if non_hooks.is_empty() {
                    vec![text("regular function definition")]
                } else {
                    vec![
                        text("regular "),
                        friendly::hardcoded_string_desc_ref(
                            "function definition",
                            loc_of_aloc(&non_hooks[0]),
                        ),
                    ]
                };
                let mut features = vec![
                    text("Cannot call function because "),
                    friendly::hardcoded_string_desc_ref("callee", loc_of_aloc(callee_loc)),
                    text(" may be a "),
                ];
                features.extend(hook_blame);
                features.push(text(" or may be a "));
                features.extend(non_hook_blame);
                features.push(text(
                    ". Function callees must either be definitely a hook or definitely not a hook, because the same hook must be called every time a component renders. ",
                ));
                features.push(text("(https://react.dev/reference/rules/rules-of-hooks)"));
                friendly::Message(features)
            }
            MessageCannotCallNonHookSyntaxHook(callee_loc) => friendly::Message(vec![
                text("Cannot call function because "),
                friendly::hardcoded_string_desc_ref("callee", loc_of_aloc(callee_loc)),
                text(" has a name that indicates it is a React hook (starting with "),
                code("use"),
                text(") but it is not defined with hook syntax. "),
                text("(https://react.dev/reference/rules/rules-of-hooks)"),
            ]),
            MessageCannotCallObjectFunctionOnEnum { function, enum_ } => {
                let suggestion = match &enum_.name {
                    Some(enum_name) => vec![
                        text(" "),
                        text("You can use "),
                        code(&format!("{}.members()", enum_name)),
                        text(" to get an iterator of the enum's members. "),
                        text("You can turn that into an array using "),
                        code("Array.from"),
                        text(
                            ", which optionally takes a second argument if you wish to also map over the result.",
                        ),
                    ],
                    None => vec![],
                };
                let mut features = vec![
                    text("Cannot call function "),
                    render_named_reference(function),
                    text(" with argument "),
                    render_enum_reference(enum_),
                    text(" because it is not an object."),
                ];
                features.extend(suggestion);
                friendly::Message(features)
            }
            MessageCannotCallReactComponent(component) => friendly::Message(vec![
                text("Cannot call "),
                ref_of_type_desc(&component.loc, &component.desc),
                text(" because React components cannot be called. Use JSX instead. "),
                text("(https://react.dev/reference/rules/react-calls-components-and-hooks)"),
            ]),
            MessageCannotCallReactHookConditionally(callee_loc) => friendly::Message(vec![
                text("Cannot call "),
                friendly::hardcoded_string_desc_ref("hook", loc_of_aloc(callee_loc)),
                text(" because React hooks cannot be called in conditional contexts. "),
                text("(https://react.dev/reference/rules/rules-of-hooks)"),
            ]),
            MessageCannotCallReactHookInDefinitelyNonComponentOrHook(callee_loc) => {
                friendly::Message(vec![
                    text("Cannot call "),
                    friendly::hardcoded_string_desc_ref("hook", loc_of_aloc(callee_loc)),
                    text(" because React hooks can only be called within components or hooks. "),
                    text("This hook is definitely not called in a component or hook. "),
                    text("(https://react.dev/reference/rules/rules-of-hooks)"),
                ])
            }
            MessageCannotCallReactHookInNonComponentSyntaxComponentOrHookSyntaxHook(callee_loc) => {
                friendly::Message(vec![
                    text("Cannot call "),
                    friendly::hardcoded_string_desc_ref("hook", loc_of_aloc(callee_loc)),
                    text(" because React hooks can only be called within components or hooks. "),
                    text("Under the current configuration, "),
                    text("we only infer component-syntax components to be components"),
                    text(" and hook-syntax hooks to be hooks. "),
                    text("(https://react.dev/reference/rules/rules-of-hooks)"),
                ])
            }
            MessageCannotCallReactHookInUnknownContext(callee_loc) => friendly::Message(vec![
                text("Cannot call "),
                friendly::hardcoded_string_desc_ref("hook", loc_of_aloc(callee_loc)),
                text(" because React hooks can only be called within components or hooks. "),
                text("(https://react.dev/reference/rules/rules-of-hooks)"),
            ]),
            MessageCannotCallReactHookWithIllegalName(callee_loc) => friendly::Message(vec![
                text("Cannot call hook because "),
                friendly::hardcoded_string_desc_ref("callee", loc_of_aloc(callee_loc)),
                text("'s name does not conform to React hook rules. Hook names must begin with "),
                code("use"),
                text(" followed by a capitalized letter. "),
                text("(https://react.dev/reference/rules/rules-of-hooks)"),
            ]),
            MessageCannotCallFunctionWithExtraArg {
                function_reference,
                param_count,
            } => {
                let msg = match param_count {
                    0 => "no arguments are expected by".to_string(),
                    1 => "no more than 1 argument is expected by".to_string(),
                    n => format!("no more than {} arguments are expected by", n),
                };
                friendly::Message(vec![
                    text(&msg),
                    text(" "),
                    render_function_reference(function_reference),
                ])
            }
            MessageCannotChangeEnumMember(enum_) => friendly::Message(vec![
                text("Cannot change member of "),
                render_enum_reference(enum_),
                text(" because enums are frozen."),
            ]),
            MessageCannotCompare(box MessageCannotCompareData {
                lower,
                upper,
                strict_comparison_opt,
            }) => {
                let mut features = vec![
                    text("Cannot compare "),
                    ref_of_type_desc(&lower.loc, &lower.desc),
                    text(" to "),
                    ref_of_type_desc(&upper.loc, &upper.desc),
                ];
                match strict_comparison_opt {
                    None => features.push(text(".")),
                    Some(info) => {
                        match info {
                            StrictComparisonInfo::General { left, right } => {
                                features.extend(vec![
                                    text(", because "),
                                    ref_of_type_desc(&left.loc, &left.desc),
                                    text(" is not a subtype of "),
                                    ref_of_type_desc(&right.loc, &right.desc),
                                    text(" and "),
                                    ref_of_type_desc(&right.loc, &right.desc),
                                    text(" is not a subtype of "),
                                    ref_of_type_desc(&left.loc, &left.desc),
                                    text(". In **rare** cases, these types may have overlapping values but lack a subtyping relationship. "),
                                    text("If that happens, you can cast one side to the union of both types to pass the flow check. "),
                                ]);
                            }
                            StrictComparisonInfo::Null { null_loc, other } => {
                                features.extend(vec![
                                    text(", because"),
                                    friendly::no_desc_ref(&loc_of_aloc(null_loc)),
                                    text(" is null and "),
                                    ref_of_type_desc(&other.loc, &other.desc),
                                    text(" does not contain null. "),
                                    text("Perhaps you meant to use "),
                                    code("=="),
                                    text(", which checks for both "),
                                    code("undefined"),
                                    text(" and "),
                                    code("null"),
                                    text("?"),
                                ]);
                            }
                            StrictComparisonInfo::Empty { empty } => {
                                features.extend(vec![
                                    text(", because "),
                                    ref_of_type_desc(&empty.loc, &empty.desc),
                                    text(" is empty. "),
                                ]);
                            }
                        }
                    }
                }
                friendly::Message(features)
            }
            MessageCannotCompareNonStrict { lower, upper } => friendly::Message(vec![
                text("Cannot compare "),
                ref_of_type_desc(&lower.loc, &lower.desc),
                text(" to "),
                ref_of_type_desc(&upper.loc, &upper.desc),
                text(" with a non-strict equality check. "),
                text("Make sure the arguments are valid, "),
                text("or try using strict equality ("),
                code("==="),
                text(" or "),
                code("!=="),
                text(") instead."),
            ]),
            MessageCannotCreateExactType(value) => friendly::Message(vec![
                text("Cannot create exact type from "),
                ref_of_type_desc(&value.loc, &value.desc),
                text("."),
            ]),
            MessageInvalidExactObjectIntersection { prop, exact_object } => {
                use super::error_message::mk_prop_message;
                let mut features = vec![text("Cannot normalize this intersection because ")];
                features.extend(mk_prop_message(Some(prop.as_str())));
                features.extend(vec![
                    text(" is missing in exact object "),
                    ref_of_type_desc(&exact_object.loc, &exact_object.desc),
                    text("."),
                ]);
                friendly::Message(features)
            }
            MessageCannotDeclareAlreadyBoundName(x) => friendly::Message(vec![
                text("Cannot declare "),
                render_named_reference(x),
                text(" because the name is already bound."),
            ]),
            MessageCannotDeclareAlreadyBoundNameInCoreJs(x) => friendly::Message(vec![
                text("Cannot declare "),
                render_named_reference(x),
                text(" because the name is a core builtin type."),
            ]),
            MessageCannotDeclareAlreadyBoundNameInNamespace(x) => friendly::Message(vec![
                text("Cannot declare the name in the namespace because the name "),
                render_named_reference(x),
                text(" is already bound."),
            ]),
            MessageInterfaceMergePropertyConflict(x) => friendly::Message(vec![
                text("Duplicate property "),
                render_named_reference(x),
                text(" in a merged interface declaration."),
            ]),
            MessageInterfaceMergeTparamMismatch(x) => friendly::Message(vec![
                text("Cannot merge interface "),
                render_named_reference(x),
                text(" because type parameter lists differ."),
            ]),
            MessageCannotDeclareReservedType { binding, keyword } => friendly::Message(vec![
                text("Cannot declare "),
                render_named_reference(binding),
                text(", because "),
                code(keyword.incorrect_of_kind()),
                text(" is a reserved type name."),
            ]),
            MessageCannotDelete(expr) => friendly::Message(vec![
                text("Cannot delete "),
                ref_of_type_desc(&expr.loc, &expr.desc),
                text(" because only member expressions and variables can be deleted."),
            ]),
            MessageCannotDetermineEmptyArrayLiteralType => friendly::Message(vec![text(
                "Cannot determine type of empty array literal. Please provide an annotation.",
            )]),
            MessageCannotDetermineModuleType => friendly::Message(vec![
                text("Unable to determine module type (CommonJS vs ES) if both an export "),
                text("statement and "),
                code("module.exports"),
                text(" are used in the "),
                text("same module!"),
            ]),
            MessageCannotExportRenamedDefault(box MessageCannotExportRenamedDefaultData {
                name,
                is_reexport,
            }) => {
                let reexport_msg = vec![
                    text("If you intended to set the default export please "),
                    code("import"),
                    text(" and then "),
                    code("export default"),
                    text(" instead."),
                ];
                match (name, is_reexport) {
                    (None, _) => {
                        let mut features = vec![
                            text("Cannot set the default export of a module by re-exporting the "),
                            code("default"),
                            text(" property. "),
                        ];
                        features.extend(reexport_msg);
                        friendly::Message(features)
                    }
                    (Some(name), true) => {
                        let mut features = vec![
                            text("Cannot set the default export of a module by re-exporting "),
                            code(name),
                            text(" as "),
                            code("default"),
                            text(". "),
                        ];
                        features.extend(reexport_msg);
                        friendly::Message(features)
                    }
                    (Some(name), false) => friendly::Message(vec![
                        text("Cannot set the default export of a module by renaming "),
                        code(name),
                        text(" to "),
                        code("default"),
                        text(". If you intended to set the default export use "),
                        code(&format!("export default {}", name)),
                        text(" instead."),
                    ]),
                }
            }
            MessageCannotExhaustivelyCheckAbstractEnums(
                box MessageCannotExhaustivelyCheckAbstractEnumsData {
                    description_name,
                    enum_loc,
                    enum_name,
                },
            ) => {
                let enum_reference = match enum_name {
                    Some(name) => hardcoded_string_desc_ref(&format!("`{name}`"), enum_loc),
                    None => hardcoded_string_desc_ref("enum", enum_loc),
                };
                friendly::Message(vec![
                    text("Cannot exhaustively check "),
                    match description_name.as_ref().or(enum_name.as_ref()) {
                        Some(name) => code(name),
                        None => text("enum"),
                    },
                    text(" because "),
                    enum_reference,
                    text(" is an abstract enum value, so has no members."),
                ])
            }
            MessageCannotExhaustivelyCheckEnumWithUnknowns(
                box MessageCannotExhaustivelyCheckEnumWithUnknownsData {
                    description_name,
                    enum_,
                },
            ) => friendly::Message(vec![
                text("Missing "),
                code("default"),
                text(" case in the check of "),
                code(description_name.as_ref().unwrap_or(&enum_.name)),
                text(". "),
                render_named_reference(enum_),
                text(" has unknown members (specified using "),
                code("..."),
                text(") so checking it requires the use of a "),
                code("default"),
                text(" case to cover those members."),
            ]),
            MessageCannotImplementNonInterface(type_) => friendly::Message(vec![
                text("Cannot implement "),
                ref_of_type_desc(&type_.loc, &type_.desc),
                text(" because it is not an interface."),
            ]),
            MessageCannotInstantiateObjectUtilTypeWithEnum(
                box MessageCannotInstantiateObjectUtilTypeWithEnumData {
                    operation_name,
                    enum_,
                },
            ) => {
                let suggestion = match &enum_.name {
                    Some(enum_name) => vec![
                        text(" "),
                        text("You can use the enum's name "),
                        code(enum_name),
                        text(" as the type of its members."),
                    ],
                    None => vec![],
                };
                let mut features = vec![
                    text("Cannot instantiate "),
                    code(operation_name),
                    text(" because "),
                    render_enum_reference(enum_),
                    text(" is not an object."),
                ];
                features.extend(suggestion);
                friendly::Message(features)
            }
            MessageCannotIterateEnumForIn(enum_) => {
                let suggestion = match &enum_.name {
                    Some(enum_name) => vec![
                        text(" "),
                        text("You can use "),
                        code(&format!("for (... of {}.members()) {{ ... }}", enum_name)),
                        text(" to iterate over the enum's members."),
                    ],
                    None => vec![],
                };
                let mut features = vec![
                    text("Cannot iterate using a "),
                    code("for...in"),
                    text(" loop because "),
                    render_enum_reference(enum_),
                    text(" is not an object, null, or undefined."),
                ];
                features.extend(suggestion);
                friendly::Message(features)
            }
            MessageCannotIterateEnum(enum_) => {
                let suggestion = match &enum_.name {
                    Some(enum_name) => vec![
                        text(" "),
                        text("You can use "),
                        code(&format!("{}.members()", enum_name)),
                        text(" to get an iterator for the enum's members."),
                    ],
                    None => vec![],
                };
                let mut features = vec![
                    render_enum_reference(enum_),
                    text(" is not an iterable."),
                ];
                features.extend(suggestion);
                friendly::Message(features)
            }
            MessageCannotIterateWithForIn(rhs) => friendly::Message(vec![
                text("Cannot iterate using a "),
                code("for...in"),
                text(" statement "),
                text("because "),
                ref_of_type_desc(&rhs.loc, &rhs.desc),
                text(" is not an object, null, or undefined."),
            ]),
            MessageCannotMutateThisPrototype => {
                friendly::Message(vec![text("Mutating this prototype is unsupported.")])
            }
            MessageCannotNestComponents => friendly::Message(vec![text(
                "Components may not be nested directly within other components or hooks.",
            )]),
            MessageCannotNestHook => friendly::Message(vec![text(
                "Hooks may not be nested directly within other components or hooks.",
            )]),
            MessageCannotOptimizeUnionDueToNonUniqueKeys(non_unique_keys) => {
                let string_of_union_enum =
                    |x: &UnionEnum| -> friendly::MessageFeature<Loc> { code(&x.to_string()) };
                let string_of_non_unique_key =
                    |name: &Name,
                     map: &BTreeMap<UnionEnum, Vec1<L>>|
                     -> Vec<friendly::MessageFeature<Loc>> {
                        let ref_texts = |rs: &Vec1<L>| -> Vec<friendly::MessageFeature<Loc>> {
                            let mut result = vec![hardcoded_string_desc_ref("object type", &rs[0])];
                            for r in rs.iter().skip(1) {
                                    result.push(text(", "));
                                    result.push(hardcoded_string_desc_ref("object type", r));
                                }
                                result
                            };
                        map.iter()
                            .flat_map(|(enum_val, rs)| {
                                let mut features = vec![
                                    text("\n - Key "),
                                    code(&name.display_smol_str()),
                                    text(" has value "),
                                    string_of_union_enum(enum_val),
                                    text(" in "),
                                ];
                                features.extend(ref_texts(rs));
                                features.push(text("."));
                                features
                            })
                            .collect()
                    };
                let keys: Vec<friendly::MessageFeature<Loc>> = non_unique_keys
                    .iter()
                    .flat_map(|(name, map)| string_of_non_unique_key(name, map))
                    .collect();
                let mut features = vec![text(
                    "Flow cannot use these keys to quickly check this union because the same supported value appears in multiple members:",
                )];
                features.extend(keys);
                friendly::Message(features)
            }
            MessageCannotOptimizeUnionInternally(kind) => {
                use flow_typing_type::type_::union_rep::OptimizedError;

                use super::error_message::expect_type_desc;
                let features = match kind {
                    OptimizedError::ContainsUnresolved { loc, desc } => vec![
                        text("Flow cannot resolve "),
                        ref_of_type_desc(loc, &expect_type_desc(desc.clone())),
                        text(" well enough to quickly check this union. "),
                        text("Try inlining or simplifying this type."),
                    ],
                    OptimizedError::NoCandidateMembers => vec![
                        text(
                            "Flow needs at least one object member, literal member (string, numeric, boolean, or bigint), ",
                        ),
                        code("void"),
                        text(", or "),
                        code("null"),
                        text(" to quickly check this union. This union has none of those."),
                    ],
                    OptimizedError::NoCommonKeys => vec![
                        text(
                            "Flow checks object unions faster when every member has a shared key, such as ",
                        ),
                        code("type"),
                        text(" or "),
                        code("kind"),
                        text(
                            ", with a supported literal value. Add a shared key with a different literal value for each member.",
                        ),
                        text(
                            " See https://flow.org/en/docs/types/unions/ for more information on unions.",
                        ),
                    ],
                };
                friendly::Message(features)
            }
            MessageCannotPassReactRefAsArgument { usage, in_hook } => {
                let context = if *in_hook {
                    text("within hooks")
                } else {
                    text("during render")
                };
                friendly::Message(vec![
                    text("Cannot pass "),
                    render_expression_reference(usage),
                    text(" as an argument because "),
                    code("ref"),
                    text(
                        " values may not be passed to functions because they could read the ref value (",
                    ),
                    code("current"),
                    text(") property) "),
                    context,
                    text(". (https://react.dev/reference/react/useRef)."),
                ])
            }
            MessageCannotPerformArithOnNonNumbersOrBigInt(operand) => {
                friendly::Message(vec![
                    text("Cannot perform arithmetic operation because "),
                    ref_of_type_desc(&operand.loc, &operand.desc),
                    text(" is not a number or bigint."),
                ])
            }
            MessageCannotPerformBigIntRShift3(operand) => friendly::Message(vec![
                text("Cannot perform unsigned right shift because "),
                ref_of_type_desc(&operand.loc, &operand.desc),
                text(" "),
                text("is a bigint, and all bigints are signed."),
            ]),
            MessageCannotPerformBigIntUnaryPlus(operand) => friendly::Message(vec![
                text("Cannot perform unary plus because a "),
                ref_of_type_desc(&operand.loc, &operand.desc),
                text(" "),
                text("cannot be coerced to number."),
            ]),
            MessageCannotPerformBinaryArith {
                kind,
                left,
                right,
            } => {
                let kind_str = kind.to_string();
                friendly::Message(vec![
                    text("Cannot use operator `"),
                    text(kind_str),
                    text("` with operands "),
                    ref_of_type_desc(&left.loc, &left.desc),
                    text(" and "),
                    ref_of_type_desc(&right.loc, &right.desc),
                ])
            }
            MessageCannotReassignConstant(x) => friendly::Message(vec![
                text("Cannot reassign constant "),
                render_named_reference(x),
                text("."),
            ]),
            MessageCannotReassignConstantLikeBinding {
                definition,
                binding_kind,
            } => friendly::Message(vec![
                text("Cannot reassign "),
                text(binding_kind.as_str()),
                text(" binding "),
                render_named_reference(definition),
                text("."),
            ]),
            MessageCannotReassignEnum(x) => friendly::Message(vec![
                text("Cannot reassign enum "),
                render_named_reference(x),
                text("."),
            ]),
            MessageCannotReassignImport(x) => friendly::Message(vec![
                text("Cannot reassign import "),
                render_named_reference(x),
                text("."),
            ]),
            MessageCannotRedeclareVar(x) => friendly::Message(vec![
                text("Cannot declare "),
                render_named_reference(x),
                text(" because var redeclaration is not supported."),
            ]),
            MessageCannotReferenceTypeGuardParameter {
                type_guard,
                binding_loc,
                binding_kind,
            } => {
                let binding_description = match binding_kind {
                    TypeGuardBindingKind::RestParameter => {
                        format!("rest parameter `{}`", type_guard.name)
                    }
                    TypeGuardBindingKind::PatternParameter => {
                        format!("pattern parameter `{}`", type_guard.name)
                    }
                };
                friendly::Message(vec![
                    text("A "),
                    type_guard_parameter_reference(type_guard),
                    text(" cannot reference "),
                    hardcoded_string_desc_ref(&binding_description, binding_loc),
                    text("."),
                ])
            }
            MessageCannotResolveBuiltinName(name) => {
                friendly::Message(vec![text("Cannot resolve name "), code(name), text(".")])
            }
            MessageUmdGlobalValueInModule(name) => friendly::Message(vec![
                code(name),
                text(
                    " refers to a UMD global declared with `export as namespace`, which cannot be \
                     used as a value in a module. Import it instead.",
                ),
            ]),
            MessageCannotResolveBuiltinModule(box MessageCannotResolveBuiltinModuleData {
                name,
                potential_generator,
            }) => {
                let mut features = vec![text("Cannot resolve module "), code(name), text(".")];
                if let Some(generator) = potential_generator {
                    features.extend(vec![
                        text(" Try running the command "),
                        code(generator),
                        text(" to generate the missing module."),
                    ]);
                }
                friendly::Message(features)
            }
            MessageCannotImportGlobalLibdef(box MessageCannotImportGlobalLibdefData {
                module_name,
                libdef_name,
            }) => friendly::Message(vec![
                text("Cannot import "),
                code(module_name),
                text(" because "),
                code(libdef_name),
                text(
                    " is a global library definition file, so it is not a module. Its \
                     declarations are already available in global scope; remove this import and \
                     reference them directly.",
                ),
            ]),
            MessageCannotResolveExpectedModule {
                name,
                expected_module_purpose,
            } => {
                use super::intermediate_error_types::ExpectedModulePurpose;
                let explanation = match expected_module_purpose {
                    ExpectedModulePurpose::ReactModuleForJSXFragment => vec![
                        text("The "),
                        code("react"),
                        text(" module must exist to type check JSX fragments."),
                    ],
                    ExpectedModulePurpose::ReactModuleForReactClassComponent => vec![
                        text("The "),
                        code("react"),
                        text(" module must exist to type check React class components."),
                    ],
                    ExpectedModulePurpose::ReactModuleForReactMixedElementType => vec![
                        text("The "),
                        code("react"),
                        text(" module must exist to provide a type for "),
                        code("React.MixedElement"),
                        text("."),
                    ],
                    ExpectedModulePurpose::ReactModuleForReactNodeType => vec![
                        text("The "),
                        code("react"),
                        text(" module must exist to provide a type for "),
                        code("React.Node"),
                        text("."),
                    ],
                    ExpectedModulePurpose::ReactModuleForReactRefSetterType => vec![
                        text("The "),
                        code("react"),
                        text(" module must exist to provide a type for "),
                        code("React.RefSetter"),
                        text("."),
                    ],
                    ExpectedModulePurpose::ReactModuleForReactElementRefType => vec![
                        text("The "),
                        code("react"),
                        text(" module must exist to provide a type for "),
                        code("React.ElementRef"),
                        text("."),
                    ],
                    ExpectedModulePurpose::ReactModuleForReactManagedAttributesType => vec![
                        text("The "),
                        code("react"),
                        text(" module must exist to provide a type for "),
                        code("React.ReactManagedAttributes"),
                        text("."),
                    ],
                };
                let mut features = vec![text("Cannot resolve module "), code(name), text(". ")];
                features.extend(explanation);
                friendly::Message(features)
            }
            MessageCannotSpreadDueToPotentialOverwrite {
                spread_loc,
                object,
                key,
            } => friendly::Message(vec![
                text("Flow cannot determine a type for the "),
                hardcoded_string_desc_ref("spread", spread_loc),
                text(". "),
                ref_of_type_desc(&object.loc, &object.desc),
                text(" cannot be spread because the indexer "),
                ref_of_type_desc(&key.loc, &key.desc),
                text(
                    " may overwrite properties with explicit keys in a way that Flow cannot track. ",
                ),
                text("Try spreading "),
                ref_of_type_desc(&object.loc, &object.desc),
                text(" first or remove the indexer"),
            ]),
            MessageCannotSpreadGeneral(box MessageCannotSpreadGeneralData {
                spread_loc,
                object1,
                object2,
                propname,
                error_kind,
            }) => {
                use super::intermediate_error_types::ExactnessErrorKind;
                let (error_reason, fix_suggestion) = match error_kind {
                    ExactnessErrorKind::UnexpectedInexact => (
                        "is inexact",
                        vec![
                            text(" Try making "),
                            ref_of_type_desc(&object2.loc, &object2.desc),
                            text(" exact"),
                        ],
                    ),
                    ExactnessErrorKind::UnexpectedIndexer => (
                        "has an indexer",
                        vec![
                            text(" Try removing the indexer in "),
                            ref_of_type_desc(&object2.loc, &object2.desc),
                            text(" or make "),
                            code(&propname.display_smol_str()),
                            text(" a required property"),
                        ],
                    ),
                };
                let mut features = vec![
                    text("Flow cannot determine a type for the "),
                    hardcoded_string_desc_ref("spread", spread_loc),
                    text(". "),
                    ref_of_type_desc(&object2.loc, &object2.desc),
                    text(" "),
                    text(error_reason),
                    text(", so it may contain "),
                    code(&propname.display_smol_str()),
                    text(" with a type that conflicts with "),
                    code(&propname.display_smol_str()),
                    text("'s definition in "),
                    ref_of_type_desc(&object1.loc, &object1.desc),
                    text("."),
                ];
                features.extend(fix_suggestion);
                friendly::Message(features)
            }
            MessageCannotSpreadInexactMayOverwriteIndexer(
                box MessageCannotSpreadInexactMayOverwriteIndexerData {
                    spread_loc,
                    key,
                    value,
                    object2,
                },
            ) => friendly::Message(vec![
                text("Flow cannot determine a type for the "),
                hardcoded_string_desc_ref("spread", spread_loc),
                text(". "),
                ref_of_type_desc(&object2.loc, &object2.desc),
                text(" is inexact and may "),
                text("have a property key that conflicts with "),
                ref_of_type_desc(&key.loc, &key.desc),
                text(" or a property value that conflicts with "),
                ref_of_type_desc(&value.loc, &value.desc),
                text(". Try making "),
                ref_of_type_desc(&object2.loc, &object2.desc),
                text(" exact"),
            ]),
            MessageCannotSpreadInterface {
                spread_loc,
                interface,
            } => friendly::Message(vec![
                text("Flow cannot determine a type for the "),
                hardcoded_string_desc_ref("spread", spread_loc),
                text(". "),
                ref_of_type_desc(&interface.loc, &interface.desc),
                text(" cannot be spread because interfaces do not "),
                text("track the own-ness of their properties. Try using an object type instead"),
            ]),
            MessageCannotUseAsConstructor(value) => friendly::Message(vec![
                text("Cannot use "),
                code("new"),
                text(" on "),
                ref_of_type_desc(&value.loc, &value.desc),
                text(
                    ". Only classes and interfaces with a construct signature can be constructed.",
                ),
            ]),
            MessageCannotUseAsPrototype(prototype) => friendly::Message(vec![
                text("Cannot use "),
                ref_of_type_desc(&prototype.loc, &prototype.desc),
                text(" as a prototype. Expected an object or null."),
            ]),
            MessageCannotUseAsSuperClass(expression) => friendly::Message(vec![
                text("Cannot use "),
                render_expression_reference(expression),
                text(" as a superclass. Only variables and member expressions may be extended"),
            ]),
            MessageCannotUseBeforeDeclaration(x) => friendly::Message(vec![
                text("Cannot use variable "),
                render_named_reference(x),
                text(" because the declaration "),
                text("either comes later or was skipped."),
            ]),
            MessageCannotUseDefaultImportWithDestrucuturing => friendly::Message(vec![
                text(
                    "The default export of a module cannot be accessed from import destructuring. ",
                ),
                text("To use the default export you must import it directly."),
            ]),
            MessageCannotUseDollarExports => friendly::Message(vec![
                text("Cannot use "),
                code("$Exports"),
                text(" because the first type "),
                text("argument must be a string literal."),
            ]),
            MessageCannotUseEnumMemberUsedAsType(
                box MessageCannotUseEnumMemberUsedAsTypeData {
                    description_name,
                    enum_,
                },
            ) => friendly::Message(vec![
                text("Cannot use "),
                match description_name.as_ref().or(enum_.name.as_ref()) {
                    Some(name) => code(name),
                    None => text("enum"),
                },
                text(" as a type. "),
                text("Enum members are not separate types. "),
                text("Only the enum itself, "),
                render_enum_value_reference(enum_),
                text(", is a type."),
            ]),
            MessageCannotUseExportInNonLegalToplevelContext(name) => friendly::Message(vec![
                code(name),
                text(" may only be used as part of a legal top level export statement"),
            ]),
            MessageCannotUseImportStar(import_star_loc) => friendly::Message(vec![
                hardcoded_string_desc_ref("`import *`", import_star_loc),
                text(" object can only be used by accessing one of its named exports"),
                text(" with a member access or destructuring."),
            ]),
            MessageCannotUseInOperatorDueToBadLHS(lhs) => friendly::Message(vec![
                text("Cannot use "),
                code("in"),
                text(" because on the left-hand side, "),
                ref_of_type_desc(&lhs.loc, &lhs.desc),
                text(" must be a string or number."),
            ]),
            MessageCannotUseInOperatorDueToBadRHS(rhs) => friendly::Message(vec![
                text("Cannot use "),
                code("in"),
                text(" because on the right-hand side, "),
                ref_of_type_desc(&rhs.loc, &rhs.desc),
                text(" must be an object or array."),
            ]),
            MessageCannotUseInstanceOfOperatorDueToBadRHS(rhs) => friendly::Message(vec![
                text("The right-hand side of an "),
                code("instanceof"),
                text(" expression must be an object, but got "),
                ref_of_type_desc(&rhs.loc, &rhs.desc),
                text("."),
            ]),
            MessageCannotUseMixedImportAndRequire(import_loc) => friendly::Message(vec![
                text("Cannot use a mix of non-type toplevel "),
                hardcoded_string_desc_ref("`import`", import_loc),
                text(" and "),
                code("require"),
                text(" statements in the same file."),
            ]),
            MessageCannotUseNonPolymorphicTypeWithTypeArgs {
                is_new,
                callee,
                expected_arity,
            } => {
                let use_word = if *is_new { "construct " } else { "call " };
                let callee = match callee.as_ref() {
                    CallTypeArityCallee::Type(callee) => ref_of_type_desc(&callee.loc, &callee.desc),
                    CallTypeArityCallee::Builtin(name) => code(name),
                    CallTypeArityCallee::Function => text("function"),
                };
                if *expected_arity == 0 {
                    friendly::Message(vec![
                        text("Cannot "),
                        text(use_word),
                        text("non-polymorphic "),
                        callee,
                        text(" with type arguments."),
                    ])
                } else {
                    let suffix = if *expected_arity == 1 { "" } else { "s" };
                    friendly::Message(vec![
                        text("Cannot "),
                        text(use_word),
                        callee,
                        text(" without exactly "),
                        text(&format!("{} type argument{}.", expected_arity, suffix)),
                    ])
                }
            }
            MessageCannotUsePrimitiveAsInterface {
                lower,
                upper,
                kind,
            } => {
                let kind_str = match kind {
                    super::intermediate_error_types::PrimitiveKind::Boolean => "Boolean",
                    super::intermediate_error_types::PrimitiveKind::Number => "Number",
                    super::intermediate_error_types::PrimitiveKind::String => "String",
                };
                friendly::Message(vec![
                    ref_of_type_desc(&lower.loc, &lower.desc),
                    text(", a primitive, cannot be used as a subtype of "),
                    ref_of_type_desc(&upper.loc, &upper.desc),
                    text(". "),
                    text("You can wrap it in "),
                    code(&format!("new {}(...))", kind_str)),
                    text(
                        " to turn it into an object and attempt to use it as a subtype of an interface",
                    ),
                ])
            }
            MessageCannotUseTypeDueToPolarityMismatch {
                type_param,
                expected_polarity,
                actual_polarity,
            } => {
                let polarity_string = |p: &Polarity| match p {
                    Polarity::Positive => "output",
                    Polarity::Negative => "input",
                    Polarity::Neutral => "input/output",
                };
                let expected = polarity_string(expected_polarity);
                let actual = polarity_string(actual_polarity);
                friendly::Message(vec![
                    text("Cannot use "),
                    render_named_reference(type_param),
                    text(&format!(" in an {} ", actual)),
                    text("position because "),
                    render_named_reference(type_param),
                    text(" is expected to occur only in "),
                    text(&format!("{} positions.", expected)),
                ])
            }
            MessageCannotUseTypeForAnnotationInference(
                box MessageCannotUseTypeForAnnotationInferenceData {
                    operation,
                    operation_loc,
                    target_loc,
                    target_desc,
                },
            ) => {
                let operation_desc = annotation_inference_operation_description(operation);
                let mut features = vec![
                    text("Cannot use "),
                ];
                features.extend(friendly::message_of_string::<Loc>(&operation_desc).0);
                features.extend(vec![
                    text(" on "),
                    ref_of_type_desc(target_loc, target_desc),
                    text(" in an export position. "),
                    text("Please provide an (alternative) annotation for "),
                    friendly::hardcoded_string_desc_ref(
                        &operation_desc,
                        loc_of_aloc(operation_loc),
                    ),
                    text("."),
                ]);
                friendly::Message(features)
            }
            MessageCannotUseTypeGuardWithFunctionParamHavoced(
                box MessageCannotUseTypeGuardWithFunctionParamHavocedData {
                    type_guard,
                    param,
                    call_locs,
                },
            ) => {
                let loc_str: Vec<_> = if call_locs.is_empty() {
                    vec![text("in this function")]
                } else if call_locs.len() == 1 {
                    vec![
                        text("in"),
                        friendly::no_desc_ref(&loc_of_aloc(&call_locs[0])),
                    ]
                } else {
                    let mut v = vec![text("in the following expressions:")];
                    for loc in call_locs {
                        v.push(friendly::no_desc_ref(&loc_of_aloc(loc)));
                    }
                    v
                };
                let mut features = vec![
                    text("Cannot use "),
                    friendly::MessageFeature::Inline(friendly::message_inlines_of_string(
                        &type_guard_description(type_guard),
                    )),
                    text(", because "),
                    type_guard_reference(param),
                    text(" is reassigned "),
                ];
                features.extend(loc_str);
                features.push(text("."));
                friendly::Message(features)
            }
            MessageCannotUseTypeInValuePosition(box MessageCannotUseTypeInValuePositionData {
                binding,
                type_only_namespace,
                imported_name,
            }) => {
                let base = if *type_only_namespace {
                    vec![
                        text("Cannot use type-only namespace "),
                        render_named_reference(binding),
                        text(" as a value. "),
                        text("Type-only namespaces are erased and don't exist at runtime."),
                    ]
                } else {
                    vec![
                        text("Cannot use type "),
                        render_named_reference(binding),
                        text(" as a value. "),
                        text("Types are erased and don't exist at runtime."),
                    ]
                };
                match imported_name {
                    None => friendly::Message(base),
                    Some(name) => {
                        let mut features = base;
                        features.extend(vec![
                            text(" If the exported binding can also be used as a value, try importing it using "),
                            code(&format!("import {}", name)),
                            text(" instead of "),
                            code(&format!("import type {}", name)),
                            text(" and "),
                            code(&format!("import {{{}}}", name)),
                            text(" instead of "),
                            code(&format!("import type {{{}}}", name)),
                            text("."),
                        ]);
                        friendly::Message(features)
                    }
                }
            }
            MessageCannotUseTypeWithoutAnyTypeArgs {
                type_,
                min_arity,
                max_arity,
            } => {
                let (arity, args) = if min_arity == max_arity {
                    (
                        format!("{}", max_arity),
                        if *max_arity == 1 {
                            "argument"
                        } else {
                            "arguments"
                        },
                    )
                } else {
                    (format!("{}-{}", min_arity, max_arity), "arguments")
                };
                friendly::Message(vec![
                    text("Cannot use "),
                    ref_of_type_desc(&type_.loc, &type_.desc),
                    text(&format!(" without {} type {}.", arity, args)),
                ])
            }
            MessageCannotUseTypeWithoutExactlyNTypeArgs(n) => {
                let suffix = if *n == 1 { "argument" } else { "arguments" };
                friendly::Message(vec![
                    text("Cannot use type without exactly "),
                    text(&format!("{} type {}.", n, suffix)),
                ])
            }
            MessageCannotUseTypeWithTooFewTypeArgs { reason_arity, n } => {
                let suffix = if *n == 1 { "argument" } else { "arguments" };
                friendly::Message(vec![
                    text("Cannot use "),
                    ref_(reason_arity),
                    text(" with fewer than "),
                    text(&format!("{} type {}.", n, suffix)),
                ])
            }
            MessageCannotUseTypeWithTooManyTypeArgs { reason_arity, n } => {
                let suffix = if *n == 1 { "argument" } else { "arguments" };
                friendly::Message(vec![
                    text("Cannot use "),
                    ref_(reason_arity),
                    text(" with more than "),
                    text(&format!("{} type {}.", n, suffix)),
                ])
            }
            MessageCannotUseThisSuperBeforeSuperCall(x) => friendly::Message(vec![
                text("Must call "),
                code("super"),
                text(" before accessing "),
                render_named_reference(x),
                text(" in a derived constructor."),
            ]),
            MessageComponentMissingReturn(component) => {
                let description = match &component.name {
                    Some(name) => format!("component {name}"),
                    None => "component".to_string(),
                };
                friendly::Message(vec![
                    text("Cannot declare component because "),
                    hardcoded_string_desc_ref(&description, &component.loc),
                    text(
                        " is not guaranteed to reach a return statement. An explicit return statement must be included for all possible branches.",
                    ),
                ])
            }
            MessageComponentMissingBody => friendly::Message(vec![text(
                "Components in non-ambient contexts must have a body.",
            )]),
            MessageComponentBodyInAmbientContext => friendly::Message(vec![text(
                "Components in ambient contexts (library definitions, .flow files, declare module, declare namespace) cannot have a body.",
            )]),
            MessageComponentNonUpperCase => friendly::Message(vec![text(
                "Component identifiers must begin with an upper-case character",
            )]),
            MessageDeclareComponentInvalidParam(kind) => {
                use crate::intermediate_error_types::DeclareComponentInvalidParamKind::*;
                match kind {
                    DeclareComponentParamAsBinding => friendly::Message(vec![text(
                        "Cannot use `as` binding in `declare component` parameters, unless the param name is a string literal. Use the type-only syntax `paramName: Type` instead.",
                    )]),
                    DeclareComponentParamDefaultValue => friendly::Message(vec![text(
                        "Cannot use default values in `declare component` parameters.",
                    )]),
                    DeclareComponentParamMissingAnnotation => friendly::Message(vec![text(
                        "Missing type annotation on `declare component` parameter.",
                    )]),
                    DeclareComponentParamStringLiteralWithoutAs => friendly::Message(vec![text(
                        "String literal parameter names in `declare component` require an `as` binding. Use `'param-name' as paramName: Type`.",
                    )]),
                }
            }
            MessageDefinitionCycle(dependencies) => {
                use flow_env_builder::env_api::AnnotLoc;
                let compare = |a: &L, b: &L| loc_of_aloc(a).cmp(&loc_of_aloc(b));
                let deps: Vec<friendly::MessageFeature<Loc>> = dependencies
                    .iter()
                    .enumerate()
                    .filter_map(|(i, (definition, dep, _annot_locs))| {
                        if dep.is_empty() {
                            return None;
                        }
                        if i == 10 {
                            return Some(vec![text(" - ...\n")]);
                        }
                        if i > 10 {
                            return None;
                        }
                        let mut sorted_dep = dep.clone();
                        sorted_dep.sort_by(&compare);
                        sorted_dep.dedup_by(|a, b| compare(a, b) == std::cmp::Ordering::Equal);
                        let hd = sorted_dep.remove(0);
                        let tl = sorted_dep;
                        let (suffix, tl): (Vec<friendly::MessageFeature<Loc>>, Vec<L>) =
                            if tl.len() > 4 {
                                (vec![text(", [...]")], tl.into_iter().take(4).collect())
                            } else {
                                (vec![], tl)
                            };
                        let tl_dep: Vec<friendly::MessageFeature<Loc>> = tl
                            .iter()
                            .flat_map(|loc| {
                                vec![text(","), friendly::no_desc_ref(&loc_of_aloc(loc))]
                            })
                            .collect();
                        let mut result = vec![
                            text(" - "),
                            hardcoded_string_desc_ref(
                                &definition_description(&definition.kind),
                                &definition.loc,
                            ),
                            text(" depends on "),
                            friendly::hardcoded_string_desc_ref(
                                "other definition",
                                loc_of_aloc(&hd),
                            ),
                        ];
                        result.extend(tl_dep);
                        result.extend(suffix);
                        result.push(text("\n"));
                        Some(result)
                    })
                    .flatten()
                    .collect();
                let (mut locs, mut properties): (Vec<L>, Vec<L>) = (vec![], vec![]);
                for (_reason, _dep, annot_locs) in dependencies {
                    for annot_loc in annot_locs {
                        match annot_loc {
                            AnnotLoc::Loc(l) => locs.push(l.dupe()),
                            AnnotLoc::Object { loc, props } => {
                                locs.push(loc.dupe());
                                properties.extend(props.iter().map(|p| p.dupe()));
                            }
                        }
                    }
                }
                locs.sort_by(&compare);
                locs.dedup_by(|a, b| compare(a, b) == std::cmp::Ordering::Equal);
                locs.truncate(10);
                properties.sort_by(&compare);
                properties.dedup_by(|a, b| compare(a, b) == std::cmp::Ordering::Equal);
                let annot_message = |ls: &[L]| -> Vec<friendly::MessageFeature<Loc>> {
                    ls.iter()
                        .map(|l| friendly::no_desc_ref(&loc_of_aloc(l)))
                        .collect()
                };
                let mut features = vec![text(
                    "The following definitions recursively depend on each other, and Flow cannot compute their types:\n",
                )];
                features.extend(deps);
                features.push(text("Please add type annotations to these definitions"));
                features.extend(annot_message(&locs));
                if !properties.is_empty() && properties.len() <= 5 {
                    features.push(text(" or to these object properties"));
                    features.extend(annot_message(&properties));
                }
                friendly::Message(features)
            }
            MessageDefinitionInvalidRecursive(box MessageDefinitionInvalidRecursiveData {
                definition,
                recursion,
                annot_locs,
            }) => {
                use flow_env_builder::env_api::AnnotLoc;
                let (itself, tl_recur): (
                    friendly::MessageFeature<Loc>,
                    Vec<friendly::MessageFeature<Loc>>,
                ) = match recursion.as_slice() {
                    [hd, tl @ ..] => {
                        let (suffix, tl): (Vec<friendly::MessageFeature<Loc>>, &[L]) =
                            if tl.len() > 4 {
                                (vec![text(", [...]")], &tl[..4])
                            } else {
                                (vec![], tl)
                            };
                        let mut tl_recur: Vec<friendly::MessageFeature<Loc>> = tl
                            .iter()
                            .flat_map(|loc| {
                                vec![text(", "), friendly::no_desc_ref(&loc_of_aloc(loc))]
                            })
                            .collect();
                        tl_recur.extend(suffix);
                        (
                            friendly::hardcoded_string_desc_ref("itself", loc_of_aloc(hd)),
                            tl_recur,
                        )
                    }
                    [] => (text("itself"), vec![]),
                };
                let annot_message: Vec<friendly::MessageFeature<Loc>> = match annot_locs.as_slice()
                {
                    [] => vec![text("this definition")],
                    [AnnotLoc::Loc(loc)] => {
                        vec![friendly::hardcoded_string_desc_ref(
                            "this definition",
                            loc_of_aloc(loc),
                        )]
                    }
                    [AnnotLoc::Object { loc, props }] if props.is_empty() => {
                        vec![friendly::hardcoded_string_desc_ref(
                            "this definition",
                            loc_of_aloc(loc),
                        )]
                    }
                    [AnnotLoc::Object { loc, props }] if props.len() > 5 => {
                        vec![friendly::hardcoded_string_desc_ref(
                            "this definition",
                            loc_of_aloc(loc),
                        )]
                    }
                    [AnnotLoc::Object { loc, props }] if props.len() == 1 => vec![
                        friendly::hardcoded_string_desc_ref("this definition", loc_of_aloc(loc)),
                        text("or to"),
                        friendly::hardcoded_string_desc_ref("its property", loc_of_aloc(&props[0])),
                    ],
                    [AnnotLoc::Object { loc, props }] => {
                        let mut result = vec![
                            friendly::hardcoded_string_desc_ref(
                                "this definition",
                                loc_of_aloc(loc),
                            ),
                            text(" or to its properties"),
                        ];
                        for prop in props {
                            result.push(friendly::no_desc_ref(&loc_of_aloc(prop)));
                        }
                        result
                    }
                    ls => {
                        let compare = |a: &L, b: &L| loc_of_aloc(a).cmp(&loc_of_aloc(b));
                        let (mut locs, mut properties): (Vec<L>, Vec<L>) = (vec![], vec![]);
                        for annot_loc in ls {
                            match annot_loc {
                                AnnotLoc::Loc(l) => locs.push(l.dupe()),
                                AnnotLoc::Object { loc, props } => {
                                    locs.push(loc.dupe());
                                    properties.extend(props.iter().map(|p| p.dupe()));
                                }
                            }
                        }
                        locs.sort_by(&compare);
                        locs.dedup_by(|a, b| compare(a, b) == std::cmp::Ordering::Equal);
                        locs.truncate(10);
                        properties.sort_by(&compare);
                        properties.dedup_by(|a, b| compare(a, b) == std::cmp::Ordering::Equal);
                        let props: Vec<friendly::MessageFeature<Loc>> =
                            if !properties.is_empty() && properties.len() <= 5 {
                                let these = if properties.len() > 1 {
                                    text("these object properties")
                                } else {
                                    text("this object property")
                                };
                                let mut result = vec![text(" or to "), these];
                                for prop in &properties {
                                    result.push(friendly::no_desc_ref(&loc_of_aloc(prop)));
                                }
                                result
                            } else {
                                vec![]
                            };
                        let mut result = vec![text("these definitions")];
                        for l in &locs {
                            result.push(friendly::no_desc_ref(&loc_of_aloc(l)));
                        }
                        result.extend(props);
                        result
                    }
                };
                let friendly::Message(definition_desc) =
                    friendly::message_of_string(&definition_description(definition));
                let mut features = vec![text("Cannot compute a type for ")];
                features.extend(definition_desc);
                features.push(text(" because its definition includes references to "));
                features.push(itself);
                features.extend(tl_recur);
                features.push(text(". Please add an annotation to "));
                features.extend(annot_message);
                friendly::Message(features)
            }
            MessageDeprecatedBool => friendly::Message(vec![
                text("Deprecated type. Use "),
                code("boolean"),
                text(" instead."),
            ]),
            MessageDeprecatedTypeParamColonBound => friendly::Message(vec![
                text("Using "),
                code(":"),
                text(" for type parameter bounds is deprecated. Use "),
                code("extends"),
                text(" instead (e.g., "),
                code("<T extends Bound>"),
                text(")."),
            ]),
            MessageDocblockError(err) => {
                use super::intermediate_error_types::DocblockError;
                match err {
                    DocblockError::MultipleFlowAttributes => friendly::Message(vec![
                        text("Unexpected "),
                        code("@flow"),
                        text(" declaration. Only one per "),
                        text("file is allowed."),
                    ]),
                    DocblockError::InvalidFlowMode(s) => friendly::Message(vec![
                        code(&format!("@flow {}", s)),
                        text(" is not a valid "),
                        code("@flow"),
                        text(" mode. Valid ones are: "),
                        code("@flow"),
                        text(", "),
                        code("@flow strict"),
                        text(", and "),
                        code("@flow strict-local"),
                        text("."),
                    ]),
                    DocblockError::MultipleJSXAttributes => friendly::Message(vec![
                        text("Unexpected "),
                        code("@jsx"),
                        text(" declaration. Only one per "),
                        text("file is allowed."),
                    ]),
                    DocblockError::InvalidJSXAttribute(first_error) => {
                        let mut features = vec![
                            text("Invalid "),
                            code("@jsx"),
                            text(" declaration. Should have the form "),
                            code("@jsx LeftHandSideExpression"),
                            text(" with no spaces."),
                        ];
                        if let Some(err) = first_error {
                            features.push(text(&format!(" Parse error: {}.", err)));
                        }
                        friendly::Message(features)
                    }
                    DocblockError::MultipleJSXRuntimeAttributes => friendly::Message(vec![
                        text("Unexpected "),
                        code("@jsxRuntime"),
                        text(" declaration. Only one per "),
                        text("file is allowed."),
                    ]),
                    DocblockError::InvalidJSXRuntimeAttribute => friendly::Message(vec![
                        text("Invalid "),
                        code("@jsxRuntime"),
                        text(" declaration. The only supported values are "),
                        code("classic"),
                        text(" and "),
                        code("automatic"),
                        text("."),
                    ]),
                    DocblockError::InvalidSupportsPlatform(p) => friendly::Message(vec![
                        text("Invalid "),
                        code("@supportsPlatform"),
                        text(" declaration. "),
                        code(p),
                        text(" is not configured in "),
                        code("experimental.multi_platform.extensions"),
                        text(" in your flow config."),
                    ]),
                    DocblockError::DisallowedSupportsPlatform => friendly::Message(vec![
                        code("@supportsPlatform"),
                        text(" declaration is disallowed in platform specific files."),
                    ]),
                }
            }
            MessageDuplicateClassMember {
                name,
                static_,
                class_kind,
            } => {
                let static_str = if *static_ { "static " } else { "" };
                let (class_kind_str, item_name) = match class_kind {
                    super::intermediate_error_types::ClassKind::Class => ("class", "member"),
                    super::intermediate_error_types::ClassKind::Record => ("record", "property"),
                };
                let member_type = format!("{}{} {}", static_str, class_kind_str, item_name);
                let member_type = member_type
                    .chars()
                    .next()
                    .map(|c| c.to_ascii_uppercase().to_string())
                    .unwrap_or_default()
                    + &member_type[1..];
                friendly::Message(vec![
                    code(name.as_str()),
                    text(" has already been declared in this "),
                    text(class_kind_str),
                    text(". "),
                    text(&member_type),
                    text(" names must be unique."),
                ])
            }
            MessageDuplicateEnumMember {
                prev_use_loc,
                enum_,
            } => friendly::Message(vec![
                text(
                    "Invalid enum member initializer. Initializers need to be unique, but this one ",
                ),
                text("has already been used for a "),
                friendly::hardcoded_string_desc_ref("previous member", loc_of_aloc(prev_use_loc)),
                text(" of "),
                render_enum_reference(enum_),
                text("."),
            ]),
            MessageDuplicateModuleProvider(box MessageDuplicateModuleProviderData {
                module_name,
                provider,
                conflict,
            }) => {
                let provider_file = loc_of_aloc(provider).source.clone();
                let conflict_file = loc_of_aloc(conflict).source.clone();
                match (&provider_file, &conflict_file) {
                    (Some(pf), Some(cf))
                        if pf.check_suffix(flow_common::files::FLOW_EXT)
                            && cf.check_suffix(".js") =>
                    {
                        friendly::Message(vec![
                            text("This file is being illegally shadowed by the "),
                            friendly::hardcoded_string_desc_ref(
                                "js.flow file",
                                loc_of_aloc(provider),
                            ),
                            text(". This file can only be shadowed by a js.flow file "),
                            text("in the same directory with the same base name."),
                        ])
                    }
                    _ => friendly::Message(vec![
                        text("Duplicate module provider for "),
                        code(module_name),
                        text(". Change "),
                        text("either the name of this file or the "),
                        friendly::hardcoded_string_desc_ref(
                            "name of the current module provider",
                            loc_of_aloc(provider),
                        ),
                        text("."),
                    ]),
                }
            }
            MessageEnumsNotEnabled => friendly::Message(vec![
                text("Flow Enums are not enabled. "),
                text("You may opt-in to using enums by putting "),
                code("enums=true"),
                text(" into the "),
                code("[options]"),
                text(" section of your "),
                code(".flowconfig"),
                text("."),
            ]),
            MessageEnumConstNotSupported => friendly::Message(vec![
                code("const"),
                text(
                    " enums are not supported. Flow Enums are designed to allow for inlining, however the inlining itself needs to be part of the build system (whatever you use) rather than Flow itself.",
                ),
            ]),
            MessageEnumNonIdentifierMemberName {
                member_name,
                enum_,
            } => friendly::Message(vec![
                text("Enum member names must be identifiers, not string literals. "),
                code(member_name),
                text(" is not a valid member name in "),
                render_enum_reference(enum_),
                text("."),
            ]),
            MessageInvalidEnumMemberName {
                member_name,
                enum_,
            } => {
                let suggestion = {
                    let mut chars: Vec<char> = member_name.chars().collect();
                    if !chars.is_empty() {
                        chars[0] = chars[0].to_ascii_uppercase();
                    }
                    chars.into_iter().collect::<String>()
                };
                friendly::Message(vec![
                    text(
                        "Enum member names cannot start with lowercase 'a' through 'z'. Instead of using ",
                    ),
                    code(member_name),
                    text(", consider using "),
                    code(&suggestion),
                    text(", in "),
                    render_enum_reference(enum_),
                    text("."),
                ])
            }
            MessageEnumDuplicateMemberName(box MessageEnumDuplicateMemberNameData {
                member_name,
                prev_use_loc,
                enum_,
            }) => friendly::Message(vec![
                text("Enum member names need to be unique, but the name "),
                code(member_name),
                text(" has already been used for a "),
                friendly::hardcoded_string_desc_ref("previous member", loc_of_aloc(prev_use_loc)),
                text(" of "),
                render_enum_reference(enum_),
                text("."),
            ]),
            MessageEnumInconsistentMemberValues { enum_ } => friendly::Message(vec![
                render_enum_reference(enum_),
                text(
                    " has been specified with inconsistent member initializers. All members need to consistently either use no initializer, or have a literal (boolean, number, bigint, or string) initializer.",
                ),
            ]),
            MessageEnumInvalidMemberInitializer(box MessageEnumInvalidMemberInitializerData {
                member_name,
                explicit_type,
                enum_,
            }) => {
                use flow_parser::ast::statement::enum_declaration::ExplicitType;
                match explicit_type {
                    Some(ExplicitType::Symbol) => friendly::Message(vec![
                        text("Symbol enum members cannot be initialized. Use "),
                        code(&format!("{member_name},")),
                        text(" in "),
                        render_enum_reference(enum_),
                        text("."),
                    ]),
                    Some(t) => {
                        let type_str = t.as_str();
                        friendly::Message(vec![
                            text("The enum member initializer for "),
                            code(member_name),
                            text(" needs to be a "),
                            code(type_str),
                            text(" literal in "),
                            render_enum_reference(enum_),
                            text("."),
                        ])
                    }
                    None => friendly::Message(vec![
                        text("The enum member initializer for "),
                        code(member_name),
                        text(
                            " needs to be a literal (either a boolean, number, bigint, or string) in ",
                        ),
                        render_enum_reference(enum_),
                        text("."),
                    ]),
                }
            }
            MessageEnumBooleanMemberNotInitialized {
                member_name,
                enum_,
            } => friendly::Message(vec![
                text("The enum member "),
                code(member_name),
                text(" of boolean "),
                render_enum_reference(enum_),
                text(
                    " has been left uninitialized. Boolean enum members need to be initialized, e.g. ",
                ),
                code(&format!("{member_name} = true,")),
                text("."),
            ]),
            MessageEnumNumberMemberNotInitialized {
                member_name,
                enum_,
            } => friendly::Message(vec![
                text("The enum member "),
                code(member_name),
                text(" of number "),
                render_enum_reference(enum_),
                text(
                    " has been left uninitialized. Number enum members need to be initialized, e.g. ",
                ),
                code(&format!("{member_name} = 1,")),
                text("."),
            ]),
            MessageEnumBigIntMemberNotInitialized {
                member_name,
                enum_,
            } => friendly::Message(vec![
                text("The enum member "),
                code(member_name),
                text(" of bigint "),
                render_enum_reference(enum_),
                text(
                    " has been left uninitialized. BigInt enum members need to be initialized, e.g. ",
                ),
                code(&format!("{member_name} = 1n,")),
                text("."),
            ]),
            MessageEnumStringMemberInconsistentlyInitialized { enum_ } => {
                friendly::Message(vec![
                    text("String "),
                    render_enum_reference(enum_),
                    text(
                        " has been specified with inconsistent member initializers. Either all members need a string literal initializer, or none.",
                    ),
                ])
            }
            MessageTSEnumInvalidMember {
                member_name,
                enum_,
                kind,
            } => match kind {
                super::intermediate_error_types::TsEnumInvalidMemberKind::TSEnumMemberInvalidLiteral => {
                    friendly::Message(vec![
                        text("The enum member initializer for "),
                        code(member_name),
                        text(" in "),
                        render_enum_reference(enum_),
                        text(
                            " must be a number or string literal, the only member types TypeScript enums allow.",
                        ),
                    ])
                }
                super::intermediate_error_types::TsEnumInvalidMemberKind::TSEnumMemberMissingInitializer => {
                    friendly::Message(vec![
                        text("The enum member "),
                        code(member_name),
                        text(" in "),
                        render_enum_reference(enum_),
                        text(
                            " must have an initializer because the preceding member is not a numeric constant, so it cannot be auto-numbered.",
                        ),
                    ])
                }
                super::intermediate_error_types::TsEnumInvalidMemberKind::TSEnumMemberNumericName => {
                    friendly::Message(vec![
                        text("The enum member "),
                        code(member_name),
                        text(" in "),
                        render_enum_reference(enum_),
                        text(" cannot have a numeric name, which TypeScript does not allow."),
                    ])
                }
            },
            MessageTSEnumInvalidSyntax { enum_, kind } => match kind {
                super::intermediate_error_types::TsEnumInvalidSyntaxKind::TSEnumUnknownMembers => {
                    friendly::Message(vec![
                        render_enum_reference(enum_),
                        text(
                            " cannot have unknown members (`...`), which is a Flow Enums feature that TypeScript enums do not support.",
                        ),
                    ])
                }
                super::intermediate_error_types::TsEnumInvalidSyntaxKind::TSEnumExplicitType => {
                    friendly::Message(vec![
                        render_enum_reference(enum_),
                        text(
                            " cannot have an explicit representation type (`of ...`), which is a Flow Enums feature that TypeScript enums do not support.",
                        ),
                    ])
                }
            },
            MessageExponentialSpread(box MessageExponentialSpreadData {
                spread_loc,
                operand1,
                operand2,
            }) => {
                use super::intermediate_error_types::ExponentialSpreadOperandGroup;
                let format_operand_group =
                    |ExponentialSpreadOperandGroup { first, second }: &ExponentialSpreadOperandGroup<
                        MessageTypeReferenceData<L>,
                    >|
                     -> Vec<friendly::MessageFeature<Loc>> {
                        match second {
                            None => vec![ref_of_type_desc(&first.loc, &first.desc)],
                            Some(second) => vec![
                                text("inferred union from "),
                                ref_of_type_desc(&first.loc, &first.desc),
                                text(" | "),
                                ref_of_type_desc(&second.loc, &second.desc),
                            ],
                        }
                    };
                let mut union_refs = format_operand_group(operand1);
                union_refs.push(text(" and "));
                union_refs.extend(format_operand_group(operand2));
                let mut features = vec![
                    text("Computing the "),
                    hardcoded_string_desc_ref("spread", spread_loc),
                    text(
                        " may lead to an exponentially large number of cases to reason about because ",
                    ),
                ];
                features.extend(union_refs);
                features.push(text(" are both unions, and spreading multiple unions produces one result for every combination of their members."));
                features.push(text(" Please use at most one union type per spread to simplify reasoning about the spread result."));
                features.push(text(" You may be able to get rid of a union by specifying a more general type that captures all of the branches of the union."));
                friendly::Message(features)
            }
            MessageExportValueAsType(name) => friendly::Message(vec![
                text("Cannot export the value "),
                code(name.as_str()),
                text(" as a type."),
            ]),
            MessageImportTypeAsTypeof(export_name) => {
                let (prefix, export) = msg_export("the type ", export_name);
                let mut features = vec![text("Cannot import ")];
                features.push(prefix);
                features.push(export);
                features.extend(vec![
                    text(" as a type. "),
                    code("import typeof"),
                    text(" only works on value exports like variables, "),
                    text("functions, and classes. If you intended to import a type use "),
                    code("import type"),
                    text(" instead."),
                ]);
                friendly::Message(features)
            }
            MessageImportTypeAsValue(export_name) => {
                let (prefix, export) = msg_export("the type ", export_name);
                let mut features = vec![text("Cannot import ")];
                features.push(prefix);
                features.push(export);
                features.extend(vec![
                    text(" as a value. "),
                    text("Use "),
                    code("import type"),
                    text(" instead."),
                ]);
                friendly::Message(features)
            }
            MessageImportValueAsType(export_name) => {
                let (prefix, export) = msg_export("the value ", export_name);
                let mut features = vec![text("Cannot import ")];
                features.push(prefix);
                features.push(export);
                features.extend(vec![
                    text(" as a type. "),
                    code("import type"),
                    text(" only works on type exports like type aliases, "),
                    text("interfaces, and classes. If you intended to import the type of a "),
                    text("value use "),
                    code("import typeof"),
                    text(" instead."),
                ]);
                friendly::Message(features)
            }
            MessageIncompatibleGeneralWithPrintedTypes(
                box MessageIncompatibleGeneralWithPrintedTypesData {
                    lower_loc,
                    upper_loc,
                    lower_desc,
                    upper_desc,
                },
            ) => friendly::Message(vec![
                ref_of_type_desc(lower_loc, lower_desc),
                text(" is incompatible with "),
                ref_of_type_desc(upper_loc, upper_desc),
            ]),
            MessageIncompatibleMappedTypeKey {
                source_type,
                mapped_type,
            } => friendly::Message(vec![
                ref_of_type(source_type),
                text(" is incompatible with "),
                code("string | number | symbol"),
                text(", so it cannot be used to generate keys for "),
                ref_of_type(mapped_type),
            ]),
            MessageIncompatibleTupleArity(box MessageIncompatibleTupleArityData {
                lower,
                lower_arity,
                lower_inexact,
                lower_rest,
                upper,
                upper_arity,
                upper_inexact,
                upper_rest,
                ..
            }) => {
                let str_of_arity =
                    |inexact: bool, rest: bool, (num_req, num_total): &(i32, i32)| {
                        if rest {
                            return format!("{} or more elements", num_req);
                        }
                        let suffix = if inexact {
                            "or more elements (inexact tuple)"
                        } else {
                            "elements"
                        };
                        if num_req == num_total {
                            if *num_total == 1 && !inexact {
                                format!("{} element", num_total)
                            } else {
                                format!("{} {}", num_total, suffix)
                            }
                        } else {
                            format!("{}-{} {}", num_req, num_total, suffix)
                        }
                    };
                friendly::Message(vec![
                    ref_of_type_desc(&lower.loc, &lower.desc),
                    text(" has "),
                    text(&str_of_arity(*lower_inexact, *lower_rest, lower_arity)),
                    text(" but "),
                    ref_of_type_desc(&upper.loc, &upper.desc),
                    text(" has "),
                    text(&str_of_arity(*upper_inexact, *upper_rest, upper_arity)),
                ])
            }
            MessageIncompatibleClassToObject {
                lower,
                upper,
                kind,
            } => {
                let kind_str = match kind {
                    super::intermediate_error_types::ClassKind::Class => "Class instances",
                    super::intermediate_error_types::ClassKind::Record => "Records",
                };
                friendly::Message(vec![
                    ref_of_type_desc(&lower.loc, &lower.desc),
                    text(" is not a subtype of "),
                    ref_of_type_desc(&upper.loc, &upper.desc),
                    text(". "),
                    text(kind_str),
                    text(" are not subtypes of object types; consider rewriting "),
                    ref_of_type_desc(&upper.loc, &upper.desc),
                    text(" as an interface"),
                ])
            }
            MessageIncompatibleNonLiteralArrayToTuple { lower, upper } => friendly::Message(vec![
                ref_of_type_desc(&lower.loc, &lower.desc),
                text(" has an unknown number of elements, so is "),
                text("incompatible with "),
                ref_of_type_desc(&upper.loc, &upper.desc),
            ]),
            MessageIncompatibleNonTypeGuardToTypeGuard { lower, upper } => {
                let reference = |reference: &TypeGuardFuncReference<
                    L,
                    MessageTypeReferenceData<L>,
                >| match reference {
                    TypeGuardFuncReference::Type(reference) => {
                        ref_of_type_desc(&reference.loc, &reference.desc)
                    }
                    TypeGuardFuncReference::TypeGuard(reference) => type_guard_reference(reference),
                };
                friendly::Message(vec![
                    reference(lower),
                    text(", a non-type-guard function, is incompatible with "),
                    reference(upper),
                    text(", which is a type-guard function"),
                ])
            }
            MessageIncompatibleReactHooksDueToUniqueness { lower, upper } => {
                friendly::Message(vec![
                    ref_of_type_desc(&lower.loc, &lower.desc),
                    text(" and "),
                    ref_of_type_desc(&upper.loc, &upper.desc),
                    text(" are different React hooks"),
                ])
            }
            MessageIncompatibleWithIndexed { lower, upper } => friendly::Message(vec![
                ref_of_type_desc(&lower.loc, &lower.desc),
                text(" is incompatible with indexed "),
                ref_of_type_desc(&upper.loc, &upper.desc),
            ]),
            MessageIncompleteExhausiveCheckEnum(box MessageIncompleteExhausiveCheckEnumData {
                description_name,
                enum_,
                left_to_check,
                default_case_loc,
            }) => {
                let left_to_check_features: Vec<friendly::MessageFeature<Loc>> =
                    if left_to_check.len() == 1 {
                        vec![
                            text("the member "),
                            code(left_to_check[0].as_str()),
                            text(" of enum "),
                            render_named_reference(enum_),
                            text(" has"),
                        ]
                    } else {
                        let number_to_check = left_to_check.len();
                        let members_features: Vec<friendly::MessageFeature<Loc>> =
                            if number_to_check > 5 {
                                let max_display_amount = 4;
                                let mut features: Vec<friendly::MessageFeature<Loc>> =
                                    left_to_check
                                        .iter()
                                        .take(max_display_amount)
                                        .flat_map(|member| vec![code(member.as_str()), text(", ")])
                                        .collect();
                                features.push(text(&format!(
                                    "and {} others",
                                    number_to_check - max_display_amount
                                )));
                                features
                            } else {
                                let member_msgs: Vec<friendly::Message<Loc>> = left_to_check
                                    .iter()
                                    .map(|member| friendly::Message(vec![code(member.as_str())]))
                                    .collect();
                                friendly::conjunction_concat(member_msgs, "and", None).0
                            };
                        let mut features = vec![text("the members ")];
                        features.extend(members_features);
                        features.extend(vec![
                            text(" of enum "),
                            render_named_reference(enum_),
                            text(" have"),
                        ]);
                        features
                    };
                let default_features: Vec<friendly::MessageFeature<Loc>> = match default_case_loc {
                    Some(default_reason) => {
                        use flow_lint_settings::lints::LintKind;
                        vec![
                            text(" The "),
                            friendly::hardcoded_string_desc_ref(
                                "default case",
                                loc_of_aloc(default_reason),
                            ),
                            text(" does not check for the missing members as the "),
                            code(LintKind::RequireExplicitEnumSwitchCases.as_str()),
                            text(" lint has been enabled."),
                        ]
                    }
                    None => vec![],
                };
                let mut features = vec![text("Incomplete exhaustive check: ")];
                features.extend(left_to_check_features);
                features.extend(vec![
                    text(" not been considered in check of "),
                    code(description_name.as_ref().unwrap_or(&enum_.name)),
                    text("."),
                ]);
                features.extend(default_features);
                friendly::Message(features)
            }
            MessageIncorrectType(kind) => {
                let incorrect_name = kind.incorrect_of_kind();
                let replacement_name = kind.replacement_of_kind();
                match kind.error_type_of_kind() {
                    super::intermediate_error_types::IncorrectTypeErrorType::DeprecatedUtility => {
                        let mut features = vec![
                            text("The utility type "),
                            code(incorrect_name),
                            text(" is deprecated, use "),
                            code(replacement_name),
                            text(" instead."),
                        ];
                        if matches!(
                            *kind,
                            super::intermediate_error_types::IncorrectType::DollarKeys
                        ) {
                            features
                                .push(text(" For example, `type KeyType = keyof MyObjectType;` "));
                        }
                        friendly::Message(features)
                    }
                    super::intermediate_error_types::IncorrectTypeErrorType::TSType => {
                        friendly::Message(vec![
                            text("The equivalent of TypeScript's "),
                            code(incorrect_name),
                            text(" type in Flow is "),
                            code(replacement_name),
                            text("."),
                        ])
                    }
                }
            }
            MessageInternalType(kind) => {
                use super::intermediate_error_types::InternalType;
                match kind {
                    InternalType::DollarReactDeepReadOnly => friendly::Message(vec![
                        code("$ReactDeepReadOnly"),
                        text(" is a secret internal Flow type exposed for testing purposes. "),
                        text("There will be no stability guarantees."),
                    ]),
                    InternalType::DollarUtilityTypeWithNonDollarAliases(name) => {
                        friendly::Message(vec![
                            code(&format!("${}", name)),
                            text(" is an internal Flow type. Use "),
                            code(name),
                            text(" instead."),
                        ])
                    }
                    InternalType::ReactDollarUtilityTypesWithNonDollarAliases(name) => {
                        friendly::Message(vec![
                            code(&format!("React${}", name)),
                            text(" is an internal Flow type. Use "),
                            code(&format!("React.{}", name)),
                            text(" instead."),
                        ])
                    }
                }
            }
            MessageInvalidCatchParameterAnnotation { .. } => {
                friendly::Message(vec![
                    text("Invalid catch parameter type annotation. "),
                    text("Annotation must be "),
                    code("any"),
                    text(" or "),
                    code("unknown"),
                    text(" if specified."),
                ])
            }
            MessageInvalidComponentRestParam => friendly::Message(vec![text(
                "You may only use an identifier or a destructured object as a component rest param.",
            )]),
            MessageInvalidGenericRef(typename) => friendly::Message(vec![
                text("Cannot compare the result of "),
                code("typeof"),
                text(" to string "),
                text("literal "),
                code(typename),
                text(" because it is not a valid "),
                code("typeof"),
                text(" return value."),
            ]),
            MessageInvalidEnumMemberCheck(box MessageInvalidEnumMemberCheckData {
                enum_,
                example_member,
                from_match,
            }) => {
                let suggestion = match &enum_.name {
                    Some(enum_name) => {
                        let example_member =
                            example_member.as_ref().map(|s| s.as_str()).unwrap_or("A");
                        let (prefix, suffix) = if *from_match {
                            ("", "")
                        } else {
                            ("case ", ":")
                        };
                        vec![
                            text(" "),
                            text("For example "),
                            code(&format!(
                                "{}{}.{}{}",
                                prefix, enum_name, example_member, suffix
                            )),
                            text("."),
                        ]
                    }
                    None => vec![],
                };
                let at = if *from_match { "match pattern" } else { "case" };
                let mut features = vec![
                    text(&format!("Invalid enum member check at {}. ", at)),
                    text("The format must be dot-access of a member of "),
                    render_enum_value_reference(enum_),
                    text("."),
                ];
                features.extend(suggestion);
                friendly::Message(features)
            }
            MessageInvalidGraphQL(kind) => {
                use flow_parser_utils::graphql::GraphqlError;
                match kind {
                    GraphqlError::InvalidTaggedTemplate => friendly::Message(vec![text(
                        "Template literal substitutions are not allowed in GraphQL literals.",
                    )]),
                    GraphqlError::InvalidGraphQL => friendly::Message(vec![text(
                        "Expected a GraphQL fragment, query, mutation, or subscription.",
                    )]),
                }
            }
            MessageInvalidHookNaming => friendly::Message(vec![
                text("Hooks must have names that begin with "),
                code("use"),
                text(". "),
                text("(https://react.dev/reference/rules/rules-of-hooks)"),
            ]),
            MessageInvalidImportStarUse(import_star_loc) => friendly::Message(vec![
                text("The default export of a module cannot be accessed from an "),
                hardcoded_string_desc_ref("`import *`", import_star_loc),
                text(" object. To use the default export you must import it directly."),
            ]),
            MessageInvalidMappedTypeInInterfaceOrDeclaredClass => friendly::Message(vec![text(
                "Mapped Types are not supported in interfaces or declared classes.",
            )]),
            MessageInvalidMappedTypeWithExactOrInexact => friendly::Message(vec![
                text(
                    "Mapped Types take on the exactness of the argument passed to keyof. They do not ",
                ),
                text("support explicit exact or inexact syntax."),
            ]),
            MessageInvalidMappedTypeWithExtraProps => friendly::Message(vec![text(
                "Mapped Types cannot be used when other properties or indexers are present.",
            )]),
            MessageInvalidMappedTypeWithVarianceOnArrayInput => friendly::Message(vec![text(
                "Mapped Types do not yet support write-only variance annotations on array inputs.",
            )]),
            MessageInvalidTemplateLiteralTypeComplexity => friendly::Message(vec![text(
                "Template literal type produces a union type that is too complex to represent.",
            )]),
            MessageInvalidTemplateLiteralTypePlaceholder => friendly::Message(vec![
                text("Template literal type placeholder must be a subtype of "),
                code("string | number | bigint | boolean | null | undefined"),
                text("."),
            ]),
            MessageInvalidInferType => friendly::Message(vec![
                text("Invalid infer type declaration. "),
                code("infer"),
                text(" declarations are only permitted in the "),
                code("extends"),
                text(" clause of a conditional type."),
            ]),
            MessageInvalidLintSettings(kind) => {
                use flow_lint_settings::lint_settings::LintParseError;
                match kind {
                    LintParseError::RedundantArgument => friendly::Message(vec![text(
                        "Redundant argument. This argument doesn't change any lint settings.",
                    )]),
                    LintParseError::OverwrittenArgument => friendly::Message(vec![
                        text("Redundant argument. The values set by this argument are "),
                        text("overwritten later in this comment."),
                    ]),
                    LintParseError::NakedComment => friendly::Message(vec![text(
                        "Malformed lint rule. At least one argument is required.",
                    )]),
                    LintParseError::NonexistentRule => friendly::Message(vec![
                        text("Nonexistent/misspelled lint rule. Perhaps you have a "),
                        text("missing/extra "),
                        code(","),
                        text("?"),
                    ]),
                    LintParseError::InvalidSetting => friendly::Message(vec![text(
                        "Invalid setting. Valid settings are error, warn, and off.",
                    )]),
                    LintParseError::MalformedArgument => friendly::Message(vec![
                        text("Malformed lint rule. Properly formed rules contain a single "),
                        code(":"),
                        text(" character. Perhaps you have a missing/extra "),
                        code(","),
                        text("?"),
                    ]),
                }
            }
            MessageInvalidReactCreateElement(react) => friendly::Message(vec![
                text("Cannot create react element because the "),
                code("createElement"),
                text(" property of "),
                render_named_reference(react),
                text(" is incompatible with builtin "),
                code("React.createElement"),
                text(" type. "),
                text("Please check the "),
                render_named_reference(react),
                text(" identifier in scope to ensure it is the right one."),
            ]),
            MessageInvalidThisArgMissingReceiver {
                name,
                callee_expression,
            } => friendly::Message(vec![
                text("Cannot use "),
                code(name.as_str()),
                text(" because "),
                render_expression_reference(callee_expression),
                text(" is not a member access. "),
                code(&format!("Function.prototype.{}", name)),
                text(" may only re-supply the receiver a method was read from, as in "),
                code(&format!("obj.method.{}(obj, ...)", name)),
                text("."),
                text(" See https://flow.org/en/docs/types/classes/#toc-this-rebinding"),
            ]),
            MessageInvalidThisArgReceiverMismatch {
                name,
                receiver_expression,
            } => friendly::Message(vec![
                text("Cannot use "),
                code(name.as_str()),
                text(" because its first argument must be the exact same value as the receiver "),
                render_expression_reference(receiver_expression),
                text(" the method was read from. "),
                code(&format!("Function.prototype.{}", name)),
                text(" may not rebind a method to a different receiver."),
                text(" See https://flow.org/en/docs/types/classes/#toc-this-rebinding"),
            ]),
            MessageInvalidKeyPropertyInSpread(box MessageInvalidKeyPropertyInSpreadData {
                key_loc,
                spread_loc,
            }) => friendly::Message(vec![
                text("Cannot "),
                friendly::hardcoded_string_desc_ref("spread", loc_of_aloc(spread_loc)),
                text(" an object that contains a "),
                friendly::hardcoded_string_desc_ref("`key` property", loc_of_aloc(key_loc)),
            ]),
            MessageInvalidSelfReferencingTypeAnnotation(
                box MessageInvalidSelfReferencingTypeAnnotationData { name, loc },
            ) => friendly::Message(vec![
                text("Invalid type annotation for "),
                code(name.as_str()),
                text(". It contains a "),
                friendly::hardcoded_string_desc_ref("reference", loc_of_aloc(loc)),
                text(" to the binding being declared."),
            ]),
            MessageInvalidSelfReferencingDefault(
                box MessageInvalidSelfReferencingDefaultData {
                    name,
                    def_loc,
                    ref_loc,
                },
            ) => friendly::Message(vec![
                text("Invalid default expression for parameter "),
                friendly::hardcoded_string_desc_ref(name.as_str(), loc_of_aloc(def_loc)),
                text(". It contains a "),
                friendly::hardcoded_string_desc_ref("reference", loc_of_aloc(ref_loc)),
                text(" to the binding being declared."),
            ]),
            MessageInvalidTrivialRecursiveDefinition => {
                friendly::Message(vec![text("Invalid trivially recursive definition of this type. ")])
            }
            MessageInvalidTrivialRecursiveTypeParameter(name) => friendly::Message(vec![
                text("Invalid trivially recursive definition of "),
                code(name),
                text(". "),
            ]),
            MessageClassTypeParamInStatic(name) => friendly::Message(vec![
                text("Static members cannot reference class type parameter "),
                code(name),
                text("."),
            ]),
            MessageInvalidTupleRequiredAfterOptional {
                tuple_loc,
                required,
                optional,
            } => {
                let tuple_element_ref = |element: &TupleElementReferenceData<L>| {
                    let description = element.name.as_ref().map_or_else(
                        || "tuple element".to_string(),
                        |name| format!("tuple element (labeled '{name}')"),
                    );
                    friendly::hardcoded_string_desc_ref(
                        &description,
                        loc_of_aloc(&element.loc),
                    )
                };
                friendly::Message(vec![
                    text("Invalid "),
                    friendly::hardcoded_string_desc_ref("tuple type", loc_of_aloc(tuple_loc)),
                    text(", required "),
                    tuple_element_ref(required),
                    text(" must be after optional "),
                    tuple_element_ref(optional),
                    text("."),
                ])
            }
            MessageInvalidTupleTypeSpread(reason_arg) => friendly::Message(vec![
                text("Cannot spread non-tuple ("),
                ref_of_type_desc(&reason_arg.loc, &reason_arg.desc),
                text(") into tuple type."),
            ]),
            MessageTupleElementAfterInexactSpread => friendly::Message(vec![text(
                "Cannot have element after spread of inexact tuple.",
            )]),
            MessageTupleElementAfterRestSpread => friendly::Message(vec![text(
                "Cannot have element after spread of tuple with a rest element.",
            )]),
            MessageInvalidRendersTypeArgument(box MessageInvalidRendersTypeArgumentData {
                renders_variant,
                invalid_render_type_kind,
                invalid_types,
            }) => {
                use flow_parser::ast::types::RendersVariant;

                use super::intermediate_error_types::InvalidRenderTypeKind;
                let additional_explanation: Vec<friendly::MessageFeature<Loc>> = match (
                    invalid_render_type_kind,
                    renders_variant,
                ) {
                    (InvalidRenderTypeKind::InvalidRendersStructural(r), _) => vec![
                        text(" You can only use an element of "),
                        code("AbstractComponent"),
                        text(" when the third type argument is a render type and "),
                        ref_of_type_desc(&r.loc, &r.desc),
                        text(" is not a render type."),
                    ],
                    (InvalidRenderTypeKind::InvalidRendersNonNominalElement(r), _) => vec![
                        text(
                            " Only elements of a component-syntax components can appear in renders but ",
                        ),
                        ref_of_type_desc(&r.loc, &r.desc),
                        text(" is not a component-syntax component."),
                    ],
                    (InvalidRenderTypeKind::InvalidRendersNullVoidFalse, RendersVariant::Maybe) => {
                        vec![
                            text(
                                " Only elements of a component-syntax components can appear in renders. ",
                            ),
                            code("renders?"),
                            text(" already includes React nodes that render nothing."),
                        ]
                    }
                    (InvalidRenderTypeKind::InvalidRendersNullVoidFalse, RendersVariant::Star) => {
                        vec![
                            text(
                                " Only elements of a component-syntax components can appear in renders. ",
                            ),
                            code("renders*"),
                            text(" already includes React nodes that render nothing."),
                        ]
                    }
                    (InvalidRenderTypeKind::InvalidRendersIterable, RendersVariant::Star) => {
                        vec![
                            text(
                                " Only elements of a component-syntax components can appear in renders. ",
                            ),
                            code("renders*"),
                            text(
                                " already models rendering any amount of children in all possible nesting structures.",
                            ),
                        ]
                    }
                    (InvalidRenderTypeKind::InvalidRendersNullVoidFalse, _) => vec![
                        text(
                            " Only elements of a component-syntax components can appear in renders. ",
                        ),
                        text(
                            "If you want to express the idea of rendering zero or one item, please use ",
                        ),
                        code("renders?"),
                        text(" instead."),
                    ],
                    (InvalidRenderTypeKind::InvalidRendersIterable, _) => vec![
                        text(
                            " Only elements of a component-syntax components can appear in renders. ",
                        ),
                        text(
                            "If you want to express the idea of rendering zero or more items, please use ",
                        ),
                        code("renders*"),
                        text(" instead."),
                    ],
                    (InvalidRenderTypeKind::InvalidRendersGenericT, RendersVariant::Star) => {
                        vec![
                            text(" Generic render types are not allowed with "),
                            code("renders*"),
                            text("."),
                        ]
                    }
                    (InvalidRenderTypeKind::InvalidRendersGenericT, _) => vec![
                        text(" Generic render types are only allowed in rendering declaration of "),
                        text(
                            "component syntax components, and only one or a union of generic types are permitted.",
                        ),
                    ],
                    (InvalidRenderTypeKind::UncategorizedInvalidRenders, _) => vec![],
                };
                let refs = |types: &Vec1<MessageTypeReferenceData<L>>| {
                    let len = types.len();
                    let mut result = Vec::new();
                    for (i, type_) in types.iter().enumerate() {
                        if i > 0 {
                            if i == len - 1 {
                                result.push(text(" and "));
                            } else {
                                result.push(text(", "));
                            }
                        }
                        result.push(ref_of_type_desc(&type_.loc, &type_.desc));
                    }
                    result
                };
                let mut features = vec![text("Cannot use ")];
                features.extend(refs(invalid_types));
                features.push(text(" as the type argument of renders type."));
                features.extend(additional_explanation);
                friendly::Message(features)
            }
            MessageInvalidTypeCastingSyntax => {
                use super::error_message::type_casting_examples;
                let (valid, invalid) = type_casting_examples();
                friendly::Message(vec![
                    text("Invalid type cast syntax. Use the form "),
                    code(valid),
                    text(" instead of the form "),
                    code(invalid),
                    text("."),
                ])
            }
            MessageInvalidTypeGuardFunctionKind(kind) => friendly::Message(vec![
                text("Cannot declare a type guard on a(n) "),
                text(kind),
                text(" function."),
            ]),
            MessageInvalidTypeGuardFunctionWritten {
                type_guard,
                write_locs,
            } => {
                let loc_str: Vec<friendly::MessageFeature<Loc>> = match write_locs.as_slice() {
                    [] => vec![text("in this function")],
                    [loc] => vec![text("in"), friendly::no_desc_ref(&loc_of_aloc(loc))],
                    _ => {
                        let mut v = vec![text("in the following statements:")];
                        for loc in write_locs {
                            v.push(friendly::no_desc_ref(&loc_of_aloc(loc)));
                        }
                        v
                    }
                };
                let mut features = vec![
                    text("Cannot use "),
                    hardcoded_string_desc_ref(
                        &format!("type guard parameter `{}`", type_guard.name),
                        &type_guard.loc,
                    ),
                    text(" because at this return point it is written to "),
                ];
                features.extend(loc_str);
                features.push(text("."));
                friendly::Message(features)
            }
            MessageInvalidTypeGuardParamUnbound(type_guard) => friendly::Message(vec![
                text("Cannot find "),
                type_guard_parameter_reference(type_guard),
                text(" in the parameters of this function (type)."),
            ]),
            MessageInvalidTypeGuardThisParam(loc) => friendly::Message(vec![
                text("Cannot use "),
                hardcoded_string_desc_ref("this", loc),
                text(" as a type guard variable in this context. "),
                code("this"),
                text(" type guards are only supported in non-static class or interface methods."),
            ]),
            MessageInvalidUseOfFlowEnforceOptimized(arg) => friendly::Message(vec![
                text("Invalid use of $Flow$EnforceOptimized on non-union type "),
                ref_of_type_desc(&arg.loc, &arg.desc),
                text("."),
            ]),
            MessageMissingAnnotation(d) => friendly::Message(vec![
                text("Missing an annotation on "),
                friendly::desc_of_reason_desc(d.as_ref()),
                text("."),
            ]),
            MessageMissingThisAnnotation => friendly::Message(vec![
                text("Missing an annotation on "),
                code("this"),
                text("."),
            ]),
            MessageMissingReturnAnnotation => friendly::Message(vec![
                text("Missing an annotation on "),
                text("return"),
                text("."),
            ]),
            MessageMissingReturnAnnotationForGenericFunction => friendly::Message(vec![
                text("Missing an annotation on "),
                text("return"),
                text(" because generic functions must be fully annotated."),
            ]),
            MessageMissingParameterAnnotation => {
                friendly::Message(vec![text("Missing an annotation on this parameter.")])
            }
            MessageMissingParameterAnnotationDueToContextualTypingFailure => {
                friendly::Message(vec![text(
                    "An annotation on this parameter is required because Flow cannot infer its type from local context.",
                )])
            }
            MessageMissingAnnotationDueToContextualTypingFailure(d) => friendly::Message(vec![
                text("An annotation on "),
                friendly::desc_of_reason_desc(d.as_ref()),
                text(" is required because Flow cannot infer its type from local context."),
            ]),
            MessageMissingAnnotationForGenericFunction(d) => friendly::Message(vec![
                text("Missing an annotation on "),
                friendly::desc_of_reason_desc(d.as_ref()),
                text(" because generic functions must be fully annotated."),
            ]),
            MessageLowerIsNotReactComponent(lower) => {
                friendly::Message(vec![
                    ref_of_type_desc(&lower.loc, &lower.desc),
                    text(" is not a React component"),
                ])
            }
            MessageLowerIsNotWithPrintedType {
                lower,
                requirement,
            } => {
                let suffix = match requirement {
                    LowerRequirement::Array => " is not an array",
                    LowerRequirement::ArrayIndex => " is not an array index",
                    LowerRequirement::Class => " is not a class",
                    LowerRequirement::ClassWithPrivateProperties => {
                        " is not a class with private properties"
                    }
                    LowerRequirement::Function => " is not a function",
                    LowerRequirement::FunctionType => " is not a function type",
                    LowerRequirement::Inheritable => " is not inheritable",
                    LowerRequirement::InstanceType => " is not an instance type",
                    LowerRequirement::Object => " is not an object",
                    LowerRequirement::ObjectOrArrayOrTuple => " is not an object, array, or tuple",
                    LowerRequirement::PolymorphicType => " is not a polymorphic type",
                };
                friendly::Message(vec![
                    ref_of_type_desc(&lower.loc, &lower.desc),
                    text(suffix),
                ])
            }
            MessageLowerIsNotSupportedByUnclassifiedUseWithPrintedType { lower, ctor } => {
                friendly::Message(vec![
                    ref_of_type_desc(&lower.loc, &lower.desc),
                    text(" is not supported by unclassified use "),
                    text(ctor),
                ])
            }
            MessageLowerDoesNotHavePropertiesWithPrintedType(lower) => friendly::Message(vec![
                ref_of_type_desc(&lower.loc, &lower.desc),
                text(" does not have properties"),
            ]),
            MessageNoDefaultExport(box MessageNoDefaultExportData {
                module_name,
                suggestion,
            }) => {
                let mut features = vec![
                    text("Cannot import a default export because there is no default export "),
                    text("in "),
                    code(module_name),
                    text("."),
                ];
                if let Some(s) = suggestion {
                    features.extend(vec![
                        text(" "),
                        text("Did you mean "),
                        code(&format!("import {{{}}} from \"{}\"", s, module_name)),
                        text("?"),
                    ]);
                }
                friendly::Message(features)
            }
            MessageNoNamedExport(box MessageNoNamedExportData {
                module_name,
                export_name,
                suggestion,
            }) => {
                let mut features = vec![
                    text("Cannot import "),
                    code(export_name),
                    text(" because "),
                    text("there is no "),
                    code(export_name),
                    text(" export in "),
                    code(module_name),
                    text("."),
                ];
                if let Some(s) = suggestion {
                    features.extend(vec![text(" Did you mean "), code(s), text("?")]);
                }
                friendly::Message(features)
            }
            MessageNonConstVarExport(declaration) => {
                let declaration_part = match declaration {
                    Some(declaration) => vec![text("variable "), render_named_reference(declaration)],
                    None => vec![text("variable")],
                };
                let mut features = vec![text("Cannot export ")];
                features.extend(declaration_part);
                features.extend(vec![
                    text(" declared using "),
                    code("var"),
                    text(" or "),
                    code("let"),
                    text(". All exported variables must be "),
                    code("const"),
                    text("."),
                ]);
                friendly::Message(features)
            }
            MessageNonStrictImport => friendly::Message(vec![
                text("Dependencies of a "),
                code("@flow strict"),
                text(" module must "),
                text("also be "),
                code("@flow strict"),
                text("!"),
            ]),
            MessageNonToplevelExport => {
                friendly::Message(vec![text("Exports can only appear at the top level")])
            }
            MessageOnlyDefaultExport(box MessageOnlyDefaultExportData {
                module_name,
                export_name,
            }) => friendly::Message(vec![
                text("Cannot import "),
                code(export_name),
                text(" because "),
                text("there is no "),
                code(export_name),
                text(" export in "),
                code(module_name),
                text(". Did you mean "),
                code(&format!("import {} from \"...\"", export_name)),
                text("?"),
            ]),
            MessagePlatformSpecificImplementationModuleLookupFailed(name) => {
                friendly::Message(vec![
                    text("Cannot resolve platform-specific implementation module "),
                    code(name),
                    text(". "),
                    text("All platform-specific implementations must exist for this interface. "),
                    text("Read the docs on Flow's multi-platform support for more information: "),
                    text("https://flow.org/en/docs/react/multiplatform"),
                ])
            }
            MessagePropMissingWithPrintedType {
                lower,
                upper,
                prop,
                suggestion,
            } => {
                use super::error_message::mk_prop_message;
                let mut features = mk_prop_message(prop.as_deref());
                if let Some(s) = suggestion {
                    features.extend(vec![text(" (did you mean "), code(s), text("?)")]);
                }
                features.extend(vec![
                    text(" is missing in "),
                    ref_of_type_desc(&lower.loc, &lower.desc),
                ]);
                if let Some(upper) = upper {
                    features.extend(vec![
                        text(" but exists in "),
                        ref_of_type_desc(&upper.loc, &upper.desc),
                    ]);
                }
                friendly::Message(features)
            }
            MessagePrivatePropMissing { object, prop } => {
                use super::error_message::mk_prop_message;
                let mut features = mk_prop_message(Some(prop.as_str()));
                features.extend(vec![
                    text(" is missing in "),
                    ref_of_type_desc(&object.loc, &object.desc),
                ]);
                friendly::Message(features)
            }
            MessageIndexerCheckFailed(box MessageIndexerCheckFailedData {
                prop,
                lower,
                upper,
                indexer,
            }) => {
                use super::error_message::mk_prop_message;
                let mut features = mk_prop_message(Some(prop.as_str()));
                features.extend(vec![
                    text(" is missing in "),
                    ref_of_type_desc(&lower.loc, &lower.desc),
                    text(" but exists in "),
                    ref_of_type_desc(&upper.loc, &upper.desc),
                    text(". Any property that does not exist in "),
                    ref_of_type_desc(&lower.loc, &lower.desc),
                    text(" must be compatible with its indexer "),
                    ref_of_type_desc(&indexer.loc, &indexer.desc),
                ]);
                friendly::Message(features)
            }
            MessageConstructSignatureMissing(box MessageConstructSignatureMissingData {
                lower,
                upper,
            }) => friendly::Message(vec![
                text(
                    "a construct signature declaring the expected parameter / return type is missing in ",
                ),
                ref_of_type_desc(&lower.loc, &lower.desc),
                text(" but exists in "),
                ref_of_type_desc(&upper.loc, &upper.desc),
            ]),
            MessagePropsMissingWithPrintedType(box MessagePropsMissingWithPrintedTypeData {
                lower,
                upper,
                props,
            }) => {
                let lower = ref_of_type_desc(&lower.loc, &lower.desc);
                let upper = ref_of_type_desc(&upper.loc, &upper.desc);
                let (first_prop, rest_props) = props.clone().split_off_first();
                if rest_props.is_empty() {
                    friendly::Message(vec![
                        text("property "),
                        code(&first_prop),
                        text(" is missing in "),
                        lower,
                        text(" but exists in "),
                        upper,
                    ])
                } else {
                    let all_props: Vec<_> = std::iter::once(first_prop).chain(rest_props).collect();
                    let max_props = 10;
                    let num_props = all_props.len();
                    let (displayed, truncated_count) = if num_props > max_props {
                        (&all_props[..max_props], num_props - max_props)
                    } else {
                        (&all_props[..], 0)
                    };
                    let mut features = vec![text("properties ")];
                    let prop_msgs: Vec<_> = displayed
                        .iter()
                        .map(|p| friendly::Message(vec![code(p.as_str())]))
                        .collect();
                    let friendly::Message(concat_result) =
                        friendly::conjunction_concat(prop_msgs, "and", None);
                    features.extend(concat_result);
                    if truncated_count > 0 {
                        features.push(text(&format!(", ... ({} more)", truncated_count)));
                    }
                    features.extend(vec![
                        text(" are missing in "),
                        lower,
                        text(" but exist in "),
                        upper,
                    ]);
                    friendly::Message(features)
                }
            }
            MessagePropPolarityMismatch(box MessagePropPolarityMismatchData {
                lower,
                upper,
                props,
            }) => {
                use super::error_message::mk_prop_message;
                use super::error_message::polarity_explanation;
                let reference = |object: &PropPolarityMismatchObject<
                    MessageTypeReferenceData<L>,
                    L,
                >| match object {
                    PropPolarityMismatchObject::Type(reference) => {
                        ref_of_type_desc(&reference.loc, &reference.desc)
                    }
                    PropPolarityMismatchObject::Class(class) => render_named_reference(class),
                };
                let (first_prop, rest_props) = props.clone().split_off_first();
                let all_props: Vec<_> = std::iter::once(first_prop).chain(rest_props).collect();
                let mut sorted_props = all_props.clone();
                sorted_props.sort_by(|(a, _, _), (b, _, _)| a.cmp(b));
                let number_to_check = sorted_props.len();
                let props_msgs: Vec<friendly::Message<Loc>> = if number_to_check > 5 {
                    let max_display = 4;
                    let cropped: Vec<_> = sorted_props.into_iter().take(max_display).collect();
                    let mut msgs: Vec<_> = cropped
                        .into_iter()
                        .map(|(prop, lpole, upole)| {
                            let expected = polarity_explanation(lpole, upole);
                            let actual = polarity_explanation(upole, lpole);
                            let mut f = mk_prop_message(prop.as_deref());
                            f.extend(vec![
                                text(" is "),
                                text(expected),
                                text(" in "),
                                reference(lower),
                                text(" but "),
                                text(actual),
                                text(" in "),
                                reference(upper),
                            ]);
                            friendly::Message(f)
                        })
                        .collect();
                    msgs.push(friendly::Message(vec![text(&format!(
                        "{} other properties are incompatible",
                        number_to_check - max_display
                    ))]));
                    msgs
                } else {
                    sorted_props
                        .into_iter()
                        .map(|(prop, lpole, upole)| {
                            let expected = polarity_explanation(lpole, upole);
                            let actual = polarity_explanation(upole, lpole);
                            let mut f = mk_prop_message(prop.as_deref());
                            f.extend(vec![
                                text(" is "),
                                text(expected),
                                text(" in "),
                                reference(lower),
                                text(" but "),
                                text(actual),
                                text(" in "),
                                reference(upper),
                            ]);
                            friendly::Message(f)
                        })
                        .collect()
                };
                friendly::conjunction_concat(props_msgs, "and", None)
            }
            MessagePropNotReadable(x) => {
                use super::error_message::mk_prop_message;
                let prop = x.as_ref().map(|n| n.display_smol_str().to_string());
                let mut features = mk_prop_message(prop.as_deref());
                features.push(text(" is not readable"));
                friendly::Message(features)
            }
            MessagePropNotWritable(x) => {
                use super::error_message::mk_prop_message;
                let prop = x.as_ref().map(|n| n.display_smol_str().to_string());
                let mut features = mk_prop_message(prop.as_deref());
                features.push(text(" is not writable"));
                friendly::Message(features)
            }
            MessageReactIntrinsicOverlap(box MessageReactIntrinsicOverlapData {
                use_,
                def,
                type_,
                mixed,
            }) => friendly::Message(vec![
                text("The name of intrinsic element "),
                hardcoded_string_desc_ref(&format!("`{}`", use_.name), &use_.loc),
                text(" overlaps with a "),
                friendly::hardcoded_string_desc_ref("local definition", loc_of_aloc(def)),
                text(" which has a "),
                friendly::hardcoded_string_desc_ref("type", loc_of_aloc(type_)),
                text(" that "),
                if *mixed { text("may") } else { text("can") },
                text(
                    " be instantiated as an element. To avoid confusion between this definition and the intrinsic, rename the definition",
                ),
            ]),
            MessageReadonlyArraysCannotBeWrittenTo => {
                friendly::Message(vec![text("read-only arrays cannot be written to")])
            }
            MessageRecursionLimitExceeded => {
                friendly::Message(vec![text("*** Recursion limit exceeded ***")])
            }
            MessageRedeclareComponentProp(box MessageRedeclareComponentPropData {
                duplicates,
                spread_loc,
            }) => {
                let ((first_loc, name, second_loc), rest) = duplicates.clone().split_off_first();
                if rest.is_empty() {
                    friendly::Message(vec![
                        text("Component property "),
                        hardcoded_string_desc_ref(
                            &format!("`{}`", name.display_smol_str()),
                            &first_loc,
                        ),
                        text(" is "),
                        friendly::hardcoded_string_desc_ref(
                            "re-declared",
                            loc_of_aloc(&second_loc),
                        ),
                        text(" within a "),
                        friendly::hardcoded_string_desc_ref("spread", loc_of_aloc(spread_loc)),
                        text(
                            ". Property names may only be have one definition within a component.",
                        ),
                    ])
                } else {
                    let all_dupes: Vec<_> = std::iter::once((first_loc, name, second_loc))
                        .chain(rest)
                        .collect();
                    let mut features = vec![
                        text("Multiple component properties are re-declared within a "),
                        friendly::hardcoded_string_desc_ref("spread", loc_of_aloc(spread_loc)),
                        text(".\n"),
                    ];
                    for (first_loc, name, second_loc) in all_dupes {
                        features.extend(vec![
                            text(" - "),
                            hardcoded_string_desc_ref(
                                &format!("`{}`", name.display_smol_str()),
                                &first_loc,
                            ),
                            text(" is re-declared "),
                            friendly::hardcoded_string_desc_ref("here", loc_of_aloc(&second_loc)),
                            text("\n"),
                        ]);
                    }
                    features.push(text(
                        "Property names may only be have one definition within a component.",
                    ));
                    friendly::Message(features)
                }
            }
            MessageShouldAnnotateVariableOnlyInitializedInGenericContext {
                declaration,
                possible_generic_escape_locs,
            } => {
                let mut features = vec![
                    text("Variable "),
                    render_named_reference(declaration),
                    text(
                        " should be annotated, because it is only initialized in a generic context",
                    ),
                ];
                let locs: Vec<_> = possible_generic_escape_locs
                    .iter()
                    .map(|l| friendly::no_desc_ref(&loc_of_aloc(l)))
                    .collect();
                for (i, loc_ref) in locs.into_iter().enumerate() {
                    if i > 0 {
                        features.push(text(","));
                    }
                    features.push(loc_ref);
                }
                friendly::Message(features)
            }
            MessageShouldAnnotateVariableUsedInGenericContext(
                box MessageShouldAnnotateVariableUsedInGenericContextData {
                    declaration,
                    null_loc,
                    initialized,
                    possible_generic_escape_locs,
                },
            ) => {
                let null_ref = if *initialized {
                    code("null")
                } else {
                    null_literal_reference(null_loc)
                };
                let mut features = vec![
                    text("Variable "),
                    render_named_reference(declaration),
                    text(" should be annotated, because it is only ever assigned to by "),
                    null_ref,
                    text(" and in generic context"),
                ];
                let locs: Vec<_> = possible_generic_escape_locs
                    .iter()
                    .map(|l| friendly::no_desc_ref(&loc_of_aloc(l)))
                    .collect();
                for (i, loc_ref) in locs.into_iter().enumerate() {
                    if i > 0 {
                        features.push(text(","));
                    }
                    features.push(loc_ref);
                }
                friendly::Message(features)
            }
            MessageShouldNotBeCoerced(lower) => {
                friendly::Message(vec![ref_(lower), text(" should not be coerced")])
            }
            MessageShouldUseArrayLiteral => friendly::Message(vec![
                text("Use an array literal instead of "),
                code("new Array(...)"),
                text("."),
            ]),
            MessageSketchyNumber(value) => friendly::Message(vec![
                text("Avoid using "),
                code("&&"),
                text(" to check the value of "),
                ref_of_type_desc(&value.loc, &value.desc),
                text(". "),
                text(
                    "Consider handling falsy values (0 and NaN) by using a conditional to choose an ",
                ),
                text("explicit default instead."),
            ]),
            MessageSketchyNullCheck(box MessageSketchyNullCheckData {
                kind,
                falsy_loc,
                null_loc,
            }) => {
                use flow_lint_settings::lints::SketchyNullKind;
                let (type_str, value_str) = match kind {
                    SketchyNullKind::Bool => ("boolean", "false"),
                    SketchyNullKind::Number => ("number", "0"),
                    SketchyNullKind::BigInt => ("bigint", "0n"),
                    SketchyNullKind::String => ("string", "an empty string"),
                    SketchyNullKind::Mixed => ("unknown", "false"),
                    SketchyNullKind::EnumBool => ("boolean enum", "false at runtime"),
                    SketchyNullKind::EnumNumber => ("number enum", "0 at runtime"),
                    SketchyNullKind::EnumBigInt => ("bigint enum", "0n at runtime"),
                    SketchyNullKind::EnumString => ("string enum", "an empty string at runtime"),
                };
                friendly::Message(vec![
                    text("Sketchy null check on "),
                    friendly::hardcoded_string_desc_ref(type_str, loc_of_aloc(falsy_loc)),
                    text(" "),
                    text("which is potentially "),
                    text(value_str),
                    text(". Perhaps you meant to "),
                    text("check for "),
                    hardcoded_string_desc_ref("null or undefined", null_loc),
                    text("?"),
                ])
            }
            MessageSuppressionMalformedCode => friendly::Message(vec![
                text("Suppression contains a malformed error code. Suppressions with error codes "),
                text("should be formatted as "),
                code("$FlowFixMe[<CODE>]"),
                text("."),
            ]),
            MessageSuppressionMissingCode => friendly::Message(vec![
                text(
                    "Suppression is missing a code. Please update this suppression to use an error code: ",
                ),
                code("$FlowFixMe[<CODE>]"),
            ]),
            MessageThisInComponent(component_loc) => friendly::Message(vec![
                text("Cannot reference "),
                code("this"),
                text(" from within "),
                friendly::hardcoded_string_desc_ref(
                    "component declaration",
                    loc_of_aloc(component_loc),
                ),
            ]),
            MessageThisInExportedFunction => friendly::Message(vec![
                text("Cannot use "),
                code("this"),
                text(" in an exported function."),
            ]),
            MessageThisSuperInObject {
                method_loc,
                method_name,
                kind,
            } => {
                use super::intermediate_error_types::ThisFinderKind;
                let (v, suggestion) = match kind {
                    ThisFinderKind::This => (
                        "this",
                        vec![
                            text(" Consider replacing the reference to "),
                            code("this"),
                            text(
                                " with the name of the object, or rewriting the object as a class.",
                            ),
                        ],
                    ),
                    ThisFinderKind::Super => (
                        "super",
                        vec![text(" Consider rewriting the object as a class.")],
                    ),
                };
                let method_description = match method_name {
                    Some(name) => format!("method `{name}`"),
                    None => "computed method".to_string(),
                };
                let mut features = vec![
                    text("Cannot reference "),
                    code(v),
                    text(" from within "),
                    hardcoded_string_desc_ref(&method_description, method_loc),
                    text(". For safety, Flow restricts access to "),
                    code(v),
                    text(" inside object methods since these methods may be unbound and rebound."),
                ];
                features.extend(suggestion);
                friendly::Message(features)
            }
            MessageTSNeverType => friendly::Message(vec![
                text("The closest equivalent of TypeScript's "),
                code("never"),
                text(" type in Flow is "),
                code("empty"),
                text("."),
            ]),
            MessageTSReadonlyOperatorOnArray => friendly::Message(vec![
                text("The equivalent of TypeScript's "),
                code("readonly"),
                text(" type operator applied to an array type is "),
                code("ReadonlyArray<T>"),
                text("."),
            ]),
            MessageTSReadonlyOperatorOnTuple => friendly::Message(vec![
                text("The equivalent of TypeScript's "),
                code("readonly"),
                text(" type operator applied to a tuple type is "),
                code("Readonly<[T, S]>"),
                text("."),
            ]),
            MessageTSReadonlyType => friendly::Message(vec![
                text("TypeScript's "),
                code("readonly"),
                text(" type operator is not valid in Flow. "),
                text("For array types, you can use "),
                code("ReadonlyArray<T>"),
                text(". For object and tuple types you can use "),
                code("Readonly<T>"),
                text("."),
            ]),
            MessageTSClassAccessibility(kind) => {
                use flow_parser::ast::class::ts_accessibility::Kind;
                let (modifier, suffix) = match kind {
                    Kind::Private => (
                        "private",
                        vec![
                            text("Use JavaScript private elements instead. To fix, change "),
                            code("private foo"),
                            text(" to "),
                            code("#foo"),
                            text("."),
                        ],
                    ),
                    Kind::Public => (
                        "public",
                        vec![
                            text("Fields and methods are public by default. To fix, remove the "),
                            code("public"),
                            text(" modifier."),
                        ],
                    ),
                    Kind::Protected => (
                        "protected",
                        vec![
                            text("To fix, remove the "),
                            code("protected"),
                            text(" modifier."),
                        ],
                    ),
                };
                let mut parts = vec![
                    text("Flow does not support using "),
                    code(modifier),
                    text(" in classes. "),
                ];
                parts.extend(suffix);
                friendly::Message(parts)
            }
            MessageTSParameterProperty => friendly::Message(vec![
                text("Flow does not support TypeScript parameter properties. "),
                text(
                    "To fix, declare the property in the class body and assign it in the constructor.",
                ),
            ]),
            MessageAbstractClass => friendly::Message(vec![
                text("Flow does not support "),
                code("abstract"),
                text(" classes."),
            ]),
            MessageAbstractMethod => friendly::Message(vec![
                text("Flow does not support "),
                code("abstract"),
                text(" methods."),
            ]),
            MessageAbstract(kind) => {
                use crate::intermediate_error_types::AbstractErrorKind::*;
                match kind {
                    AbstractClassInstantiation => friendly::Message(vec![
                        text("Cannot instantiate an "),
                        code("abstract"),
                        text(" class."),
                    ]),
                    AbstractMemberNotImplemented {
                        class_name,
                        member_name,
                        member_def_loc,
                    } => {
                        let lead = match class_name {
                            Some(n) => {
                                vec![text("Non-"), code("abstract"), text(" class "), code(n)]
                            }
                            None => vec![text("Non-"), code("abstract"), text(" subclass")],
                        };
                        let mut parts = lead;
                        parts.extend(vec![
                            text(" does not implement inherited "),
                            code("abstract"),
                            text(" member "),
                            friendly::hardcoded_string_desc_ref(
                                member_name,
                                loc_of_aloc(member_def_loc),
                            ),
                            text("."),
                        ]);
                        friendly::Message(parts)
                    }
                    AbstractMemberOnNonAbstractClass { member_name } => friendly::Message(vec![
                        code("abstract"),
                        text(" member "),
                        code(member_name),
                        text(" can only appear within an "),
                        code("abstract"),
                        text(" class."),
                    ]),
                    AbstractPrivateMember { member_name } => friendly::Message(vec![
                        text("The "),
                        code("abstract"),
                        text(" modifier cannot be used with "),
                        code("private"),
                        text(" member "),
                        code(member_name),
                        text("."),
                    ]),
                    AbstractSuperCall { member_name } => friendly::Message(vec![
                        code("abstract"),
                        text(" member "),
                        code(member_name),
                        text(" cannot be accessed via "),
                        code("super"),
                        text("."),
                    ]),
                    AbstractConstructorAssignedToNonAbstract => friendly::Message(vec![
                        text("Cannot assign an "),
                        code("abstract"),
                        text(" constructor type to a non-"),
                        code("abstract"),
                        text(" constructor type."),
                    ]),
                }
            }
            MessageOverride(kind) => {
                use crate::intermediate_error_types::OverrideErrorKind::*;
                match kind {
                    OverrideWithoutExtends {
                        class_name,
                        member_name,
                    } => {
                        let containing = match class_name {
                            Some(n) => {
                                vec![text(" its containing class "), code(n), text(" doesn't")]
                            }
                            None => vec![text(" its containing class doesn't")],
                        };
                        let mut parts = vec![
                            text("Member "),
                            code(member_name),
                            text(" can't have an "),
                            code("override"),
                            text(" modifier because"),
                        ];
                        parts.extend(containing);
                        parts.push(text(" extend another class."));
                        friendly::Message(parts)
                    }
                    OverrideOfNonInheritedMember {
                        class_name: _,
                        base_class_name,
                        member_name,
                    } => {
                        let base = match base_class_name {
                            Some(n) => vec![text(" the base class "), code(n), text(".")],
                            None => vec![text(" the base class.")],
                        };
                        let mut parts = vec![
                            text("Member "),
                            code(member_name),
                            text(" can't have an "),
                            code("override"),
                            text(" modifier because it isn't declared in"),
                        ];
                        parts.extend(base);
                        friendly::Message(parts)
                    }
                    ImplicitOverrideMissingModifier {
                        class_name: _,
                        base_class_name,
                        member_name,
                        inherited_def_loc,
                    } => {
                        let base = match base_class_name {
                            Some(n) => vec![text(" the base class "), code(n)],
                            None => vec![text(" the base class")],
                        };
                        let mut parts = vec![
                            text("Member "),
                            code(member_name),
                            text(" must have an "),
                            code("override"),
                            text(" modifier because it overrides "),
                            friendly::hardcoded_string_desc_ref(
                                "a member",
                                loc_of_aloc(inherited_def_loc),
                            ),
                            text(" in"),
                        ];
                        parts.extend(base);
                        parts.push(text("."));
                        friendly::Message(parts)
                    }
                }
            }
            MessageTSVarianceInOut => friendly::Message(vec![
                text("The equivalent of TypeScript's "),
                code("in out"),
                text(" variance annotation in Flow is to simply leave it out - "),
                text("it's the default if you don't have a variance annotation."),
            ]),
            MessageDeprecatedVarianceSigil(sigil) => {
                use crate::intermediate_error_types::VarianceSigilKind;
                use crate::intermediate_error_types::VarianceSigilParent;
                let (sigil_str, replacement) = match sigil {
                    VarianceSigilKind::Plus(VarianceSigilParent::Property) => ("+", "readonly"),
                    VarianceSigilKind::Plus(VarianceSigilParent::TypeParam) => ("+", "out"),
                    VarianceSigilKind::Minus(VarianceSigilParent::Property) => ("-", "writeonly"),
                    VarianceSigilKind::Minus(VarianceSigilParent::TypeParam) => ("-", "in"),
                };
                friendly::Message(vec![
                    text("The "),
                    code(sigil_str),
                    text(" variance sigil is deprecated. Use "),
                    code(replacement),
                    text(" instead."),
                ])
            }
            MessageTSUndefinedType => friendly::Message(vec![
                text("The equivalent of TypeScript's "),
                code("undefined"),
                text(" type in Flow is "),
                code("void"),
                text(". "),
                text("Flow does not have separate "),
                code("void"),
                text(" and "),
                code("undefined"),
                text(" types."),
            ]),
            MessageUnclearType => friendly::Message(vec![
                text("Unclear type. Using "),
                code("any"),
                text(" is not safe!"),
            ]),
            MessageUnderconstrainedImplicitInstantiaton {
                call,
                type_param,
            } => friendly::Message(vec![
                render_named_reference(type_param),
                text(" is underconstrained by "),
                render_implicit_instantiation_reference(call),
                text(
                    ". Either add explicit type arguments or cast the expression to your expected type",
                ),
            ]),
            MessageUndocumentedFeature => friendly::Message(vec![
                text("You are using an undocumented feature. "),
                text("It might be removed in the future, "),
                text("and the behavior might change at any time without warning."),
            ]),
            MessageIllegalAssertOperator { obj, specialized } => {
                let explanation = if *specialized {
                    vec![text(
                        "The assert operator can only be used with arrays and objects with indexers.",
                    )]
                } else {
                    vec![text(
                        "The assert operator can only be applied to values with nullable types.",
                    )]
                };
                let mut features = vec![
                    render_expression_reference(obj),
                    text(" is not a valid target of the nonnull assertion operator ("),
                    code("!"),
                    text("). "),
                ];
                features.extend(explanation);
                friendly::Message(features)
            }
            MessageTupleElementNotReadable(box MessageTupleElementNotReadableData {
                index_def_loc,
                index,
                name,
            }) => {
                let index_ref = friendly::MessageFeature::Reference(
                    vec![friendly::MessageInline::Code(format!("{}", index))],
                    loc_of_aloc(index_def_loc),
                );
                let label = match name {
                    Some(n) => vec![text(" labeled "), code(n)],
                    None => vec![],
                };
                let mut features = vec![text("tuple element at index "), index_ref];
                features.extend(label);
                features.push(text(" is not readable"));
                friendly::Message(features)
            }
            MessageTupleElementNotWritable(box MessageTupleElementNotWritableData {
                index_def_loc,
                index,
                name,
            }) => {
                let index_ref = friendly::MessageFeature::Reference(
                    vec![friendly::MessageInline::Code(format!("{}", index))],
                    loc_of_aloc(index_def_loc),
                );
                let label = match name {
                    Some(n) => vec![text(" labeled "), code(n)],
                    None => vec![],
                };
                let mut features = vec![text("tuple element at index "), index_ref];
                features.extend(label);
                features.push(text(" is not writable"));
                friendly::Message(features)
            }
            MessageTupleIndexOutOfBound(box MessageTupleIndexOutOfBoundData {
                tuple,
                inexact,
                length,
                index,
            }) => {
                let plural = if *length == 1 { "" } else { "s" };
                let explicit = if *inexact { " explicitly defined" } else { "" };
                let bounds_msg = if *inexact {
                    "unknown or out of bounds"
                } else {
                    "out of bounds"
                };
                friendly::Message(vec![
                    ref_of_type_desc(&tuple.loc, &tuple.desc),
                    text(&format!(
                        " only has {} element{}{}, so index {} is {}",
                        length, plural, explicit, index, bounds_msg
                    )),
                ])
            }
            MessageTupleNonIntegerIndex(box MessageTupleNonIntegerIndexData {
                index_def_loc,
                index,
            }) => friendly::Message(vec![
                text("the index into a tuple must be an integer, but "),
                friendly::MessageFeature::Reference(
                    vec![friendly::MessageInline::Code(index.to_string())],
                    loc_of_aloc(index_def_loc),
                ),
                text(" is not an integer"),
            ]),
            MessageTupleNonStaticallyKnownIndex => friendly::Message(vec![text(
                "the index must be statically known to write a tuple element",
            )]),
            MessageTuplePolarityMismatch {
                index,
                lower,
                upper,
                polarity_lower,
                polarity_upper,
            } => {
                use super::error_message::polarity_explanation;
                let expected = polarity_explanation(*polarity_lower, *polarity_upper);
                let actual = polarity_explanation(*polarity_upper, *polarity_lower);
                friendly::Message(vec![
                    text("tuple element at index "),
                    code(&format!("{}", index)),
                    text(" is "),
                    text(expected),
                    text(" in "),
                    ref_of_type_desc(&lower.loc, &lower.desc),
                    text(" but "),
                    text(actual),
                    text(" in "),
                    ref_of_type_desc(&upper.loc, &upper.desc),
                ])
            }
            MessageTypeGuardIndexMismatch { lower, upper } => friendly::Message(vec![
                type_guard_parameter_reference(lower),
                text(" does not appear in the same position as "),
                type_guard_parameter_reference(upper),
            ]),
            MessageTypeGuardImpliesMismatch { lower, upper } => friendly::Message(vec![
                text("one-sided "),
                type_guard_reference(lower),
                text(" is incompatible with default "),
                type_guard_reference(upper),
            ]),
            MessageNegativeTypeGuardConsistency {
                return_desc,
                type_,
            } => friendly::Message(vec![
                text("Cannot return "),
                friendly::desc_of_reason_desc(return_desc.as_ref()),
                text(" because the negation of the predicate encoded in this expression"),
                text(" needs to completely refine away the guard type "),
                ref_of_type_desc(&type_.loc, &type_.desc),
                text(". "),
                text("Consider using a one-sided type-guard (`implies x is T`). "),
                text("See 2. in "),
                text(
                    "https://flow.org/en/docs/types/type-guards/#toc-consistency-checks-of-type-guard-functions.",
                ),
            ]),
            MessageAssertionFunctionFalsyAtExit {
                param,
                falsy,
                implicit_return,
            } => friendly::Message(vec![
                text(if *implicit_return {
                    "Cannot return from assertion function at the end of its body because "
                } else {
                    "Cannot return from assertion function because "
                }),
                hardcoded_string_desc_ref(&format!("`{}`", param.name), &param.loc),
                text(" may still be falsy here: it may be "),
                ref_of_type_desc(&falsy.loc, &falsy.desc),
                text(". Throw an error or call another assertion function on every path "),
                text("where it is falsy."),
            ]),
            MessageUnrecognizedAssertionCall(UnrecognizedAssertionCallee::UnsupportedTarget) => {
                friendly::Message(vec![
                    text("Cannot narrow with this call to an assertion function because the call "),
                    text("target is not an identifier or a dotted name such as "),
                    code("a.b.c"),
                    text("."),
                ])
            }
            MessageUnrecognizedAssertionCall(UnrecognizedAssertionCallee::UnannotatedRoot(
                root,
            )) => friendly::Message(vec![
                text("Cannot narrow with this call to an assertion function because "),
                hardcoded_string_desc_ref(&format!("`{}`", root.name), &root.loc),
                text(" is not declared with an explicit type annotation. Annotate "),
                code(root.name.as_str()),
                text(" or call the assertion function directly."),
            ]),
            MessageUnrecognizedAssertionCall(UnrecognizedAssertionCallee::UnsupportedType) => {
                friendly::Message(vec![
                    text("Cannot narrow with this call to an assertion function because its "),
                    text("declared type is not a single assertion function type. Overloaded "),
                    text("functions, intersections, and type parameters do not narrow."),
                ])
            }
            MessageUnexpectedUseOfThisType => friendly::Message(vec![
                text("Unexpected use of "),
                code("this"),
                text(" type."),
            ]),
            MessageUninitializedInstanceProperty(kind) => {
                use flow_lint_settings::lints::PropertyAssignmentKind;
                match kind {
                    PropertyAssignmentKind::MethodCallBeforeEverythingInitialized => {
                        friendly::Message(vec![
                            text("It is unsafe to call a method in the constructor before all "),
                            text("class properties are definitely initialized."),
                        ])
                    }
                    PropertyAssignmentKind::PropertyFunctionCallBeforeEverythingInitialized => {
                        friendly::Message(vec![
                            text("It is unsafe to call a property function in the constructor "),
                            text("before all class properties are definitely initialized."),
                        ])
                    }
                    PropertyAssignmentKind::PropertyNotDefinitelyInitialized => {
                        friendly::Message(vec![
                            text("Class property not definitely initialized in the constructor. "),
                            text("Can you add an assignment to the property declaration?"),
                        ])
                    }
                    PropertyAssignmentKind::ReadFromUninitializedProperty => {
                        friendly::Message(vec![
                            text("It is unsafe to read from a class property before it is "),
                            text("definitely initialized."),
                        ])
                    }
                    PropertyAssignmentKind::ThisBeforeEverythingInitialized => {
                        friendly::Message(vec![
                            text("It is unsafe to use "),
                            code("this"),
                            text(" in the constructor "),
                            text("before all class properties are definitely initialized."),
                        ])
                    }
                }
            }
            MessageUninitializedProperty { name, kind } => match kind {
                UninitializedPropertyKind::NotDefinitelyAssigned => friendly::Message(vec![
                    text("Property "),
                    code(name),
                    text(" has no initializer and is not definitely assigned in the constructor."),
                ]),
                UninitializedPropertyKind::UsedBeforeAssigned => friendly::Message(vec![
                    text("Property "),
                    code(name),
                    text(" is used before being assigned."),
                ]),
                UninitializedPropertyKind::UsedBeforeInitialization => friendly::Message(vec![
                    text("Property "),
                    code(name),
                    text(" is used before its initialization."),
                ]),
            },
            MessageUnknownParameterTypes(function_loc) => friendly::Message(vec![
                text("the parameter types of an "),
                hardcoded_string_desc_ref("unknown function", function_loc),
                text(" are unknown"),
            ]),
            MessageUnnecessaryDeclareTypeOnlyExport => friendly::Message(vec![
                text("The "),
                code("declare"),
                text(" keyword is unnecessary for type exports."),
            ]),
            MessageUnnecessaryInvariant {
                callee,
                condition,
                condition_kind,
            } => {
                let condition = match condition_kind {
                    UnnecessaryInvariantConditionKind::Type => {
                        ref_of_type_desc(&condition.loc, &condition.desc)
                    }
                    UnnecessaryInvariantConditionKind::IntersectionType => {
                        hardcoded_string_desc_ref("intersection type", &condition.loc)
                    }
                };
                friendly::Message(vec![
                    text(&format!("This use of `{callee}` is unnecessary because ")),
                    condition,
                    text(" is always truthy."),
                ])
            }
            MessageUnnecessaryOptionalChain { lhs_expression } => friendly::Message(vec![
                text("This use of optional chaining ("),
                code("?."),
                text(") is unnecessary because "),
                render_expression_reference(lhs_expression),
                text(" cannot be nullish or because an earlier "),
                code("?."),
                text(" will short-circuit the nullish case."),
            ]),
            MessageUnreachableCode => friendly::Message(vec![text("Unreachable code.")]),
            MessageUnsafeGetterSetter => friendly::Message(vec![text(
                "Getters and setters can have side effects and are unsafe.",
            )]),
            MessageUnsafeObjectAssign => friendly::Message(vec![
                text("Flow's support for "),
                code("Object.assign"),
                text(" is unsafe. Use spreads instead."),
            ]),
            MessageUnsupportedComputedKeyInClass => friendly::Message(vec![text(
                "Unsupported computed key. Computed keys are only supported on class fields and methods, and must resolve to a single string literal, number literal, or `unique symbol`.",
            )]),
            MessageUnsupportedKeyInObject {
                key_error_kind,
                obj_kind,
            } => {
                use super::intermediate_error_types::InvalidObjKey;
                use super::intermediate_error_types::ObjKind;
                let suffix = match key_error_kind {
                    InvalidObjKey::Other | InvalidObjKey::NullOrVoid => vec![text(
                        " Only identifier, string literal, and number literal keys are allowed.",
                    )],
                    InvalidObjKey::NumberNonInt => {
                        vec![text(" Only integer-like number literals are allowed.")]
                    }
                    InvalidObjKey::NumberTooLarge => vec![
                        text(" Number literals must not be larger than "),
                        code("Number.MAX_SAFE_INTEGER"),
                        text("."),
                    ],
                    InvalidObjKey::NumberTooSmall => vec![
                        text(" Number literals must not be smaller than "),
                        code("Number.MIN_SAFE_INTEGER"),
                        text("."),
                    ],
                    InvalidObjKey::ComputedNotLiteral => vec![
                        text(" A computed key must have a single string or number literal type."),
                        text(" To declare an index signature instead, label the key, as in "),
                        code("[key: K]"),
                        text("."),
                    ],
                    InvalidObjKey::ComputedTypeName => vec![
                        text(" This name refers to a type, but an unlabeled key is read as a"),
                        text(" value. To declare an index signature over that type, label the"),
                        text(" key, as in "),
                        code("[key: K]"),
                        text("."),
                    ],
                };
                let obj_kind_str = match obj_kind {
                    ObjKind::Type => "object type",
                    ObjKind::Literal => "object literal",
                    ObjKind::Interface => "interface",
                    ObjKind::DeclareClass => "declare class",
                };
                let mut features = vec![text("Unsupported key in "), text(obj_kind_str), text(".")];
                features.extend(suffix);
                friendly::Message(features)
            }
            MessageUnsupportedVarianceAnnotation(kind) => friendly::Message(vec![
                text("Variance modifiers cannot appear on a type parameter of a "),
                text(kind),
                text("."),
            ]),
            MessageUntypedImport(module_name) => friendly::Message(vec![
                text("Importing from an untyped module makes it "),
                code("any"),
                text(" "),
                text("and is not safe! Did you mean to add "),
                code("// @flow"),
                text(" "),
                text("to the top of "),
                code(module_name),
                text("?"),
            ]),
            MessageUntypedTypeImport(module_name) => friendly::Message(vec![
                text("Importing a type from an untyped module makes it "),
                code("any"),
                text(" "),
                text("and is not safe! Did you mean to add "),
                code("// @flow"),
                text(" to "),
                text("the top of "),
                code(module_name),
                text("?"),
            ]),
            MessageUnusedPromiseInAsyncScope => friendly::Message(vec![
                code("Promise"),
                text(" in async scope is unused. Did you mean to "),
                code("await"),
                text(" it?"),
            ]),
            MessageUnusedPromiseInSyncScope => friendly::Message(vec![
                code("Promise"),
                text(
                    " in sync scope is unused. Promises must be handled by calling .then with a rejection handler, .catch, or .finally.",
                ),
            ]),
            MessageUnusedSuppression => {
                friendly::Message(vec![text("Unused suppression comment.")])
            }
            MessageValueUsedAsType { reference, value } => friendly::Message(vec![
                text("Cannot use "),
                desc_of_value_as_type_reference(reference.as_ref(), value),
                text(" as a type. "),
                text("A name can be used as a type only if it refers to "),
                text("a type, interface, class, or enum definition. "),
                text("To get the type of a non-class value, use "),
                code("typeof"),
                text("."),
            ]),
            MessageVariableNeverInitAssignedAnnotated(declaration) => friendly::Message(vec![
                text("Variable "),
                render_named_reference(declaration),
                text(" is never initialized, annotated, or assigned to."),
            ]),
            MessageVariableOnlyAssignedByNull(box MessageVariableOnlyAssignedByNullData {
                declaration,
                null_loc,
            }) => {
                let null_ref = match null_loc {
                    Some(loc) => null_literal_reference(loc),
                    None => code("null"),
                };
                friendly::Message(vec![
                    text("Variable "),
                    render_named_reference(declaration),
                    text(" is only ever assigned to by "),
                    null_ref,
                    text(". This is likely unintended; if it is intended, annotate "),
                    code(declaration.name.as_str()),
                    text(" with "),
                    code(": null"),
                    text(" to disambiguate."),
                ])
            }
            // MessageUnsupportedSyntax variants
            MessageUnsupportedSyntax(syntax) => {
                use crate::intermediate_error_types::ContextDependentUnsupportedStatement::*;
                use crate::intermediate_error_types::DeclareClassPropKind;
                use crate::intermediate_error_types::NamespaceExportDeclarationKind;
                use crate::intermediate_error_types::TsLibSyntaxKind;
                use crate::intermediate_error_types::UnsupportedSyntax::*;
                match syntax {
                    AnnotationInsideDestructuring => friendly::Message(vec![
                        text("Annotations inside of destructuring are not supported. "),
                        text("Annotate the top-level pattern instead. "),
                        text("For example, instead of the invalid "),
                        code("const [a: number, b: string] = ..."),
                        text(" do "),
                        code("const [a, b]: [number, string] = ..."),
                        text("."),
                    ]),
                    AsConstOnNonLiteral => friendly::Message(vec![
                        text("The "),
                        code("as const"),
                        text(" assertion can only be used on string, numeric, boolean, object, "),
                        text("or array literals, "),
                        text("or const-variables initialized with primitive literals."),
                    ]),
                    CatchParameterDeclaration => {
                        friendly::Message(vec![text("Unsupported catch parameter declaration.")])
                    }
                    ClassPropertyLiteral => {
                        friendly::Message(vec![text("Literal properties not yet supported.")])
                    }
                    ClassStaticBlock => {
                        friendly::Message(vec![text("Class static blocks are not supported.")])
                    }
                    ClassDeclareMethod => friendly::Message(vec![text(
                        "Declare methods in classes are not supported.",
                    )]),
                    ClassIndexSignature => friendly::Message(vec![text(
                        "Index signatures in classes are not supported.",
                    )]),
                    ComponentSyntax => friendly::Message(vec![
                        text("Component syntax is not enabled. "),
                        text("You may opt-in to using component syntax by putting "),
                        code("component_syntax=true"),
                        text(" into the "),
                        code("[options]"),
                        text(" section of your "),
                        code(".flowconfig"),
                        text("."),
                    ]),
                    ContextDependentUnsupportedStatement(stmt) => match stmt {
                        NonLibdefToplevelDeclareModule => friendly::Message(vec![
                            code("declare module"),
                            text(" statement is only supported at the toplevel of a library file."),
                        ]),
                        ToplevelLibraryImport => friendly::Message(vec![
                            text(
                                "Cannot use an import statement at the toplevel of a library file. ",
                            ),
                            text("Import statements may only appear inside a "),
                            code("declare module"),
                            text(". The statement will be ignored."),
                        ]),
                        UnsupportedStatementInLibdef(kind) => friendly::Message(vec![
                            text("Cannot use "),
                            text(kind),
                            text(" statements in a library file. "),
                            text("The statement will be ignored."),
                        ]),
                        UnsupportedStatementInDeclareModule(kind) => friendly::Message(vec![
                            text("Cannot use "),
                            text(kind),
                            text(" statements with in "),
                            code("declare module"),
                            text(". The statement will be ignored."),
                        ]),
                        UnsupportedStatementInDeclareNamespace(kind) => friendly::Message(vec![
                            text("Cannot use "),
                            text(kind),
                            text(" statements with in "),
                            code("declare namespace"),
                            text(". The statement will be ignored."),
                        ]),
                        UnsupportedStatementInDeclareGlobal(kind) => friendly::Message(vec![
                            text("Cannot use "),
                            text(kind),
                            text(" statements within "),
                            code("declare global"),
                            text(". The statement will be ignored."),
                        ]),
                    },
                    DeclareGlobal => friendly::Message(vec![
                        code("declare global"),
                        text(" statement is not supported yet."),
                    ]),
                    DestructuringExpressionPattern => friendly::Message(vec![text(
                        "Unsupported expression pattern in destructuring.",
                    )]),
                    DestructuringObjectPropertyInvalidLiteral => friendly::Message(vec![text(
                        "Unsupported literal object property in destructuring. String literals and int-like number literals are supported.",
                    )]),
                    ExistsType => friendly::Message(vec![
                        text("The existential type "),
                        code("*"),
                        text(" is deprecated. This syntax is no longer supported."),
                    ]),
                    ExplicitCallAfterProto => friendly::Message(vec![text(
                        "Unexpected call property after explicit prototype.",
                    )]),
                    ExplicitProtoAfterCall => {
                        friendly::Message(vec![text("Unexpected prototype after call property.")])
                    }
                    IllegalName => friendly::Message(vec![text("Illegal name.")]),
                    UniqueSymbolNotConst => friendly::Message(vec![
                        text("A variable whose type is "),
                        code("unique symbol"),
                        text(" must be "),
                        code("const"),
                        text(
                            ". A binding that can be reassigned would let a different symbol \
                             take the place of the one this annotation names.",
                        ),
                    ]),
                    UniqueSymbolNotReadOnly => friendly::Message(vec![
                        text("A property whose type is "),
                        code("unique symbol"),
                        text(" must be "),
                        code("readonly"),
                        text(
                            ". A property that can be written would let a different symbol take \
                             the place of the one this annotation names.",
                        ),
                    ]),
                    UniqueSymbolNotStaticReadOnly => friendly::Message(vec![
                        text("A class property whose type is "),
                        code("unique symbol"),
                        text(" must be both "),
                        code("static"),
                        text(" and "),
                        code("readonly"),
                        text(
                            ". An instance property declares one member per instance, and a \
                             property that can be written would let a different symbol take \
                             the place of the one this annotation names.",
                        ),
                    ]),
                    UniqueSymbolLoopBinding => friendly::Message(vec![
                        text("A loop binding whose type is "),
                        code("unique symbol"),
                        text(
                            " takes a different value on each iteration, so it cannot name one \
                             symbol. Use ",
                        ),
                        code("symbol"),
                        text(" instead, or "),
                        code("typeof"),
                        text(" a binding declared with "),
                        code("unique symbol"),
                        text("."),
                    ]),
                    UniqueSymbolPosition => friendly::Message(vec![
                        code("unique symbol"),
                        text(" is only allowed as the type of a "),
                        code("const"),
                        text(" variable, of a "),
                        code("readonly"),
                        text(" property, or of a "),
                        code("static readonly"),
                        text(
                            " class field. Anywhere else one annotation would stand for every \
                             value written against it, so distinct symbols would be treated as \
                             the same property. Use ",
                        ),
                        code("symbol"),
                        text(" instead, or "),
                        code("typeof"),
                        text(" a binding declared with "),
                        code("unique symbol"),
                        text("."),
                    ]),
                    ImportDynamic => friendly::Message(vec![
                        text("Dynamic "),
                        code("import"),
                        text(" is not supported in this file."),
                    ]),
                    ImportDynamicArgument => friendly::Message(vec![
                        text("The parameter passed to "),
                        code("import"),
                        text(" must be a string literal."),
                    ]),
                    MatchExpression => friendly::Message(vec![
                        code("match"),
                        text(" expressions are not supported."),
                    ]),
                    MatchStatement => friendly::Message(vec![
                        code("match"),
                        text(" statements are not supported."),
                    ]),
                    MatchInstancePattern => friendly::Message(vec![
                        code("match"),
                        text(" instance patterns are not supported."),
                    ]),
                    MetaPropertyExpression => friendly::Message(vec![text("Not supported.")]),
                    MultipleIndexers => {
                        friendly::Message(vec![text("Multiple indexers are not supported.")])
                    }
                    MultipleProtos => {
                        friendly::Message(vec![text("Multiple prototypes specified.")])
                    }
                    ObjectPropertyGetSet => {
                        friendly::Message(vec![text("Get/set properties not yet supported.")])
                    }
                    ObjectPropertyComputedGetSet => friendly::Message(vec![text(
                        "Computed getters and setters are not yet supported.",
                    )]),
                    PredicateFunction => friendly::Message(vec![text(
                        "Support for predicate functions is removed. `%checks` declaration is now ignored.",
                    )]),
                    Records => friendly::Message(vec![text("Records are not enabled.")]),
                    DeclareClassMethodMissingReturnType => friendly::Message(vec![
                        text("Return type annotation is required for "),
                        code("declare class"),
                        text(" methods."),
                    ]),
                    DeclareVariableNonLiteralInit => friendly::Message(vec![
                        text("Initializer in a "),
                        code("declare"),
                        text(" variable must be a literal (string, number, bigint, or boolean)."),
                    ]),
                    DeclareVariableDestructuring => friendly::Message(vec![
                        text("Destructuring is not supported in "),
                        code("declare"),
                        text(" variable declarations."),
                    ]),
                    DeclareVariableMissingAnnotationOrInit => friendly::Message(vec![
                        code("declare"),
                        text(
                            " variable declarations require a type annotation or a literal initializer.",
                        ),
                    ]),
                    DeclareVariableAnnotationAndInit => friendly::Message(vec![
                        code("declare"),
                        text(
                            " variable declarations cannot have both a type annotation and an initializer.",
                        ),
                    ]),
                    DeclareClassProperty(kind) => {
                        use DeclareClassPropKind::*;
                        match kind {
                            AnnotationAndInit => friendly::Message(vec![
                                code("declare"),
                                text(
                                    " class properties cannot have both a type annotation and an initializer.",
                                ),
                            ]),
                            MissingAnnotationOrInit => friendly::Message(vec![
                                code("declare"),
                                text(
                                    " class properties require a type annotation or a literal initializer.",
                                ),
                            ]),
                            NonLiteralInit => friendly::Message(vec![
                                text("Initializer in a "),
                                code("declare"),
                                text(
                                    " class property must be a literal (string, number, bigint, or boolean).",
                                ),
                            ]),
                            InitWithoutReadonly => friendly::Message(vec![
                                text("Only "),
                                code("readonly"),
                                text(" properties in "),
                                code("declare"),
                                text(" class can have initializers."),
                            ]),
                        }
                    }
                    ExportTypeSpecifierInExportType => friendly::Message(vec![
                        text("The "),
                        code("type"),
                        text(" keyword on named exports can only be used on regular "),
                        code("export"),
                        text(" statements. It cannot be used with "),
                        code("export type"),
                        text(" statements."),
                    ]),
                    TSLibSyntax(crate::intermediate_error_types::TsLibSyntaxKind::MappedTypeKeyRemappingOnArraySource) => {
                        friendly::Message(vec![text("Key remapping (`as`) on tuple/array sources is not yet supported in mapped types.")])
                    }
                    TSLibSyntax(kind) => {
                        use TsLibSyntaxKind::*;
                        let kind_str = match kind {
                            DeclarationWithoutDeclare => "Declaration without `declare`",
                            ImportTypeAnnotation => "`import(\"module\")` type syntax",
                            DeclareExportNamespace => "`declare export namespace`",
                            ExportAssignment => "`export =` syntax",
                            ImportEqualsDeclaration => "`import ... = ...` syntax",
                            ImportEqualsQualifiedName => "`import ... = <QualifiedName>` syntax",
                            DeclareVariableMultipleDeclarators => {
                                "Multiple `declare` variable declarators"
                            }
                            DeclareVariableLiteralInit => {
                                "`declare` variable with literal initializer"
                            }
                            ExportTypeSpecifier => "Inline `type` modifier on export specifier",
                            TemplateLiteralType => "Template literal type",
                            ConstructorType => {
                                "Constructor type expression (`new (...) => T` / `abstract new (...) => T`)"
                            }
                            UniqueSymbolType => "`unique symbol` type",
                            TypeofImport => "`typeof import(\"module\")` syntax",
                            ImplementsDottedPath => "`implements` with dotted path",
                            OptionalClassProperty => "Optional class property (`?:`)",
                            OptionalShorthandMethod => "Optional method signature",
                            AnonymousDefaultExportFunction => {
                                "Anonymous default export function declaration"
                            }
                            MappedTypeKeyRemapping => "Key remapping (`as`) in mapped types",
                            MappedTypeKeyRemappingOnArraySource => unreachable!(),
                            ReadonlyMappedTypeVarianceOp => {
                                "`+readonly`/`-readonly` mapped type modifier"
                            }
                            OptionalUnlabeledTupleElement => "Optional unlabeled tuple element",
                            OptionalIndexer => "Optional indexer (`?:`)",
                            NamespaceExportDeclaration => "`export as namespace`",
                            PrivateClassField => "Private class field (`#private`)",
                            GenericTaggedTemplate => {
                                "Type arguments for tagged template expression"
                            }
                            TypeofThis => "`typeof this` syntax",
                            PropertyValueInitializer => "Property value initializer in declaration",
                            ClassExtendsCall => "Call expression in class `extends` clause",
                            OverrideModifier => "`override` modifier on class members",
                            Satisfies => "`satisfies` expression",
                        };
                        friendly::Message(vec![text(kind_str), text(" is not enabled.")])
                    }
                    NamespaceExportDeclaration(kind) => {
                        use NamespaceExportDeclarationKind::*;
                        let suffix = match kind {
                            NotTopLevel => " may only appear at the top level.",
                            NotInModule => " may only appear in module files.",
                            NotInDeclarationFile => " may only appear in declaration files.",
                            ModuleWithImports => {
                                " is not supported in a module with imports or re-exports. The global will not be declared."
                            }
                        };
                        friendly::Message(vec![code("export as namespace"), text(suffix)])
                    }
                    RequireDynamicArgument => friendly::Message(vec![
                        text("The parameter passed to "),
                        code("require"),
                        text(" must be a string literal."),
                    ]),
                    SpreadArgument => {
                        friendly::Message(vec![text("A spread argument is unsupported here.")])
                    }
                    UserDefinedTypeGuards { kind } => {
                        use flow_parser::ast::types::TypeGuardKind;
                        let kind_str = match kind {
                            TypeGuardKind::Default => "This kind of type guard is",
                            TypeGuardKind::Asserts => "Type guard assertions are",
                            TypeGuardKind::Implies => "One-sided type guards are",
                        };
                        friendly::Message(vec![text(&format!("{} not yet supported.", kind_str))])
                    }
                    UnsupportedInternalSlot { name, static_ } => {
                        if *static_ {
                            friendly::Message(vec![
                                text("Unsupported static internal slot "),
                                code(name),
                                text("."),
                            ])
                        } else {
                            friendly::Message(vec![
                                text("Unsupported internal slot "),
                                code(name),
                                text("."),
                            ])
                        }
                    }
                    ExportDefaultInterface => friendly::Message(vec![
                        text("Flow doesn't support "),
                        code("export default interface"),
                        text(" declarations."),
                    ]),
                    WithStatement => friendly::Message(vec![
                        text("Flow doesn't support "),
                        code("with"),
                        text(" statements."),
                    ]),
                    NonnullAssertion => friendly::Message(vec![
                        text("Flow doesn't support non-null assertions (the postfix "),
                        code("!"),
                        text(" operator)."),
                    ]),
                    AsyncComponentSyntax => {
                        friendly::Message(vec![text("Async component syntax is not supported.")])
                    }
                    AsyncHookSyntax => {
                        friendly::Message(vec![text("Async hook syntax is not supported.")])
                    }
                }
            }
            // MessageMatch* variants
            MessageMatchNotExhaustive { examples } => {
                let examples_msgs: Vec<friendly::Message<Loc>> = examples
                    .iter()
                    .map(|(pattern, reasons)| {
                        let reason_refs: Vec<friendly::Message<Loc>> = reasons
                            .iter()
                            .map(|reference| {
                                friendly::Message(vec![match reference {
                                    MatchExampleReference::Type(reference) => {
                                        ref_of_type_desc(&reference.loc, &reference.desc)
                                    }
                                    MatchExampleReference::EnumMember {
                                        loc,
                                        member_name,
                                        enum_name,
                                    } => hardcoded_string_desc_ref(
                                        &format!("member {member_name} of enum `{enum_name}`"),
                                        loc,
                                    ),
                                    MatchExampleReference::EnumUnknownMembers { loc, enum_name } => {
                                        hardcoded_string_desc_ref(
                                            &format!(
                                                "the unknown members of enum `{enum_name}` (specified using `...`)"
                                            ),
                                            loc,
                                        )
                                    }
                                }])
                            })
                            .collect();
                        let friendly::Message(concat_result) =
                            friendly::conjunction_concat(reason_refs, "and", Some(3));
                        let mut msg_features = vec![code(pattern), text(" to match ")];
                        msg_features.extend(concat_result);
                        friendly::Message(msg_features)
                    })
                    .collect();
                let result = match examples_msgs.len() {
                    0 => vec![],
                    1 => {
                        let friendly::Message(single_example) =
                            examples_msgs.into_iter().next().unwrap();
                        let mut result = vec![text("missing pattern: ")];
                        result.extend(single_example);
                        result.push(text("."));
                        result
                    }
                    _ => {
                        let size = examples_msgs.len();
                        let (limited_examples, suffix) = if size > 6 {
                            let taken: Vec<_> = examples_msgs.into_iter().take(5).collect();
                            (taken, vec![text(&format!("\nand {} others.", size - 5))])
                        } else {
                            (examples_msgs, vec![])
                        };
                        let mut result = vec![text("missing patterns:\n")];
                        for (i, friendly::Message(example)) in
                            limited_examples.into_iter().enumerate()
                        {
                            if i > 0 {
                                result.push(text("\n"));
                            }
                            result.push(text("- "));
                            result.extend(example);
                        }
                        result.extend(suffix);
                        result
                    }
                };
                let mut msg = vec![
                    code("match"),
                    text(" hasn't checked all possible cases of the input type. To fix, add the "),
                ];
                msg.extend(result);
                friendly::Message(msg)
            }
            MessageMatchUnnecessaryPattern {
                pattern,
                already_seen,
            } => {
                let detail = match already_seen {
                    Some(already_seen) => vec![
                        text("The values it matches were already covered by a previous "),
                        hardcoded_string_desc_ref(already_seen.kind.to_string(), &already_seen.loc),
                        text(". "),
                    ],
                    None => vec![text(
                        "The values it matches are either already covered by previous patterns, or are not part of the input type. ",
                    )],
                };
                let mut msg = vec![
                    text("This "),
                    hardcoded_string_desc_ref(pattern.kind.to_string(), &pattern.loc),
                    text(" is unused. "),
                ];
                msg.extend(detail);
                msg.push(text(
                    "To fix, either remove this pattern or restructure previous patterns.",
                ));
                friendly::Message(msg)
            }
            MessageMatchNonExhaustiveObjectPattern(
                box MessageMatchNonExhaustiveObjectPatternData {
                    rest,
                    missing_props,
                    pattern_kind,
                },
            ) => {
                let pattern_kind_str = pattern_kind.to_string();
                let prefix = vec![
                    text("This "),
                    text(pattern_kind_str),
                    text(" hasn't considered all possible properties of the input. "),
                ];
                let has_missing_props = !missing_props.is_empty();
                let (properties_text, those_properties_text) = if missing_props.len() > 1 {
                    ("properties", "those properties")
                } else {
                    ("property", "that property")
                };
                let missing_props_msgs: Vec<_> = if has_missing_props {
                    let prop_msgs: Vec<friendly::Message<Loc>> = missing_props
                        .iter()
                        .map(|prop| {
                            let prop =
                                if flow_parser::js_id_unicode::string_is_valid_identifier_name(prop)
                                {
                                    prop.to_string()
                                } else {
                                    let quote =
                                        flow_common_ty::ty_printer::better_quote(true, prop);
                                    let escaped =
                                        flow_common_ty::ty_printer::utf8_escape(&quote, prop);
                                    format!("{}{}{}", quote, escaped, quote)
                                };
                            friendly::Message(vec![code(&prop)])
                        })
                        .collect();
                    let friendly::Message(concat_result) =
                        friendly::conjunction_concat(prop_msgs, "and", Some(5));
                    let mut result = vec![text(&format!("The {} ", properties_text))];
                    result.extend(concat_result);
                    result.push(text(" are missing from the pattern"));
                    result
                } else {
                    vec![]
                };
                let suffix = match rest {
                    Some(rest) => {
                        let base_msg = vec![
                            text("could be additional properties due to "),
                            ref_of_type_desc(&rest.loc, &rest.desc),
                            text(". To fix, add "),
                        ];
                        if has_missing_props {
                            let mut result = missing_props_msgs;
                            result.push(text(", and there "));
                            result.extend(base_msg);
                            result
                        } else {
                            let mut result = vec![text("There ")];
                            result.extend(base_msg);
                            result
                        }
                    }
                    None => {
                        let mut result = missing_props_msgs;
                        result.push(text(&format!(
                            ". To fix, either add {} to the {}, or add ",
                            those_properties_text, pattern_kind_str
                        )));
                        result
                    }
                };
                let mut msg = prefix;
                msg.extend(suffix);
                msg.push(code("..."));
                msg.push(text(
                    " to the end of the pattern to match all other properties.",
                ));
                friendly::Message(msg)
            }
            MessageMatchNonExplicitEnumCheck(box MessageMatchNonExplicitEnumCheckData {
                wildcard_loc,
                unchecked_members,
            }) => {
                let member_msgs: Vec<friendly::Message<Loc>> = unchecked_members
                    .iter()
                    .map(|member| friendly::Message(vec![code(member)]))
                    .collect();
                let friendly::Message(concat_result) =
                    friendly::conjunction_concat(member_msgs, "and", Some(5));
                let member_word = if unchecked_members.len() > 1 {
                    "members "
                } else {
                    "member "
                };
                let mut msg = vec![
                    text("The "),
                    hardcoded_string_desc_ref("match wildcard", wildcard_loc),
                    text(" does not check for missing enum "),
                    text(member_word),
                ];
                msg.extend(concat_result);
                msg.extend(vec![
                    text(", because the "),
                    code("require-explicit-enum-checks"),
                    text(" lint has been enabled. "),
                    text("To fix, add cases covering those enum members."),
                ]);
                friendly::Message(msg)
            }
            MessageMatchInvalidGuardedWildcard => friendly::Message(vec![
                text("Cannot have a wildcard which is guarded using an "),
                code("if"),
                text(" in the last case of a "),
                code("match"),
                text(". "),
                text("Either reorder this case, add a final unguarded (no "),
                code("if"),
                text(") wildcard "),
                code("_"),
                text(" in a case after this one, or remove the pattern."),
            ]),
            MessageMatchInvalidIdentOrMemberPattern { type_ } => friendly::Message(vec![
                text("Cannot have "),
                ref_of_type_desc(&type_.loc, &type_.desc),
                text(" in a match pattern position. "),
                text("Valid types for match patterns include string literals, number literals, "),
                text("bigint literals, boolean literals, enum members, null, or undefined."),
            ]),
            MessageMatchInvalidBindingKind { kind } => friendly::Message(vec![
                text("Cannot use "),
                code(kind.as_str()),
                text(" for match pattern binding. Only "),
                code("const"),
                text(" is allowed."),
            ]),
            MessageMatchInvalidObjectPropertyLiteral { pattern_kind } => friendly::Message(vec![
                text("Unsupported "),
                text(pattern_kind.to_string()),
                text(" property literal in match pattern. "),
                text("String literals and int-like number literals are supported."),
            ]),
            MessageMatchInvalidUnaryZero => friendly::Message(vec![
                text("Unary pattern on "),
                code("0"),
                text(" is not supported."),
            ]),
            MessageMatchInvalidUnaryPlusBigInt => friendly::Message(vec![
                text("Unary pattern "),
                code("+"),
                text(" on bigint literal is not supported."),
            ]),
            MessageMatchDuplicateObjectProperty { name, pattern_kind } => friendly::Message(vec![
                text("Duplicate property "),
                code(name),
                text(" in "),
                text(pattern_kind.to_string()),
                text("."),
            ]),
            MessageMatchBindingInOrPattern => friendly::Message(vec![text(
                "New bindings in 'or' patterns are not yet supported.",
            )]),
            MessageMatchInvalidAsPattern => friendly::Message(vec![
                text("Invalid "),
                code("as"),
                text(" pattern. Direct use on a binding pattern is not allowed."),
            ]),
            MessageMatchInvalidPatternReference { binding } => friendly::Message(vec![
                text("Can't use variable "),
                render_named_reference(binding),
                text(" within the same match pattern it is defined."),
            ]),
            MessageMatchInvalidObjectShorthand { name, pattern_kind } => {
                let pattern_kind_str = pattern_kind.to_string();
                friendly::Message(vec![
                    code("match"),
                    text(" "),
                    text(pattern_kind_str),
                    text("s don't allow this property shorthand syntax. "),
                    text("To fix, be explicit and either use "),
                    code(&format!("{{const {}}}", name)),
                    text(" if you want to create a new variable with the value of property "),
                    code(name),
                    text(", or use "),
                    code(&format!("{{{}: {}}}", name, name)),
                    text(" if you want to match property "),
                    code(name),
                    text(" against the value of the variable named "),
                    code(name),
                    text("."),
                ])
            }
            MessageMatchStatementInvalidBody => friendly::Message(vec![
                code("match"),
                text(" statements use blocks for each case body. "),
                text("To fix, wrap this statement with "),
                code("{"),
                text(" and "),
                code("}"),
                text("."),
            ]),
            MessageMatchInvalidCaseSyntax(kind) => {
                use crate::intermediate_error_types::MatchInvalidCaseSyntax::*;
                let msg_invalid_prefix_case = vec![
                    text("Drop the "),
                    code("case"),
                    text(". Just <pattern>. It's cleaner."),
                ];
                let msg_invalid_infix_colon = vec![
                    code("match"),
                    text(" cases use "),
                    code("=>"),
                    text(" to separate the pattern and the case body. To fix, replace the "),
                    code(";"),
                    text(" with "),
                    code("=>"),
                    text("."),
                ];
                let msg_invalid_suffix_semicolon = vec![
                    code("match"),
                    text(" uses commas "),
                    code(","),
                    text(" to separate cases. To fix, replace the "),
                    code(";"),
                    text(" with "),
                    code(","),
                    text("."),
                ];
                match kind {
                    InvalidMatchCaseMultiple {
                        invalid_prefix_case_locs,
                        invalid_infix_colon_locs,
                        invalid_suffix_semicolon_locs,
                    } => {
                        let msg_with_locs =
                            |msg: Vec<friendly::MessageFeature<Loc>>,
                             locs: &[L]|
                             -> Option<Vec<friendly::MessageFeature<Loc>>> {
                                if locs.is_empty() {
                                    None
                                } else {
                                    let refs: Vec<_> = locs
                                        .iter()
                                        .map(|loc| friendly::no_desc_ref(&loc_of_aloc(loc)))
                                        .collect();
                                    let mut result = msg;
                                    result.push(text(" At"));
                                    result.extend(refs);
                                    result.push(text("."));
                                    Some(result)
                                }
                            };
                        let errors: Vec<Vec<friendly::MessageFeature<Loc>>> = [
                            msg_with_locs(
                                msg_invalid_prefix_case.clone(),
                                invalid_prefix_case_locs,
                            ),
                            msg_with_locs(
                                msg_invalid_infix_colon.clone(),
                                invalid_infix_colon_locs,
                            ),
                            msg_with_locs(
                                msg_invalid_suffix_semicolon.clone(),
                                invalid_suffix_semicolon_locs,
                            ),
                        ]
                        .into_iter()
                        .flatten()
                        .collect();
                        match errors.len() {
                            0 => friendly::Message(vec![]),
                            1 => friendly::Message(errors.into_iter().next().unwrap()),
                            _ => {
                                let mut result = vec![text("Invalid match case syntax:")];
                                for error in errors {
                                    result.push(text("\n- "));
                                    result.extend(error);
                                }
                                friendly::Message(result)
                            }
                        }
                    }
                    InvalidMatchCasePrefixCase => friendly::Message(msg_invalid_prefix_case),
                    InvalidMatchCaseInfixColon => friendly::Message(msg_invalid_infix_colon),
                    InvalidMatchCaseSuffixSemicolon => {
                        friendly::Message(msg_invalid_suffix_semicolon)
                    }
                }
            }
            MessageMatchInvalidWildcardSyntax => friendly::Message(vec![
                code("match"),
                text(" uses "),
                code("_"),
                text(" for wildcard patterns which match everything. "),
                text("To fix, replace "),
                code("default"),
                text(" with "),
                code("_"),
                text("."),
            ]),
            MessageMatchInvalidInstancePattern => friendly::Message(vec![text(
                "Invalid match instance pattern constructor. It must reference a single class.",
            )]),
            // MessageRecord* variants
            MessageRecordBannedTypeUtil { op_loc, op, record } => friendly::Message(vec![
                text("Operation "),
                hardcoded_string_desc_ref(op.to_string(), op_loc),
                text(" is not allowed on record "),
                ref_of_type_desc(&record.loc, &record.desc),
                text(". "),
                text("To fix, turn the record type into an object type first using "),
                code("{...MyRecord}"),
                text(", for example: "),
                code("TypeUtil<{...MyRecord}>"),
                text("."),
            ]),
            MessageRecordInvalidNew { record_name } => friendly::Message(vec![
                text("Use a record expression directly instead of "),
                code("new"),
                text(" with an object literal. For example: "),
                code(&format!("{} {{...}}", record_name)),
                text("."),
            ]),
            MessageRecordInvalidName { name } => friendly::Message(vec![
                text("Invalid record name "),
                code(name),
                text(". Record names cannot start with lowercase 'a' through 'z'."),
            ]),
            MessageRecordDeclarationInvalidSyntax(kind) => {
                use crate::intermediate_error_types::RecordDeclarationInvalidSyntax::*;
                let msg_invalid_infix_equals = vec![
                    text("Record declarations don't need the "),
                    code("="),
                    text(", remove it."),
                ];
                let msg_invalid_variance = vec![
                    text("Record declaration properties are read-only by default, "),
                    text("so don't allow variance annotations. Remove to fix."),
                ];
                let msg_invalid_optional = vec![
                    text("Record declaration properties can't be optional. "),
                    text("Instead, you can specify a default value for the property using "),
                    code("= <expression>,"),
                    text(" after the type annotation. "),
                    text("This default value will be used if the property "),
                    text("is omitted when creating the record. "),
                    text("To closely mimic the existing behavior of an optional property, "),
                    text("you can change "),
                    code("foo?: T,"),
                    text(" to "),
                    code("foo: T | void = undefined,"),
                    text(", though there is likely a better default value you could use instead."),
                ];
                let msg_invalid_suffix_semicolon = vec![
                    text("Record declarations use commas "),
                    code(","),
                    text(" to separate properties. "),
                    text("To fix, replace the "),
                    code(";"),
                    text(" with "),
                    code(","),
                    text("."),
                ];
                match kind {
                    InvalidRecordDeclarationSyntaxMultiple {
                        invalid_infix_equals_loc,
                        invalid_variance_locs,
                        invalid_optional_locs,
                        invalid_suffix_semicolon_locs,
                    } => {
                        let msg_with_locs =
                            |msg: Vec<friendly::MessageFeature<Loc>>,
                             locs: &[L]|
                             -> Option<Vec<friendly::MessageFeature<Loc>>> {
                                if locs.is_empty() {
                                    None
                                } else {
                                    let refs: Vec<_> = locs
                                        .iter()
                                        .map(|loc| friendly::no_desc_ref(&loc_of_aloc(loc)))
                                        .collect();
                                    let mut result = msg;
                                    result.push(text(" At"));
                                    result.extend(refs);
                                    result.push(text("."));
                                    Some(result)
                                }
                            };
                        let msg_with_loc_opt =
                            |msg: Vec<friendly::MessageFeature<Loc>>,
                             loc_opt: &Option<L>|
                             -> Option<Vec<friendly::MessageFeature<Loc>>> {
                                match loc_opt {
                                    None => None,
                                    Some(loc) => {
                                        let mut result = msg;
                                        result.push(text(" At "));
                                        result.push(friendly::no_desc_ref(&loc_of_aloc(loc)));
                                        result.push(text("."));
                                        Some(result)
                                    }
                                }
                            };
                        let errors: Vec<Vec<friendly::MessageFeature<Loc>>> = [
                            msg_with_loc_opt(
                                msg_invalid_infix_equals.clone(),
                                invalid_infix_equals_loc,
                            ),
                            msg_with_locs(msg_invalid_variance.clone(), invalid_variance_locs),
                            msg_with_locs(msg_invalid_optional.clone(), invalid_optional_locs),
                            msg_with_locs(
                                msg_invalid_suffix_semicolon.clone(),
                                invalid_suffix_semicolon_locs,
                            ),
                        ]
                        .into_iter()
                        .flatten()
                        .collect();
                        match errors.len() {
                            0 => friendly::Message(vec![]),
                            1 => friendly::Message(errors.into_iter().next().unwrap()),
                            _ => {
                                let mut result = vec![text("Invalid record declaration syntax:")];
                                for error in errors {
                                    result.push(text("\n- "));
                                    result.extend(error);
                                }
                                friendly::Message(result)
                            }
                        }
                    }
                    InvalidRecordDeclarationSyntaxVariance => {
                        friendly::Message(msg_invalid_variance)
                    }
                    InvalidRecordDeclarationSyntaxOptional => {
                        friendly::Message(msg_invalid_optional)
                    }
                    InvalidRecordDeclarationSyntaxSuffixSemicolon => {
                        friendly::Message(msg_invalid_suffix_semicolon)
                    }
                    InvalidRecordDeclarationSyntaxInfixEquals => {
                        friendly::Message(msg_invalid_infix_equals)
                    }
                }
            }
            // Remaining Message variants
            MessageIncompatiblETypeParamConstIncompatibility { lower, upper } => {
                friendly::Message(vec![
                    text("type parameters "),
                    hardcoded_string_desc_ref(&format!("`{}`", lower.name), &lower.loc),
                    text(" and "),
                    hardcoded_string_desc_ref(&format!("`{}`", upper.name), &upper.loc),
                    text(" do not have matching const-modifier values"),
                ])
            }
            MessageTypeParamConstInvalidPosition(type_param) => friendly::Message(vec![
                text("Type parameter "),
                render_named_reference(type_param),
                text(" cannot be declared as 'const'. "),
                text("'const' modifier can only appear on a function or method type parameter."),
            ]),
            MessageConstantCondition {
                is_truthy,
                warning,
                constant_condition_kind,
            } => {
                use crate::intermediate_error_types::ConstantConditionKind;
                use crate::intermediate_error_types::ConstantConditionWarning;
                let truthy_str = if *is_truthy { "truthy" } else { "falsy" };
                let likely_str = match warning {
                    ConstantConditionWarning::Definite => "",
                    ConstantConditionWarning::Likely { .. } => " likely",
                };
                let base_message = vec![text(&format!(
                    "This condition is{} {}.",
                    likely_str, truthy_str
                ))];
                let help_message = match constant_condition_kind {
                    ConstantConditionKind::ConstCondGeneral => vec![],
                    ConstantConditionKind::UnawaitedPromise => {
                        vec![text(
                            " Perhaps you meant to use `await` to resolve the promise?",
                        )]
                    }
                    ConstantConditionKind::UncalledFunction => {
                        vec![text(" Perhaps you meant to call the function?")]
                    }
                };
                let warning_message = if let ConstantConditionWarning::Likely { suggested_loc } = warning {
                    let mut msg = vec![
                        text(
                            "\n[WARNING]: Flow's type inference may be incorrect that it could be null ",
                        ),
                        text(
                            "at runtime (due to `any` annotations, out-of-bounds array accesses, etc.). ",
                        ),
                        text("If the check is valid, you might want to make"),
                    ];
                    match suggested_loc.as_ref() {
                        Some(loc) => msg.push(friendly::no_desc_ref(&loc_of_aloc(loc))),
                        None => msg.push(text(" the type of this expression")),
                    }
                    msg.push(text(" nullable (`T` -> `?T`)."));
                    msg
                } else {
                    vec![]
                };
                let mut msg = base_message;
                msg.extend(help_message);
                msg.extend(warning_message);
                friendly::Message(msg)
            }
            MessageCannotAssignToObjectWithComputedPropWithKey {
                property,
                key,
                kind,
            } => {
                use crate::intermediate_error_types::InvalidObjKey;
                let suffix = match kind {
                    InvalidObjKey::Other
                    | InvalidObjKey::NullOrVoid
                    | InvalidObjKey::ComputedNotLiteral
                    | InvalidObjKey::ComputedTypeName => vec![
                        text(" Computed properties may only be numeric or string literal values,"),
                        text(" but this one is a "),
                        match kind {
                            InvalidObjKey::NullOrVoid => {
                                hardcoded_string_desc_ref("null or undefined", &property.loc)
                            }
                            _ => ref_of_type_desc(&property.loc, &property.desc),
                        },
                        text(". Can you add an appropriate type annotation to "),
                        render_expression_reference(key),
                        text("?"),
                        text(
                            " See https://flow.org/en/docs/types/literals/ for more information on literal types.",
                        ),
                    ],
                    InvalidObjKey::NumberNonInt => {
                        vec![text(" Only integer-like number literals are allowed.")]
                    }
                    InvalidObjKey::NumberTooLarge => vec![
                        text(" Number literals must not be larger than "),
                        code("Number.MAX_SAFE_INTEGER"),
                        text("."),
                    ],
                    InvalidObjKey::NumberTooSmall => vec![
                        text(" Number literals must not be smaller than "),
                        code("Number.MIN_SAFE_INTEGER"),
                        text("."),
                    ],
                };
                let mut features = vec![
                    text("Cannot use "),
                    render_expression_reference(key),
                    text(" to assign a computed property."),
                ];
                features.extend(suffix);
                friendly::Message(features)
            }
            MessageDevOnlyRefinedLocInfo { refining_locs } => {
                let mut features = vec![text("Refined at")];
                for loc in refining_locs {
                    features.push(friendly::no_desc_ref(&loc_of_aloc(loc)));
                }
                friendly::Message(features)
            }
            MessageDevOnlyInvalidatedRefinementInfo(invalidation_info) => {
                use flow_common::refinement_invalidation::string_of_reason;
                let info_msgs: Vec<friendly::Message<Loc>> = invalidation_info
                    .iter()
                    .map(|(loc, reason)| {
                        let reason_str = string_of_reason(*reason);
                        friendly::Message(vec![
                            text(reason_str),
                            text(" at"),
                            friendly::no_desc_ref(&loc_of_aloc(loc)),
                        ])
                    })
                    .collect();
                let friendly::Message(concat_result) =
                    friendly::conjunction_concat(info_msgs, "and", None);
                let mut features = vec![text("Refinement invalidated due to ")];
                features.extend(concat_result);
                friendly::Message(features)
            }
            MessageMissingPlatformSupportWithAvailablePlatforms(
                box MessageMissingPlatformSupportWithAvailablePlatformsData {
                    available_platforms,
                    required_platforms,
                },
            ) => {
                let missing_platforms: Vec<_> = required_platforms
                    .difference(available_platforms)
                    .cloned()
                    .collect();
                let platform_features =
                    |platforms: &[FlowSmolStr]| -> Vec<friendly::MessageFeature<Loc>> {
                        match platforms {
                            [] => vec![text("no platforms")],
                            [p] => vec![text("the "), code(p.as_str()), text(" platform")],
                            [first, rest @ ..] => {
                                let mut result =
                                    vec![text("the following platforms: "), code(first.as_str())];
                                for p in rest {
                                    result.push(text(", "));
                                    result.push(code(p.as_str()));
                                }
                                result
                            }
                        }
                    };
                let available: Vec<_> = available_platforms.iter().cloned().collect();
                let required: Vec<_> = required_platforms.iter().cloned().collect();
                let mut features = vec![text("The imported module supports ")];
                features.extend(platform_features(&available));
                features.push(text(", but the current module requires the support of "));
                features.extend(platform_features(&required));
                features.push(text(". Support for "));
                features.extend(platform_features(&missing_platforms));
                features.push(text(" is missing."));
                friendly::Message(features)
            }
            MessageParseError(parse_error) => friendly::message_of_string(&parse_error.to_string()),
            MessageDoesNotRender { lower, upper } => {
                friendly::Message(vec![ref_(lower), text(" does not render "), ref_(upper)])
            }
            MessageDoesNotRenderWithPrintedTypes(
                box MessageIncompatibleGeneralWithPrintedTypesData {
                    lower_loc,
                    upper_loc,
                    lower_desc,
                    upper_desc,
                },
            ) => friendly::Message(vec![
                ref_of_type_desc(lower_loc, &rendered_type_desc(lower_desc)),
                text(" does not render "),
                ref_of_type_desc(upper_loc, &rendered_type_desc(upper_desc)),
            ]),
            MessageFunctionRequiresAnotherArgument { def, from } => match from {
                None => friendly::Message(vec![
                    ref_of_type_desc(&def.loc, &def.desc),
                    text(" requires another argument"),
                ]),
                Some(from) => friendly::Message(vec![
                    ref_of_type_desc(&def.loc, &def.desc),
                    text(" requires another argument from "),
                    ref_(from),
                ]),
            },
            MessageFunctionRequiresAnotherArgumentFromCall { def, call } => {
                friendly::Message(vec![
                    ref_of_type_desc(&def.loc, &def.desc),
                    text(" requires another argument from "),
                    render_implicit_instantiation_reference(call),
                ])
            }
            MessageIncompatibleComponentRestParam(rest_param) => friendly::Message(vec![
                text("Cannot use "),
                ref_of_type(rest_param),
                text(
                    " as a component rest param. Component rest params must use an object type and cannot be optional",
                ),
            ]),
            MessageIncompatibleDueToInvariantSubtyping(
                box MessageIncompatibleDueToInvariantSubtypingData {
                    sub_component,
                    lower_loc,
                    upper_loc,
                    lower_desc,
                    upper_desc,
                },
            ) => match sub_component {
                None => friendly::Message(vec![
                    ref_of_type_desc(lower_loc, lower_desc),
                    text(" is not exactly the same as "),
                    ref_of_type_desc(upper_loc, upper_desc),
                ]),
                Some(SubComponentOfInvariantSubtypingError::ObjectProps(props)) => {
                    let prop_codes: Vec<friendly::Message<Loc>> = props
                        .iter()
                        .map(|prop| friendly::Message(vec![code(&prop.display_smol_str())]))
                        .collect();
                    let mut features = vec![text("properties ")];
                    features.extend(friendly::conjunction_concat(prop_codes, "and", None).0);
                    features.push(text(" of "));
                    features.push(ref_of_type_desc(lower_loc, lower_desc));
                    features.push(text(" are not exactly the same as those of "));
                    features.push(ref_of_type_desc(upper_loc, upper_desc));
                    friendly::Message(features)
                }
            },
            MessagePropExtraAgainstExactObject(box MessagePropExtraAgainstExactObjectData {
                lower,
                upper,
                upper_is_record,
                props,
            }) => {
                let number_to_check = props.len();
                let prop_message: Vec<friendly::MessageFeature<Loc>> = if number_to_check > 5 {
                    let max_display_amount = 4;
                    let mut result: Vec<_> = props
                        .iter()
                        .take(max_display_amount)
                        .flat_map(|prop| vec![text("property "), code(prop), text(", ")])
                        .collect();
                    result.push(text(&format!(
                        "and {} others",
                        number_to_check - max_display_amount
                    )));
                    result
                } else {
                    let items: Vec<friendly::Message<Loc>> = props
                        .iter()
                        .map(|prop| friendly::Message(vec![text("property "), code(prop)]))
                        .collect();
                    friendly::conjunction_concat(items, "and", None).0
                };
                let plural = props.len() > 1;
                let upper_kind = if *upper_is_record {
                    "Records"
                } else {
                    "Exact objects"
                };
                let mut result = prop_message;
                result.push(text(" "));
                result.push(text(if plural { "are" } else { "is" }));
                result.push(text(" extra in "));
                result.push(ref_of_type_desc(&lower.loc, &lower.desc));
                result.push(text(" but missing in "));
                result.push(ref_of_type_desc(&upper.loc, &upper.desc));
                result.push(text(". "));
                result.push(text(upper_kind));
                result.push(text(" do not accept extra props"));
                friendly::Message(result)
            }
        }
    };

    fn convert_error_message<
        L: Dupe + PartialEq + Eq + PartialOrd + Ord + Clone + std::fmt::Debug,
    >(
        explanation_to_friendly_msgs: &impl Fn(&Explanation<L>) -> friendly::Message<Loc>,
        frame_to_friendly_msgs: &impl Fn(
            bool,
            &ErrorFrame<L>,
        )
            -> (Option<friendly::Message<Loc>>, friendly::Message<Loc>),
        root_msg_to_root_kind_and_friendly_msgs: &impl Fn(
            &RootMessage<L>,
        ) -> (RootKind, friendly::Message<Loc>),
        msg_to_friendly_msgs: &impl Fn(&Message<L>) -> friendly::Message<Loc>,
        error: &IntermediateError<L>,
    ) -> PrintableError<Loc> {
        use flow_common_errors::error_utils::mk_error as flow_mk_error;
        use flow_common_errors::error_utils::mk_speculation_error as flow_mk_speculation_error;

        fn first_singleton_message<L: Dupe>(error: &IntermediateError<L>) -> Option<&Message<L>> {
            match &error.message {
                ErrorMessage::SingletonMessage { message, .. }
                | ErrorMessage::SingletonMessageWithExample { message, .. } => Some(message),
                ErrorMessage::SpeculationMessage { branches, .. } => branches
                    .first()
                    .and_then(|(_, error)| first_singleton_message(error)),
            }
        }

        let root = error.root.as_ref().map(|(loc, root_msg)| {
            let (kind, msg) = root_msg_to_root_kind_and_friendly_msgs(root_msg);
            (loc.dupe(), kind, msg)
        });

        match &error.message {
            ErrorMessage::SingletonMessage {
                message,
                frames,
                explanations,
            } => {
                let friendly_frames: Option<
                    Vec<(Option<friendly::Message<Loc>>, friendly::Message<Loc>)>,
                > = frames
                    .as_ref()
                    .map(|fs| fs.iter().map(|f| frame_to_friendly_msgs(true, f)).collect());

                let friendly_explanations: Option<Vec<friendly::Message<Loc>>> = explanations
                    .as_ref()
                    .map(|exps| exps.iter().map(explanation_to_friendly_msgs).collect());

                let friendly_message = msg_to_friendly_msgs(message);

                flow_mk_error(
                    Some(error.kind),
                    root,
                    friendly_frames,
                    friendly_explanations,
                    error.loc.dupe(),
                    error.error_code,
                    friendly_message,
                )
            }

            ErrorMessage::SingletonMessageWithExample {
                message,
                frames,
                explanations,
                example,
            } => {
                let friendly_frames: Option<
                    Vec<(Option<friendly::Message<Loc>>, friendly::Message<Loc>)>,
                > = frames
                    .as_ref()
                    .map(|fs| fs.iter().map(|f| frame_to_friendly_msgs(true, f)).collect());

                let friendly_explanations: Option<Vec<friendly::Message<Loc>>> = explanations
                    .as_ref()
                    .map(|exps| exps.iter().map(explanation_to_friendly_msgs).collect());

                let mut friendly_message = msg_to_friendly_msgs(message);
                if let Some(example_message) = first_singleton_message(example)
                    && !matches!(
                        (message, example_message),
                        (
                            Message::MessageStandaloneCallRequiresReceiver(_),
                            Message::MessageStandaloneCallRequiresReceiver(_)
                        )
                    )
                {
                    friendly_message.push(friendly::text(". For example, "));
                    friendly_message.extend(msg_to_friendly_msgs(example_message));
                }

                flow_mk_error(
                    Some(error.kind),
                    root,
                    friendly_frames,
                    friendly_explanations,
                    error.loc.dupe(),
                    error.error_code,
                    friendly_message,
                )
            }

            ErrorMessage::SpeculationMessage {
                frames,
                explanations,
                branches,
            } => {
                let friendly_frames: Vec<friendly::Message<Loc>> = frames
                    .iter()
                    .map(|f| frame_to_friendly_msgs(false, f).1)
                    .collect();

                let friendly_explanations: Vec<friendly::Message<Loc>> = explanations
                    .iter()
                    .map(explanation_to_friendly_msgs)
                    .collect();

                let friendly_branches: Vec<(i32, PrintableError<Loc>)> = branches
                    .iter()
                    .map(|(score, branch_error)| {
                        let converted = convert_error_message(
                            explanation_to_friendly_msgs,
                            frame_to_friendly_msgs,
                            root_msg_to_root_kind_and_friendly_msgs,
                            msg_to_friendly_msgs,
                            branch_error,
                        );
                        (*score, converted)
                    })
                    .collect();

                flow_mk_speculation_error(
                    Some(error.kind),
                    error.loc.dupe(),
                    root,
                    friendly_frames,
                    friendly_explanations,
                    error.error_code,
                    friendly_branches,
                )
            }
        }
    }

    let printable_error = convert_error_message(
        &explanation_to_friendly_msgs,
        &frame_to_friendly_msgs,
        &root_msg_to_root_kind_and_friendly_msgs,
        &msg_to_friendly_msgs,
        &intermediate_error,
    );

    let printable_error = if intermediate_error.unsuppressable {
        flow_common_errors::error_utils::patch_unsuppressable_error(printable_error)
    } else {
        printable_error
    };

    match &intermediate_error.misplaced_source_file {
        None => printable_error,
        Some(source_file) => flow_common_errors::error_utils::patch_misplaced_error(
            strip_root,
            source_file,
            printable_error,
        ),
    }
}

/// Convert a set of FlowErrors to printable errors.
#[allow(dead_code)]
pub fn make_errors_printable<F, G, H>(
    loc_of_aloc: F,
    get_ast: G,
    strip_root: Option<&std::path::Path>,
    errors: ErrorSet,
    is_lib: H,
) -> ConcreteLocPrintableErrorSet
where
    F: Fn(&ALoc) -> Loc + Clone,
    G: Fn(&FileKey) -> Option<Arc<flow_parser::ast::Program<Loc, Loc>>> + Clone,
    H: Fn(&FileKey) -> bool + Clone,
{
    errors.fold(ConcreteLocPrintableErrorSet::empty(), |mut acc, err| {
        let intermediate =
            make_intermediate_error(loc_of_aloc.clone(), false, &err, is_lib.clone());
        let printable = to_printable_error(
            loc_of_aloc.clone(),
            get_ast.clone(),
            strip_root,
            intermediate,
        );
        acc.add(printable);
        acc
    })
}
