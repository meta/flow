/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

#![feature(box_patterns)]

use std::collections::BTreeSet;
use std::ops::Deref;
use std::rc::Rc;

use flow_aloc::ALoc;
use flow_common::flow_symbol::dump_symbol;
use flow_common::polarity::Polarity;
use flow_common::reason;
use flow_common::reason::Reason;
use flow_common::reason::VirtualReasonDesc;
use flow_common::reason::string_of_aloc;
use flow_lint_settings::lint_settings::LintParseError;
use flow_lint_settings::lints::PropertyAssignmentKind;
use flow_lint_settings::lints::SketchyNullKind;
use flow_lint_settings::lints::SketchyNumberKind;
use flow_parser::loc_sig::LocSig;
use flow_type_sig::signature_error::BindingValidation;
use flow_type_sig::signature_error::SignatureError;
use flow_typing_context::Context;
use flow_typing_context::Cx;
use flow_typing_default::Default;
use flow_typing_errors::error_message::CallTypeArityCallee;
use flow_typing_errors::error_message::EAbstractClassData;
use flow_typing_errors::error_message::EAnnotationInferenceData;
use flow_typing_errors::error_message::EArithmeticOperandData;
use flow_typing_errors::error_message::EAssertionFunctionFalsyAtExitData;
use flow_typing_errors::error_message::EAssignConstLikeBindingData;
use flow_typing_errors::error_message::EBinaryInLHSData;
use flow_typing_errors::error_message::EBinaryInRHSData;
use flow_typing_errors::error_message::EBuiltinModuleLookupFailedData;
use flow_typing_errors::error_message::EBuiltinNameLookupFailedData;
use flow_typing_errors::error_message::ECallTypeArityData;
use flow_typing_errors::error_message::ECannotDeleteData;
use flow_typing_errors::error_message::ECannotImportGlobalLibdefData;
use flow_typing_errors::error_message::ECannotSpreadIndexerOnRightData;
use flow_typing_errors::error_message::ECannotSpreadInterfaceData;
use flow_typing_errors::error_message::EClassToObjectData;
use flow_typing_errors::error_message::EComparisonData;
use flow_typing_errors::error_message::EComponentThisReferenceData;
use flow_typing_errors::error_message::EConstantConditionData;
use flow_typing_errors::error_message::EConstructSignatureMissingInSubtypingData;
use flow_typing_errors::error_message::EDevOnlyInvalidatedRefinementInfoData;
use flow_typing_errors::error_message::EDevOnlyRefinedLocInfoData;
use flow_typing_errors::error_message::EDuplicateClassMemberData;
use flow_typing_errors::error_message::EDuplicateComponentPropData;
use flow_typing_errors::error_message::EDuplicateModuleProviderData;
use flow_typing_errors::error_message::EExpectedBigIntLitData;
use flow_typing_errors::error_message::EExpectedBooleanLitData;
use flow_typing_errors::error_message::EExpectedModuleLookupFailedData;
use flow_typing_errors::error_message::EExpectedNumberLitData;
use flow_typing_errors::error_message::EExpectedStringLitData;
use flow_typing_errors::error_message::EExponentialSpreadData;
use flow_typing_errors::error_message::EExportRenamedDefaultData;
use flow_typing_errors::error_message::EForInRHSData;
use flow_typing_errors::error_message::EFunctionCallExtraArgData;
use flow_typing_errors::error_message::EHookIncompatibleData;
use flow_typing_errors::error_message::EHookRuleViolationData;
use flow_typing_errors::error_message::EHookUniqueIncompatibleData;
use flow_typing_errors::error_message::EIllegalAssertOperatorData;
use flow_typing_errors::error_message::EImplicitInstantiationUnderconstrainedErrorData;
use flow_typing_errors::error_message::EIncompatiblePropData;
use flow_typing_errors::error_message::EIncompatibleSpeculationData;
use flow_typing_errors::error_message::EIncompatibleTypeData;
use flow_typing_errors::error_message::EIncompatibleTypesWithUseOpData;
use flow_typing_errors::error_message::EIncorrectTypeWithReplacementData;
use flow_typing_errors::error_message::EIndexerCheckFailedData;
use flow_typing_errors::error_message::EInexactMayOverwriteIndexerData;
use flow_typing_errors::error_message::EInstanceofRHSData;
use flow_typing_errors::error_message::EInvalidBinaryArithData;
use flow_typing_errors::error_message::EInvalidConstructorData;
use flow_typing_errors::error_message::EInvalidDeclarationData;
use flow_typing_errors::error_message::EInvalidExactObjectIntersectionData;
use flow_typing_errors::error_message::EInvalidObjectKitData;
use flow_typing_errors::error_message::EInvalidPrototypeData;
use flow_typing_errors::error_message::EInvalidReactCreateElementData;
use flow_typing_errors::error_message::EInvalidRendersTypeArgumentData;
use flow_typing_errors::error_message::EInvalidThisArgData;
use flow_typing_errors::error_message::EInvariantSubtypingWithUseOpData;
use flow_typing_errors::error_message::EKeySpreadPropData;
use flow_typing_errors::error_message::EMissingPlatformSupportWithAvailablePlatformsData;
use flow_typing_errors::error_message::EMissingTypeArgsData;
use flow_typing_errors::error_message::ENegativeTypeGuardConsistencyData;
use flow_typing_errors::error_message::ENonStrictEqualityComparisonData;
use flow_typing_errors::error_message::ENotAReactComponentData;
use flow_typing_errors::error_message::EObjectComputedPropertyAccessData;
use flow_typing_errors::error_message::EObjectComputedPropertyPotentialOverwriteData;
use flow_typing_errors::error_message::EObjectThisSuperReferenceData;
use flow_typing_errors::error_message::EOverrideData;
use flow_typing_errors::error_message::EPlatformSpecificImplementationModuleLookupFailedData;
use flow_typing_errors::error_message::EPolarityMismatchData;
use flow_typing_errors::error_message::EPrimitiveAsInterfaceData;
use flow_typing_errors::error_message::EPrivateLookupFailedData;
use flow_typing_errors::error_message::EPropNotFoundInLookupData;
use flow_typing_errors::error_message::EPropNotFoundInSubtypingData;
use flow_typing_errors::error_message::EPropNotReadableData;
use flow_typing_errors::error_message::EPropNotWritableData;
use flow_typing_errors::error_message::EPropPolarityMismatchData;
use flow_typing_errors::error_message::EPropsExtraAgainstExactObjectData;
use flow_typing_errors::error_message::EPropsNotFoundInInvariantSubtypingData;
use flow_typing_errors::error_message::EPropsNotFoundInSubtypingData;
use flow_typing_errors::error_message::EReactIntrinsicOverlapData;
use flow_typing_errors::error_message::ERecursiveDefinitionData;
use flow_typing_errors::error_message::ESketchyNullLintData;
use flow_typing_errors::error_message::ETSSyntaxData;
use flow_typing_errors::error_message::ETooFewTypeArgsData;
use flow_typing_errors::error_message::ETooManyTypeArgsData;
use flow_typing_errors::error_message::ETupleArityMismatchData;
use flow_typing_errors::error_message::ETupleElementNotReadableData;
use flow_typing_errors::error_message::ETupleElementNotWritableData;
use flow_typing_errors::error_message::ETupleElementPolarityMismatchData;
use flow_typing_errors::error_message::ETupleInvalidTypeSpreadData;
use flow_typing_errors::error_message::ETupleNonIntegerIndexData;
use flow_typing_errors::error_message::ETupleOutOfBoundsData;
use flow_typing_errors::error_message::ETupleRequiredAfterOptionalData;
use flow_typing_errors::error_message::ETypeGuardFunctionInvalidWritesData;
use flow_typing_errors::error_message::ETypeGuardFunctionParamHavocedData;
use flow_typing_errors::error_message::ETypeGuardIncompatibleWithFunctionKindData;
use flow_typing_errors::error_message::ETypeGuardInvalidParameterData;
use flow_typing_errors::error_message::ETypeParamConstIncompatibilityData;
use flow_typing_errors::error_message::EUmdGlobalValueInModuleData;
use flow_typing_errors::error_message::EUnableToSpreadData;
use flow_typing_errors::error_message::EUnionOptimizationData;
use flow_typing_errors::error_message::EUnionOptimizationOnNonUnionData;
use flow_typing_errors::error_message::EUnionPartialOptimizationNonUniqueKeyData;
use flow_typing_errors::error_message::EUnionSpeculationFailedData;
use flow_typing_errors::error_message::EUnnecessaryInvariantData;
use flow_typing_errors::error_message::EUnrecognizedAssertionCallData;
use flow_typing_errors::error_message::EUnsupportedExactData;
use flow_typing_errors::error_message::EVarianceKeywordData;
use flow_typing_errors::error_message::EnumAllMembersAlreadyCheckedData;
use flow_typing_errors::error_message::EnumBigIntMemberNotInitializedData;
use flow_typing_errors::error_message::EnumBooleanMemberNotInitializedData;
use flow_typing_errors::error_message::EnumDuplicateMemberNameData;
use flow_typing_errors::error_message::EnumErrorKind;
use flow_typing_errors::error_message::EnumInconsistentMemberValuesData;
use flow_typing_errors::error_message::EnumInvalidAbstractUseData;
use flow_typing_errors::error_message::EnumInvalidCheckData;
use flow_typing_errors::error_message::EnumInvalidMemberAccessData;
use flow_typing_errors::error_message::EnumInvalidMemberInitializerData;
use flow_typing_errors::error_message::EnumInvalidMemberNameData;
use flow_typing_errors::error_message::EnumInvalidObjectFunctionData;
use flow_typing_errors::error_message::EnumInvalidObjectUtilTypeData;
use flow_typing_errors::error_message::EnumMemberAccess;
use flow_typing_errors::error_message::EnumMemberAlreadyCheckedData;
use flow_typing_errors::error_message::EnumMemberDuplicateValueData;
use flow_typing_errors::error_message::EnumMemberUsedAsTypeData;
use flow_typing_errors::error_message::EnumModificationData;
use flow_typing_errors::error_message::EnumNonIdentifierMemberNameData;
use flow_typing_errors::error_message::EnumNotAllCheckedData;
use flow_typing_errors::error_message::EnumNotIterableData;
use flow_typing_errors::error_message::EnumNotIterableForInData;
use flow_typing_errors::error_message::EnumNumberMemberNotInitializedData;
use flow_typing_errors::error_message::EnumReferenceData;
use flow_typing_errors::error_message::EnumStringMemberInconsistentlyInitializedData;
use flow_typing_errors::error_message::EnumUnknownNotCheckedData;
use flow_typing_errors::error_message::ErrorMessage;
use flow_typing_errors::error_message::ErrorTypeReferenceWithLocData;
use flow_typing_errors::error_message::InternalError;
use flow_typing_errors::error_message::InvalidMappedTypeErrorKind;
use flow_typing_errors::error_message::InvalidTemplateLiteralTypeErrorKind;
use flow_typing_errors::error_message::MatchDuplicateObjectPropertyData;
use flow_typing_errors::error_message::MatchErrorKind;
use flow_typing_errors::error_message::MatchInvalidCaseSyntaxData;
use flow_typing_errors::error_message::MatchInvalidIdentOrMemberPatternData;
use flow_typing_errors::error_message::MatchInvalidObjectShorthandData;
use flow_typing_errors::error_message::MatchInvalidPatternReferenceData;
use flow_typing_errors::error_message::MatchNonExhaustiveObjectPatternData;
use flow_typing_errors::error_message::MatchNonExplicitEnumCheckData;
use flow_typing_errors::error_message::MatchNotExhaustiveData;
use flow_typing_errors::error_message::MatchUnusedPatternData;
use flow_typing_errors::error_message::RecordErrorKind;
use flow_typing_errors::error_message::TSEnumInvalidMemberData;
use flow_typing_errors::error_message::TSEnumInvalidSyntaxData;
use flow_typing_errors::error_message::UpperKind;
use flow_typing_errors::error_message::string_of_invalid_render_type_kind;
use flow_typing_errors::intermediate_error_types::ConstantConditionKind;
use flow_typing_errors::intermediate_error_types::DocblockError;
use flow_typing_errors::intermediate_error_types::ExponentialSpreadOperandGroup;
use flow_typing_errors::intermediate_error_types::ExpressionFunctionKind;
use flow_typing_errors::intermediate_error_types::ExpressionReferenceData;
use flow_typing_errors::intermediate_error_types::ExpressionReferenceKind;
use flow_typing_errors::intermediate_error_types::FunctionReferenceKind;
use flow_typing_errors::intermediate_error_types::ObjKind as IntermediateObjKind;
use flow_typing_errors::intermediate_error_types::PropPolarityMismatchObject;
use flow_typing_errors::intermediate_error_types::ValueAsTypeReference;
use flow_typing_type::type_;
use flow_typing_type::type_::AnySource;
use flow_typing_type::type_::ArrRestTData;
use flow_typing_type::type_::CallArgInner;
use flow_typing_type::type_::CallElemTData;
use flow_typing_type::type_::CallTData;
use flow_typing_type::type_::CanonicalRendersForm;
use flow_typing_type::type_::CondTData;
use flow_typing_type::type_::ConditionalTData;
use flow_typing_type::type_::DefTInner;
use flow_typing_type::type_::DepthTrace;
use flow_typing_type::type_::Destructor;
use flow_typing_type::type_::DestructorConditionalTypeData;
use flow_typing_type::type_::DestructorMappedTypeData;
use flow_typing_type::type_::DestructorSpreadTupleTypeData;
use flow_typing_type::type_::ElemTData;
use flow_typing_type::type_::EnumInfoInner;
use flow_typing_type::type_::EvalTypeDestructorTData;
use flow_typing_type::type_::ExtendsUseTData;
use flow_typing_type::type_::GenericTData;
use flow_typing_type::type_::GetElemTData;
use flow_typing_type::type_::GetEnumTData;
use flow_typing_type::type_::HasOwnPropTData;
use flow_typing_type::type_::InstanceT;
use flow_typing_type::type_::Literal;
use flow_typing_type::type_::LookupTData;
use flow_typing_type::type_::MapTypeTData;
use flow_typing_type::type_::MethodTData;
use flow_typing_type::type_::MixedFlavor;
use flow_typing_type::type_::ObjKind;
use flow_typing_type::type_::OptionalIndexedAccessTData;
use flow_typing_type::type_::PolyTData;
use flow_typing_type::type_::Property;
use flow_typing_type::type_::PropertyInner;
use flow_typing_type::type_::PropertyType;
use flow_typing_type::type_::ROArrayATData;
use flow_typing_type::type_::ReactKitTData;
use flow_typing_type::type_::RendersVariant;
use flow_typing_type::type_::ReposUseTData;
use flow_typing_type::type_::ResolveSpreadTData;
use flow_typing_type::type_::ResolveUnionTData;
use flow_typing_type::type_::SealGenericTData;
use flow_typing_type::type_::Selector;
use flow_typing_type::type_::SetElemTData;
use flow_typing_type::type_::SpecializeTData;
use flow_typing_type::type_::SuperTData;
use flow_typing_type::type_::TestPropTData;
use flow_typing_type::type_::ThisInstanceTData;
use flow_typing_type::type_::ThisTypeAppTData;
use flow_typing_type::type_::TupleElement;
use flow_typing_type::type_::Type;
use flow_typing_type::type_::TypeAppTData;
use flow_typing_type::type_::TypeInner;
use flow_typing_type::type_::TypeMap;
use flow_typing_type::type_::UseT;
use flow_typing_type::type_::UseTInner;
use flow_typing_type::type_::ValueToTypeReferenceTData;
use flow_typing_type::type_::nominal;
use flow_typing_type::type_::string_of_ctor;
use flow_typing_type::type_::string_of_use_ctor;
use flow_typing_type::type_::string_of_use_op;
use flow_typing_type::type_::type_or_type_desc::ErrorTypeDesc;
use flow_typing_type::type_::type_or_type_desc::TypeOrTypeDescT;
use flow_typing_type::type_util::reason_of_t;

pub fn string_of_selector(selector: &Selector) -> String {
    match selector {
        // TODO print info about the key
        Selector::Elem(_) => "Elem _".to_string(),
        Selector::Prop(x, _) => format!("Prop {}", x),
        Selector::ArrRest(i) => format!("ArrRest {}", i),
        Selector::ObjRest(xs) => {
            let xs_str: Vec<&str> = xs.iter().map(|s| s.as_str()).collect();
            format!("ObjRest [{}]", xs_str.join("; "))
        }
        Selector::Default => "Default".to_string(),
    }
}

pub fn string_of_destructor(destructor: &Destructor) -> String {
    match destructor {
        Destructor::NormalizeIntersection { check_exact } => {
            format!("NormalizeIntersection(check_exact={check_exact})")
        }
        Destructor::NonMaybeType => "NonMaybeType".to_string(),
        Destructor::PropertyType { name, .. } => {
            format!("PropertyType {}", name)
        }
        Destructor::ElementType { .. } => "ElementType".to_string(),
        Destructor::EnumType => "EnumType".to_string(),
        Destructor::OptionalIndexedAccessNonMaybeType { .. } => {
            "OptionalIndexedAccessNonMaybeType".to_string()
        }
        Destructor::OptionalIndexedAccessResultType { .. } => {
            "OptionalIndexedAccessResultType".to_string()
        }
        Destructor::ExactType => "ExactType".to_string(),
        Destructor::ReadOnlyType => "ReadOnly".to_string(),
        Destructor::ReactCheckComponentConfig { .. } => "ReactCheckComponentConfig".to_string(),
        Destructor::PartialType => "PartialType".to_string(),
        Destructor::RequiredType => "RequiredType".to_string(),
        Destructor::SpreadType(..) => "Spread".to_string(),
        Destructor::SpreadTupleType(box DestructorSpreadTupleTypeData { .. }) => {
            "SpreadTupleType".to_string()
        }
        Destructor::RestType(..) => "Rest".to_string(),
        Destructor::ValuesType => "Values".to_string(),
        Destructor::ConditionalType(box DestructorConditionalTypeData { .. }) => {
            "ConditionalType".to_string()
        }
        Destructor::TypeMap(TypeMap::ObjectKeyMirror) => "ObjectKeyMirror".to_string(),
        Destructor::ReactElementConfigType => "ReactElementConfig".to_string(),
        Destructor::MappedType(box DestructorMappedTypeData { .. }) => "MappedType".to_string(),
        Destructor::ReactDRO(_) => "ReactDRO".to_string(),
    }
}

fn bool_of_sealtype(seal: &type_::object::spread::SealType) -> bool {
    match seal {
        type_::object::spread::SealType::Sealed => true,
        _ => false,
    }
}

// ***************************************************************** //

// debug printer

pub fn dump_reason(cx: &Context, reason: &Reason) -> String {
    if cx.should_strip_root() {
        reason::dump_reason(Some(cx.root().to_string_lossy().deref()), reason)
    } else {
        reason::dump_reason(None, reason)
    }
}

fn dump_error_type_reference(
    cx: &Context,
    reference: &ErrorTypeReferenceWithLocData<ALoc>,
) -> String {
    dump_loc_type_desc(cx, &reference.loc, &reference.type_desc)
}

fn dump_enum_reference(enum_: &EnumReferenceData<ALoc>) -> String {
    format!(
        "{} {:?}",
        string_of_aloc(None, &enum_.loc),
        VirtualReasonDesc::<ALoc>::REnum {
            name: enum_.name.clone(),
        }
    )
}

fn dump_loc_type_desc(cx: &Context, loc: &ALoc, type_desc: &TypeOrTypeDescT<ALoc>) -> String {
    match type_desc {
        TypeOrTypeDescT::Type(t) => {
            dump_reason(cx, &Reason::new(reason_of_t(t).desc.clone(), loc.clone()))
        }
        TypeOrTypeDescT::TypeDesc(ErrorTypeDesc::ReasonDesc(desc)) => {
            format!("{} {desc:?}", string_of_aloc(None, loc))
        }
        TypeOrTypeDescT::TypeDesc(ErrorTypeDesc::Normalized(desc)) => {
            format!("{desc:?} at {loc:?}")
        }
        TypeOrTypeDescT::TypeDesc(ErrorTypeDesc::Unknown) => {
            format!("? at {loc:?}")
        }
    }
}

fn dump_named_type_desc(
    cx: &Context,
    loc: &ALoc,
    name: Option<&str>,
    type_desc: &TypeOrTypeDescT<ALoc>,
) -> String {
    match name {
        Some(name) => format!("{} RIdentifier({name:?})", string_of_aloc(None, loc)),
        None => dump_loc_type_desc(cx, loc, type_desc),
    }
}

fn dump_value_as_type_reference(
    cx: &Context,
    loc: &ALoc,
    reference: Option<&ValueAsTypeReference>,
    type_desc: &TypeOrTypeDescT<ALoc>,
) -> String {
    match reference {
        Some(ValueAsTypeReference::Name(name)) => {
            dump_named_type_desc(cx, loc, Some(name.as_str()), type_desc)
        }
        Some(ValueAsTypeReference::ImportType(module)) => format!(
            "{} ImportType({:?})",
            string_of_aloc(None, loc),
            module.as_str()
        ),
        None => dump_loc_type_desc(cx, loc, type_desc),
    }
}

fn dump_expression_reference(reference: &ExpressionReferenceData<ALoc>) -> String {
    let description = match &reference.kind {
        ExpressionReferenceKind::Code(code) => format!("RCode({code:?})"),
        ExpressionReferenceKind::ObjectLiteral => "RObjectLit".to_string(),
        ExpressionReferenceKind::ArrayLiteral => "RArrayLit".to_string(),
        ExpressionReferenceKind::Function(kind) => match kind {
            ExpressionFunctionKind::Normal => "RFunction(RNormal)".to_string(),
            ExpressionFunctionKind::Async => "RFunction(RAsync)".to_string(),
            ExpressionFunctionKind::Generator => "RFunction(RGenerator)".to_string(),
            ExpressionFunctionKind::AsyncGenerator => "RFunction(RAsyncGenerator)".to_string(),
            ExpressionFunctionKind::Unknown => "RFunction(RUnknown)".to_string(),
        },
        ExpressionReferenceKind::EmptyString => "RStringLit(\"\")".to_string(),
        ExpressionReferenceKind::TemplateString => "RTemplateString".to_string(),
        ExpressionReferenceKind::Expression => "expression".to_string(),
    };
    format!("{} {description}", string_of_aloc(None, &reference.loc))
}

fn dump_tuple_element_reference(loc: &ALoc, name: Option<&str>) -> String {
    let description = name.map_or_else(
        || "tuple element".to_string(),
        |name| format!("tuple element (labeled '{name}')"),
    );
    format!("{}: {description:?}", string_of_aloc(None, loc))
}

fn dump_t_(depth: u32, tvars: &mut BTreeSet<i32>, cx: &Context, t: &Type) -> String {
    if depth == 0 {
        return string_of_ctor(t).to_string();
    }

    fn p(cx: &Context, t: &Type, reason: bool, extra: &str) -> String {
        let ctor = string_of_ctor(t);
        let reason_str = if reason {
            format!("{:?}", dump_reason(cx, reason_of_t(t)))
        } else {
            String::new()
        };
        let sep = if reason && !extra.is_empty() {
            ", "
        } else {
            ""
        };
        format!("{} ({}{}{})", ctor, reason_str, sep, extra)
    }
    let (kid, tvar) = {
        let kid = move |tvars: &mut BTreeSet<i32>, t: &Type| -> String {
            dump_t_(depth - 1, tvars, cx, t)
        };

        let tvar = move |tvars: &mut BTreeSet<i32>, id: i32| -> String {
            dump_tvar_(depth - 1, tvars, cx, id)
        };
        (kid, tvar)
    };

    let defer_use = |expr: &type_::TypeDestructorT, t: &str| -> String {
        let type_::TypeDestructorTInner(use_op, _, destructor) = expr.deref();
        format!(
            "{}, TypeDestruct {} on {}",
            string_of_use_op(use_op),
            string_of_destructor(destructor),
            t
        )
    };

    fn string_of_mixed_flavor(flavor: &MixedFlavor) -> &'static str {
        match flavor {
            MixedFlavor::MixedEverything => "Mixed_everything",
            MixedFlavor::MixedFunction => "Mixed_function",
            MixedFlavor::MixedTruthy => "Mixed_truthy",
            MixedFlavor::MixedNonMaybe => "Mixed_non_maybe",
            MixedFlavor::MixedNonNull => "Mixed_non_null",
            MixedFlavor::MixedNonVoid => "Mixed_non_void",
        }
    }

    fn string_of_any_source(source: &AnySource) -> &'static str {
        match source {
            AnySource::AnnotatedAny => "AnnotatedAny",
            AnySource::CatchAny => "CatchAny",
            AnySource::AnyError(_) => "Error",
            AnySource::Unsound(_) => "Unsound",
            AnySource::Untyped => "Untyped",
            AnySource::Placeholder => "Placeholder",
        }
    }

    let instance_t = |tvars: &mut BTreeSet<i32>, inst: &InstanceT| -> String {
        let type_args_str: Vec<String> = inst
            .inst
            .type_args
            .iter()
            .map(|(n, _, t, _)| format!("{}={}", n, kid(tvars, t)))
            .collect();
        format!(
            "[{}] #{}",
            type_args_str.join(", "),
            inst.inst.class_id.0.debug_to_string(false)
        )
    };

    let tuple_elements = |tvars: &mut BTreeSet<i32>,
                          elements: &[TupleElement],
                          rest: Option<&TupleElement>,
                          inexact: bool|
     -> String {
        let mut elems: Vec<String> = elements
            .iter()
            .map(|TupleElement { t, .. }| kid(tvars, t))
            .collect();
        if let Some(TupleElement { t, .. }) = rest {
            elems.push(format!("...{}", kid(tvars, t)));
        }
        if inexact {
            elems.push("...".to_string());
        }
        elems.join(", ")
    };

    match t.deref() {
        TypeInner::OpenT(tv) => {
            let extra = tvar(tvars, tv.id() as i32);
            p(cx, t, true, &extra)
        }
        TypeInner::ImplicitInstantiationTvar(data) => {
            let extra = tvar(tvars, data.id);
            p(cx, t, true, &extra)
        }
        TypeInner::DefT(_, def) => match def.deref() {
            DefTInner::NumGeneralT(lit) => {
                let extra = match lit {
                    Literal::Truthy => "truthy",
                    Literal::AnyLiteral => "",
                };
                p(cx, t, true, extra)
            }
            DefTInner::StrGeneralT(lit) => {
                let extra = match lit {
                    Literal::Truthy => "truthy",
                    Literal::AnyLiteral => "",
                };
                p(cx, t, true, extra)
            }
            DefTInner::BoolGeneralT => p(cx, t, true, ""),
            DefTInner::BigIntGeneralT(lit) => {
                let extra = match lit {
                    Literal::Truthy => "truthy",
                    Literal::AnyLiteral => "",
                };
                p(cx, t, true, extra)
            }
            DefTInner::FunT(_, fun) => {
                let this_str = kid(tvars, &fun.this_t.0);
                let params_str: Vec<String> =
                    fun.params.iter().map(|fp| kid(tvars, &fp.1)).collect();
                let rest_str = fun
                    .rest_param
                    .as_ref()
                    .map(|rp| format!("...{}", kid(tvars, &rp.2)))
                    .unwrap_or_default();
                let return_str = kid(tvars, &fun.return_t);
                let guard_str = match &fun.type_guard {
                    Some(tg) => {
                        let prefix = match tg.kind {
                            flow_typing_type::type_::TypeGuardKind::Default => "",
                            flow_typing_type::type_::TypeGuardKind::Implies => "implies ",
                            flow_typing_type::type_::TypeGuardKind::Asserts => "asserts ",
                        };
                        match &tg.type_guard {
                            Some(type_guard) => format!(
                                " {}{}is {}",
                                prefix,
                                tg.param_name.1.as_str(),
                                kid(tvars, type_guard)
                            ),
                            None => format!(" {}{}", prefix, tg.param_name.1.as_str()),
                        }
                    }
                    None => String::new(),
                };
                let extra = format!(
                    "<this: {}>({}{}){} => {}{}",
                    this_str,
                    params_str.join("; "),
                    rest_str,
                    "",
                    return_str,
                    guard_str
                );
                p(cx, t, true, &extra)
            }
            DefTInner::MixedT(flavor) => {
                let extra = string_of_mixed_flavor(flavor);
                p(cx, t, true, extra)
            }
            DefTInner::EmptyT
            | DefTInner::SymbolT
            | DefTInner::UniqueSymbolT(_)
            | DefTInner::NullT
            | DefTInner::VoidT => p(cx, t, true, ""),
            DefTInner::PolyT(box PolyTData {
                tparams, t_out, id, ..
            }) => {
                let c_str = kid(tvars, t_out);
                let tparams_str: Vec<String> =
                    tparams.iter().map(|tp| tp.name.to_string()).collect();
                let extra = format!(
                    "{} [{}] #{}",
                    c_str,
                    tparams_str.join("; "),
                    id.debug_string()
                );
                p(cx, t, true, &extra)
            }
            DefTInner::ObjT(obj) => {
                let obj_kind_str = match &obj.flags.obj_kind {
                    ObjKind::Exact => "Exact".to_string(),
                    ObjKind::Inexact => "Inexact".to_string(),
                    ObjKind::Indexed(d) => {
                        format!(
                            "Indexed {{[{}]: {}}}",
                            dump_t_(depth, tvars, cx, &d.key),
                            dump_t_(depth, tvars, cx, &d.value)
                        )
                    }
                };
                let extra = format!("{}, {}", obj.props_tmap.debug_string(), obj_kind_str);
                p(cx, t, true, &extra)
            }
            DefTInner::ArrT(arr) => {
                use flow_typing_type::type_::ArrType;
                use flow_typing_type::type_::ArrayATData;
                use flow_typing_type::type_::TupleATData;
                match arr.as_ref() {
                    ArrType::ArrayAT(box ArrayATData {
                        elem_t,
                        tuple_view: None,
                        ..
                    }) => {
                        let extra = format!("Array {}", kid(tvars, elem_t));
                        p(cx, t, true, &extra)
                    }
                    ArrType::ArrayAT(box ArrayATData {
                        elem_t,
                        tuple_view: Some(tv),
                        ..
                    }) => {
                        let elems_str =
                            tuple_elements(tvars, &tv.elements, tv.rest.as_ref(), tv.inexact);
                        let extra = format!("Array {}, [{}]", kid(tvars, elem_t), elems_str);
                        p(cx, t, true, &extra)
                    }
                    ArrType::TupleAT(box TupleATData {
                        elements,
                        inexact,
                        rest,
                        ..
                    }) => {
                        let elems_str = tuple_elements(tvars, elements, rest.as_ref(), *inexact);
                        let extra = format!("Tuple [{}]", elems_str);
                        p(cx, t, true, &extra)
                    }
                    ArrType::ROArrayAT(box ROArrayATData { elem_t, .. }) => {
                        let extra = format!("ReadOnlyArray {}", kid(tvars, elem_t));
                        p(cx, t, true, &extra)
                    }
                }
            }
            DefTInner::ClassT(inst) => {
                let extra = kid(tvars, inst);
                p(cx, t, true, &extra)
            }
            DefTInner::InstanceT(inst) => {
                let extra = instance_t(tvars, inst.as_ref());
                p(cx, t, true, &extra)
            }
            DefTInner::TypeT(kind, arg) => {
                let extra = format!(
                    "{}, {}",
                    type_::string_of_type_t_kind(kind),
                    kid(tvars, arg)
                );
                p(cx, t, true, &extra)
            }
            DefTInner::EnumValueT(enum_info) => match &***enum_info {
                EnumInfoInner::AbstractEnum { representation_t } => {
                    let extra = format!("abstract: {}", kid(tvars, representation_t));
                    p(cx, t, true, &extra)
                }
                EnumInfoInner::ConcreteEnum(info) => {
                    let extra = format!(
                        "enum concrete: {} #{}",
                        info.enum_name.as_str(),
                        info.enum_id.0.debug_to_string(false)
                    );
                    p(cx, t, true, &extra)
                }
            },
            DefTInner::EnumObjectT { enum_info, .. } => match &***enum_info {
                EnumInfoInner::AbstractEnum { representation_t } => {
                    let extra = format!("abstract: {}", kid(tvars, representation_t));
                    p(cx, t, true, &extra)
                }
                EnumInfoInner::ConcreteEnum(info) => {
                    let extra = format!(
                        "enum concrete: {} #{}",
                        info.enum_name.as_str(),
                        info.enum_id.0.debug_to_string(false)
                    );
                    p(cx, t, true, &extra)
                }
            },
            DefTInner::NumericStrKeyT(num) => {
                let extra = num.1.as_str();
                p(cx, t, true, extra)
            }
            DefTInner::SingletonStrT { from_annot, value } => {
                let extra = format!("{:?} (from_annot={})", value, from_annot);
                p(cx, t, true, &extra)
            }
            DefTInner::SingletonNumT { from_annot, value } => {
                let extra = format!("{} (from_annot={})", value.1.as_str(), from_annot);
                p(cx, t, true, &extra)
            }
            DefTInner::SingletonBoolT { value, .. } => {
                let extra = format!("{}", value);
                p(cx, t, true, &extra)
            }
            DefTInner::SingletonBigIntT { value, .. } => {
                let extra = value.1.as_str();
                p(cx, t, true, extra)
            }
            DefTInner::ReactAbstractComponentT(_) => p(cx, t, true, ""),
            DefTInner::RendersT(renders) => {
                let extra = match renders.as_ref() {
                    CanonicalRendersForm::IntrinsicRenders(n) => format!("intrinsic {}", n),
                    CanonicalRendersForm::NominalRenders { renders_name, .. } => {
                        format!("Nominal({})", renders_name)
                    }
                    CanonicalRendersForm::StructuralRenders {
                        renders_variant,
                        renders_structural_type,
                    } => {
                        let variant = match renders_variant {
                            RendersVariant::RendersNormal => "Normal",
                            RendersVariant::RendersMaybe => "Maybe",
                            RendersVariant::RendersStar => "Star",
                        };
                        format!(
                            "Structural({}, {})",
                            variant,
                            kid(tvars, renders_structural_type)
                        )
                    }
                    CanonicalRendersForm::DefaultRenders => "Default".to_string(),
                };
                p(cx, t, true, &extra)
            }
        },
        TypeInner::AnnotT(_, arg, use_desc) => {
            let extra = format!("use_desc={}, {}", use_desc, kid(tvars, arg));
            p(cx, t, true, &extra)
        }
        TypeInner::NominalT { nominal_type, .. } => {
            let type_args_str: Vec<String> = nominal_type
                .nominal_type_args
                .iter()
                .map(|(n, _, t, _)| format!("{}={}", n, kid(tvars, t)))
                .collect();
            let underlying_str = match &nominal_type.underlying_t {
                nominal::UnderlyingT::OpaqueWithLocal { t } => {
                    format!(" ({})", kid(tvars, t))
                }
                nominal::UnderlyingT::CustomError(box nominal::CustomErrorData {
                    t,
                    custom_error_loc,
                }) => {
                    format!(
                        " ({}, custom_error_loc={})",
                        kid(tvars, t),
                        custom_error_loc.debug_to_string(false)
                    )
                }
                nominal::UnderlyingT::FullyOpaque => String::new(),
            };
            let extra = format!("[{}]{}", type_args_str.join("; "), underlying_str);
            p(cx, t, true, &extra)
        }
        TypeInner::AnyT(_, src) => {
            let extra = string_of_any_source(src);
            p(cx, t, true, extra)
        }
        TypeInner::NullProtoT(_) | TypeInner::ObjProtoT(_) | TypeInner::FunProtoT(_) => {
            p(cx, t, true, "")
        }
        TypeInner::ThisInstanceT(box ThisInstanceTData { instance, .. }) => {
            let extra = instance_t(tvars, instance);
            p(cx, t, true, &extra)
        }
        TypeInner::GenericT(box GenericTData { name, bound, .. }) => {
            let extra = format!("{}: {}", name, kid(tvars, bound));
            p(cx, t, true, &extra)
        }
        TypeInner::OptionalT { type_, .. } => {
            let extra = kid(tvars, type_);
            p(cx, t, true, &extra)
        }
        TypeInner::EvalT {
            type_,
            defer_use_t,
            id,
        } => {
            let arg_str = kid(tvars, type_);
            let defer_str = defer_use(defer_use_t, &arg_str);
            let extra = format!("{}, {}", defer_str, id.debug_string());
            p(cx, t, true, &extra)
        }
        TypeInner::TypeAppT(box TypeAppTData { type_, targs, .. }) => {
            let type_str = kid(tvars, type_);
            let targs_str: Vec<String> = targs.iter().map(|t| kid(tvars, t)).collect();
            let extra = format!("{}, [{}]", type_str, targs_str.join("; "));
            p(cx, t, true, &extra)
        }
        TypeInner::ThisTypeAppT(box ThisTypeAppTData {
            type_,
            this_t,
            targs,
            ..
        }) => {
            let base_str = kid(tvars, type_);
            let this_str = kid(tvars, this_t);
            let extra = match targs {
                Some(args) => {
                    let args_str: Vec<String> = args.iter().map(|t| kid(tvars, t)).collect();
                    format!("{}, {}, [{}]", base_str, this_str, args_str.join("; "))
                }
                None => format!("{}, {}", base_str, this_str),
            };
            p(cx, t, false, &extra)
        }
        TypeInner::MaybeT(_, arg) => {
            let extra = kid(tvars, arg);
            p(cx, t, true, &extra)
        }
        TypeInner::IntersectionT(_, rep) => {
            let members: Vec<String> = rep.members_iter().map(|t| kid(tvars, t)).collect();
            let extra = format!("[{}]", members.join("; "));
            p(cx, t, true, &extra)
        }
        TypeInner::UnionT(_, rep) => {
            let kind_str = match rep.union_kind() {
                type_::union_rep::UnionKind::ProvidersKind => "ProvidersKind",
                type_::union_rep::UnionKind::ConditionalKind => "ConditionalKind",
                type_::union_rep::UnionKind::ImplicitInstantiationKind => {
                    "ImplicitInstantiationKind"
                }
                type_::union_rep::UnionKind::ResolvedKind => "ResolvedKind",
                type_::union_rep::UnionKind::LogicalKind => "LogicalKind",
                type_::union_rep::UnionKind::UnknownKind => "UnknownKind",
            };
            let members: Vec<String> = rep.members_iter().map(|t| kid(tvars, t)).collect();
            let spec_str = rep.string_of_specialization();
            let extra = format!("(kind={})[{}]{}", kind_str, members.join("; "), spec_str);
            p(cx, t, true, &extra)
        }
        TypeInner::KeysT(_, arg) => {
            let extra = kid(tvars, arg);
            p(cx, t, true, &extra)
        }
        TypeInner::TemplateLiteralT { quasis, types, .. } => {
            let extra = format!(
                "quasis:[{}], types:[{}]",
                quasis
                    .iter()
                    .map(|s| format!("{:?}", s.as_str()))
                    .collect::<Vec<_>>()
                    .join("; "),
                types
                    .iter()
                    .map(|t| kid(tvars, t))
                    .collect::<Vec<_>>()
                    .join("; ")
            );
            p(cx, t, true, &extra)
        }
        TypeInner::StringMappingT { kind, arg, .. } => {
            // Inlined rather than calling
            // `flow_typing_flow_common::string_case_transform::name_of_kind` —
            // this crate is a dependency of `flow_typing_flow_common` (where
            // `string_case_transform` lives).
            let kind_s = match kind {
                flow_typing_type::type_::StringMappingKind::StringMappingUppercase => "Uppercase",
                flow_typing_type::type_::StringMappingKind::StringMappingLowercase => "Lowercase",
                flow_typing_type::type_::StringMappingKind::StringMappingCapitalize => "Capitalize",
                flow_typing_type::type_::StringMappingKind::StringMappingUncapitalize => {
                    "Uncapitalize"
                }
            };
            let extra = format!("{}<{}>", kind_s, kid(tvars, arg));
            p(cx, t, true, &extra)
        }
        TypeInner::NamespaceT(ns) => {
            let extra = format!(
                "name={}, values={}, types={}",
                dump_symbol(&ns.namespace_symbol),
                kid(tvars, &ns.values_type),
                ns.types_tmap.debug_string()
            );
            p(cx, t, true, &extra)
        }
    }
}

fn dump_use_t_(depth: u32, tvars: &mut BTreeSet<i32>, cx: &Context, use_t: &UseT<Cx>) -> String {
    if depth == 0 {
        return string_of_use_ctor(use_t);
    }

    fn p(_cx: &Context, use_t: &UseT<Cx>, reason: bool, extra: &str) -> String {
        let ctor = string_of_use_ctor(use_t);
        let reason_str = String::new();
        let sep = if reason && !extra.is_empty() {
            ", "
        } else {
            ""
        };
        format!("{}({}{}{})", ctor, reason_str, sep, extra)
    }

    let kid =
        move |tvars: &mut BTreeSet<i32>, t: &Type| -> String { dump_t_(depth - 1, tvars, cx, t) };

    let use_kid = move |tvars: &mut BTreeSet<i32>, u: &UseT<Cx>| -> String {
        dump_use_t_(depth - 1, tvars, cx, u)
    };

    let tvar = move |tvars: &mut BTreeSet<i32>, id: i32| -> String {
        dump_tvar_(depth - 1, tvars, cx, id)
    };

    let normalized_prop = move |tvars: &mut BTreeSet<i32>, p: &PropertyType| -> String {
        dump_normalized_prop_(depth - 1, tvars, cx, p)
    };

    let call_arg_kid = |tvars: &mut BTreeSet<i32>, arg: &type_::CallArg| -> String {
        match arg.deref() {
            CallArgInner::Arg(t) => kid(tvars, t),
            CallArgInner::SpreadArg(t) => format!("...{}", kid(tvars, t)),
        }
    };

    let propref = |tvars: &mut BTreeSet<i32>, prop: &type_::PropRef| -> String {
        match prop {
            type_::PropRef::Named { reason, name, .. } => {
                format!("{:?} {}", dump_reason(cx, reason), name)
            }
            type_::PropRef::Computed(t) => kid(tvars, t),
        }
    };

    let lookup_kind = |tvars: &mut BTreeSet<i32>, kind: &type_::LookupKind| -> String {
        match kind {
            type_::LookupKind::NonstrictReturning(box type_::NonstrictReturningData(
                default_opt,
                testid_opt,
            )) => {
                let default_str = match default_opt {
                    Some((t, _)) => format!(" returning {}", kid(tvars, t)),
                    None => String::new(),
                };
                let testid_str = match testid_opt {
                    Some((id, _)) => format!(" for test id {}", id),
                    None => String::new(),
                };
                format!("Nonstrict{}{}", default_str, testid_str)
            }
            type_::LookupKind::Strict(t) => {
                format!("Strict {:?}", dump_reason(cx, reason_of_t(t)))
            }
        }
    };

    let lookup_action = |tvars: &mut BTreeSet<i32>, action: &type_::LookupAction| -> String {
        match action {
            type_::LookupAction::ReadProp(box type_::ReadPropData { tout, .. }) => {
                format!(
                    "Read ({}, {})",
                    string_of_reason(cx, tout.reason()),
                    tvar(tvars, tout.id() as i32)
                )
            }
            type_::LookupAction::WriteProp(box type_::WritePropData { tin, .. }) => {
                format!("Write {}", kid(tvars, tin))
            }
            type_::LookupAction::LookupPropForTvarPopulation { tout, polarity } => {
                format!(
                    "LookupPropForTvarPopulation ({:?}, {})",
                    polarity,
                    kid(tvars, tout)
                )
            }
            type_::LookupAction::LookupPropsForSubtyping(
                box type_::LookupPropsForSubtypingData { use_op, props, .. },
            ) => {
                let props = props
                    .iter()
                    .map(|(_, prop)| normalized_prop(tvars, &type_::property::property_type(prop)))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!(
                    "LookupPropsForSubtyping ({}, [{}])",
                    string_of_use_op(use_op),
                    props
                )
            }
            type_::LookupAction::SuperProp(box (_, p, _, _)) => {
                format!("Super {}", dump_prop_(depth - 1, tvars, cx, p))
            }
            type_::LookupAction::MatchProp(box type_::LookupActionMatchPropData {
                prop_t, ..
            }) => {
                format!("Match {}", kid(tvars, prop_t))
            }
        }
    };

    let react_kit = |tvars: &mut BTreeSet<i32>, tool: &type_::react::Tool<Cx>| -> String {
        match tool {
            type_::react::Tool::CreateElement(box type_::react::CreateElementData {
                jsx_props,
                tout,
                ..
            }) => {
                let jsx_str = kid(tvars, jsx_props);
                let tout_str = tvar(tvars, tout.id() as i32);
                format!("CreateElement ({}) => {}", jsx_str, tout_str)
            }
            type_::react::Tool::ConfigCheck { props } => {
                format!("ConfigCheck ({})", kid(tvars, props))
            }
            type_::react::Tool::GetConfig { tout } => {
                format!("GetConfig ({})", kid(tvars, tout))
            }
        }
    };

    let slice = |s: &type_::object::Slice| -> String {
        let mut xs: Vec<String> = Vec::new();
        match &s.flags.obj_kind {
            ObjKind::Indexed(d) => {
                let variance = match d.dict_polarity {
                    Polarity::Positive => "readonly ",
                    Polarity::Negative => "writeonly ",
                    Polarity::Neutral => "",
                };
                xs.push(format!("{}[]", variance));
            }
            ObjKind::Exact | ObjKind::Inexact => {}
        }
        for (k, prop) in &s.props {
            let opt = if matches!(
                type_::property::type_(prop).deref(),
                TypeInner::OptionalT { .. }
            ) {
                "?"
            } else {
                ""
            };
            xs.push(format!("{}{}", k, opt));
        }
        let xs_str = xs.join("; ");
        format!("{{{}}}", xs_str)
    };

    let operand_slice = |reason: &Reason,
                         prop_map: &type_::properties::PropertiesMap,
                         dict: &type_::object::Dict|
     -> String {
        use type_::property::first_loc;
        use type_::property::read_t;
        use type_::property::write_t;
        let props: type_::properties::PropertiesMap = prop_map
            .iter()
            .filter_map(|(k, p)| match (read_t(p), write_t(p)) {
                (Some(t), _) | (_, Some(t)) => Some((
                    k.clone(),
                    type_::Property::new(type_::PropertyInner::Field(Box::new(type_::FieldData {
                        preferred_def_locs: None,
                        key_loc: first_loc(p),
                        type_: t,
                        polarity: Polarity::Neutral,
                    }))),
                )),
                _ => None,
            })
            .collect();
        let obj_kind = match dict {
            None => ObjKind::Exact,
            Some(d) => ObjKind::Indexed(d.clone()),
        };
        let flags = type_::Flags {
            obj_kind,
            react_dro: None,
        };
        let s = type_::object::Slice {
            reason: reason.clone(),
            strictness_kind: type_::TypeStrictnessKind::Flow,
            props,
            flags,
            frozen: false,
            generics: flow_typing_generics::spread_empty(),
            interface: None,
            reachable_targs: Rc::from([]),
            literal: false,
            exact_object_type: false,
        };
        slice(&s)
    };

    let object_kit = |tvars: &mut BTreeSet<i32>,
                      resolve_tool: &type_::object::ResolveTool,
                      tool: &type_::object::Tool|
     -> String {
        let join = |j: &type_::object::Join| -> &'static str {
            match j.1 {
                type_::object::JoinOp::And => "And",
                type_::object::JoinOp::Or => "Or",
            }
        };
        let resolved = |xs: &type_::object::Resolved| -> String {
            let slices: Vec<String> = xs.0.iter().map(slice).collect();
            format!("[{}]", slices.join("; "))
        };
        let resolve = |tvars: &mut BTreeSet<i32>, r: &type_::object::Resolve| -> String {
            match r {
                type_::object::Resolve::Next => "Next".to_string(),
                type_::object::Resolve::List0(todo, j) => {
                    let kids: Vec<String> = todo.iter().map(|t| kid(tvars, t)).collect();
                    format!("List0 ([{}], {})", kids.join("; "), join(j))
                }
                type_::object::Resolve::List(todo, done_rev, j) => {
                    let kids: Vec<String> = todo.iter().map(|t| kid(tvars, t)).collect();
                    let done_strs: Vec<String> = done_rev.iter().map(resolved).collect();
                    format!(
                        "List ([{}], [{}], {})",
                        kids.join("; "),
                        done_strs.join("; "),
                        join(j)
                    )
                }
            }
        };
        let resolve_tool_str = match resolve_tool {
            type_::object::ResolveTool::Resolve(r) => {
                format!("Resolve {}", resolve(tvars, r))
            }
            type_::object::ResolveTool::Super(s, pending, r) => {
                let kids: Vec<String> = pending.iter().map(|t| kid(tvars, t)).collect();
                format!(
                    "Super ({}, [{}], {})",
                    slice(s),
                    kids.join("; "),
                    resolve(tvars, r)
                )
            }
        };
        let acc_element = |elem: &type_::object::spread::AccElement| -> String {
            match elem {
                type_::object::spread::AccElement::InlineSlice(os) => {
                    operand_slice(&os.reason, &os.prop_map, &os.dict)
                }
                type_::object::spread::AccElement::ResolvedSlice(xs) => resolved(xs),
            }
        };
        let spread_fn = |tvars: &mut BTreeSet<i32>,
                         target: &type_::object::spread::Target,
                         state: &type_::object::spread::State|
         -> String {
            let target_str = match target {
                type_::object::spread::Target::Annot { make_exact, .. } => {
                    format!("Annot {{ make_exact={} }}", make_exact)
                }
                type_::object::spread::Target::Value { make_seal, .. } => {
                    format!("Value {{make_seal={}", bool_of_sealtype(make_seal))
                }
            };
            let spread_operand =
                |tvars: &mut BTreeSet<i32>, op: &type_::object::spread::Operand| -> String {
                    match op {
                        type_::object::spread::Operand::Slice(os) => {
                            operand_slice(&os.reason, &os.prop_map, &os.dict)
                        }
                        type_::object::spread::Operand::Type(t) => kid(tvars, t),
                    }
                };
            let todo_strs: Vec<String> = state
                .todo_rev
                .iter()
                .map(|op| spread_operand(tvars, op))
                .collect();
            let acc_strs: Vec<String> = state.acc.iter().map(acc_element).collect();
            let union_reason_str = match &state.union_t {
                Some(t) => dump_reason(cx, reason_of_t(t)),
                None => "None".to_string(),
            };
            format!(
                "Spread ({}, {{todo_rev=[{}]; acc=[{}]; spread_id={}; curr_resolve_idx={}; union_reason={}}})",
                target_str,
                todo_strs.join("; "),
                acc_strs.join("; "),
                state.spread_id,
                state.curr_resolve_idx,
                union_reason_str
            )
        };
        let rest_fn = |tvars: &mut BTreeSet<i32>,
                       merge_mode: &type_::object::rest::MergeMode,
                       state: &type_::object::rest::State|
         -> String {
            let merge_str = match merge_mode {
                type_::object::rest::MergeMode::SpreadReversal => "SpreadReversal",
                type_::object::rest::MergeMode::Omit => "Omit",
                type_::object::rest::MergeMode::ReactConfigMerge(_) => "ReactConfigMerge",
            };
            let state_str = match state {
                type_::object::rest::State::One(t) => format!("One ({})", kid(tvars, t)),
                type_::object::rest::State::Done(o) => format!("Done ({})", resolved(o)),
            };
            format!("Rest ({{merge_mode={}}}, {})", merge_str, state_str)
        };
        let react_props_fn = |state: &type_::object::react_config::State,
                              ref_manipulation: &type_::object::react_config::RefManipulation|
         -> String {
            let state_str = match state {
                type_::object::react_config::State::Config { .. } => "Config",
                type_::object::react_config::State::Defaults { .. } => "Defaults",
            };
            let ref_str = match ref_manipulation {
                type_::object::react_config::RefManipulation::KeepRef => "keep",
                type_::object::react_config::RefManipulation::AddRef(_) => "add",
            };
            format!("({}, ref_manipulation={})", state_str, ref_str)
        };
        let object_map_fn = |tvars: &mut BTreeSet<i32>, prop_type: &Type| -> String {
            format!("ObjectMap {{prop_type: {}}}", kid(tvars, prop_type))
        };
        let tool_str = match tool {
            type_::object::Tool::MakeExact => "MakeExact".to_string(),
            type_::object::Tool::ReadOnly => "ReadOnly".to_string(),
            type_::object::Tool::ReactCheckComponentConfig { .. } => {
                "ReactCheckComponentConfig".to_string()
            }
            type_::object::Tool::Partial => "Partial".to_string(),
            type_::object::Tool::Required => "Required".to_string(),
            type_::object::Tool::ObjectRep => "ObjectRep".to_string(),
            type_::object::Tool::Spread(box (target, state)) => spread_fn(tvars, target, state),
            type_::object::Tool::Rest(box (merge_mode, state)) => rest_fn(tvars, merge_mode, state),
            type_::object::Tool::ReactConfig(box type_::object::ObjectToolReactConfigData {
                state,
                ref_manipulation,
                literal: _,
            }) => react_props_fn(state, ref_manipulation),
            type_::object::Tool::ObjectMap(box type_::object::ObjectToolObjectMapData {
                prop_type,
                ..
            }) => object_map_fn(tvars, prop_type),
        };
        format!("({}, {})", resolve_tool_str, tool_str)
    };

    let method_action = |tvars: &mut BTreeSet<i32>, action: &type_::MethodAction<Cx>| -> String {
        match action {
            type_::MethodAction::CallM(box type_::CallMData { methodcalltype, .. })
            | type_::MethodAction::ChainM(box type_::ChainMData { methodcalltype, .. }) => {
                let this_str = match &methodcalltype.meth_generic_this {
                    Some(t) => kid(tvars, t),
                    None => "None".to_string(),
                };
                let args: Vec<String> = methodcalltype
                    .meth_args_tlist
                    .iter()
                    .map(|arg| call_arg_kid(tvars, arg))
                    .collect();
                let tout_str = tvar(tvars, methodcalltype.meth_tout.id() as i32);
                format!(
                    "<this: {}>({}) => ({}, {})",
                    this_str,
                    args.join("; "),
                    string_of_reason(cx, methodcalltype.meth_tout.reason()),
                    tout_str
                )
            }
            type_::MethodAction::NoMethodAction(_) => "NoMethodAction".to_string(),
        }
    };

    match use_t.deref() {
        UseTInner::UseT(use_op, t) => match t.deref() {
            TypeInner::OpenT(tv) => {
                format!(
                    "UseT ({}, OpenT ({:?}, {}))",
                    string_of_use_op(use_op),
                    dump_reason(cx, tv.reason()),
                    tv.id()
                )
            }
            _ => {
                let extra = kid(tvars, t);
                format!("UseT ({}, {})", string_of_use_op(use_op), extra)
            }
        },
        UseTInner::ArrRestT(box ArrRestTData { use_op, .. }) => {
            p(cx, use_t, true, &string_of_use_op(use_op))
        }
        UseTInner::CallElemT(box CallElemTData { .. }) => p(cx, use_t, true, ""),
        UseTInner::CallT(box CallTData {
            use_op,
            call_action,
            ..
        }) => match call_action.as_ref() {
            type_::CallAction::Funcalltype(funcall) => {
                let this_str = kid(tvars, &funcall.call_this_t);
                let args: Vec<String> = funcall
                    .call_args_tlist
                    .iter()
                    .map(|arg| match arg.deref() {
                        CallArgInner::Arg(t) => kid(tvars, t),
                        CallArgInner::SpreadArg(t) => {
                            format!("...{}", kid(tvars, t))
                        }
                    })
                    .collect();
                let tout_str = tvar(tvars, funcall.call_tout.id() as i32);
                let extra = format!(
                    "{}, <this: {}>({}) => ({}, {})",
                    string_of_use_op(use_op),
                    this_str,
                    args.join("; "),
                    string_of_reason(cx, funcall.call_tout.reason()),
                    tout_str
                );
                p(cx, use_t, true, &extra)
            }
            type_::CallAction::ConcretizeCallee(_) => {
                let extra = format!("{} ConcretizeCallee", string_of_use_op(use_op));
                p(cx, use_t, true, &extra)
            }
        },
        UseTInner::ConstructorT(..) => p(cx, use_t, true, ""),
        UseTInner::ElemT(box ElemTData { obj, .. }) => {
            let extra = format!("obj: {}", kid(tvars, obj));
            p(cx, use_t, true, &extra)
        }
        UseTInner::ConditionalT(box ConditionalTData {
            distributive_tparam_name,
            infer_tparams,
            extends_t,
            true_t,
            false_t,
            tout,
            ..
        }) => {
            let dist = distributive_tparam_name
                .as_ref()
                .map(|n| format!("distributive over {}: ", n))
                .unwrap_or_default();
            let infer_str: Vec<String> =
                infer_tparams.iter().map(|tp| tp.name.to_string()).collect();
            let extra = format!(
                "{}[{}] extends {} ? {} : {} => {}",
                dist,
                infer_str.join("; "),
                kid(tvars, extends_t),
                kid(tvars, true_t),
                kid(tvars, false_t),
                tvar(tvars, tout.id() as i32)
            );
            p(cx, use_t, true, &extra)
        }
        UseTInner::GetElemT(box GetElemTData {
            from_annot,
            access_iterables,
            key_t,
            tout,
            ..
        }) => {
            let extra = format!(
                "{}, ({}, {}), from_annot={}, access_iterables={}",
                kid(tvars, key_t),
                string_of_reason(cx, tout.reason()),
                tvar(tvars, tout.id() as i32),
                from_annot,
                access_iterables
            );
            p(cx, use_t, true, &extra)
        }
        UseTInner::GetKeysT { .. }
        | UseTInner::GetKeysDictKeyT { .. }
        | UseTInner::GetValuesT(_, _)
        | UseTInner::GetDictValuesT(_, _) => p(cx, use_t, true, ""),
        UseTInner::GetPropT(data) => {
            let extra = format!(
                "{}, ({}), ({}, {})",
                string_of_use_op(&data.use_op),
                propref(tvars, &data.propref),
                string_of_reason(cx, data.tout.reason()),
                tvar(tvars, data.tout.id() as i32)
            );
            p(cx, use_t, true, &extra)
        }
        UseTInner::GetPrivatePropT(data) => {
            let extra = format!(
                "({}), ({}, {})",
                data.name,
                string_of_reason(cx, data.tout.reason()),
                tvar(tvars, data.tout.id() as i32)
            );
            p(cx, use_t, true, &extra)
        }
        UseTInner::GetProtoT(..) => p(cx, use_t, true, ""),
        UseTInner::GetStaticsT(..) => p(cx, use_t, true, ""),
        UseTInner::HasOwnPropT(box HasOwnPropTData { .. }) | UseTInner::ConcretizeT { .. } => {
            p(cx, use_t, true, "")
        }
        UseTInner::LookupT(box LookupTData {
            propref: prop,
            lookup_kind: kind,
            lookup_action: action,
            ids,
            ..
        }) => {
            let ids_str = match ids {
                None => "None".to_string(),
                Some(_ids) => "Some [...]".to_string(),
            };
            let extra = format!(
                "{:?}, {}, {}, [{}]",
                propref(tvars, prop),
                lookup_kind(tvars, kind),
                lookup_action(tvars, action),
                ids_str
            );
            p(cx, use_t, true, &extra)
        }
        UseTInner::MapTypeT(box MapTypeTData { .. }) => p(cx, use_t, true, ""),
        UseTInner::MethodT(box MethodTData {
            propref: prop,
            method_action: action,
            ..
        }) => {
            let extra = format!(
                "({}, {})",
                propref(tvars, prop),
                method_action(tvars, action)
            );
            p(cx, use_t, true, &extra)
        }
        UseTInner::PrivateMethodT(data) => {
            let extra = format!(
                "({}), ({})",
                data.name,
                method_action(tvars, &data.method_action)
            );
            p(cx, use_t, true, &extra)
        }
        UseTInner::MixinT(..) => p(cx, use_t, true, ""),
        UseTInner::ObjRestT(_, xs, arg, _, _) => {
            let extra = format!("[{}], {}", xs.join("; "), kid(tvars, arg));
            p(cx, use_t, true, &extra)
        }
        UseTInner::ObjTestProtoT(_, _) => p(cx, use_t, true, ""),
        UseTInner::OptionalIndexedAccessT(box OptionalIndexedAccessTData { index, .. }) => {
            let extra = match index {
                type_::OptionalIndexedAccessIndex::OptionalIndexedAccessTypeIndex(t) => {
                    kid(tvars, t)
                }
                type_::OptionalIndexedAccessIndex::OptionalIndexedAccessStrLitIndex(n) => {
                    n.to_string()
                }
            };
            p(cx, use_t, true, &extra)
        }
        UseTInner::ReactKitT(box ReactKitTData { use_op, tool, .. }) => {
            let extra = format!("{}, {}", string_of_use_op(use_op), react_kit(tvars, tool));
            p(cx, use_t, true, &extra)
        }
        UseTInner::ReposLowerT {
            use_desc,
            use_t: arg,
            ..
        } => {
            let extra = format!("use_desc={}, {}", use_desc, use_kid(tvars, arg));
            p(cx, use_t, true, &extra)
        }
        UseTInner::ReposUseT(box ReposUseTData {
            use_desc,
            use_op,
            type_: arg,
            ..
        }) => {
            let inner = UseT::new(UseTInner::UseT(use_op.clone(), arg.clone()));
            let extra = format!("use_desc={}, {}", use_desc, use_kid(tvars, &inner));
            p(cx, use_t, true, &extra)
        }
        UseTInner::ResolveSpreadT(box ResolveSpreadTData {
            use_op,
            resolve_spread_type: spread,
            ..
        }) => {
            let extra = match &spread.rrt_resolve_to {
                type_::SpreadResolve::ResolveSpreadsToTupleType { elem_t, tout, .. }
                | type_::SpreadResolve::ResolveSpreadsToArrayLiteral { elem_t, tout, .. } => {
                    format!(
                        "{}, {}, {}",
                        string_of_use_op(use_op),
                        kid(tvars, elem_t),
                        kid(tvars, tout)
                    )
                }
                type_::SpreadResolve::ResolveSpreadsToArray { elem_t, tout, .. } => {
                    format!(
                        "{}, {}, {}",
                        string_of_use_op(use_op),
                        kid(tvars, elem_t),
                        kid(tvars, tout)
                    )
                }
                type_::SpreadResolve::ResolveSpreadsToMultiflowCallFull(_, _)
                | type_::SpreadResolve::ResolveSpreadsToMultiflowSubtypeFull(_, _) => {
                    string_of_use_op(use_op)
                }
            };
            p(cx, use_t, true, &extra)
        }
        UseTInner::SuperT(box SuperTData { .. }) => p(cx, use_t, true, ""),
        UseTInner::ImplementsT(_, arg) => {
            let extra = kid(tvars, arg);
            p(cx, use_t, false, &extra)
        }
        UseTInner::SetElemT(box SetElemTData {
            key_t: ix,
            tin: etype,
            ..
        }) => {
            let extra = format!("{}, {}", kid(tvars, ix), kid(tvars, etype));
            p(cx, use_t, true, &extra)
        }
        UseTInner::SetPropT(use_op, _, prop, _, _, ptype, _) => {
            let extra = format!(
                "{}, ({}), {}",
                string_of_use_op(use_op),
                propref(tvars, prop),
                kid(tvars, ptype)
            );
            p(cx, use_t, true, &extra)
        }
        UseTInner::SetPrivatePropT(data) => {
            let extra = format!("({}), {}", data.name, kid(tvars, &data.tin));
            p(cx, use_t, true, &extra)
        }
        UseTInner::SpecializeT(box SpecializeTData {
            targs: args_opt,
            upper,
            ..
        }) => {
            let ret = match upper {
                Some(box type_::Cont::Upper(ret)) => dump_use_t_(depth - 1, tvars, cx, ret),
                Some(box type_::Cont::Lower(_, ret)) => kid(tvars, ret),
                Some(box type_::Cont::Collect(_)) => "TypeCollector".to_owned(),
                None => "None".to_owned(),
            };
            let extra = match args_opt {
                Some(args) => {
                    let args_str: Vec<String> = args.iter().map(|t| kid(tvars, t)).collect();
                    format!("[{}], {}", args_str.join("; "), ret)
                }
                None => ret,
            };
            p(cx, use_t, true, &extra)
        }
        UseTInner::ObjKitT(use_op, _, resolve, tool, tout) => {
            let extra = format!(
                "{}, {}, {}",
                string_of_use_op(use_op),
                object_kit(tvars, resolve, tool),
                kid(tvars, tout)
            );
            p(cx, use_t, true, &extra)
        }
        UseTInner::TestPropT(box TestPropTData {
            use_op,
            propref: prop,
            tout,
            ..
        }) => {
            let extra = format!(
                "{}, ({}), ({}, {})",
                string_of_use_op(use_op),
                propref(tvars, prop),
                string_of_reason(cx, tout.reason()),
                tvar(tvars, tout.id() as i32)
            );
            p(cx, use_t, true, &extra)
        }
        UseTInner::ThisSpecializeT(_, this, _) => {
            let extra = kid(tvars, this);
            p(cx, use_t, true, &extra)
        }
        UseTInner::ToStringT { t_out, .. } => {
            let extra = use_kid(tvars, t_out);
            p(cx, use_t, true, &extra)
        }
        UseTInner::ValueToTypeReferenceT(box ValueToTypeReferenceTData {
            use_op,
            kind,
            tout,
            ..
        }) => {
            let extra = format!(
                "{}, reason, {}, {}",
                string_of_use_op(use_op),
                type_::string_of_type_t_kind(kind),
                tvar(tvars, tout.id() as i32)
            );
            p(cx, use_t, true, &extra)
        }
        UseTInner::ConcretizeTypeAppsT(_, _, _, _) => p(cx, use_t, true, ""),
        UseTInner::GetEnumT(box GetEnumTData { kind, .. }) => {
            let extra = match kind {
                type_::GetEnumKind::GetEnumObject => "get enum object",
                type_::GetEnumKind::GetEnumValue => "get enum value",
            };
            p(cx, use_t, true, extra)
        }
        UseTInner::FilterOptionalT(_, arg) => {
            let extra = kid(tvars, arg);
            p(cx, use_t, false, &extra)
        }
        UseTInner::FilterMaybeT(_, arg) => {
            let extra = kid(tvars, arg);
            p(cx, use_t, false, &extra)
        }
        UseTInner::DeepReadOnlyT(tv, _) => {
            let extra = tvar(tvars, tv.id() as i32);
            p(cx, use_t, true, &extra)
        }
        UseTInner::HooklikeT(tv) => {
            let extra = tvar(tvars, tv.id() as i32);
            p(cx, use_t, true, &extra)
        }
        UseTInner::SealGenericT(box SealGenericTData { name, cont, .. }) => {
            let extra = match cont {
                type_::Cont::Lower(_, l) => {
                    format!("{} <~ {}", name, kid(tvars, l))
                }
                type_::Cont::Upper(u) => {
                    format!("{} ~> {}", name, use_kid(tvars, u))
                }
                type_::Cont::Collect(_) => format!("{} ~> TypeCollector", name),
            };
            p(cx, use_t, true, &extra)
        }
        UseTInner::CondT(box CondTData {
            opt_type: then_t,
            true_t: else_t,
            ..
        }) => {
            let then_str = match then_t {
                Some(t) => format!("Some ({})", kid(tvars, t)),
                None => "None".to_string(),
            };
            let extra = format!("{}, {}", then_str, kid(tvars, else_t));
            p(cx, use_t, true, &extra)
        }
        UseTInner::ExtendsUseT(box ExtendsUseTData {
            targs: nexts,
            true_t: l,
            false_t: u,
            ..
        }) => {
            let nexts_str: Vec<String> = nexts.iter().map(|t| kid(tvars, t)).collect();
            let extra = format!(
                "[{}], {}, {}",
                nexts_str.join("; "),
                kid(tvars, l),
                kid(tvars, u)
            );
            p(cx, use_t, true, &extra)
        }
        UseTInner::ResolveUnionT(box ResolveUnionTData {
            resolved,
            unresolved,
            upper,
            id,
            ..
        }) => {
            let resolved_str: Vec<String> = resolved.iter().map(|t| kid(tvars, t)).collect();
            let unresolved_str: Vec<String> = unresolved.iter().map(|t| kid(tvars, t)).collect();
            let extra = format!(
                "{} [{}], [{}], {}",
                id,
                resolved_str.join("; "),
                unresolved_str.join("; "),
                use_kid(tvars, upper)
            );
            p(cx, use_t, true, &extra)
        }
        UseTInner::ConvertEmptyPropsToMixedT(_, _) => "ConvertEmptyPropsToMixedT".to_string(),
        UseTInner::ExitRendersT { .. } => "ExitRendersT".to_string(),
        UseTInner::EvalTypeDestructorT(box EvalTypeDestructorTData {
            destructor, tout, ..
        }) => {
            let extra = format!(
                "{} on upper, ({}, {})",
                string_of_destructor(destructor),
                string_of_reason(cx, tout.reason()),
                tvar(tvars, tout.id() as i32)
            );
            p(cx, use_t, true, &extra)
        }
    }
}

fn dump_tvar_(depth: u32, tvars: &mut BTreeSet<i32>, cx: &Context, id: i32) -> String {
    use flow_utils_union_find::Node;
    use type_::constraint::Constraints;

    if tvars.contains(&id) {
        return format!("{}, ^", id);
    }
    tvars.insert(id);

    let node = if cx.is_inference_node(id) {
        Node::create_root(cx.constraints_of(id).1)
    } else {
        cx.find_tvar(id)
    };
    match node {
        Node::Goto { parent } => {
            format!("{}, Goto {}", id, parent)
        }
        Node::Root(root) => match &root.constraints {
            Constraints::Resolved(t) => {
                format!(
                    "{}, Resolved {}",
                    id,
                    dump_t_(depth.saturating_sub(1), tvars, cx, t)
                )
            }
            Constraints::FullyResolved(s) => {
                let payload = match s.forced() {
                    None => "unevaluated".to_string(),
                    Some(t) => dump_t_(depth.saturating_sub(1), tvars, cx, &t),
                };
                format!("{}, FullyResolved {}", id, payload)
            }
            Constraints::Unresolved(bounds) => {
                let bounds = bounds.borrow();
                if bounds.lower.is_empty() && bounds.upper.is_empty() {
                    format!("{}", id)
                } else {
                    let lower_strs: Vec<String> = bounds
                        .lower
                        .keys()
                        .map(|t| dump_t_(depth.saturating_sub(1), tvars, cx, t))
                        .collect();
                    let upper_strs: Vec<String> = bounds
                        .upper
                        .keys()
                        .map(|key| dump_use_t_(depth.saturating_sub(1), tvars, cx, &key.use_t))
                        .collect();
                    format!(
                        "{}, [{}], [{}]",
                        id,
                        lower_strs.join("; "),
                        upper_strs.join("; ")
                    )
                }
            }
        },
    }
}

fn dump_prop_(depth: u32, tvars: &mut BTreeSet<i32>, cx: &Context, p: &Property) -> String {
    match p.deref() {
        PropertyInner::Field(fd) => {
            format!(
                "Field ({}) {}",
                fd.polarity.string(),
                dump_t_(depth, tvars, cx, &fd.type_)
            )
        }
        PropertyInner::Get { type_, .. } => {
            format!("Get {}", dump_t_(depth, tvars, cx, type_))
        }
        PropertyInner::Set { type_, .. } => {
            format!("Set {}", dump_t_(depth, tvars, cx, type_))
        }
        PropertyInner::GetSet(gs) => {
            format!(
                "Get {} Set {}",
                dump_t_(depth, tvars, cx, &gs.get_type),
                dump_t_(depth, tvars, cx, &gs.set_type)
            )
        }
        PropertyInner::Method { type_, .. } => {
            format!("Method {}", dump_t_(depth, tvars, cx, type_))
        }
    }
}

fn dump_normalized_prop_(
    depth: u32,
    tvars: &mut BTreeSet<i32>,
    cx: &Context,
    p: &PropertyType,
) -> String {
    let kid_opt = |t: &Option<Type>, tvars: &mut BTreeSet<i32>| -> String {
        match t {
            Some(t) => dump_t_(depth, tvars, cx, t),
            None => "None".to_string(),
        }
    };

    match p {
        PropertyType::OrdinaryField { type_, polarity } => {
            format!(
                "OrdinaryField ({}) {}",
                polarity.string(),
                dump_t_(depth, tvars, cx, type_)
            )
        }
        PropertyType::SyntheticField { get_type, set_type } => {
            format!(
                "SyntheticField({}, {})",
                kid_opt(get_type, tvars),
                kid_opt(set_type, tvars)
            )
        }
    }
}

// This is the type-dump debugging API.
//  We should make sure these are not called recursively to avoid circumventing
// one of the termination mechanisms: depth or tvar-set.

pub fn dump_t(depth: Option<u32>, cx: &Context, t: &Type) -> String {
    dump_t_(depth.unwrap_or(3), &mut BTreeSet::new(), cx, t)
}

pub fn dump_use_t(depth: Option<u32>, cx: &Context, use_t: &UseT<Cx>) -> String {
    dump_use_t_(depth.unwrap_or(3), &mut BTreeSet::new(), cx, use_t)
}

pub fn dump_prop(depth: Option<u32>, cx: &Context, prop: &Property) -> String {
    dump_prop_(depth.unwrap_or(3), &mut BTreeSet::new(), cx, prop)
}

pub fn dump_normalized_prop(depth: Option<u32>, cx: &Context, prop: &PropertyType) -> String {
    dump_normalized_prop_(depth.unwrap_or(3), &mut BTreeSet::new(), cx, prop)
}

pub fn dump_tvar(depth: Option<u32>, cx: &Context, id: i32) -> String {
    dump_tvar_(depth.unwrap_or(3), &mut BTreeSet::new(), cx, id)
}

pub fn dump_flow(depth: Option<u32>, cx: &Context, t: &Type, use_t: &UseT<Cx>) -> String {
    let depth = depth.unwrap_or(3);
    format!(
        "Lower: {} ~>\n Upper: {}",
        dump_t(Some(depth), cx, t),
        dump_use_t(Some(depth), cx, use_t)
    )
}

// types

pub fn string_of_reason(cx: &Context, reason: &Reason) -> String {
    let strip_root = if cx.should_strip_root() {
        Some(cx.root().to_string_lossy().to_string())
    } else {
        None
    };
    reason::string_of_reason(strip_root.as_deref(), reason)
}

pub fn string_of_file(cx: &Context) -> String {
    let filename = cx.file().to_absolute();
    match cx.is_verbose() {
        false => filename,
        true => {
            let root_str = format!(
                "{}{}",
                cx.root().to_string_lossy(),
                std::path::MAIN_SEPARATOR
            );
            if filename.starts_with(&root_str) {
                flow_common::files::relative_path(std::path::Path::new(&root_str), &filename)
            } else {
                filename
            }
        }
    }
}

pub fn string_of_default<L: Clone + std::fmt::Debug>(default: &Default<L>) -> String {
    flow_typing_default::fold(
        default,
        &|loc| format!("Expr {:?}", loc),
        &|str1, str2| format!("Cons ({}) ({})", str1, str2),
        &mut |_, str, sel| format!("Selector ({}) ({})", str, string_of_selector(&sel)),
    )
}

pub fn string_of_signature_error<L>(
    pp_loc: impl Fn(&L) -> String,
    err: &SignatureError<L>,
) -> String {
    match err {
        SignatureError::ExpectedAnnotation(loc, sort) => {
            format!("Expected annotation at {} @ {}", sort, pp_loc(loc))
        }
        SignatureError::UnexpectedObjectKey(_loc, key_loc) => {
            format!("Expected simple object key @ {}", pp_loc(key_loc))
        }
        SignatureError::UnexpectedArraySpread(_loc, spread_loc) => {
            format!("Unexpected array spread @ {}", pp_loc(spread_loc))
        }
        SignatureError::UnexpectedArrayHole(loc) => {
            format!("Unexpected array hole @ {}", pp_loc(loc))
        }
        SignatureError::EmptyArray(loc) => {
            format!(
                "Cannot determine the element type of an empty array @ {}",
                pp_loc(loc)
            )
        }
        SignatureError::EmptyObject(loc) => {
            format!(
                "Cannot determine types of initialized properties of an empty object @ {}",
                pp_loc(loc)
            )
        }
        SignatureError::UnexpectedExpression(loc, esort) => {
            format!(
                "Cannot determine the type of this {:?} @ {}",
                esort,
                pp_loc(loc)
            )
        }
    }
}

pub fn dump_error_message(cx: &Context, err: &ErrorMessage<ALoc>) -> String {
    fn dump_internal_error(err: &InternalError) -> &'static str {
        match err {
            InternalError::ReadOfUnreachedTvar(_) => "ReadOfUnreachedTvar",
            InternalError::ForcedReadOfUnderResolutionTvar(_) => "ForcedReadOfUnderResolutionTvar",
            InternalError::MethodNotAFunction => "MethodNotAFunction",
            InternalError::OptionalMethod => "OptionalMethod",
            InternalError::PropertyDescriptorPropertyCannotBeRead => {
                "PropertyDescriptorPropertyCannotBeRead"
            }
            InternalError::ForInLHS => "ForInLHS",
            InternalError::ForOfLHS => "ForOfLHS",
            InternalError::PropRefComputedOpen => "PropRefComputedOpen",
            InternalError::PropRefComputedLiteral => "PropRefComputedLiteral",
            InternalError::RestParameterNotIdentifierPattern => "RestParameterNotIdentifierPattern",
            InternalError::InterfaceTypeSpread => "InterfaceTypeSpread",
            InternalError::DebugThrow => "DebugThrow",
            InternalError::ParseJobException(_) => "ParseJobException",
            InternalError::CheckTimeout(_) => "CheckTimeout",
            InternalError::UnexpectedAnnotationInference(_) => "UnexpectedAnnotationInference",
            InternalError::UnexpectedInlineInterfaceType => "UnexpectedInlineInterfaceType",
            InternalError::MissingSwitchExhaustiveCheck => "MissingSwitchExhaustiveCheck",
            InternalError::MissingEnvRead(_) => "MissingEnvRead",
            InternalError::MissingEnvWrite(_) => "MissingEnvWrite",
            InternalError::EnvInvariant(_) => "EnvInvariant",
            InternalError::ImplicitInstantiationInvariant(_) => "ImplicitInstantiationInvariant",
            InternalError::UnresolvedTvarInConditionalType => "UnresolvedTvarInConditionalType",
            InternalError::MethodBivariantInvariant(_) => "MethodBivariantInvariant",
            InternalError::WorkerCanceled => "WorkerCanceled",
        }
    }

    fn dump_upper_kind<L: LocSig>(upper_kind: &UpperKind<L>) -> String {
        match upper_kind {
            UpperKind::IncompatibleGetPropT(_, _) => "IncompatibleGetPropT".to_string(),
            UpperKind::IncompatibleSetPropT(_, _) => "IncompatibleSetPropT".to_string(),
            UpperKind::IncompatibleGetPrivatePropT => "IncompatibleGetPrivatePropT".to_string(),
            UpperKind::IncompatibleSetPrivatePropT => "IncompatibleSetPrivatePropT".to_string(),
            UpperKind::IncompatibleMethodT(_, _) => "IncompatibleMethodT".to_string(),
            UpperKind::IncompatibleCallT => "IncompatibleCallT".to_string(),
            UpperKind::IncompatibleMixedCallT => "IncompatibleMixedCallT".to_string(),
            UpperKind::IncompatibleGetElemT(_) => "IncompatibleGetElemT".to_string(),
            UpperKind::IncompatibleSetElemT(_) => "IncompatibleSetElemT".to_string(),
            UpperKind::IncompatibleCallElemT(_) => "IncompatibleCallElemT".to_string(),
            UpperKind::IncompatibleElemTOfArrT => "IncompatibleElemTOfArrT".to_string(),
            UpperKind::IncompatibleObjAssignFromTSpread => {
                "IncompatibleObjAssignFromTSpread".to_string()
            }
            UpperKind::IncompatibleObjAssignFromT => "IncompatibleObjAssignFromT".to_string(),
            UpperKind::IncompatibleObjRestT => "IncompatibleObjRestT".to_string(),
            UpperKind::IncompatibleArrRestT => "IncompatibleArrRestT".to_string(),
            UpperKind::IncompatibleSuperT => "IncompatibleSuperT".to_string(),
            UpperKind::IncompatibleMixinT => "IncompatibleMixinT".to_string(),
            UpperKind::IncompatibleSpecializeT => "IncompatibleSpecializeT".to_string(),
            UpperKind::IncompatibleThisSpecializeT => "IncompatibleThisSpecializeT".to_string(),
            UpperKind::IncompatibleVarianceCheckT => "IncompatibleVarianceCheckT".to_string(),
            UpperKind::IncompatibleGetKeysT => "IncompatibleGetKeysT".to_string(),
            UpperKind::IncompatibleHasOwnPropT(_, _) => "IncompatibleHasOwnPropT".to_string(),
            UpperKind::IncompatibleGetValuesT => "IncompatibleGetValuesT".to_string(),
            UpperKind::IncompatibleMapTypeTObject => "IncompatibleMapTypeTObject".to_string(),
            UpperKind::IncompatibleGetStaticsT => "IncompatibleGetStaticsT".to_string(),
            UpperKind::IncompatibleUnclassified(ctor) => {
                format!("IncompatibleUnclassified {:?}", ctor)
            }
        }
    }

    match err {
        ErrorMessage::EIncompatibleType(box EIncompatibleTypeData {
            lower_loc,
            lower_desc,
            upper,
            use_op,
            ..
        }) => {
            let lower_desc = match lower_desc {
                TypeOrTypeDescT::Type(t) => dump_t(None, cx, t),
                TypeOrTypeDescT::TypeDesc(desc) => format!("{desc:?}"),
            };
            format!(
                "EIncompatibleType {{ lower_loc = {}; lower = {}; upper = ({}, {}); use_op = {} }}",
                string_of_aloc(None, lower_loc),
                lower_desc,
                format_args!("loc {:?}", upper.loc),
                dump_upper_kind(&upper.kind),
                match use_op {
                    None => "None".to_string(),
                    Some(use_op) => format!("Some({})", string_of_use_op(use_op)),
                }
            )
        }
        ErrorMessage::EIncompatibleSpeculation(box EIncompatibleSpeculationData {
            loc,
            use_op,
            branches: _,
        }) => {
            format!(
                "EIncompatibleSpeculation {{ upper = {}; use_op = {}; branches = _ }}",
                string_of_aloc(None, loc),
                match use_op {
                    None => "None".to_string(),
                    Some(use_op) => format!("Some({})", string_of_use_op(use_op)),
                }
            )
        }
        ErrorMessage::EIncompatibleProp(box EIncompatiblePropData {
            prop_loc,
            object,
            special: _,
            prop: _,
            use_op: _,
        }) => {
            format!(
                "EIncompatibleProp(Box::new(EIncompatiblePropData {{ reason_prop = {}; object = {}; special = _; prop = _; use_op = _ }}))",
                string_of_aloc(None, prop_loc),
                dump_error_type_reference(cx, object)
            )
        }
        ErrorMessage::EExportValueAsType(box (loc, name)) => {
            format!(
                "EExportValueAsType(Box::new(({}, {})))",
                string_of_aloc(None, loc),
                name
            )
        }
        ErrorMessage::EImportValueAsType(box (loc, s)) => {
            format!(
                "EImportValueAsType(Box::new(({}, {})))",
                string_of_aloc(None, loc),
                s
            )
        }
        ErrorMessage::EImportTypeAsTypeof(box (loc, s)) => {
            format!(
                "EImportTypeAsTypeof(Box::new(({}, {})))",
                string_of_aloc(None, loc),
                s
            )
        }
        ErrorMessage::EImportTypeAsValue(box (loc, s)) => {
            format!(
                "EImportTypeAsValue(Box::new(({}, {})))",
                string_of_aloc(None, loc),
                s
            )
        }
        ErrorMessage::ENoDefaultExport(box (loc, module_name, _)) => {
            format!(
                "ENoDefaultExport(Box::new(({}, {})))",
                string_of_aloc(None, loc),
                module_name.display()
            )
        }
        ErrorMessage::EOnlyDefaultExport(box (loc, module_name, export_name)) => {
            format!(
                "EOnlyDefaultExport(Box::new(({}, {}, {})))",
                string_of_aloc(None, loc),
                module_name.display(),
                export_name
            )
        }
        ErrorMessage::ENoNamedExport(box (loc, module_name, export_name, _)) => {
            format!(
                "ENoNamedExport(Box::new(({}, {}, {})))",
                string_of_aloc(None, loc),
                module_name.display(),
                export_name
            )
        }
        ErrorMessage::EMissingTypeArgs(box EMissingTypeArgsData {
            loc,
            type_,
            arity_loc,
            min_arity,
            max_arity,
        }) => {
            format!(
                "EMissingTypeArgs(Box::new(EMissingTypeArgsData {{ loc={}; type_={}; arity_loc={}; min_arity={}; max_arity={} }}))",
                string_of_aloc(None, loc),
                dump_error_type_reference(cx, type_),
                string_of_aloc(None, arity_loc),
                min_arity,
                max_arity
            )
        }
        ErrorMessage::EAnyValueUsedAsType { reference, value } => {
            format!(
                "EAnyValueUsedAsType {{ use = {} }}",
                dump_value_as_type_reference(cx, &value.loc, reference.as_ref(), &value.type_desc,)
            )
        }
        ErrorMessage::EValueUsedAsType { reference, value } => {
            format!(
                "EValueUsedAsType {{ use = {} }}",
                dump_value_as_type_reference(cx, &value.loc, reference.as_ref(), &value.type_desc,)
            )
        }
        ErrorMessage::EExpectedStringLit(box EExpectedStringLitData {
            lower,
            upper,
            use_op,
        }) => {
            let dump_type =
                |type_ref: &ErrorTypeReferenceWithLocData<ALoc>| match &type_ref.type_desc {
                    TypeOrTypeDescT::Type(t) => dump_t(None, cx, t),
                    TypeOrTypeDescT::TypeDesc(desc) => format!("{desc:?}"),
                };
            format!(
                "EExpectedStringLit(Box::new(EExpectedStringLitData {{ lower = {}; upper = {}; use_op = {} }}))",
                dump_type(lower),
                dump_type(upper),
                string_of_use_op(use_op)
            )
        }
        ErrorMessage::EExpectedNumberLit(box EExpectedNumberLitData {
            lower,
            upper,
            use_op,
        }) => {
            let dump_type =
                |type_ref: &ErrorTypeReferenceWithLocData<ALoc>| match &type_ref.type_desc {
                    TypeOrTypeDescT::Type(t) => dump_t(None, cx, t),
                    TypeOrTypeDescT::TypeDesc(desc) => format!("{desc:?}"),
                };
            format!(
                "EExpectedNumberLit(Box::new(EExpectedNumberLitData {{ lower = {}; upper = {}; use_op = {} }}))",
                dump_type(lower),
                dump_type(upper),
                string_of_use_op(use_op)
            )
        }
        ErrorMessage::EExpectedBooleanLit(box EExpectedBooleanLitData {
            lower,
            upper,
            use_op,
        }) => {
            let dump_type =
                |type_ref: &ErrorTypeReferenceWithLocData<ALoc>| match &type_ref.type_desc {
                    TypeOrTypeDescT::Type(t) => dump_t(None, cx, t),
                    TypeOrTypeDescT::TypeDesc(desc) => format!("{desc:?}"),
                };
            format!(
                "EExpectedBooleanLit(Box::new(EExpectedBooleanLitData {{ lower = {}; upper = {}; use_op = {} }}))",
                dump_type(lower),
                dump_type(upper),
                string_of_use_op(use_op)
            )
        }
        ErrorMessage::EExpectedBigIntLit(box EExpectedBigIntLitData {
            lower,
            upper,
            use_op,
        }) => {
            let dump_type =
                |type_ref: &ErrorTypeReferenceWithLocData<ALoc>| match &type_ref.type_desc {
                    TypeOrTypeDescT::Type(t) => dump_t(None, cx, t),
                    TypeOrTypeDescT::TypeDesc(desc) => format!("{desc:?}"),
                };
            format!(
                "EExpectedBigIntLit(Box::new(EExpectedBigIntLitData {{ lower = {}; upper = {}; use_op = {} }}))",
                dump_type(lower),
                dump_type(upper),
                string_of_use_op(use_op)
            )
        }
        ErrorMessage::EPropNotFoundInLookup(box EPropNotFoundInLookupData {
            prop_name,
            prop_loc,
            object,
            use_op,
            suggestion,
        }) => {
            let prop_str = match prop_name {
                Some(prop) => format!("Some {}", prop),
                None => "None".to_string(),
            };
            let suggestion_str = match suggestion {
                Some(s) => format!("Some {}", s),
                None => "None".to_string(),
            };
            format!(
                "EPropNotFoundInLookup ({}, {}, {}, {}, {})",
                prop_str,
                string_of_aloc(None, prop_loc),
                dump_error_type_reference(cx, object),
                string_of_use_op(use_op),
                suggestion_str
            )
        }
        ErrorMessage::EPropNotFoundInSubtyping(box EPropNotFoundInSubtypingData {
            prop_name,
            lower,
            upper,
            use_op,
            suggestion,
        }) => {
            let prop_str = match prop_name {
                Some(prop) => format!("Some {}", prop),
                None => "None".to_string(),
            };
            let suggestion_str = match suggestion {
                Some(s) => format!("Some {}", s),
                None => "None".to_string(),
            };
            format!(
                "EPropNotFoundInSubtyping ({}, {}, {}, {}, {})",
                prop_str,
                dump_error_type_reference(cx, lower),
                dump_error_type_reference(cx, upper),
                string_of_use_op(use_op),
                suggestion_str
            )
        }
        ErrorMessage::EConstructSignatureMissingInSubtyping(
            box EConstructSignatureMissingInSubtypingData {
                lower,
                upper,
                use_op,
            },
        ) => format!(
            "EConstructSignatureMissingInSubtyping ({}, {}, {})",
            dump_error_type_reference(cx, lower),
            dump_error_type_reference(cx, upper),
            string_of_use_op(use_op)
        ),
        ErrorMessage::EPropsNotFoundInSubtyping(box EPropsNotFoundInSubtypingData {
            prop_names,
            lower,
            upper,
            use_op,
        }) => {
            let names: Vec<String> = prop_names.iter().map(|n| n.to_string()).collect();
            format!(
                "EPropsNotFoundInSubtyping ([{}], {}, {}, {})",
                names.join(","),
                dump_error_type_reference(cx, lower),
                dump_error_type_reference(cx, upper),
                string_of_use_op(use_op)
            )
        }
        ErrorMessage::EPropsNotFoundInInvariantSubtyping(
            box EPropsNotFoundInInvariantSubtypingData {
                prop_names,
                lower,
                upper,
                use_op,
            },
        ) => {
            let names: Vec<String> = prop_names.iter().map(|n| n.to_string()).collect();
            format!(
                "EPropsNotFoundInSubtyping ([{}], {}, {}, {}, {}, {})",
                names.join(","),
                dump_error_type_reference(cx, lower),
                dump_error_type_reference(cx, upper),
                string_of_aloc(None, &lower.definition_loc),
                string_of_aloc(None, &upper.definition_loc),
                string_of_use_op(use_op)
            )
        }
        ErrorMessage::EIndexerCheckFailed(box EIndexerCheckFailedData {
            prop_name,
            lower,
            upper,
            indexer_desc,
            use_op,
            ..
        }) => {
            let indexer = match indexer_desc {
                TypeOrTypeDescT::Type(t) => dump_t(None, cx, t),
                TypeOrTypeDescT::TypeDesc(desc) => format!("{desc:?}"),
            };
            let dump_type =
                |type_ref: &ErrorTypeReferenceWithLocData<ALoc>| match &type_ref.type_desc {
                    TypeOrTypeDescT::Type(t) => dump_t(None, cx, t),
                    TypeOrTypeDescT::TypeDesc(desc) => format!("{desc:?}"),
                };
            format!(
                "EIndexerCheckFailed ({}, {}, {}, {}, {})",
                prop_name,
                dump_type(lower),
                dump_type(upper),
                indexer,
                string_of_use_op(use_op)
            )
        }
        ErrorMessage::EPropsExtraAgainstExactObject(box EPropsExtraAgainstExactObjectData {
            prop_names,
            lower,
            upper,
            use_op,
            ..
        }) => {
            let names: Vec<String> = prop_names.iter().map(|n| n.to_string()).collect();
            let dump_type =
                |type_ref: &ErrorTypeReferenceWithLocData<ALoc>| match &type_ref.type_desc {
                    TypeOrTypeDescT::Type(t) => dump_t(None, cx, t),
                    TypeOrTypeDescT::TypeDesc(desc) => format!("{desc:?}"),
                };
            format!(
                "EPropsExtraAgainstExactObject ([{}], {}, {}, {})",
                names.join(", "),
                dump_type(lower),
                dump_type(upper),
                string_of_use_op(use_op)
            )
        }
        ErrorMessage::EPropNotReadable(box EPropNotReadableData {
            prop_loc,
            prop_name,
            use_op,
        }) => {
            let prop_str = match prop_name {
                Some(x) => format!("{:?}", x.display_smol_str()),
                None => "(computed)".to_string(),
            };
            format!(
                "EPropNotReadable(Box::new(EPropNotReadableData {{ reason_prop = {}; prop_name = {}; use_op = {} }}))",
                string_of_aloc(None, prop_loc),
                prop_str,
                string_of_use_op(use_op)
            )
        }
        ErrorMessage::EPropNotWritable(box EPropNotWritableData {
            prop_loc,
            prop_name,
            use_op,
        }) => {
            let prop_str = match prop_name {
                Some(x) => format!("{:?}", x.display_smol_str()),
                None => "(computed)".to_string(),
            };
            format!(
                "EPropNotWritable(Box::new(EPropNotWritableData {{ reason_prop = {}; prop_name = {}; use_op = {} }}))",
                string_of_aloc(None, prop_loc),
                prop_str,
                string_of_use_op(use_op)
            )
        }
        ErrorMessage::EPropPolarityMismatch(box EPropPolarityMismatchData {
            lower,
            upper,
            props,
            use_op: _,
        }) => {
            let dump_object = |object: &PropPolarityMismatchObject<_, _>| match object {
                PropPolarityMismatchObject::Type(t) => dump_error_type_reference(cx, t),
                PropPolarityMismatchObject::Class(class) => {
                    format!("{} `{}`", string_of_aloc(None, &class.loc), class.name)
                }
            };
            let props_str: Vec<String> = props
                .iter()
                .map(|(x, _)| match x {
                    Some(x) => format!("{:?}", x.display_smol_str()),
                    None => "(computed)".to_string(),
                })
                .collect();
            format!(
                "EPropPolarityMismatch (({}, {}), {}, _, _)",
                dump_object(lower),
                dump_object(upper),
                props_str.join(", ")
            )
        }
        ErrorMessage::EPolarityMismatch(box EPolarityMismatchData {
            loc,
            name,
            expected_polarity,
            actual_polarity,
            ..
        }) => {
            format!(
                "EPolarityMismatch(Box::new(EPolarityMismatchData {{ reason={}; name={:?}; expected_polarity={}; actual_polarity={} }}))",
                string_of_aloc(None, loc),
                name,
                expected_polarity.string(),
                actual_polarity.string()
            )
        }
        ErrorMessage::EBuiltinNameLookupFailed(box EBuiltinNameLookupFailedData { loc, name }) => {
            format!(
                "EBuiltinNameLookupFailed(Box::new(EBuiltinNameLookupFailedData {{ loc = {}; name = {:?} }}))",
                string_of_aloc(None, loc),
                name
            )
        }
        ErrorMessage::EUmdGlobalValueInModule(box EUmdGlobalValueInModuleData { loc, name }) => {
            format!(
                "EUmdGlobalValueInModule(Box::new(EUmdGlobalValueInModuleData {{ loc = {}; name = {:?} }}))",
                string_of_aloc(None, loc),
                name
            )
        }
        ErrorMessage::EBuiltinModuleLookupFailed(box EBuiltinModuleLookupFailedData {
            loc,
            name,
            potential_generator,
        }) => {
            let gen_str = match potential_generator {
                Some(g) => format!("Some({})", g),
                None => "None".to_string(),
            };
            format!(
                "EBuiltinModuleLookupFailed(Box::new(EBuiltinModuleLookupFailedData {{ loc = {}; name = {:?}; potential_generator = {} }}))",
                string_of_aloc(None, loc),
                name,
                gen_str
            )
        }
        ErrorMessage::EExpectedModuleLookupFailed(box EExpectedModuleLookupFailedData {
            loc,
            name,
            expected_module_purpose: _,
        }) => {
            format!(
                "EExpectedModuleLookupFailed(Box::new(EExpectedModuleLookupFailedData {{ loc = {}; name = {:?} }}))",
                string_of_aloc(None, loc),
                name
            )
        }
        ErrorMessage::ECannotImportGlobalLibdef(box ECannotImportGlobalLibdefData {
            loc,
            module_name,
            libdef_name,
        }) => {
            format!(
                "ECannotImportGlobalLibdef(Box::new(ECannotImportGlobalLibdefData {{ loc = {}; module_name = {:?}; libdef_name = {:?} }}))",
                string_of_aloc(None, loc),
                module_name,
                libdef_name
            )
        }
        ErrorMessage::EPrivateLookupFailed(box EPrivateLookupFailedData {
            loc,
            object,
            prop_name,
            use_op,
        }) => {
            let object = match &object.type_desc {
                TypeOrTypeDescT::Type(t) => dump_t(None, cx, t),
                TypeOrTypeDescT::TypeDesc(desc) => format!("{desc:?}"),
            };
            format!(
                "EPrivateLookupFailed(Box::new(({}, {}), {}, {}))",
                string_of_aloc(None, loc),
                object,
                prop_name,
                string_of_use_op(use_op)
            )
        }
        ErrorMessage::EPlatformSpecificImplementationModuleLookupFailed(
            box EPlatformSpecificImplementationModuleLookupFailedData { loc: _, name },
        ) => {
            format!(
                "EPlatformSpecificImplementationModuleLookupFailed({})",
                name
            )
        }
        ErrorMessage::EComparison(box EComparisonData {
            r1,
            r2,
            loc_opt,
            strict_comparison_opt,
        }) => {
            let loc_str = match loc_opt {
                Some(loc) => string_of_aloc(None, loc),
                None => "None".to_string(),
            };
            let strict_str = match strict_comparison_opt {
                Some(_) => "Some(...)".to_string(),
                None => "None".to_string(),
            };
            let r1 = match &r1.type_desc {
                TypeOrTypeDescT::Type(t) => dump_t(None, cx, t),
                TypeOrTypeDescT::TypeDesc(desc) => format!("{desc:?}"),
            };
            let r2 = match &r2.type_desc {
                TypeOrTypeDescT::Type(t) => dump_t(None, cx, t),
                TypeOrTypeDescT::TypeDesc(desc) => format!("{desc:?}"),
            };
            format!("EComparison ({}, {}, {}, {})", r1, r2, loc_str, strict_str)
        }
        ErrorMessage::ENonStrictEqualityComparison(box ENonStrictEqualityComparisonData {
            lower,
            upper,
        }) => {
            let dump_type = |type_desc: &TypeOrTypeDescT<ALoc>| match type_desc {
                TypeOrTypeDescT::Type(t) => dump_t(None, cx, t),
                TypeOrTypeDescT::TypeDesc(desc) => format!("{desc:?}"),
            };
            format!(
                "ENonStrictEqualityComparison {{ lower = {}; upper = {} }}",
                dump_type(&lower.type_desc),
                dump_type(&upper.type_desc)
            )
        }
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
            let (num_req1, num_total1) = lower_arity;
            let (num_req2, num_total2) = upper_arity;
            format!(
                "ETupleArityMismatch ({}, {}, {}-{} inexact:{} rest:{}, {}-{} inexact:{} rest:{}, unify:{}, {})",
                match &lower.type_desc {
                    TypeOrTypeDescT::Type(t) => dump_t(None, cx, t),
                    TypeOrTypeDescT::TypeDesc(desc) => format!("{desc:?}"),
                },
                match &upper.type_desc {
                    TypeOrTypeDescT::Type(t) => dump_t(None, cx, t),
                    TypeOrTypeDescT::TypeDesc(desc) => format!("{desc:?}"),
                },
                num_req1,
                num_total1,
                lower_inexact,
                lower_rest,
                num_req2,
                num_total2,
                upper_inexact,
                upper_rest,
                unify,
                string_of_use_op(use_op)
            )
        }
        ErrorMessage::ENonLitArrayToTuple((lower, upper), use_op) => {
            format!(
                "ENonLitArrayToTuple (({}, {}), {})",
                match &lower.type_desc {
                    TypeOrTypeDescT::Type(t) => dump_t(None, cx, t),
                    TypeOrTypeDescT::TypeDesc(desc) => format!("{desc:?}"),
                },
                match &upper.type_desc {
                    TypeOrTypeDescT::Type(t) => dump_t(None, cx, t),
                    TypeOrTypeDescT::TypeDesc(desc) => format!("{desc:?}"),
                },
                string_of_use_op(use_op)
            )
        }
        ErrorMessage::ETupleRequiredAfterOptional(box ETupleRequiredAfterOptionalData {
            tuple_loc,
            required,
            optional,
        }) => {
            let tuple = format!("{}: {:?}", string_of_aloc(None, tuple_loc), "tuple type");
            format!(
                "ETupleRequiredAfterOptional(Box::new(ETupleRequiredAfterOptionalData {{reason_tuple = {}; reason_required = {}; reason_optional = {}}}))",
                tuple,
                dump_tuple_element_reference(&required.loc, required.name.as_deref()),
                dump_tuple_element_reference(&optional.loc, optional.name.as_deref())
            )
        }
        ErrorMessage::ETupleInvalidTypeSpread(box ETupleInvalidTypeSpreadData {
            spread_loc,
            argument,
        }) => {
            let argument_type = match &argument.type_desc {
                TypeOrTypeDescT::Type(t) => dump_t(None, cx, t),
                TypeOrTypeDescT::TypeDesc(desc) => format!("{desc:?}"),
            };
            format!(
                "ETupleInvalidTypeSpread(Box::new(ETupleInvalidTypeSpreadData {{reason_spread = {}; argument = {}}}))",
                string_of_aloc(None, spread_loc),
                argument_type
            )
        }
        ErrorMessage::ETupleOutOfBounds(box ETupleOutOfBoundsData {
            use_op,
            loc,
            tuple,
            inexact,
            length,
            index,
        }) => {
            let tuple_type = match &tuple.type_desc {
                TypeOrTypeDescT::Type(t) => dump_t(None, cx, t),
                TypeOrTypeDescT::TypeDesc(desc) => format!("{desc:?}"),
            };
            format!(
                "ETupleOutOfBounds(Box::new(ETupleOutOfBoundsData {{ use_op = {}; reason = {}; tuple = {}; inexact = {}; length = {}; index = {} }}))",
                string_of_use_op(use_op),
                string_of_aloc(None, loc),
                tuple_type,
                inexact,
                length,
                index
            )
        }
        ErrorMessage::ETupleNonIntegerIndex(box ETupleNonIntegerIndexData {
            use_op,
            loc,
            index,
            ..
        }) => {
            format!(
                "ETupleNonIntegerIndex(Box::new(ETupleNonIntegerIndexData {{ use_op = {}; reason = {}; index = {} }}))",
                string_of_use_op(use_op),
                string_of_aloc(None, loc),
                index
            )
        }
        ErrorMessage::ETupleUnsafeWrite { loc, use_op } => {
            format!(
                "ETupleUnsafeWrite {{ reason = {}; use_op = {} }}",
                string_of_aloc(None, loc),
                string_of_use_op(use_op)
            )
        }
        ErrorMessage::ETupleElementNotReadable(box ETupleElementNotReadableData {
            loc,
            index,
            name: _,
            use_op,
            ..
        }) => {
            format!(
                "ETupleElementNotReadable(Box::new(ETupleElementNotReadableData {{ reason = {}; index = {}; use_op = {} }}))",
                string_of_aloc(None, loc),
                index,
                string_of_use_op(use_op)
            )
        }
        ErrorMessage::ETupleElementNotWritable(box ETupleElementNotWritableData {
            loc,
            index,
            name: _,
            use_op,
            ..
        }) => {
            format!(
                "ETupleElementNotWritable(Box::new(ETupleElementNotWritableData {{ reason = {}; index = {}; use_op = {} }}))",
                string_of_aloc(None, loc),
                index,
                string_of_use_op(use_op)
            )
        }
        ErrorMessage::ETupleElementPolarityMismatch(box ETupleElementPolarityMismatchData {
            index,
            lower,
            polarity_lower,
            upper,
            polarity_upper,
            use_op,
        }) => {
            format!(
                "ETupleElementPolarityMismatch(Box::new(ETupleElementPolarityMismatchData {{ index = {}; lower = {}; polarity_lower = {}; upper = {}; polarity_upper = {}; use_op = {} }}))",
                index,
                match &lower.type_desc {
                    TypeOrTypeDescT::Type(t) => dump_t(None, cx, t),
                    TypeOrTypeDescT::TypeDesc(desc) => format!("{desc:?}"),
                },
                polarity_lower.string(),
                match &upper.type_desc {
                    TypeOrTypeDescT::Type(t) => dump_t(None, cx, t),
                    TypeOrTypeDescT::TypeDesc(desc) => format!("{desc:?}"),
                },
                polarity_upper.string(),
                string_of_use_op(use_op)
            )
        }
        ErrorMessage::ETupleElementAfterRestSpread(loc) => {
            format!(
                "ETupleElementAfterRestSpread ({})",
                string_of_aloc(None, loc)
            )
        }
        ErrorMessage::ETupleElementAfterInexactSpread(loc) => {
            format!(
                "ETupleElementAfterInexactSpread ({})",
                string_of_aloc(None, loc)
            )
        }
        ErrorMessage::EROArrayWrite(loc, use_op) => {
            format!(
                "EROArrayWrite ({}, {})",
                string_of_aloc(None, loc),
                string_of_use_op(use_op)
            )
        }
        ErrorMessage::EUnionSpeculationFailed(box EUnionSpeculationFailedData {
            use_op,
            loc,
            branches: _,
        }) => {
            format!(
                "EUnionSpeculationFailed {{ use_op = {}; loc = {}; branches = _ }}",
                string_of_use_op(use_op),
                string_of_aloc(None, loc),
            )
        }
        ErrorMessage::EIncompatibleWithExact((lower, upper), use_op, _) => {
            format!(
                "EIncompatibleWithExact (({}, {}), {})",
                dump_error_type_reference(cx, lower),
                dump_error_type_reference(cx, upper),
                string_of_use_op(use_op)
            )
        }
        ErrorMessage::EFunctionIncompatibleWithIndexer((lower, upper), use_op) => {
            format!(
                "EFunctionIncompatibleWithIndexer(({}, {}), {})",
                match &lower.type_desc {
                    TypeOrTypeDescT::Type(t) => dump_t(None, cx, t),
                    TypeOrTypeDescT::TypeDesc(desc) => format!("{desc:?}"),
                },
                match &upper.type_desc {
                    TypeOrTypeDescT::Type(t) => dump_t(None, cx, t),
                    TypeOrTypeDescT::TypeDesc(desc) => format!("{desc:?}"),
                },
                string_of_use_op(use_op)
            )
        }
        ErrorMessage::EUnsupportedExact(box EUnsupportedExactData { value_desc, .. }) => {
            let value = match value_desc {
                TypeOrTypeDescT::Type(t) => dump_t(None, cx, t),
                TypeOrTypeDescT::TypeDesc(desc) => format!("{desc:?}"),
            };
            format!("EUnsupportedExact(Box::new({}))", value)
        }
        ErrorMessage::EInvalidExactObjectIntersection(
            box EInvalidExactObjectIntersectionData { loc, prop, .. },
        ) => format!(
            "EInvalidExactObjectIntersection({}, {})",
            string_of_aloc(None, loc),
            prop.display_smol_str()
        ),
        ErrorMessage::EUnexpectedThisType(loc) => {
            format!("EUnexpectedThisType ({})", string_of_aloc(None, loc))
        }
        ErrorMessage::ETypeParamArity(loc, expected) => {
            format!(
                "ETypeParamArity ({}, {})",
                string_of_aloc(None, loc),
                expected
            )
        }
        ErrorMessage::ECallTypeArity(box ECallTypeArityData {
            call_loc,
            is_new,
            callee,
            expected_arity,
        }) => {
            let callee = match callee {
                CallTypeArityCallee::Type(callee) => {
                    dump_loc_type_desc(cx, &callee.reference_loc, &callee.type_desc)
                }
                CallTypeArityCallee::Builtin(name) => name.to_string(),
                CallTypeArityCallee::Function => "function".to_string(),
            };
            format!(
                "ECallTypeArity(Box::new(ECallTypeArityData {{ call_loc={}; is_new={}; callee={}; expected_arity={}; }}))",
                string_of_aloc(None, call_loc),
                is_new,
                callee,
                expected_arity
            )
        }
        ErrorMessage::ETooManyTypeArgs(box ETooManyTypeArgsData {
            reason_tapp,
            arity_loc,
            maximum_arity,
        }) => {
            format!(
                "ETooManyTypeArgs ({}, {}, {})",
                format_args!(
                    "{} {:?}",
                    string_of_aloc(None, &reason_tapp.loc),
                    reason_tapp.desc
                ),
                string_of_aloc(None, arity_loc),
                maximum_arity
            )
        }
        ErrorMessage::ETooFewTypeArgs(box ETooFewTypeArgsData {
            reason_tapp,
            arity_loc,
            minimum_arity,
        }) => {
            format!(
                "ETooFewTypeArgs ({}, {}, {})",
                format_args!(
                    "{} {:?}",
                    string_of_aloc(None, &reason_tapp.loc),
                    reason_tapp.desc
                ),
                string_of_aloc(None, arity_loc),
                minimum_arity
            )
        }
        ErrorMessage::EInvalidReactCreateElement(box EInvalidReactCreateElementData {
            create_element_loc,
            react,
        }) => {
            format!(
                "EInvalidReactCreateElement {{ loc = {}; react = {} {} }}",
                string_of_aloc(None, create_element_loc),
                string_of_aloc(None, &react.loc),
                react.name,
            )
        }
        ErrorMessage::EInvalidThisArg(box EInvalidThisArgData {
            loc,
            name,
            receiver_expression,
            kind,
            ..
        }) => {
            format!(
                "EInvalidThisArg ({}) ({}) ({}) ({:?})",
                string_of_aloc(None, loc),
                name,
                dump_expression_reference(receiver_expression),
                kind
            )
        }
        ErrorMessage::EInvalidInfer(loc) => {
            format!("EInvalidInfer ({})", string_of_aloc(None, loc))
        }
        ErrorMessage::EInvalidExtends(box (loc, expression)) => {
            format!(
                "EInvalidExtends ({} RCode({expression:?}))",
                string_of_aloc(None, loc)
            )
        }
        ErrorMessage::EExportsAnnot(loc) => {
            format!("EExportsAnnot ({})", string_of_aloc(None, loc))
        }
        ErrorMessage::EInvalidRendersTypeArgument(box EInvalidRendersTypeArgumentData {
            loc,
            renders_variant,
            invalid_render_type_kind,
            invalid_types,
        }) => {
            let variant_str = match renders_variant {
                flow_parser::ast::types::RendersVariant::Normal => "renders",
                flow_parser::ast::types::RendersVariant::Maybe => "renders?",
                flow_parser::ast::types::RendersVariant::Star => "renders*",
            };
            let kind_str = string_of_invalid_render_type_kind(invalid_render_type_kind);
            let types: Vec<String> = invalid_types
                .iter()
                .map(|type_| match &type_.type_desc {
                    TypeOrTypeDescT::Type(t) => dump_t(None, cx, t),
                    TypeOrTypeDescT::TypeDesc(desc) => format!("{desc:?}"),
                })
                .collect();
            format!(
                "EInvalidRendersTypeArgument(Box::new(EInvalidRendersTypeArgumentData {{ loc = {}; renders_variant = {}, invalid_render_type_kind = {}; invalid_types = [{}] }}))",
                string_of_aloc(None, loc),
                variant_str,
                kind_str,
                types.join(", ")
            )
        }
        ErrorMessage::EUnsupportedKeyInObject {
            loc,
            obj_kind,
            key_error_kind,
        } => {
            let obj_kind_str = match obj_kind {
                IntermediateObjKind::Type => "object type",
                IntermediateObjKind::Literal => "object literal",
                IntermediateObjKind::Interface => "interface",
                IntermediateObjKind::DeclareClass => "declare class",
            };
            format!(
                "EUnsupportedKeyInObject ({}, {}, {})",
                string_of_aloc(None, loc),
                obj_kind_str,
                key_error_kind.str_of_kind()
            )
        }
        ErrorMessage::EAmbiguousNumericKeyWithVariance(loc) => {
            format!(
                "EAmbiguousNumericKeyWithVariance ({})",
                string_of_aloc(None, loc)
            )
        }
        ErrorMessage::EUnsupportedComputedKeyInClass(loc) => {
            format!(
                "EUnsupportedComputedKeyInClass ({})",
                string_of_aloc(None, loc)
            )
        }
        ErrorMessage::ETypeGuardFuncIncompatibility { use_op, .. } => {
            format!(
                "ETypeGuardFuncIncompatibility ({})",
                string_of_use_op(use_op)
            )
        }
        ErrorMessage::ETypeGuardInvalidParameter(box ETypeGuardInvalidParameterData {
            type_guard,
            ..
        }) => {
            format!(
                "ETypeGuardInvalidParameter ({} {})",
                string_of_aloc(None, &type_guard.loc),
                type_guard.name
            )
        }
        ErrorMessage::ETypeGuardIndexMismatch {
            use_op,
            parameters: _,
        } => {
            format!("ETypeGuardIndexMismatch ({})", string_of_use_op(use_op))
        }
        ErrorMessage::ETypeGuardImpliesMismatch { use_op, .. } => {
            format!("ETypeGuardImpliesMismatch ({})", string_of_use_op(use_op))
        }
        ErrorMessage::ETypeGuardParamUnbound(_) => "ETypeGuardParamUnbound".to_string(),
        ErrorMessage::ETypeGuardThisParam(_) => "ETypeGuardThisParam".to_string(),
        ErrorMessage::ETypeGuardFunctionInvalidWrites(
            box ETypeGuardFunctionInvalidWritesData { .. },
        ) => "ETypeGuardFunctionInvalidWrites".to_string(),
        ErrorMessage::ETypeGuardFunctionParamHavoced(box ETypeGuardFunctionParamHavocedData {
            ..
        }) => "ETypeGuardFunctionParamHavoced".to_string(),
        ErrorMessage::ETypeGuardIncompatibleWithFunctionKind(
            box ETypeGuardIncompatibleWithFunctionKindData { .. },
        ) => "ETypeGuardIncompatibleWithFunctionKind".to_string(),
        ErrorMessage::EInternal(box (loc, err)) => {
            format!(
                "EInternal(Box::new(({}, {})))",
                string_of_aloc(None, loc),
                dump_internal_error(err)
            )
        }
        ErrorMessage::EUnsupportedSyntax(box (loc, _)) => {
            format!(
                "EUnsupportedSyntax(Box::new(({}, _)))",
                string_of_aloc(None, loc)
            )
        }
        ErrorMessage::EUseArrayLiteral(loc) => {
            format!("EUseArrayLiteral ({})", string_of_aloc(None, loc))
        }
        ErrorMessage::EMissingLocalAnnotation {
            reason,
            hint_available: _,
            from_generic_function: _,
            is_return_annotation: _,
        } => {
            format!(
                "EMissingLocalAnnotation ({} {:?})",
                string_of_aloc(None, &reason.loc),
                reason.desc
            )
        }
        ErrorMessage::EMissingThisAnnotation(loc) => {
            format!("EMissingThisAnnotation ({})", string_of_aloc(None, loc))
        }
        ErrorMessage::EMissingParameterAnnotation {
            loc,
            hint_available,
        } => format!(
            "EMissingParameterAnnotation ({}, {})",
            string_of_aloc(None, loc),
            hint_available
        ),
        ErrorMessage::EBindingError(box (_binding_error, loc, x, entry_loc)) => {
            format!(
                "EBindingError(Box::new((_, {}, {}, {})))",
                string_of_aloc(None, loc),
                x,
                string_of_aloc(None, entry_loc)
            )
        }
        ErrorMessage::ERecursionLimit(loc) => {
            format!("ERecursionLimit({})", string_of_aloc(None, loc))
        }
        ErrorMessage::EUninitializedInstanceProperty(loc, err) => {
            let err_str = match err {
                PropertyAssignmentKind::PropertyNotDefinitelyInitialized => {
                    "PropertyNotDefinitelyInitialized"
                }
                PropertyAssignmentKind::ReadFromUninitializedProperty => {
                    "ReadFromUninitializedProperty"
                }
                PropertyAssignmentKind::MethodCallBeforeEverythingInitialized => {
                    "MethodCallBeforeEverythingInitialized"
                }
                PropertyAssignmentKind::PropertyFunctionCallBeforeEverythingInitialized => {
                    "PropertyFunctionCallBeforeEverythingInitialized"
                }
                PropertyAssignmentKind::ThisBeforeEverythingInitialized => {
                    "ThisBeforeEverythingInitialized"
                }
            };
            format!(
                "EUninitializedInstanceProperty ({}, {})",
                string_of_aloc(None, loc),
                err_str
            )
        }
        ErrorMessage::EUninitializedProperty(box (loc, name, kind)) => {
            format!(
                "EUninitializedProperty ({}, {}, {:?})",
                string_of_aloc(None, loc),
                name,
                kind
            )
        }
        ErrorMessage::EIndeterminateModuleType(loc) => {
            format!("EIndeterminateModuleType ({})", string_of_aloc(None, loc))
        }
        ErrorMessage::EBadExportPosition(loc) => {
            format!("EBadExportPosition ({})", string_of_aloc(None, loc))
        }
        ErrorMessage::EBadExportContext(box (name, loc)) => {
            format!(
                "EBadExportContext(Box::new(({}, {})))",
                name,
                string_of_aloc(None, loc)
            )
        }
        ErrorMessage::EBadDefaultImportAccess(box (loc, import_star_loc)) => {
            format!(
                "EBadDefaultImportAccess(Box::new(({}, {})))",
                string_of_aloc(None, loc),
                string_of_aloc(None, import_star_loc)
            )
        }
        ErrorMessage::EBadDefaultImportDestructuring(loc) => {
            format!(
                "EBadDefaultImportDestructuring ({})",
                string_of_aloc(None, loc)
            )
        }
        ErrorMessage::EInvalidImportStarUse(box (loc, import_star_loc)) => {
            format!(
                "EInvalidImportStarUse(Box::new(({}, {})))",
                string_of_aloc(None, loc),
                string_of_aloc(None, import_star_loc)
            )
        }
        ErrorMessage::ENonConstVarExport(box (loc, declaration)) => {
            let declaration = match declaration {
                Some((declaration_loc, name)) => {
                    format!("{} {name}", string_of_aloc(None, declaration_loc))
                }
                None => "None".to_string(),
            };
            format!(
                "ENonConstVarExport(Box::new(({}, {})))",
                string_of_aloc(None, loc),
                declaration
            )
        }
        ErrorMessage::EThisInExportedFunction(loc) => {
            format!("EThisInExportedFunction ({})", string_of_aloc(None, loc))
        }
        ErrorMessage::EMixedImportAndRequire(box (loc, import_loc)) => {
            format!(
                "EMixedImportAndRequire(Box::new(({}, {})))",
                string_of_aloc(None, loc),
                string_of_aloc(None, import_loc)
            )
        }
        ErrorMessage::EUnsupportedVarianceAnnotation(box (loc, s)) => {
            format!(
                "EUnsupportedVarianceAnnotation(Box::new(({}, {})))",
                string_of_aloc(None, loc),
                s
            )
        }
        ErrorMessage::EExportRenamedDefault(box EExportRenamedDefaultData {
            loc,
            name,
            is_reexport,
        }) => {
            let name_str = match name {
                Some(n) => n.to_string(),
                None => "None".to_string(),
            };
            format!(
                "EExportRenamedDefault(Box::new(EExportRenamedDefaultData {{ loc = {}; name = {}; is_reexport = {} }}))",
                string_of_aloc(None, loc),
                name_str,
                is_reexport
            )
        }
        ErrorMessage::EUnreachable(loc) => {
            format!("EUnreachable ({})", string_of_aloc(None, loc))
        }
        ErrorMessage::EInvalidObjectKit(box EInvalidObjectKitData {
            loc,
            value,
            use_op,
            requirement,
        }) => {
            let value = match &value.type_desc {
                TypeOrTypeDescT::Type(t) => dump_t(None, cx, t),
                TypeOrTypeDescT::TypeDesc(desc) => format!("{desc:?}"),
            };
            format!(
                "EInvalidObjectKit(Box::new(EInvalidObjectKitData {{ loc = {}; value = {}; use_op = {}; requirement = {:?} }}))",
                string_of_aloc(None, loc),
                value,
                string_of_use_op(use_op),
                requirement
            )
        }
        ErrorMessage::EInvalidTypeof(box (loc, name)) => {
            format!(
                "EInvalidTypeof(Box::new(({}, {:?})))",
                string_of_aloc(None, loc),
                name
            )
        }
        ErrorMessage::EBinaryInLHS(box EBinaryInLHSData { loc, lhs_desc, .. }) => {
            let lhs = match lhs_desc {
                TypeOrTypeDescT::Type(t) => dump_t(None, cx, t),
                TypeOrTypeDescT::TypeDesc(desc) => format!("{desc:?}"),
            };
            format!(
                "EBinaryInLHS {{ loc = {}; lhs = {} }}",
                string_of_aloc(None, loc),
                lhs,
            )
        }
        ErrorMessage::EBinaryInRHS(box EBinaryInRHSData { loc, rhs_desc, .. }) => {
            let rhs = match rhs_desc {
                TypeOrTypeDescT::Type(t) => dump_t(None, cx, t),
                TypeOrTypeDescT::TypeDesc(desc) => format!("{desc:?}"),
            };
            format!(
                "EBinaryInRHS {{ loc = {}; rhs = {} }}",
                string_of_aloc(None, loc),
                rhs,
            )
        }
        ErrorMessage::EArithmeticOperand(box EArithmeticOperandData { loc, operand }) => {
            let operand = match &operand.type_desc {
                TypeOrTypeDescT::Type(t) => dump_t(None, cx, t),
                TypeOrTypeDescT::TypeDesc(desc) => format!("{desc:?}"),
            };
            format!(
                "EArithmeticOperand {{ loc = {}; operand = {} }}",
                string_of_aloc(None, loc),
                operand,
            )
        }
        ErrorMessage::EForInRHS(box EForInRHSData { loc, rhs_desc, .. }) => {
            let rhs = match rhs_desc {
                TypeOrTypeDescT::Type(t) => dump_t(None, cx, t),
                TypeOrTypeDescT::TypeDesc(desc) => format!("{desc:?}"),
            };
            format!(
                "EForInRHS {{ loc = {}; rhs = {} }}",
                string_of_aloc(None, loc),
                rhs,
            )
        }
        ErrorMessage::EInstanceofRHS(box EInstanceofRHSData { loc, rhs_desc, .. }) => {
            let rhs = match rhs_desc {
                TypeOrTypeDescT::Type(t) => dump_t(None, cx, t),
                TypeOrTypeDescT::TypeDesc(desc) => format!("{desc:?}"),
            };
            format!(
                "EInstanceofRHS {{ loc = {}; rhs = {} }}",
                string_of_aloc(None, loc),
                rhs,
            )
        }
        ErrorMessage::EObjectComputedPropertyAccess(box EObjectComputedPropertyAccessData {
            object,
            property,
            kind,
        }) => {
            let dump_type =
                |type_ref: &ErrorTypeReferenceWithLocData<ALoc>| match &type_ref.type_desc {
                    TypeOrTypeDescT::Type(t) => dump_t(None, cx, t),
                    TypeOrTypeDescT::TypeDesc(desc) => format!("{desc:?}"),
                };
            format!(
                "EObjectComputedPropertyAccess (object={}, property={}, kind={})",
                dump_type(object),
                dump_type(property),
                kind.str_of_kind()
            )
        }
        ErrorMessage::EObjectComputedPropertyAssign(box (reason1, reason2, kind)) => {
            let reason2_str = match reason2 {
                Some(r) => format!("{r:?}"),
                None => "none".to_string(),
            };
            format!(
                "EObjectComputedPropertyAssign(Box::new(({}, {}, {})))",
                dump_error_type_reference(cx, reason1),
                reason2_str,
                kind.str_of_kind()
            )
        }
        ErrorMessage::EObjectComputedPropertyPotentialOverwrite(
            box EObjectComputedPropertyPotentialOverwriteData { key_loc, .. },
        ) => {
            format!(
                "EObjectComputedPropertyPotentialOverwrite ({})",
                string_of_aloc(None, key_loc)
            )
        }
        ErrorMessage::EInvalidLHSInAssignment(loc) => {
            format!("EInvalidLHSInAssignment ({})", string_of_aloc(None, loc))
        }
        ErrorMessage::EIncompatibleTypesWithUseOp(box EIncompatibleTypesWithUseOpData {
            lower_loc,
            upper_loc,
            lower_desc,
            upper_desc,
            use_op,
            ..
        }) => {
            let dump_type_or_desc = |type_or_desc: &TypeOrTypeDescT<ALoc>| match type_or_desc {
                TypeOrTypeDescT::Type(t) => dump_t(None, cx, t),
                TypeOrTypeDescT::TypeDesc(desc) => format!("{desc:?}"),
            };
            format!(
                "EIncompatibleTypesWithUseOp {{ lower_loc = {}; upper_loc = {}; lower = {}; upper = {}; use_op = {} }}",
                string_of_aloc(None, lower_loc),
                string_of_aloc(None, upper_loc),
                dump_type_or_desc(lower_desc),
                dump_type_or_desc(upper_desc),
                string_of_use_op(use_op)
            )
        }
        ErrorMessage::EInvariantSubtypingWithUseOp(box EInvariantSubtypingWithUseOpData {
            lower_loc,
            upper_loc,
            use_op,
            ..
        }) => {
            format!(
                "EInvariantSubtypingWithUseOp {{ lower_loc = {}; upper_loc = {};  use_op = {} }}",
                string_of_aloc(None, lower_loc),
                string_of_aloc(None, upper_loc),
                string_of_use_op(use_op)
            )
        }
        ErrorMessage::EUnsupportedImplements(type_) => {
            let type_desc = match &type_.type_desc {
                TypeOrTypeDescT::Type(t) => dump_t(None, cx, t),
                TypeOrTypeDescT::TypeDesc(desc) => format!("{desc:?}"),
            };
            format!(
                "EUnsupportedImplements ({} {:?})",
                string_of_aloc(None, &type_.loc),
                type_desc,
            )
        }
        ErrorMessage::ENotAReactComponent(box ENotAReactComponentData { component, use_op }) => {
            let component_type = match &component.type_desc {
                TypeOrTypeDescT::Type(t) => dump_t(None, cx, t),
                TypeOrTypeDescT::TypeDesc(desc) => format!("{desc:?}"),
            };
            format!(
                "ENotAReactComponent {{ component = {}; use_op = {} }}",
                component_type,
                string_of_use_op(use_op)
            )
        }
        ErrorMessage::EFunctionCallExtraArg(box EFunctionCallExtraArgData {
            loc,
            function_reference,
            param_count,
            use_op,
            ..
        }) => {
            let function = match function_reference.kind {
                FunctionReferenceKind::Function => "function",
                FunctionReferenceKind::FunctionType => "function type",
                FunctionReferenceKind::DefaultConstructor => "default constructor",
            };
            format!(
                "EFunctionCallExtraArg(Box::new(({}, {}, {}, {})))",
                string_of_aloc(None, loc),
                format_args!(
                    "{} {function:?}",
                    string_of_aloc(None, &function_reference.loc)
                ),
                param_count,
                string_of_use_op(use_op)
            )
        }
        ErrorMessage::EUnsupportedSetProto(loc) => {
            format!("EUnsupportedSetProto ({})", string_of_aloc(None, loc))
        }
        ErrorMessage::EDuplicateModuleProvider(box EDuplicateModuleProviderData {
            module_name,
            provider,
            conflict,
        }) => {
            format!(
                "EDuplicateModuleProvider ({:?}, {}, {})",
                module_name,
                string_of_aloc(None, provider),
                string_of_aloc(None, conflict)
            )
        }
        ErrorMessage::EParseError(box (loc, _)) => {
            format!("EParseError(Box::new(({}, _)))", string_of_aloc(None, loc))
        }
        ErrorMessage::EDocblockError(box (loc, err)) => {
            let err_str = match err {
                DocblockError::MultipleFlowAttributes => "MultipleFlowAttributes",
                DocblockError::InvalidFlowMode(_) => "InvalidFlowMode",
                DocblockError::MultipleJSXAttributes => "MultipleJSXAttributes",
                DocblockError::InvalidJSXAttribute(_) => "InvalidJSXAttribute",
                DocblockError::MultipleJSXRuntimeAttributes => "MultipleJSXRuntimeAttributes",
                DocblockError::InvalidJSXRuntimeAttribute => "InvalidJSXRuntimeAttribute",
                DocblockError::InvalidSupportsPlatform(_) => "InvalidSupportsPlatform",
                DocblockError::DisallowedSupportsPlatform => "DisallowedSupportsPlatform",
            };
            format!(
                "EDocblockError(Box::new(({}, {})))",
                string_of_aloc(None, loc),
                err_str
            )
        }
        ErrorMessage::EAmbiguousObjectType(loc) => {
            format!("EAmbiguousObjectType ({})", string_of_aloc(None, loc))
        }
        ErrorMessage::EUntypedTypeImport(box (loc, module_name)) => {
            format!(
                "EUntypedTypeImport(Box::new(({}, {})))",
                string_of_aloc(None, loc),
                module_name.display()
            )
        }
        ErrorMessage::EUntypedImport(box (loc, module_name)) => {
            format!(
                "EUntypedImport(Box::new(({}, {})))",
                string_of_aloc(None, loc),
                module_name.display()
            )
        }
        ErrorMessage::ENonstrictImport(loc) => {
            format!("ENonstrictImport ({})", string_of_aloc(None, loc))
        }
        ErrorMessage::EInternalType(loc, _) => {
            format!("EInternalType ({})", string_of_aloc(None, loc))
        }
        ErrorMessage::EUnclearType(loc) => {
            format!("EUnclearType ({})", string_of_aloc(None, loc))
        }
        ErrorMessage::EDeprecatedBool(loc) => {
            format!("EDeprecatedBool ({})", string_of_aloc(None, loc))
        }
        ErrorMessage::EIncorrectTypeWithReplacement(box EIncorrectTypeWithReplacementData {
            loc,
            kind,
        }) => {
            format!(
                "EIncorrectTypeWithReplacement ({}) ({:?})",
                string_of_aloc(None, loc),
                kind
            )
        }
        ErrorMessage::EUnsafeGettersSetters(loc) => {
            format!("EUnclearGettersSetters ({})", string_of_aloc(None, loc))
        }
        ErrorMessage::EUnsafeObjectAssign(loc) => {
            format!("EUnsafeObjectAssign ({})", string_of_aloc(None, loc))
        }
        ErrorMessage::EUnusedSuppression(loc) => {
            format!("EUnusedSuppression ({})", string_of_aloc(None, loc))
        }
        ErrorMessage::ECodelessSuppression(loc) => {
            format!("ECodelessSuppression ({})", string_of_aloc(None, loc))
        }
        ErrorMessage::ELintSetting(box (loc, kind)) => {
            let kind_str = match kind {
                LintParseError::InvalidSetting => "Invalid_setting",
                LintParseError::MalformedArgument => "Malformed_argument",
                LintParseError::NakedComment => "Naked_comment",
                LintParseError::NonexistentRule => "Nonexistent_rule",
                LintParseError::OverwrittenArgument => "Overwritten_argument",
                LintParseError::RedundantArgument => "Redundant_argument",
            };
            format!(
                "ELintSetting(Box::new(({}, {})))",
                string_of_aloc(None, loc),
                kind_str
            )
        }
        ErrorMessage::ESketchyNullLint(box ESketchyNullLintData {
            kind,
            loc,
            null_loc,
            falsy_loc,
        }) => {
            let kind_str = match kind {
                SketchyNullKind::Bool => "SketchyNullBool",
                SketchyNullKind::String => "SketchyNullString",
                SketchyNullKind::Number => "SketchyNullNumber",
                SketchyNullKind::BigInt => "SketchyNullBigInt",
                SketchyNullKind::Mixed => "SketchyNullMixed",
                SketchyNullKind::EnumBool => "SketchyNullEnumBool",
                SketchyNullKind::EnumString => "SketchyNullEnumString",
                SketchyNullKind::EnumNumber => "SketchyNullEnumNumber",
                SketchyNullKind::EnumBigInt => "SketchyNullEnumBigInt",
            };
            format!(
                "ESketchyNullLint(Box::new(ESketchyNullLintData {{kind={}; loc={}; null_loc={}; falsy_loc={}}}))",
                kind_str,
                string_of_aloc(None, loc),
                string_of_aloc(None, null_loc),
                string_of_aloc(None, falsy_loc)
            )
        }
        ErrorMessage::ESketchyNumberLint(kind, value) => {
            let kind_str = match kind {
                SketchyNumberKind::And => "SketchyNumberAnd",
            };
            let value = match &value.type_desc {
                TypeOrTypeDescT::Type(t) => dump_t(None, cx, t),
                TypeOrTypeDescT::TypeDesc(desc) => format!("{desc:?}"),
            };
            format!("ESketchyNumberLint ({}) ({})", kind_str, value)
        }
        ErrorMessage::EInvalidConstructor(box EInvalidConstructorData {
            loc, value_desc, ..
        }) => {
            let value = match value_desc {
                TypeOrTypeDescT::Type(t) => dump_t(None, cx, t),
                TypeOrTypeDescT::TypeDesc(desc) => format!("{desc:?}"),
            };
            format!(
                "EInvalidConstructor {{ loc = {}; value = {} }}",
                string_of_aloc(None, loc),
                value,
            )
        }
        ErrorMessage::EInvalidPrototype(box EInvalidPrototypeData {
            loc,
            prototype_desc,
            ..
        }) => {
            let prototype = match prototype_desc {
                TypeOrTypeDescT::Type(t) => dump_t(None, cx, t),
                TypeOrTypeDescT::TypeDesc(desc) => format!("{desc:?}"),
            };
            format!(
                "EInvalidPrototype {{ loc = {}; prototype = {} }}",
                string_of_aloc(None, loc),
                prototype,
            )
        }
        ErrorMessage::EUnnecessaryOptionalChain(box (loc, _)) => {
            format!("EUnnecessaryOptionalChain ({})", string_of_aloc(None, loc))
        }
        ErrorMessage::EUnnecessaryInvariant(box EUnnecessaryInvariantData {
            loc,
            condition,
            ..
        }) => {
            let condition = match &condition.type_desc {
                TypeOrTypeDescT::Type(t) => dump_t(None, cx, t),
                TypeOrTypeDescT::TypeDesc(desc) => format!("{desc:?}"),
            };
            format!(
                "EUnnecessaryInvariant {{ loc = {}; condition = {} }}",
                string_of_aloc(None, loc),
                condition,
            )
        }
        ErrorMessage::EUnnecessaryDeclareTypeOnlyExport(loc) => {
            format!(
                "EUnnecessaryDeclareTypeOnlyExport ({})",
                string_of_aloc(None, loc)
            )
        }
        ErrorMessage::ECannotDelete(box ECannotDeleteData { loc, expression }) => {
            let expression = match &expression.type_desc {
                TypeOrTypeDescT::Type(t) => dump_t(None, cx, t),
                TypeOrTypeDescT::TypeDesc(desc) => format!("{desc:?}"),
            };
            format!(
                "ECannotDelete(Box::new(ECannotDeleteData {{ loc = {}; expression = {} }}))",
                string_of_aloc(None, loc),
                expression
            )
        }
        ErrorMessage::ESignatureBindingValidation(validation) => match validation {
            BindingValidation::ModuleOverride {
                override_binding_loc,
                ..
            } => {
                format!(
                    "ESignatureBindingValidation (ModuleOverride {})",
                    string_of_aloc(None, override_binding_loc)
                )
            }
            BindingValidation::NameOverride {
                override_binding_loc,
                ..
            } => {
                format!(
                    "ESignatureBindingValidation (NameOverride {})",
                    string_of_aloc(None, override_binding_loc)
                )
            }
            BindingValidation::NamespacedNameAlreadyBound {
                invalid_binding_loc,
                ..
            } => {
                format!(
                    "ESignatureBindingValidation (NamespacedNameAlreadyBound {})",
                    string_of_aloc(None, invalid_binding_loc)
                )
            }
            BindingValidation::InterfaceMergePropertyConflict {
                current_binding_loc,
                existing_binding_loc,
                ..
            } => {
                format!(
                    "ESignatureBindingValidation (InterfaceMergePropertyConflict {} {})",
                    string_of_aloc(None, current_binding_loc),
                    string_of_aloc(None, existing_binding_loc)
                )
            }
            BindingValidation::InterfaceMergeTparamMismatch {
                current_binding_loc,
                existing_binding_loc,
                ..
            } => {
                format!(
                    "ESignatureBindingValidation (InterfaceMergeTparamMismatch {} {})",
                    string_of_aloc(None, current_binding_loc),
                    string_of_aloc(None, existing_binding_loc)
                )
            }
        },
        ErrorMessage::ESignatureVerification(sve) => {
            let msg = string_of_signature_error(|loc: &ALoc| string_of_aloc(None, loc), sve);
            format!("ESignatureVerification ({})", msg)
        }
        ErrorMessage::EPrimitiveAsInterface(box EPrimitiveAsInterfaceData {
            use_op,
            lower,
            upper,
            kind: _,
        }) => {
            let dump_type =
                |type_ref: &ErrorTypeReferenceWithLocData<ALoc>| match &type_ref.type_desc {
                    TypeOrTypeDescT::Type(t) => dump_t(None, cx, t),
                    TypeOrTypeDescT::TypeDesc(desc) => format!("{desc:?}"),
                };
            format!(
                "EPrimitiveAsInterface ({}) ({}) ({})",
                string_of_use_op(use_op),
                dump_type(lower),
                dump_type(upper)
            )
        }
        ErrorMessage::ECannotSpreadInterface(box ECannotSpreadInterfaceData {
            spread_loc,
            interface,
            use_op,
        }) => {
            format!(
                "ECannotSpreadInterface ({}) ({}) ({})",
                string_of_aloc(None, spread_loc),
                dump_error_type_reference(cx, interface),
                string_of_use_op(use_op)
            )
        }
        ErrorMessage::ECannotSpreadIndexerOnRight(box ECannotSpreadIndexerOnRightData {
            spread_loc,
            object,
            key_desc,
            use_op,
            ..
        }) => {
            let key = match key_desc {
                TypeOrTypeDescT::Type(t) => dump_t(None, cx, t),
                TypeOrTypeDescT::TypeDesc(desc) => format!("{desc:?}"),
            };
            format!(
                "ECannotSpreadIndexerOnRight ({}) ({}) ({}) ({})",
                string_of_aloc(None, spread_loc),
                dump_error_type_reference(cx, object),
                key,
                string_of_use_op(use_op)
            )
        }
        ErrorMessage::EUnableToSpread(box EUnableToSpreadData {
            spread_loc,
            object1,
            object2,
            propname,
            error_kind: _,
            use_op,
        }) => {
            format!(
                "EUnableToSpread ({}) ({}) ({}) ({}) ({})",
                string_of_aloc(None, spread_loc),
                dump_error_type_reference(cx, object1),
                dump_error_type_reference(cx, object2),
                propname,
                string_of_use_op(use_op)
            )
        }
        ErrorMessage::EInexactMayOverwriteIndexer(box EInexactMayOverwriteIndexerData {
            spread_loc,
            key_desc,
            value_desc,
            object2,
            use_op,
            ..
        }) => {
            let key = match key_desc {
                TypeOrTypeDescT::Type(t) => dump_t(None, cx, t),
                TypeOrTypeDescT::TypeDesc(desc) => format!("{desc:?}"),
            };
            let value = match value_desc {
                TypeOrTypeDescT::Type(t) => dump_t(None, cx, t),
                TypeOrTypeDescT::TypeDesc(desc) => format!("{desc:?}"),
            };
            format!(
                "EInexactMayOverwriteIndexer ({}) ({}) ({}) ({}) ({})",
                string_of_aloc(None, spread_loc),
                key,
                value,
                dump_error_type_reference(cx, object2),
                string_of_use_op(use_op)
            )
        }
        ErrorMessage::EExponentialSpread(box EExponentialSpreadData {
            spread_loc,
            operand1,
            operand2,
        }) => {
            let format_operand_group =
                |g: &ExponentialSpreadOperandGroup<ErrorTypeReferenceWithLocData<ALoc>>| {
                    let second = match &g.second {
                        Some(t) => dump_error_type_reference(cx, t),
                        None => "None".to_string(),
                    };
                    format!("[{}; {}]", dump_error_type_reference(cx, &g.first), second)
                };
            format!(
                "EExponentialSpread(Box::new(EExponentialSpreadData {})) ({}) ({})",
                string_of_aloc(None, spread_loc),
                format_operand_group(operand1),
                format_operand_group(operand2)
            )
        }
        ErrorMessage::EEnumError(enum_error) => match enum_error {
            EnumErrorKind::EnumsNotEnabled(loc) => {
                format!(
                    "EEnumError (EnumsNotEnabled ({}))",
                    string_of_aloc(None, loc)
                )
            }
            EnumErrorKind::EnumConstNotSupported(loc) => {
                format!(
                    "EEnumError (EnumConstNotSupported ({}))",
                    string_of_aloc(None, loc)
                )
            }
            EnumErrorKind::EnumInvalidMemberAccess(box EnumInvalidMemberAccessData {
                member,
                member_loc,
                enum_,
            }) => {
                let (member_str, suggestion_str, member) = match member {
                    EnumMemberAccess::Named { name, suggestion } => (
                        name.to_string(),
                        suggestion.as_ref().map_or("<None>", |s| s.as_str()),
                        format!("{} {}", string_of_aloc(None, member_loc), name),
                    ),
                    EnumMemberAccess::Computed(type_ref) => (
                        "<None>".to_string(),
                        "<None>",
                        dump_loc_type_desc(cx, &type_ref.reference_loc, &type_ref.type_desc),
                    ),
                };
                format!(
                    "EEnumError (EnumInvalidMemberAccess ({}) ({}) ({}) ({}))",
                    member_str,
                    suggestion_str,
                    member,
                    dump_enum_reference(enum_)
                )
            }
            EnumErrorKind::EnumModification(box EnumModificationData { loc, enum_ }) => {
                format!(
                    "EEnumError (EnumModification ({}) ({}))",
                    string_of_aloc(None, loc),
                    dump_enum_reference(enum_)
                )
            }
            EnumErrorKind::EnumMemberDuplicateValue(box EnumMemberDuplicateValueData {
                loc,
                prev_use_loc,
                enum_,
            }) => {
                format!(
                    "EEnumError (EnumMemberDuplicateValue ({}) ({}) ({}))",
                    string_of_aloc(None, loc),
                    string_of_aloc(None, prev_use_loc),
                    dump_enum_reference(enum_)
                )
            }
            EnumErrorKind::EnumInvalidObjectUtilType(box EnumInvalidObjectUtilTypeData {
                operation_loc,
                operation_name,
                enum_,
                ..
            }) => {
                format!(
                    "EEnumError (EnumInvalidObjectUtilType ({} RType({operation_name:?})) ({}))",
                    string_of_aloc(None, operation_loc),
                    dump_enum_reference(enum_)
                )
            }
            EnumErrorKind::EnumInvalidObjectFunction(box EnumInvalidObjectFunctionData {
                operation_loc,
                function_name,
                enum_,
                ..
            }) => {
                format!(
                    "EEnumError (EnumInvalidObjectFunction ({} RIdentifier({function_name:?})) ({}))",
                    string_of_aloc(None, operation_loc),
                    dump_enum_reference(enum_)
                )
            }
            EnumErrorKind::EnumNotIterable(box EnumNotIterableData { enum_ }) => {
                format!(
                    "EEnumError (EnumNotIterable ({}))",
                    dump_enum_reference(enum_)
                )
            }
            EnumErrorKind::EnumNotIterableForIn(box EnumNotIterableForInData { enum_, .. }) => {
                format!(
                    "EEnumError (EnumNotIterableForIn ({}))",
                    dump_enum_reference(enum_)
                )
            }
            EnumErrorKind::EnumMemberAlreadyChecked(box EnumMemberAlreadyCheckedData {
                case_test_loc,
                prev_check_loc,
                enum_,
                member_name,
            }) => {
                format!(
                    "EEnumError (EnumMemberAlreadyChecked ({}) ({}) ({} {}) ({}))",
                    string_of_aloc(None, case_test_loc),
                    string_of_aloc(None, prev_check_loc),
                    string_of_aloc(None, &enum_.loc),
                    enum_.name,
                    member_name
                )
            }
            EnumErrorKind::EnumAllMembersAlreadyChecked(box EnumAllMembersAlreadyCheckedData {
                loc,
                enum_,
            }) => {
                format!(
                    "EEnumError (EnumAllMembersAlreadyChecked ({}) ({} {}))",
                    string_of_aloc(None, loc),
                    string_of_aloc(None, &enum_.loc),
                    enum_.name
                )
            }
            EnumErrorKind::EnumNotAllChecked(box EnumNotAllCheckedData {
                loc,
                description_name,
                enum_,
                left_to_check,
                default_case_loc,
            }) => {
                let default_str = match default_case_loc {
                    Some(loc) => string_of_aloc(None, loc),
                    None => "<None>".to_string(),
                };
                format!(
                    "EEnumError (EnumNotAllChecked ({} {}) ({} {}) ({}) ({}))",
                    string_of_aloc(None, loc),
                    description_name.as_deref().unwrap_or("<None>"),
                    string_of_aloc(None, &enum_.loc),
                    enum_.name,
                    left_to_check.join(", "),
                    default_str
                )
            }
            EnumErrorKind::EnumUnknownNotChecked(box EnumUnknownNotCheckedData {
                loc,
                description_name,
                enum_,
            }) => {
                format!(
                    "EEnumError (EnumUnknownNotChecked ({} {}) ({} {}))",
                    string_of_aloc(None, loc),
                    description_name.as_deref().unwrap_or("<None>"),
                    string_of_aloc(None, &enum_.loc),
                    enum_.name
                )
            }
            EnumErrorKind::EnumInvalidCheck(box EnumInvalidCheckData {
                loc,
                enum_,
                example_member,
                from_match,
                ..
            }) => {
                let member_str = match example_member {
                    Some(m) => m.to_string(),
                    None => "<None>".to_string(),
                };
                format!(
                    "EEnumError (EnumInvalidCheck ({}) ({}) ({}) ({}))",
                    string_of_aloc(None, loc),
                    dump_enum_reference(enum_),
                    member_str,
                    from_match
                )
            }
            EnumErrorKind::EnumMemberUsedAsType(box EnumMemberUsedAsTypeData {
                loc,
                description_name,
                enum_,
            }) => {
                format!(
                    "EEnumError (EnumMemberUsedAsType ({} {}) ({}))",
                    string_of_aloc(None, loc),
                    description_name.as_deref().unwrap_or("<None>"),
                    dump_enum_reference(enum_)
                )
            }
            EnumErrorKind::EnumIncompatible(box EIncompatibleTypesWithUseOpData {
                lower_desc,
                upper_desc,
                use_op,
                ..
            }) => {
                let lower = match lower_desc {
                    TypeOrTypeDescT::Type(t) => dump_t(None, cx, t),
                    TypeOrTypeDescT::TypeDesc(desc) => format!("{desc:?}"),
                };
                let upper = match upper_desc {
                    TypeOrTypeDescT::Type(t) => dump_t(None, cx, t),
                    TypeOrTypeDescT::TypeDesc(desc) => format!("{desc:?}"),
                };
                format!(
                    "EEnumError (EnumIncompatible ({}) ({}) ({}))",
                    lower,
                    upper,
                    string_of_use_op(use_op)
                )
            }
            EnumErrorKind::EnumInvalidAbstractUse(box EnumInvalidAbstractUseData {
                loc,
                description_name,
                enum_loc,
                enum_name,
            }) => {
                format!(
                    "EEnumError (EnumInvalidAbstractUse ({} {}) ({} {}))",
                    string_of_aloc(None, loc),
                    description_name.as_deref().unwrap_or("<None>"),
                    string_of_aloc(None, enum_loc),
                    enum_name.as_deref().unwrap_or("<None>")
                )
            }
            EnumErrorKind::EnumInvalidMemberName(box EnumInvalidMemberNameData {
                loc,
                enum_,
                member_name,
            }) => {
                format!(
                    "EEnumError (EnumInvalidMemberName ({}) ({}) ({}))",
                    string_of_aloc(None, loc),
                    dump_enum_reference(enum_),
                    member_name
                )
            }
            EnumErrorKind::EnumNonIdentifierMemberName(box EnumNonIdentifierMemberNameData {
                loc,
                enum_,
                member_name,
            }) => {
                format!(
                    "EEnumError (EnumNonIdentifierMemberName ({}) ({}) ({}))",
                    string_of_aloc(None, loc),
                    dump_enum_reference(enum_),
                    member_name
                )
            }
            EnumErrorKind::EnumDuplicateMemberName(box EnumDuplicateMemberNameData {
                loc,
                prev_use_loc,
                enum_,
                member_name,
            }) => {
                format!(
                    "EEnumError (EnumDuplicateMemberName ({}) ({}) ({}) ({}))",
                    string_of_aloc(None, loc),
                    string_of_aloc(None, prev_use_loc),
                    dump_enum_reference(enum_),
                    member_name
                )
            }
            EnumErrorKind::EnumInconsistentMemberValues(box EnumInconsistentMemberValuesData {
                loc,
                enum_,
            }) => {
                format!(
                    "EEnumError (EnumInconsistentMemberValues ({}) ({}))",
                    string_of_aloc(None, loc),
                    dump_enum_reference(enum_)
                )
            }
            EnumErrorKind::EnumInvalidMemberInitializer(box EnumInvalidMemberInitializerData {
                loc,
                enum_,
                member_name,
                ..
            }) => {
                format!(
                    "EEnumError (EnumInvalidMemberInitializer ({}) ({}) ({}))",
                    string_of_aloc(None, loc),
                    dump_enum_reference(enum_),
                    member_name
                )
            }
            EnumErrorKind::EnumBooleanMemberNotInitialized(
                box EnumBooleanMemberNotInitializedData {
                    loc,
                    enum_,
                    member_name,
                },
            ) => {
                format!(
                    "EEnumError (EnumBooleanMemberNotInitialized ({}) ({}) ({}))",
                    string_of_aloc(None, loc),
                    dump_enum_reference(enum_),
                    member_name
                )
            }
            EnumErrorKind::EnumNumberMemberNotInitialized(
                box EnumNumberMemberNotInitializedData {
                    loc,
                    enum_,
                    member_name,
                },
            ) => {
                format!(
                    "EEnumError (EnumNumberMemberNotInitialized ({}) ({}) ({}))",
                    string_of_aloc(None, loc),
                    dump_enum_reference(enum_),
                    member_name
                )
            }
            EnumErrorKind::EnumBigIntMemberNotInitialized(
                box EnumBigIntMemberNotInitializedData {
                    loc,
                    enum_,
                    member_name,
                },
            ) => {
                format!(
                    "EEnumError (EnumBigIntMemberNotInitialized ({}) ({}) ({}))",
                    string_of_aloc(None, loc),
                    dump_enum_reference(enum_),
                    member_name
                )
            }
            EnumErrorKind::EnumStringMemberInconsistentlyInitialized(
                box EnumStringMemberInconsistentlyInitializedData { loc, enum_ },
            ) => {
                format!(
                    "EEnumError (EnumStringMemberInconsistentlyInitialized ({}) ({}))",
                    string_of_aloc(None, loc),
                    dump_enum_reference(enum_)
                )
            }
            EnumErrorKind::TSEnumInvalidMember(box TSEnumInvalidMemberData {
                loc,
                enum_,
                member_name,
                kind: _,
            }) => {
                format!(
                    "EEnumError (TSEnumInvalidMember ({}) ({}) ({}))",
                    string_of_aloc(None, loc),
                    dump_enum_reference(enum_),
                    member_name
                )
            }
            EnumErrorKind::TSEnumInvalidSyntax(box TSEnumInvalidSyntaxData {
                loc,
                enum_,
                kind: _,
            }) => {
                format!(
                    "EEnumError (TSEnumInvalidSyntax ({}) ({}))",
                    string_of_aloc(None, loc),
                    dump_enum_reference(enum_)
                )
            }
        },
        ErrorMessage::EAssignConstLikeBinding(box EAssignConstLikeBindingData {
            loc,
            definition_loc,
            name,
            binding_kind,
        }) => {
            format!(
                "EAssignConstLikeBinding ({}) (identifier `{}` ({}) ) ({})",
                string_of_aloc(None, loc),
                name,
                string_of_aloc(None, definition_loc),
                binding_kind.as_str()
            )
        }
        ErrorMessage::EMalformedCode(loc) => {
            format!("EMalformedCode ({})", string_of_aloc(None, loc))
        }
        ErrorMessage::EObjectThisSuperReference(box EObjectThisSuperReferenceData {
            loc,
            method_loc,
            method_name,
            kind: _,
        }) => {
            format!(
                "EObjectThisSuperReference(Box::new(({}, {}, {:?}, _)))",
                string_of_aloc(None, loc),
                string_of_aloc(None, method_loc),
                method_name
            )
        }
        ErrorMessage::EComponentThisReference(box EComponentThisReferenceData {
            this_loc,
            component_loc,
        }) => {
            format!(
                "EComponentThisReference (this={}, component={})",
                string_of_aloc(None, this_loc),
                string_of_aloc(None, component_loc)
            )
        }
        ErrorMessage::EComponentCase(loc) => {
            format!("EComponentCase ({})", string_of_aloc(None, loc))
        }
        ErrorMessage::EComponentMissingReturn(r) => {
            format!("EComponentMissingReturn ({})", string_of_aloc(None, &r.loc))
        }
        ErrorMessage::EComponentMissingBody(loc) => {
            format!("EComponentMissingBody ({})", string_of_aloc(None, loc))
        }
        ErrorMessage::EComponentBodyInAmbientContext(loc) => {
            format!(
                "EComponentBodyInAmbientContext ({})",
                string_of_aloc(None, loc)
            )
        }
        ErrorMessage::ENestedComponent(loc) => {
            format!("ENestedComponent ({})", string_of_aloc(None, loc))
        }
        ErrorMessage::ENestedHook(loc) => {
            format!("ENestedHook ({})", string_of_aloc(None, loc))
        }
        ErrorMessage::EInvalidDeclaration(box EInvalidDeclarationData {
            declaration_loc,
            name,
            ..
        }) => {
            format!(
                "EInvalidDeclaration(Box::new(EInvalidDeclarationData {} {}))",
                string_of_aloc(None, declaration_loc),
                name
            )
        }
        ErrorMessage::EImplicitInstantiationUnderconstrainedError(
            box EImplicitInstantiationUnderconstrainedErrorData { use_op, .. },
        ) => {
            format!(
                "EImplicitInstantiationUnderconstrainedError ({})",
                string_of_use_op(use_op)
            )
        }
        ErrorMessage::EClassToObject(box EClassToObjectData { .. }) => "EClassToObject".to_string(),
        ErrorMessage::EHookIncompatible(box EHookIncompatibleData {
            use_op,
            lower,
            upper,
            ..
        }) => {
            let dump_type =
                |type_ref: &ErrorTypeReferenceWithLocData<ALoc>| match &type_ref.type_desc {
                    TypeOrTypeDescT::Type(t) => dump_t(None, cx, t),
                    TypeOrTypeDescT::TypeDesc(desc) => format!("{desc:?}"),
                };
            format!(
                "EHookIncompatible ({}) ({}) ({})",
                string_of_use_op(use_op),
                dump_type(lower),
                dump_type(upper)
            )
        }
        ErrorMessage::EHookUniqueIncompatible(box EHookUniqueIncompatibleData {
            use_op,
            lower,
            upper,
        }) => {
            let dump_type =
                |type_ref: &ErrorTypeReferenceWithLocData<ALoc>| match &type_ref.type_desc {
                    TypeOrTypeDescT::Type(t) => dump_t(None, cx, t),
                    TypeOrTypeDescT::TypeDesc(desc) => format!("{desc:?}"),
                };
            format!(
                "EHookUniqueIncompatible ({}) ({}) ({})",
                string_of_use_op(use_op),
                dump_type(lower),
                dump_type(upper)
            )
        }
        ErrorMessage::EHookRuleViolation(box EHookRuleViolationData { .. }) => {
            "EHookRuleViolation ( )".to_string()
        }
        ErrorMessage::EHookNaming(_) => "EHookNaming ( )".to_string(),
        ErrorMessage::EInvalidGraphQL(box (loc, err)) => {
            let err_str = match err {
                flow_parser_utils::graphql::GraphqlError::InvalidTaggedTemplate => {
                    "invalid tagged template"
                }
                flow_parser_utils::graphql::GraphqlError::InvalidGraphQL => "invalid graphql",
            };
            format!(
                "EInvalidGraphQL ({}) ({})",
                string_of_aloc(None, loc),
                err_str
            )
        }
        ErrorMessage::EAnnotationInference(box EAnnotationInferenceData {
            loc,
            operation,
            operation_loc: _,
            target_loc,
            target_desc,
        }) => {
            format!(
                "EAnnotationInference ({}) ({:?}) ({}) ({:?})",
                string_of_aloc(None, loc),
                operation,
                string_of_aloc(None, target_loc),
                target_desc,
            )
        }
        ErrorMessage::ETrivialRecursiveDefinition(loc) => {
            format!(
                "ETrivialRecursiveDefinition ({})",
                string_of_aloc(None, loc),
            )
        }
        ErrorMessage::ETrivialRecursiveTypeParameter(data) => format!(
            "ETrivialRecursiveTypeParameter ({}) ({})",
            string_of_aloc(None, &data.0),
            data.1
        ),
        ErrorMessage::EClassTypeParamInStatic(data) => format!(
            "EClassTypeParamInStatic ({}) ({})",
            string_of_aloc(None, &data.0),
            data.1
        ),
        ErrorMessage::EDefinitionCycle(_) => "EDefinitionCycle".to_string(),
        ErrorMessage::ERecursiveDefinition(box ERecursiveDefinitionData { .. }) => {
            "ERecursiveDefinition".to_string()
        }
        ErrorMessage::EDuplicateClassMember(box EDuplicateClassMemberData {
            loc, name, ..
        }) => {
            format!(
                "EDuplicateClassMember ({}) ({})",
                string_of_aloc(None, loc),
                name
            )
        }
        ErrorMessage::EReferenceInAnnotation(box (_, _, _)) => "EReferenceInAnnotation".to_string(),
        ErrorMessage::EReferenceInDefault(box (_, _, _)) => "EReferenceInDefault".to_string(),
        ErrorMessage::EEmptyArrayNoProvider { loc } => {
            format!("EEmptyArrayNoProvider ({})", string_of_aloc(None, loc))
        }
        ErrorMessage::EUnusedPromise { loc, .. } => {
            format!("EUnusedPromise ({})", string_of_aloc(None, loc))
        }
        ErrorMessage::EReactIntrinsicOverlap(box EReactIntrinsicOverlapData { .. }) => {
            "EReactIntrinsicOverlap (_, _, _)".to_string()
        }
        ErrorMessage::EReactRefInRender { .. } => "EReactRefInRender _".to_string(),
        ErrorMessage::EBigIntRShift3(operand) => {
            let operand_desc = match &operand.type_desc {
                TypeOrTypeDescT::Type(t) => dump_t(None, cx, t),
                TypeOrTypeDescT::TypeDesc(desc) => format!("{desc:?}"),
            };
            format!("EBigIntRShift3 ({})", operand_desc)
        }
        ErrorMessage::EBigIntNumCoerce(operand) => {
            let operand_desc = match &operand.type_desc {
                TypeOrTypeDescT::Type(t) => dump_t(None, cx, t),
                TypeOrTypeDescT::TypeDesc(desc) => format!("{desc:?}"),
            };
            format!("EBigIntNumCoerce ({})", operand_desc)
        }
        ErrorMessage::EInvalidCatchParameterAnnotation {
            loc,
            ts_utility_syntax,
        } => {
            format!(
                "EInvalidCatchParameterAnnotation {{{}; {}}}",
                string_of_aloc(None, loc),
                ts_utility_syntax
            )
        }
        ErrorMessage::ETSSyntax(box ETSSyntaxData { loc, .. }) => {
            format!("ETSSyntax ({})", string_of_aloc(None, loc))
        }
        ErrorMessage::EAbstractClass(box EAbstractClassData { loc, .. }) => {
            format!("EAbstractClass ({})", string_of_aloc(None, loc))
        }
        ErrorMessage::EOverride(box EOverrideData { loc, .. }) => {
            format!("EOverride ({})", string_of_aloc(None, loc))
        }
        ErrorMessage::EVarianceKeyword(box EVarianceKeywordData { loc, .. }) => {
            format!("EVarianceKeyword ({})", string_of_aloc(None, loc))
        }
        ErrorMessage::EInvalidBinaryArith(box EInvalidBinaryArithData {
            loc,
            left,
            right,
            kind,
        }) => {
            let left = match &left.type_desc {
                TypeOrTypeDescT::Type(t) => dump_t(None, cx, t),
                TypeOrTypeDescT::TypeDesc(desc) => format!("{desc:?}"),
            };
            let right = match &right.type_desc {
                TypeOrTypeDescT::Type(t) => dump_t(None, cx, t),
                TypeOrTypeDescT::TypeDesc(desc) => format!("{desc:?}"),
            };
            format!(
                "EInvalidBinaryArith ({}, {}, {}, {:?})",
                string_of_aloc(None, loc),
                left,
                right,
                kind
            )
        }
        ErrorMessage::EInvalidMappedType { loc, kind } => {
            let kind_str = match kind {
                InvalidMappedTypeErrorKind::InterfaceOrDeclaredClass => "InterfaceOrDeclaredClass",
                InvalidMappedTypeErrorKind::ExtraProperties => "ExtraProperties",
                InvalidMappedTypeErrorKind::ExplicitExactOrInexact => "ExplicitExactOrInexact",
                InvalidMappedTypeErrorKind::VarianceOnArrayInput => "VarianceOnArrayInput",
            };
            format!(
                "EInvalidMappedType ({}, {})",
                string_of_aloc(None, loc),
                kind_str
            )
        }
        ErrorMessage::EInvalidTemplateLiteralType { loc, kind } => {
            let kind_str = match kind {
                InvalidTemplateLiteralTypeErrorKind::TooComplex => "TooComplex",
                InvalidTemplateLiteralTypeErrorKind::InvalidPlaceholderType => {
                    "InvalidPlaceholderType"
                }
            };
            format!(
                "EInvalidTemplateLiteralType ({}, {})",
                string_of_aloc(None, loc),
                kind_str
            )
        }
        ErrorMessage::EDuplicateComponentProp(box EDuplicateComponentPropData {
            spread, ..
        }) => {
            format!("EDuplicateComponentProp ({})", string_of_aloc(None, spread))
        }
        ErrorMessage::EKeySpreadProp(box EKeySpreadPropData { loc, .. }) => {
            format!("EKeySpreadProp ({})", string_of_aloc(None, loc))
        }
        ErrorMessage::EInvalidComponentRestParam(loc) => {
            format!("EInvalidComponentRestParam ({})", string_of_aloc(None, loc))
        }
        ErrorMessage::EInvalidTypeCastSyntax { loc, .. } => {
            format!("EInvalidTypeCastSyntax ({})", string_of_aloc(None, loc))
        }
        ErrorMessage::EMissingPlatformSupportWithAvailablePlatforms(
            box EMissingPlatformSupportWithAvailablePlatformsData {
                loc,
                available_platforms,
                required_platforms,
            },
        ) => {
            format!(
                "EMissingPlatformSupportWithAvailablePlatforms({}, {:?}, {:?})",
                string_of_aloc(None, loc),
                available_platforms,
                required_platforms
            )
        }
        ErrorMessage::EUnionPartialOptimizationNonUniqueKey(
            box EUnionPartialOptimizationNonUniqueKeyData { loc, .. },
        ) => {
            format!(
                "EUnionPartialOptimizationNonUniqueKey ({})",
                string_of_aloc(None, loc)
            )
        }
        ErrorMessage::EUnionOptimization(box EUnionOptimizationData { loc, .. }) => {
            format!("EUnionOptimization ({})", string_of_aloc(None, loc))
        }
        ErrorMessage::EUnionOptimizationOnNonUnion(box EUnionOptimizationOnNonUnionData {
            loc,
            arg_desc,
            ..
        }) => {
            let arg = match arg_desc {
                TypeOrTypeDescT::Type(t) => dump_t(None, cx, t),
                TypeOrTypeDescT::TypeDesc(desc) => format!("{desc:?}"),
            };
            format!(
                "EUnionOptimizationOnNonUnion {{ loc = {}; arg = {} }}",
                string_of_aloc(None, loc),
                arg,
            )
        }
        ErrorMessage::ECannotCallReactComponent { component } => {
            let component = match &component.type_desc {
                TypeOrTypeDescT::Type(t) => dump_t(None, cx, t),
                TypeOrTypeDescT::TypeDesc(desc) => format!("{desc:?}"),
            };
            format!("ECannotCallReactComponent ({component})")
        }
        ErrorMessage::ENegativeTypeGuardConsistency(box ENegativeTypeGuardConsistencyData {
            return_reason,
            ..
        }) => {
            format!(
                "ENegativeTypeGuardConsistency ({})",
                string_of_aloc(None, &return_reason.loc)
            )
        }
        ErrorMessage::EAssertionFunctionFalsyAtExit(box EAssertionFunctionFalsyAtExitData {
            loc,
            ..
        }) => format!(
            "EAssertionFunctionFalsyAtExit ({})",
            string_of_aloc(None, loc)
        ),
        ErrorMessage::EUnrecognizedAssertionCall(box EUnrecognizedAssertionCallData {
            loc,
            ..
        }) => format!("EUnrecognizedAssertionCall ({})", string_of_aloc(None, loc)),
        ErrorMessage::EMatchError(e) => match e {
            MatchErrorKind::MatchNotExhaustive(box MatchNotExhaustiveData { loc, .. }) => {
                format!("EMatchNotExhaustive ({})", string_of_aloc(None, loc))
            }
            MatchErrorKind::MatchUnusedPattern(box MatchUnusedPatternData {
                pattern,
                already_seen,
            }) => {
                let already_str = match already_seen {
                    Some(already_seen) => format!(
                        "{} {:?}",
                        string_of_aloc(None, &already_seen.loc),
                        already_seen.kind
                    ),
                    None => "".to_string(),
                };
                format!(
                    "EMatchUnusedPattern ({} {:?}) ({})",
                    string_of_aloc(None, &pattern.loc),
                    pattern.kind,
                    already_str
                )
            }
            MatchErrorKind::MatchNonExhaustiveObjectPattern(
                box MatchNonExhaustiveObjectPatternData { loc, rest, .. },
            ) => {
                let rest_str = match rest {
                    Some(r) => dump_loc_type_desc(cx, &r.loc, &r.type_desc),
                    None => "".to_string(),
                };
                format!(
                    "EMatchNonExhaustiveObjectPattern ({}) ({})",
                    string_of_aloc(None, loc),
                    rest_str
                )
            }
            MatchErrorKind::MatchNonExplicitEnumCheck(box MatchNonExplicitEnumCheckData {
                loc,
                wildcard_loc,
                ..
            }) => {
                format!(
                    "EMatchNonExplicitEnumCheck ({}) ({})",
                    string_of_aloc(None, loc),
                    string_of_aloc(None, wildcard_loc)
                )
            }
            MatchErrorKind::MatchInvalidGuardedWildcard(loc) => {
                format!(
                    "EMatchInvalidGuardedWildcard ({})",
                    string_of_aloc(None, loc)
                )
            }
            MatchErrorKind::MatchInvalidIdentOrMemberPattern(
                box MatchInvalidIdentOrMemberPatternData { loc, type_desc, .. },
            ) => {
                let type_ = match type_desc {
                    TypeOrTypeDescT::Type(t) => dump_t(None, cx, t),
                    TypeOrTypeDescT::TypeDesc(desc) => format!("{desc:?}"),
                };
                format!(
                    "EMatchInvalidIdentOrMemberPattern ({}) ({})",
                    string_of_aloc(None, loc),
                    type_
                )
            }
            MatchErrorKind::MatchInvalidBindingKind { loc, kind } => {
                format!(
                    "EMatchInvalidBindingKind ({}) ({:?})",
                    string_of_aloc(None, loc),
                    kind
                )
            }
            MatchErrorKind::MatchInvalidObjectPropertyLiteral { loc, .. } => {
                format!(
                    "EMatchInvalidObjectPropertyLiteral ({})",
                    string_of_aloc(None, loc)
                )
            }
            MatchErrorKind::MatchInvalidUnaryZero { loc } => {
                format!("EMatchInvalidUnaryZero ({})", string_of_aloc(None, loc))
            }
            MatchErrorKind::MatchInvalidUnaryPlusBigInt { loc } => {
                format!("EMatchInvalidUnaryBigInt ({})", string_of_aloc(None, loc))
            }
            MatchErrorKind::MatchDuplicateObjectProperty(
                box MatchDuplicateObjectPropertyData { loc, name, .. },
            ) => {
                format!(
                    "EMatchDuplicateObjectProperty ({}) ({})",
                    string_of_aloc(None, loc),
                    name
                )
            }
            MatchErrorKind::MatchBindingInOrPattern { loc } => {
                format!("EMatchBindingInOrPattern ({})", string_of_aloc(None, loc))
            }
            MatchErrorKind::MatchInvalidAsPattern { loc } => {
                format!("EMatchInvalidAsPattern ({})", string_of_aloc(None, loc))
            }
            MatchErrorKind::MatchInvalidPatternReference(
                box MatchInvalidPatternReferenceData {
                    loc,
                    binding_loc,
                    name,
                },
            ) => {
                format!(
                    "EMatchInvalidPatternReference ({}) ({} {})",
                    string_of_aloc(None, loc),
                    string_of_aloc(None, binding_loc),
                    name
                )
            }
            MatchErrorKind::MatchInvalidObjectShorthand(box MatchInvalidObjectShorthandData {
                loc,
                name,
                ..
            }) => {
                format!(
                    "EMatchInvalidObjectShorthand ({}) ({})",
                    string_of_aloc(None, loc),
                    name
                )
            }
            MatchErrorKind::MatchStatementInvalidBody { loc } => {
                format!("EMatchStatementInvalidBody ({})", string_of_aloc(None, loc))
            }
            MatchErrorKind::MatchInvalidCaseSyntax(box MatchInvalidCaseSyntaxData {
                loc, ..
            }) => {
                format!("EMatchInvalidCaseSyntax ({})", string_of_aloc(None, loc))
            }
            MatchErrorKind::MatchInvalidWildcardSyntax(loc) => {
                format!(
                    "EMatchInvalidWildcardSyntax ({})",
                    string_of_aloc(None, loc)
                )
            }
            MatchErrorKind::MatchInvalidInstancePattern(loc) => {
                format!(
                    "EMatchInvalidInstancePattern ({})",
                    string_of_aloc(None, loc)
                )
            }
        },
        ErrorMessage::ERecordError(e) => match e {
            RecordErrorKind::RecordBannedTypeUtil { op_loc, op, record } => {
                format!(
                    "ERecordBannedTypeUtil ({} {:?}) ({})",
                    string_of_aloc(None, op_loc),
                    op,
                    dump_error_type_reference(cx, record)
                )
            }
            RecordErrorKind::RecordInvalidName { name, loc } => {
                format!(
                    "ERecordInvalidName {{ name = {}; loc = {} }}",
                    name,
                    string_of_aloc(None, loc)
                )
            }
            RecordErrorKind::RecordInvalidNew { record_name, loc } => {
                format!(
                    "ERecordInvalidNew {{ record_name = {}; loc = {} }}",
                    record_name,
                    string_of_aloc(None, loc)
                )
            }
            RecordErrorKind::RecordDeclarationInvalidSyntax { loc, .. } => {
                format!(
                    "ERecordDeclarationInvalidSyntax ({})",
                    string_of_aloc(None, loc)
                )
            }
        },
        ErrorMessage::EUndocumentedFeature { loc } => {
            format!("EUndocumentedFeature ({})", string_of_aloc(None, loc))
        }
        ErrorMessage::EIllegalAssertOperator(box EIllegalAssertOperatorData {
            obj,
            op_loc,
            specialized,
        }) => {
            format!(
                "EIllegalAssertOperator(Box::new(EIllegalAssertOperatorData {{obj={}, op={}, specialized={}}}))",
                dump_expression_reference(obj),
                string_of_aloc(None, op_loc),
                specialized
            )
        }
        ErrorMessage::EDevOnlyRefinedLocInfo(box EDevOnlyRefinedLocInfoData {
            refined_loc,
            ..
        }) => {
            format!(
                "EDevOnlyRefinedLocInfo(Box::new(EDevOnlyRefinedLocInfoData {{refined_loc={}}}))",
                string_of_aloc(None, refined_loc)
            )
        }
        ErrorMessage::EDevOnlyInvalidatedRefinementInfo(
            box EDevOnlyInvalidatedRefinementInfoData { read_loc, .. },
        ) => {
            format!(
                "EDevOnlyInvalidatedRefinementInfo(Box::new(EDevOnlyInvalidatedRefinementInfoData {{read_loc={}}}))",
                string_of_aloc(None, read_loc)
            )
        }
        ErrorMessage::ETypeParamConstIncompatibility(box ETypeParamConstIncompatibilityData {
            lower,
            ..
        }) => {
            format!(
                "ETypeParamConstIncompatibility ({})",
                format_args!("{} {:?}", string_of_aloc(None, &lower.loc), lower.name)
            )
        }
        ErrorMessage::ETypeParamConstInvalidPosition(data) => {
            format!(
                "ETypeParamConstInvalidPosition ({} {})",
                string_of_aloc(None, &data.0),
                data.1
            )
        }
        ErrorMessage::EConstantCondition(box EConstantConditionData {
            loc,
            is_truthy,
            warning,
            constant_condition_kind,
        }) => {
            let kind_str = match constant_condition_kind {
                ConstantConditionKind::UnawaitedPromise => "UnawaitedPromise",
                ConstantConditionKind::UncalledFunction => "UncalledFunction",
                ConstantConditionKind::ConstCondGeneral => "General",
            };
            format!(
                "EInvalidComparison ({}). is_truthy={} warning={:?} constant_condition_kind={}",
                string_of_aloc(None, loc),
                is_truthy,
                warning,
                kind_str
            )
        }
        ErrorMessage::EDeclareComponentInvalidParam { loc, kind } => {
            use flow_typing_errors::intermediate_error_types::DeclareComponentInvalidParamKind::*;
            let kind_str = match kind {
                DeclareComponentParamAsBinding => "AsBinding",
                DeclareComponentParamDefaultValue => "DefaultValue",
                DeclareComponentParamMissingAnnotation => "MissingAnnotation",
                DeclareComponentParamStringLiteralWithoutAs => "StringLiteralWithoutAs",
            };
            format!(
                "EDeclareComponentInvalidParam {{ loc={}; kind={} }}",
                string_of_aloc(None, loc),
                kind_str,
            )
        }
    }
}

pub mod verbose {
    use super::*;

    pub fn print_if_verbose_lazy(
        cx: &Context,
        trace: Option<&DepthTrace>,
        delim: Option<&str>,
        indent: Option<i32>,
        lines: impl FnOnce() -> Vec<String>,
    ) {
        if !cx.is_verbose() {
            return;
        }
        let verbose_opt = cx.verbose();
        if let Some(verbose) = verbose_opt {
            let actual_indent = std::cmp::max(
                indent.unwrap_or(0) + (trace.map(|t| t.depth()).unwrap_or(0) as i32) - 1,
                0,
            );
            let prefix = " ".repeat((actual_indent as usize) * (verbose.indent as usize));
            let pid = cx.pid_prefix();
            let add_prefix = |line: &str| format!("\n{}{}{}", prefix, pid, line);
            let lines_vec = lines();
            let prefixed: Vec<String> = lines_vec.iter().map(|l| add_prefix(l)).collect();
            eprintln!("{}", prefixed.join(delim.unwrap_or("")));
        }
    }

    pub fn print_if_verbose(
        cx: &Context,
        trace: Option<&DepthTrace>,
        delim: Option<&str>,
        indent: Option<i32>,
        lines: Vec<String>,
    ) {
        if cx.is_verbose() {
            print_if_verbose_lazy(cx, trace, delim, indent, || lines);
        }
    }

    pub fn print_types_if_verbose(
        cx: &Context,
        trace: &DepthTrace,
        note: Option<&str>,
        t: &Type,
        use_t: &UseT<Cx>,
    ) {
        if !cx.is_verbose() {
            return;
        }
        if let Some(verbose) = cx.verbose() {
            let depth = verbose.depth;
            let delim = match note {
                Some(x) => format!(" ~> {}", x),
                None => " ~>".to_string(),
            };
            print_if_verbose(
                cx,
                Some(trace),
                Some(&delim),
                None,
                vec![
                    super::dump_t(Some(depth), cx, t),
                    super::dump_use_t(Some(depth), cx, use_t),
                ],
            );
        }
    }

    pub fn print_unify_types_if_verbose(
        cx: &Context,
        trace: &DepthTrace,
        note: Option<&str>,
        t1: &Type,
        t2: &Type,
    ) {
        if !cx.is_verbose() {
            return;
        }
        if let Some(verbose) = cx.verbose() {
            let depth = verbose.depth;
            let delim = match note {
                Some(x) => format!(" = {}", x),
                None => " =".to_string(),
            };
            print_if_verbose(
                cx,
                Some(trace),
                Some(&delim),
                None,
                vec![
                    super::dump_t(Some(depth), cx, t1),
                    super::dump_t(Some(depth), cx, t2),
                ],
            );
        }
    }
}
