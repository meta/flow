/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::cell::LazyCell;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::ops::Deref;
use std::rc::Rc;
use std::sync::Arc;

use dupe::Dupe;
use dupe::OptionDupedExt;
use flow_aloc::ALoc;
use flow_aloc::ALocMap;
use flow_aloc::ALocTable;
use flow_analysis::property_assignment;
use flow_common::files;
use flow_common::flow_import_specifier::FlowImportSpecifier;
use flow_common::platform_set;
use flow_common::platform_set::PlatformSet;
use flow_common::polarity::Polarity;
use flow_common::reason;
use flow_common::reason::Name;
use flow_common::reason::Reason;
use flow_common::reason::VirtualReasonDesc;
use flow_data_structure_wrapper::ord_map::FlowOrdMap;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_lint_settings::lint_settings::LintSettings;
use flow_lint_settings::lints::SketchyNullKind;
use flow_lint_settings::severity::Severity;
use flow_lint_settings::strict_mode_settings::StrictModeSettings;
use flow_parser::ast;
use flow_parser::ast_visitor;
use flow_parser::ast_visitor::AstVisitor;
use flow_parser::file_key::FileKey;
use flow_parser::file_key::has_dts_ext;
use flow_parser::loc::Loc;
use flow_parser_utils::file_sig::FileSig;
use flow_type_sig::compact_table;
use flow_type_sig::type_sig;
use flow_type_sig::type_sig_options::TypeSigOptions;
use flow_type_sig::type_sig_utils;
use flow_typing_builtins::Builtins;
use flow_typing_builtins::LazyModule;
use flow_typing_builtins::LazyVal;
use flow_typing_context::BuiltinsGroup;
use flow_typing_context::Context;
use flow_typing_context::ContextArena;
use flow_typing_context::ContextId;
use flow_typing_context::Cx;
use flow_typing_context::MasterContext;
use flow_typing_context::Metadata;
use flow_typing_context::MkBuiltins;
use flow_typing_context::ResolvedRequire;
use flow_typing_errors::error_message::EComparisonData;
use flow_typing_errors::error_message::EConstantConditionData;
use flow_typing_errors::error_message::EDevOnlyInvalidatedRefinementInfoData;
use flow_typing_errors::error_message::EDevOnlyRefinedLocInfoData;
use flow_typing_errors::error_message::EIllegalAssertOperatorData;
use flow_typing_errors::error_message::EKeySpreadPropData;
use flow_typing_errors::error_message::EPlatformSpecificImplementationModuleLookupFailedData;
use flow_typing_errors::error_message::EPropNotFoundInLookupData;
use flow_typing_errors::error_message::ESketchyNullLintData;
use flow_typing_errors::error_message::ErrorMessage;
use flow_typing_errors::error_message::ErrorTypeReferenceWithLocData;
use flow_typing_errors::flow_error;
use flow_typing_errors::flow_error::FlowError;
use flow_typing_errors::intermediate_error_types::ConstantConditionKind;
use flow_typing_errors::intermediate_error_types::ConstantConditionWarning;
use flow_typing_errors::intermediate_error_types::ExpressionReferenceData;
use flow_typing_errors::intermediate_error_types::NamespaceExportDeclarationKind;
use flow_typing_errors::intermediate_error_types::StrictComparisonInfo;
use flow_typing_errors::intermediate_error_types::UnsupportedSyntax;
use flow_typing_exists_check::ExistsCheck;
use flow_typing_flow_common::flow_js_utils;
use flow_typing_flow_common::type_subst;
use flow_typing_flow_js::flow_js;
use flow_typing_flow_js::flow_js::FlowJs;
use flow_typing_flow_js::tvar_resolver;
use flow_typing_statement::exists_marker;
use flow_typing_statement::module_info_analyzer;
use flow_typing_statement::react_rules;
use flow_typing_type::type_::AnySource;
use flow_typing_type::type_::BigIntLiteral;
use flow_typing_type::type_::ConformToCommonInterfaceData;
use flow_typing_type::type_::DefTInner;
use flow_typing_type::type_::EnumInfo;
use flow_typing_type::type_::EnumInfoInner;
use flow_typing_type::type_::GenericTData;
use flow_typing_type::type_::ImplicitInstantiationTvarData;
use flow_typing_type::type_::ImplicitInstantiationTvarKind;
use flow_typing_type::type_::Literal;
use flow_typing_type::type_::MixedFlavor;
use flow_typing_type::type_::ModuleType;
use flow_typing_type::type_::NominalType;
use flow_typing_type::type_::NominalTypeInner;
use flow_typing_type::type_::NumberLiteral;
use flow_typing_type::type_::PropertyInner;
use flow_typing_type::type_::RootUseOp;
use flow_typing_type::type_::TvarSeenSet;
use flow_typing_type::type_::Type;
use flow_typing_type::type_::TypeInner;
use flow_typing_type::type_::UseOp;
use flow_typing_type::type_::UseT;
use flow_typing_type::type_::UseTInner;
use flow_typing_type::type_::any_t;
use flow_typing_type::type_::constraint::Constraints;
use flow_typing_type::type_::constraint::forcing_state::ForcingState;
use flow_typing_type::type_::drop_generic;
use flow_typing_type::type_::eval;
use flow_typing_type::type_::exports;
use flow_typing_type::type_::nominal;
use flow_typing_type::type_::null;
use flow_typing_type::type_::properties;
use flow_typing_type::type_::property;
use flow_typing_type::type_::type_or_type_desc;
use flow_typing_type::type_::void;
use flow_typing_type::type_util::reason_of_t;
use flow_typing_utils::check_polarity;
use flow_typing_utils::exhaustive;
use flow_typing_utils::module_exports_checker;
use flow_typing_utils::strict_es6_import_export;
use flow_typing_utils::type_filter;
use flow_typing_utils::type_operation_utils::type_assertions;
use flow_typing_utils::type_sig_merge;
use flow_typing_visitors::type_visitor;
use flow_typing_visitors::type_visitor::TypeVisitor;
use flow_utils_concurrency::job_error::JobError;
use flow_utils_union_find::Node;

fn force_lazy_tvars<'cx>(cx: &mut Context<'cx>) -> Result<(), JobError> {
    cx.force_post_component_tvars()
}

fn detect_sketchy_null_checks<'cx>(cx: &mut Context<'cx>, tast: &ast::Program<ALoc, (ALoc, Type)>) {
    exists_marker::mark(cx, tast);

    let add_error = |cx: &mut Context<'cx>,
                     loc: &ALoc,
                     null_loc: &ALoc,
                     kind: SketchyNullKind,
                     falsy_loc: &ALoc| {
        flow_js::add_output_non_speculating(
            cx,
            ErrorMessage::ESketchyNullLint(Box::new(ESketchyNullLintData {
                kind,
                loc: loc.dupe(),
                null_loc: null_loc.dupe(),
                falsy_loc: falsy_loc.dupe(),
            })),
        );
    };

    let detect_function = |cx: &mut Context<'cx>,
                           exists_excuses: &ALocMap<ExistsCheck>,
                           loc: &ALoc,
                           exists_check: &ExistsCheck| {
        let exists_excuse = exists_excuses
            .get(loc)
            .cloned()
            .unwrap_or_else(ExistsCheck::empty);

        match &exists_check.null_loc {
            None => {}
            Some(null_loc) => {
                if exists_excuse.bool_loc.is_none() {
                    if let Some(ref falsy_loc) = exists_check.bool_loc {
                        add_error(cx, loc, null_loc, SketchyNullKind::Bool, falsy_loc);
                    }
                }
                if exists_excuse.number_loc.is_none() {
                    if let Some(ref falsy_loc) = exists_check.number_loc {
                        add_error(cx, loc, null_loc, SketchyNullKind::Number, falsy_loc);
                    }
                }
                if exists_excuse.bigint_loc.is_none() {
                    if let Some(ref falsy_loc) = exists_check.bigint_loc {
                        add_error(cx, loc, null_loc, SketchyNullKind::BigInt, falsy_loc);
                    }
                }
                if exists_excuse.string_loc.is_none() {
                    if let Some(ref falsy_loc) = exists_check.string_loc {
                        add_error(cx, loc, null_loc, SketchyNullKind::String, falsy_loc);
                    }
                }
                if exists_excuse.mixed_loc.is_none() {
                    if let Some(ref falsy_loc) = exists_check.mixed_loc {
                        add_error(cx, loc, null_loc, SketchyNullKind::Mixed, falsy_loc);
                    }
                }
                if exists_excuse.enum_bool_loc.is_none() {
                    if let Some(ref falsy_loc) = exists_check.enum_bool_loc {
                        add_error(cx, loc, null_loc, SketchyNullKind::EnumBool, falsy_loc);
                    }
                }
                if exists_excuse.enum_number_loc.is_none() {
                    if let Some(ref falsy_loc) = exists_check.enum_number_loc {
                        add_error(cx, loc, null_loc, SketchyNullKind::EnumNumber, falsy_loc);
                    }
                }
                if exists_excuse.enum_bigint_loc.is_none() {
                    if let Some(ref falsy_loc) = exists_check.enum_bigint_loc {
                        add_error(cx, loc, null_loc, SketchyNullKind::EnumBigInt, falsy_loc);
                    }
                }
                if exists_excuse.enum_string_loc.is_none() {
                    if let Some(ref falsy_loc) = exists_check.enum_string_loc {
                        add_error(cx, loc, null_loc, SketchyNullKind::EnumString, falsy_loc);
                    }
                }
            }
        }
    };

    let exists_checks = {
        let checks = cx.exists_checks().clone();
        if !checks.is_empty() {
            fn make_checks<'cx>(
                cx: &mut Context<'cx>,
                seen: &mut TvarSeenSet<i32>,
                cur_checks: ALocMap<ExistsCheck>,
                loc: &ALoc,
                t: &Type,
            ) -> ALocMap<ExistsCheck> {
                match t.deref() {
                    TypeInner::AnnotT(_, t, _) => make_checks(cx, seen, cur_checks, loc, t),
                    TypeInner::OpenT(tvar) if seen.contains(&(tvar.id() as i32)) => cur_checks,
                    TypeInner::OpenT(tvar) => {
                        let id = tvar.id() as i32;
                        match cx.find_resolved(t) {
                            Some(resolved) => seen.with_added(id, |seen| {
                                make_checks(cx, seen, cur_checks, loc, &resolved)
                            }),
                            None => cur_checks,
                        }
                    }
                    // Ignore AnyTs for sketchy null checks; otherwise they'd always trigger the lint.
                    TypeInner::AnyT(_, _)
                    | TypeInner::ImplicitInstantiationTvar(box ImplicitInstantiationTvarData {
                        kind: ImplicitInstantiationTvarKind::Placeholder,
                        ..
                    }) => cur_checks,
                    TypeInner::GenericT(box GenericTData { bound, .. }) => {
                        make_checks(cx, seen, cur_checks, loc, bound)
                    }
                    TypeInner::NominalT { nominal_type, .. }
                        if let Some(t) = match &nominal_type.underlying_t {
                            nominal::UnderlyingT::OpaqueWithLocal { t }
                            | nominal::UnderlyingT::CustomError(box nominal::CustomErrorData {
                                t,
                                ..
                            }) => Some(t),
                            _ => None,
                        } =>
                    {
                        make_checks(cx, seen, cur_checks, loc, t)
                    }
                    TypeInner::NominalT { nominal_type, .. }
                        if matches!(
                            &nominal_type.underlying_t,
                            nominal::UnderlyingT::FullyOpaque
                        ) && let Some(upper_t) = nominal_type.upper_t.as_ref() =>
                    {
                        make_checks(cx, seen, cur_checks, loc, upper_t)
                    }
                    TypeInner::MaybeT(r, t) => {
                        let acc = make_checks(cx, seen, cur_checks, loc, t);
                        let null_t = null::why(r.dupe());
                        let acc = make_checks(cx, seen, acc, loc, &null_t);
                        let void_t = void::why(r.dupe());
                        make_checks(cx, seen, acc, loc, &void_t)
                    }
                    TypeInner::OptionalT { reason, type_, .. } => {
                        let acc = make_checks(cx, seen, cur_checks, loc, type_);
                        let void_t = void::why(reason.dupe());
                        make_checks(cx, seen, acc, loc, &void_t)
                    }
                    TypeInner::UnionT(_, rep) => {
                        let mut acc = cur_checks;
                        for t in rep.members_iter() {
                            acc = make_checks(cx, seen, acc, loc, t);
                        }
                        acc
                    }
                    _ => {
                        let t_loc = {
                            let reason = reason_of_t(t);
                            match reason.annot_loc() {
                                Some(loc) => Some(loc.dupe()),
                                None => Some(reason.def_loc().dupe()),
                            }
                        };
                        let mut exists_check = cur_checks
                            .get(loc)
                            .cloned()
                            .unwrap_or_else(ExistsCheck::empty);
                        let filter_result = type_filter::maybe(cx, t.dupe());
                        match filter_result.type_.deref() {
                            TypeInner::DefT(_, def_t) if matches!(&**def_t, DefTInner::EmptyT) => {}
                            _ => {
                                exists_check.null_loc = t_loc.dupe();
                            }
                        }

                        let filtered = {
                            let r1 = type_filter::not_truthy(cx, t.dupe());
                            let r2 = type_filter::not_maybe(cx, r1.type_);
                            r2.type_
                        };

                        fn enum_representation_t(ev: &Rc<EnumInfo>) -> &Type {
                            match &***ev {
                                EnumInfoInner::ConcreteEnum(concrete) => &concrete.representation_t,
                                EnumInfoInner::AbstractEnum { representation_t } => {
                                    representation_t
                                }
                            }
                        }

                        match filtered.deref() {
                            TypeInner::DefT(_, def_t)
                                if matches!(
                                    &**def_t,
                                    DefTInner::BoolGeneralT | DefTInner::SingletonBoolT { .. }
                                ) =>
                            {
                                exists_check.bool_loc = t_loc.dupe();
                            }
                            TypeInner::DefT(_, def_t)
                                if matches!(
                                    &**def_t,
                                    DefTInner::StrGeneralT(_) | DefTInner::SingletonStrT { .. }
                                ) =>
                            {
                                exists_check.string_loc = t_loc.dupe();
                            }
                            TypeInner::DefT(_, def_t)
                                if matches!(
                                    &**def_t,
                                    DefTInner::NumGeneralT(_) | DefTInner::SingletonNumT { .. }
                                ) =>
                            {
                                exists_check.number_loc = t_loc.dupe();
                            }
                            TypeInner::DefT(_, def_t)
                                if matches!(&**def_t, DefTInner::BigIntGeneralT(_)) =>
                            {
                                exists_check.bigint_loc = t_loc.dupe();
                            }
                            TypeInner::DefT(_, def_t)
                                if matches!(&**def_t, DefTInner::MixedT(_)) =>
                            {
                                exists_check.mixed_loc = t_loc.dupe();
                            }
                            TypeInner::DefT(_, def_t)
                                if match def_t.deref() {
                                    DefTInner::EnumValueT(ev) => {
                                        match enum_representation_t(ev).deref() {
                                            TypeInner::DefT(_, inner) => {
                                                matches!(
                                                    inner.deref(),
                                                    DefTInner::BoolGeneralT
                                                        | DefTInner::SingletonBoolT { .. }
                                                )
                                            }
                                            _ => false,
                                        }
                                    }
                                    _ => false,
                                } =>
                            {
                                exists_check.enum_bool_loc = t_loc.dupe();
                            }
                            TypeInner::DefT(_, def_t)
                                if match def_t.deref() {
                                    DefTInner::EnumValueT(ev) => {
                                        match enum_representation_t(ev).deref() {
                                            TypeInner::DefT(_, inner) => {
                                                matches!(
                                                    inner.deref(),
                                                    DefTInner::StrGeneralT(_)
                                                        | DefTInner::SingletonStrT { .. }
                                                )
                                            }
                                            _ => false,
                                        }
                                    }
                                    _ => false,
                                } =>
                            {
                                exists_check.enum_string_loc = t_loc.dupe();
                            }
                            TypeInner::DefT(_, def_t)
                                if match def_t.deref() {
                                    DefTInner::EnumValueT(ev) => {
                                        match enum_representation_t(ev).deref() {
                                            TypeInner::DefT(_, inner) => {
                                                matches!(
                                                    inner.deref(),
                                                    DefTInner::NumGeneralT(_)
                                                        | DefTInner::SingletonNumT { .. }
                                                )
                                            }
                                            _ => false,
                                        }
                                    }
                                    _ => false,
                                } =>
                            {
                                exists_check.enum_number_loc = t_loc.dupe();
                            }
                            TypeInner::DefT(_, def_t)
                                if match def_t.deref() {
                                    DefTInner::EnumValueT(ev) => match enum_representation_t(ev)
                                        .deref()
                                    {
                                        TypeInner::DefT(_, inner) => {
                                            matches!(inner.deref(), DefTInner::BigIntGeneralT(_))
                                        }
                                        _ => false,
                                    },
                                    _ => false,
                                } =>
                            {
                                exists_check.enum_bigint_loc = t_loc.dupe();
                            }
                            _ => {}
                        }
                        if exists_check == ExistsCheck::empty() {
                            cur_checks
                        } else {
                            let mut result = cur_checks;
                            result.insert(loc.dupe(), exists_check);
                            result
                        }
                    }
                }
            }

            let mut result: ALocMap<ExistsCheck> = ALocMap::new();
            for (loc, tset) in checks.iter() {
                for t in tset.iter() {
                    result = make_checks(cx, &mut TvarSeenSet::new(), result, loc, t);
                }
            }
            result
        } else {
            ALocMap::new()
        }
    };

    let exists_excuses = cx.exists_excuses().clone();
    for (loc, exists_check) in exists_checks.iter() {
        detect_function(cx, &exists_excuses, loc, exists_check);
    }
}

fn detect_test_prop_misses<'cx>(cx: &mut Context<'cx>) {
    let misses = cx.test_prop_get_never_hit();
    for (prop_name, (reason_prop, obj_t), use_op, suggestion) in misses.iter() {
        flow_js::add_output_non_speculating(
            cx,
            ErrorMessage::EPropNotFoundInLookup(Box::new(EPropNotFoundInLookupData {
                prop_name: prop_name.dupe(),
                prop_loc: reason_prop.loc().dupe(),
                object: flow_js_utils::type_reference_with_reason_for_error(
                    obj_t,
                    reason_of_t(obj_t).dupe(),
                ),
                use_op: use_op.dupe(),
                suggestion: suggestion.dupe(),
            })),
        );
    }
}

enum TruthynessResult {
    ConstCondTruthy {
        constant_condition_kind: ConstantConditionKind,
        reason: Option<Reason>,
    },
    ConstCondFalsy {
        constant_condition_kind: ConstantConditionKind,
        reason: Option<Reason>,
    },
    ConstCondUnknown,
    ConstCondUnconcretized,
}

fn try_eval_concrete_type_truthyness<'cx>(cx: &mut Context<'cx>, t: &Type) -> TruthynessResult {
    match t.deref() {
        TypeInner::OpenT(_)
        | TypeInner::ImplicitInstantiationTvar(box ImplicitInstantiationTvarData {
            kind: ImplicitInstantiationTvarKind::Inferred,
            ..
        })
        | TypeInner::EvalT { .. }
        | TypeInner::TypeAppT(..)
        | TypeInner::GenericT(..)
        | TypeInner::AnnotT(_, _, _)
        | TypeInner::MaybeT(_, _)
        | TypeInner::OptionalT { .. }
        | TypeInner::UnionT(_, _) => TruthynessResult::ConstCondUnconcretized,
        TypeInner::DefT(reason, def_t) => match def_t.deref() {
            DefTInner::NumGeneralT(literal) => match literal {
                Literal::Truthy => TruthynessResult::ConstCondTruthy {
                    constant_condition_kind: ConstantConditionKind::ConstCondGeneral,
                    reason: Some(reason.dupe()),
                },
                // might be null if it's from member access due to flow soundness hole
                // or internal flow bugs
                Literal::AnyLiteral => TruthynessResult::ConstCondUnknown,
            },
            DefTInner::StrGeneralT(literal) => match literal {
                Literal::Truthy => TruthynessResult::ConstCondTruthy {
                    constant_condition_kind: ConstantConditionKind::ConstCondGeneral,
                    reason: Some(reason.dupe()),
                },
                // we don't know the exact string literal but we might know the truthyness from refinement
                // might be null if it's from member access due to flow soundness hole
                // or internal flow bugs
                Literal::AnyLiteral => TruthynessResult::ConstCondUnknown,
            },
            DefTInner::BoolGeneralT => TruthynessResult::ConstCondUnknown,
            DefTInner::BigIntGeneralT(literal) => match literal {
                Literal::Truthy => TruthynessResult::ConstCondTruthy {
                    constant_condition_kind: ConstantConditionKind::ConstCondGeneral,
                    reason: Some(reason.dupe()),
                },
                // might be null if it's from member access due to flow soundness hole
                // or internal flow bugs
                Literal::AnyLiteral => TruthynessResult::ConstCondUnknown,
            },
            DefTInner::EmptyT => TruthynessResult::ConstCondUnknown,
            DefTInner::MixedT(_) => TruthynessResult::ConstCondUnknown,
            DefTInner::NullT | DefTInner::VoidT => TruthynessResult::ConstCondFalsy {
                constant_condition_kind: ConstantConditionKind::ConstCondGeneral,
                reason: Some(reason.dupe()),
            },
            DefTInner::SymbolT | DefTInner::UniqueSymbolT(_) => TruthynessResult::ConstCondUnknown,
            DefTInner::FunT(..) => TruthynessResult::ConstCondTruthy {
                constant_condition_kind: ConstantConditionKind::UncalledFunction,
                reason: Some(reason.dupe()),
            },
            DefTInner::ObjT(_) => TruthynessResult::ConstCondUnknown,
            DefTInner::ArrT(_) => TruthynessResult::ConstCondUnknown,
            DefTInner::ClassT(_) => TruthynessResult::ConstCondUnknown,
            DefTInner::InstanceT(instance_t) => {
                let class_id = instance_t.inst.class_id.dupe();
                if flow_js_utils::is_builtin_class_id("Promise", class_id, cx) {
                    TruthynessResult::ConstCondTruthy {
                        constant_condition_kind: ConstantConditionKind::UnawaitedPromise,
                        reason: Some(reason.dupe()),
                    }
                } else {
                    TruthynessResult::ConstCondUnknown
                }
            }
            DefTInner::SingletonStrT { value, .. } => {
                if value.is_empty() {
                    TruthynessResult::ConstCondFalsy {
                        constant_condition_kind: ConstantConditionKind::ConstCondGeneral,
                        reason: Some(reason.dupe()),
                    }
                } else {
                    TruthynessResult::ConstCondUnknown
                }
            }
            DefTInner::NumericStrKeyT(_) => TruthynessResult::ConstCondUnknown,
            DefTInner::SingletonNumT { value, .. } => {
                let NumberLiteral(number_value, _) = value;
                if *number_value == 0.0 {
                    TruthynessResult::ConstCondFalsy {
                        constant_condition_kind: ConstantConditionKind::ConstCondGeneral,
                        reason: Some(reason.dupe()),
                    }
                } else {
                    TruthynessResult::ConstCondUnknown
                }
            }
            DefTInner::SingletonBoolT { value, .. } => {
                if *value {
                    TruthynessResult::ConstCondTruthy {
                        constant_condition_kind: ConstantConditionKind::ConstCondGeneral,
                        reason: Some(reason.dupe()),
                    }
                } else {
                    TruthynessResult::ConstCondFalsy {
                        constant_condition_kind: ConstantConditionKind::ConstCondGeneral,
                        reason: Some(reason.dupe()),
                    }
                }
            }
            DefTInner::SingletonBigIntT { value, .. } => {
                let BigIntLiteral(bigint_value, _) = value;
                match bigint_value {
                    Some(0) => TruthynessResult::ConstCondFalsy {
                        constant_condition_kind: ConstantConditionKind::ConstCondGeneral,
                        reason: Some(reason.dupe()),
                    },
                    _ => TruthynessResult::ConstCondUnknown,
                }
            }
            DefTInner::TypeT(..) => TruthynessResult::ConstCondUnknown,
            DefTInner::PolyT(_) => TruthynessResult::ConstCondUnknown,
            DefTInner::ReactAbstractComponentT(_) => TruthynessResult::ConstCondUnknown,
            DefTInner::RendersT(_) => TruthynessResult::ConstCondUnknown,
            DefTInner::EnumValueT(_) => TruthynessResult::ConstCondUnknown,
            DefTInner::EnumObjectT { .. } => TruthynessResult::ConstCondUnknown,
        },
        TypeInner::ThisInstanceT(..) => TruthynessResult::ConstCondUnknown,
        TypeInner::ThisTypeAppT(..) => TruthynessResult::ConstCondUnknown,
        TypeInner::ObjProtoT(_) => TruthynessResult::ConstCondUnknown,
        TypeInner::FunProtoT(_) => TruthynessResult::ConstCondUnknown,
        TypeInner::NullProtoT(_) => TruthynessResult::ConstCondUnknown,
        TypeInner::IntersectionT(_, _) => TruthynessResult::ConstCondUnknown,
        TypeInner::KeysT(_, _) => TruthynessResult::ConstCondUnknown,
        TypeInner::TemplateLiteralT { .. } => TruthynessResult::ConstCondUnknown,
        TypeInner::StringMappingT { .. } => TruthynessResult::ConstCondUnknown,
        TypeInner::NominalT { .. } => TruthynessResult::ConstCondUnknown,
        TypeInner::NamespaceT(_) => TruthynessResult::ConstCondUnknown,
        TypeInner::AnyT(_, _)
        | TypeInner::ImplicitInstantiationTvar(box ImplicitInstantiationTvarData {
            kind: ImplicitInstantiationTvarKind::Placeholder,
            ..
        }) => TruthynessResult::ConstCondUnknown,
    }
}

fn try_eval_type_truthyness<'cx>(
    cx: &mut Context<'cx>,
    t: &Type,
) -> Result<TruthynessResult, JobError> {
    let reason = reason_of_t(t);
    let concrete_types = flow_js_utils::flow_js_result_to_job_error(
        FlowJs::all_possible_concrete_types(cx, reason, t),
    )?;
    let results: Vec<_> = concrete_types
        .iter()
        .map(|t| try_eval_concrete_type_truthyness(cx, t))
        .collect();
    Ok(match results.len() {
        0 => TruthynessResult::ConstCondUnknown,
        1 => results.into_iter().next().unwrap(),
        _ => {
            if results
                .iter()
                .all(|r| matches!(r, TruthynessResult::ConstCondTruthy { .. }))
            {
                // we don't pass on the error kind when there's multiple types
                TruthynessResult::ConstCondTruthy {
                    constant_condition_kind: ConstantConditionKind::ConstCondGeneral,
                    reason: None,
                }
            } else if results
                .iter()
                .all(|r| matches!(r, TruthynessResult::ConstCondFalsy { .. }))
            {
                // we don't pass on the error kind when there's multiple types
                TruthynessResult::ConstCondFalsy {
                    constant_condition_kind: ConstantConditionKind::ConstCondGeneral,
                    reason: None,
                }
            } else {
                TruthynessResult::ConstCondUnknown
            }
        }
    })
}

#[derive(Clone, Dupe)]
enum CheckConditionResult {
    ConditionAllowed,
    ConditionBanned {
        is_truthy: bool,
        show_warning: bool,
        constant_condition_kind: ConstantConditionKind,
        reason: Option<Reason>,
    },
}

fn condition_banned_and_truthy() -> CheckConditionResult {
    CheckConditionResult::ConditionBanned {
        is_truthy: true,
        show_warning: false,
        constant_condition_kind: ConstantConditionKind::ConstCondGeneral,
        reason: None,
    }
}

fn condition_banned_and_falsy() -> CheckConditionResult {
    CheckConditionResult::ConditionBanned {
        is_truthy: false,
        show_warning: false,
        constant_condition_kind: ConstantConditionKind::ConstCondGeneral,
        reason: None,
    }
}

fn condition_allowed() -> CheckConditionResult {
    CheckConditionResult::ConditionAllowed
}

fn use_type_to_check_conditional<'cx>(
    cx: &mut Context<'cx>,
    ttype: &Type,
) -> Result<CheckConditionResult, JobError> {
    // We always show warning for errors generated from type inferrence
    Ok(match try_eval_type_truthyness(cx, ttype)? {
        TruthynessResult::ConstCondTruthy {
            constant_condition_kind,
            reason,
        } => CheckConditionResult::ConditionBanned {
            is_truthy: true,
            show_warning: true,
            constant_condition_kind,
            reason,
        },
        TruthynessResult::ConstCondFalsy {
            constant_condition_kind,
            reason,
        } => CheckConditionResult::ConditionBanned {
            is_truthy: false,
            show_warning: true,
            constant_condition_kind,
            reason,
        },
        TruthynessResult::ConstCondUnknown => CheckConditionResult::ConditionAllowed,
        // ConstCondUnconcretized shouldn't happen because we call
        // `all_possible_concrete_types` to concretize all possible types
        TruthynessResult::ConstCondUnconcretized => CheckConditionResult::ConditionAllowed,
    })
}

fn check_conditional<'cx>(
    should_report_error: bool,
    cx: &mut Context<'cx>,
    e: &ast::expression::Expression<ALoc, (ALoc, Type)>,
    cached_results: &mut ALocMap<CheckConditionResult>,
) -> Result<CheckConditionResult, JobError> {
    let mut should_report_error_ref = should_report_error;
    let (loc, ttype) = e.loc();
    let exp = e.deref();
    if let Some(result) = cached_results.get(loc) {
        return Ok(result.dupe());
    }
    let check_condition_result = match exp {
        ast::expression::ExpressionInner::StringLiteral { inner, .. } => {
            if inner.value.as_str() == "" {
                condition_banned_and_falsy()
            } else {
                condition_banned_and_truthy()
            }
        }
        // true/false is allowed because it's likely intentional
        // and could be useful for debugging / iterating
        ast::expression::ExpressionInner::BooleanLiteral { .. } => condition_allowed(),
        ast::expression::ExpressionInner::NullLiteral { .. } => condition_banned_and_falsy(),
        ast::expression::ExpressionInner::NumberLiteral { inner, .. } => {
            if inner.value != 0.0 && inner.value != 1.0 {
                condition_banned_and_truthy()
            } else {
                condition_allowed()
            }
        }
        ast::expression::ExpressionInner::BigIntLiteral { inner, .. } => match inner.value {
            Some(0) | Some(1) => condition_allowed(),
            _ => condition_banned_and_truthy(),
        },
        ast::expression::ExpressionInner::RegExpLiteral { .. } => condition_banned_and_truthy(),
        ast::expression::ExpressionInner::ModuleRefLiteral { .. } => condition_banned_and_truthy(),
        ast::expression::ExpressionInner::Super { .. } => condition_banned_and_truthy(),
        // we don't check `++` and `--` because their value could change
        ast::expression::ExpressionInner::Update { .. } => condition_allowed(),
        ast::expression::ExpressionInner::Logical { inner, .. } => match inner.operator {
            ast::expression::LogicalOperator::Or | ast::expression::LogicalOperator::And => {
                let left_result = check_conditional(false, cx, &inner.left, cached_results)?;
                let right_result = check_conditional(false, cx, &inner.right, cached_results)?;
                match (&left_result, &right_result) {
                    (
                        CheckConditionResult::ConditionBanned {
                            is_truthy: left_truthy,
                            show_warning: left_show_warning,
                            constant_condition_kind: left_kind,
                            ..
                        },
                        CheckConditionResult::ConditionBanned {
                            is_truthy: right_truthy,
                            show_warning: right_show_warning,
                            constant_condition_kind: right_kind,
                            ..
                        },
                    ) if left_truthy == right_truthy => CheckConditionResult::ConditionBanned {
                        is_truthy: *left_truthy,
                        show_warning: *left_show_warning || *right_show_warning,
                        constant_condition_kind: if left_kind == right_kind {
                            *left_kind
                        } else {
                            ConstantConditionKind::ConstCondGeneral
                        },
                        reason: None,
                    },
                    _ => condition_allowed(),
                }
            }
            // NullishCoalesce is allowed here because the `lhs` is stored in `conditions`
            // and we will check `lhs` in a separate call of check_conditional, so we don't need to
            // recur to the lhs here.
            ast::expression::LogicalOperator::NullishCoalesce => condition_allowed(),
        },
        ast::expression::ExpressionInner::TypeCast { inner, .. } => {
            should_report_error_ref = false;
            check_conditional(should_report_error, cx, &inner.expression, cached_results)?
        }
        ast::expression::ExpressionInner::AsConstExpression { inner, .. } => {
            should_report_error_ref = false;
            check_conditional(should_report_error, cx, &inner.expression, cached_results)?
        }
        ast::expression::ExpressionInner::TSSatisfies { inner, .. } => {
            should_report_error_ref = false;
            check_conditional(should_report_error, cx, &inner.expression, cached_results)?
        }
        ast::expression::ExpressionInner::Member { .. } => condition_allowed(),
        ast::expression::ExpressionInner::OptionalMember { .. } => condition_allowed(),
        ast::expression::ExpressionInner::Object { .. } => condition_banned_and_truthy(),
        ast::expression::ExpressionInner::Array { .. } => condition_banned_and_truthy(),
        ast::expression::ExpressionInner::New { .. } => condition_banned_and_truthy(),
        ast::expression::ExpressionInner::Conditional { inner, .. } => {
            // we don't need to check or unnest `test` here because they are unnested and checked
            // in `expression` function
            let left_result = check_conditional(false, cx, &inner.consequent, cached_results)?;
            let right_result = check_conditional(false, cx, &inner.alternate, cached_results)?;
            match (&left_result, &right_result) {
                (
                    CheckConditionResult::ConditionBanned {
                        is_truthy: left_truthy,
                        show_warning: left_show_warning,
                        constant_condition_kind: left_kind,
                        ..
                    },
                    CheckConditionResult::ConditionBanned {
                        is_truthy: right_truthy,
                        show_warning: right_show_warning,
                        constant_condition_kind: right_kind,
                        ..
                    },
                ) if left_truthy == right_truthy => CheckConditionResult::ConditionBanned {
                    is_truthy: *left_truthy,
                    show_warning: *left_show_warning || *right_show_warning,
                    constant_condition_kind: if left_kind == right_kind {
                        *left_kind
                    } else {
                        ConstantConditionKind::ConstCondGeneral
                    },
                    reason: None,
                },
                _ => condition_allowed(),
            }
        }
        ast::expression::ExpressionInner::Sequence { inner, .. } => {
            let last = inner.expressions.last().unwrap();
            should_report_error_ref = false;
            check_conditional(should_report_error, cx, last, cached_results)?
        }
        ast::expression::ExpressionInner::Function { .. } => condition_banned_and_truthy(),
        ast::expression::ExpressionInner::ArrowFunction { .. } => condition_banned_and_truthy(),
        ast::expression::ExpressionInner::TaggedTemplate { .. } => condition_banned_and_truthy(),
        ast::expression::ExpressionInner::TemplateLiteral { .. } => condition_banned_and_truthy(),
        ast::expression::ExpressionInner::JSXElement { .. } => condition_banned_and_truthy(),
        ast::expression::ExpressionInner::JSXFragment { .. } => condition_banned_and_truthy(),
        ast::expression::ExpressionInner::Class { .. } => condition_banned_and_truthy(),
        ast::expression::ExpressionInner::Yield { .. } => condition_banned_and_truthy(),
        ast::expression::ExpressionInner::Import { .. } => condition_banned_and_truthy(),
        ast::expression::ExpressionInner::Unary { inner, .. } => match inner.operator {
            ast::expression::UnaryOperator::Not => {
                let arg_eval_result =
                    check_conditional(false, cx, &inner.argument, cached_results)?;
                match arg_eval_result {
                    CheckConditionResult::ConditionBanned {
                        is_truthy,
                        show_warning,
                        constant_condition_kind,
                        reason,
                    } => CheckConditionResult::ConditionBanned {
                        is_truthy: !is_truthy,
                        show_warning,
                        constant_condition_kind,
                        reason,
                    },
                    CheckConditionResult::ConditionAllowed => {
                        CheckConditionResult::ConditionAllowed
                    }
                }
            }
            ast::expression::UnaryOperator::Minus
            | ast::expression::UnaryOperator::Plus
            | ast::expression::UnaryOperator::BitNot
            | ast::expression::UnaryOperator::Typeof => {
                check_conditional(false, cx, &inner.argument, cached_results)?
            }
            ast::expression::UnaryOperator::Void => condition_banned_and_falsy(),
            ast::expression::UnaryOperator::Delete | ast::expression::UnaryOperator::Nonnull => {
                use_type_to_check_conditional(cx, ttype)?
            }
            ast::expression::UnaryOperator::Await => condition_allowed(),
        },
        ast::expression::ExpressionInner::Assignment { inner, .. } => {
            match inner.operator {
                // vanilla assignment (e.g. `a='test_str'`) is represented by `None`
                None => {
                    should_report_error_ref = false;
                    check_conditional(should_report_error, cx, &inner.right, cached_results)?
                }
                // we don't check compound assignment (e.g. `++`, `--`, `+=`, ...)
                // because their value could change
                Some(_) => condition_allowed(),
            }
        }
        ast::expression::ExpressionInner::AsExpression { .. }
        | ast::expression::ExpressionInner::MetaProperty { .. }
        | ast::expression::ExpressionInner::Call { .. }
        | ast::expression::ExpressionInner::OptionalCall { .. }
        | ast::expression::ExpressionInner::Match { .. }
        | ast::expression::ExpressionInner::Binary { .. }
        | ast::expression::ExpressionInner::Identifier { .. }
        | ast::expression::ExpressionInner::Record { .. }
        | ast::expression::ExpressionInner::This { .. } => {
            use_type_to_check_conditional(cx, ttype)?
        }
    };
    if should_report_error_ref {
        cached_results.insert(loc.dupe(), check_condition_result.dupe());
    }
    Ok(check_condition_result)
}

fn detect_constant_conditions<'cx>(cx: &mut Context<'cx>) -> Result<(), JobError> {
    let all_conditions = cx.get_all_conditions();
    let mut all_condition_results: ALocMap<CheckConditionResult> = ALocMap::new();
    for condition in all_conditions.iter() {
        check_conditional(true, cx, condition, &mut all_condition_results)?;
    }
    let mut banned_conditions: Vec<(ALoc, bool, bool, ConstantConditionKind, Option<Reason>)> =
        Vec::new();
    for (loc, check_condition_result) in all_condition_results.iter() {
        match check_condition_result {
            CheckConditionResult::ConditionAllowed => {}
            CheckConditionResult::ConditionBanned {
                is_truthy,
                show_warning,
                constant_condition_kind,
                reason,
            } => {
                banned_conditions.push((
                    loc.dupe(),
                    *is_truthy,
                    *show_warning,
                    *constant_condition_kind,
                    reason.dupe(),
                ));
            }
        }
    }
    for (loc, is_truthy, show_warning, constant_condition_kind, reason) in banned_conditions {
        let warning = if show_warning {
            ConstantConditionWarning::Likely {
                suggested_loc: reason
                    .as_ref()
                    .and_then(|reason| reason.annot_loc().or_else(|| reason.def_loc_opt()))
                    .duped(),
            }
        } else {
            ConstantConditionWarning::Definite
        };
        flow_js_utils::add_output_non_speculating(
            cx,
            ErrorMessage::EConstantCondition(Box::new(EConstantConditionData {
                loc,
                is_truthy,
                warning,
                constant_condition_kind,
            })),
        );
    }
    Ok(())
}

struct StrictComparisonResult {
    left: ErrorTypeReferenceWithLocData<ALoc>,
    right: ErrorTypeReferenceWithLocData<ALoc>,
    primary_loc: ALoc,
    info: StrictComparisonInfo<ALoc, ErrorTypeReferenceWithLocData<ALoc>>,
}

fn check_strict_comparison<'cx>(
    cx: &mut Context<'cx>,
    all_strict_comparisons: &[(
        ALoc,
        (
            ast::expression::Expression<ALoc, (ALoc, Type)>,
            ast::expression::Expression<ALoc, (ALoc, Type)>,
        ),
    )],
) -> Result<Vec<StrictComparisonResult>, JobError> {
    Ok(all_strict_comparisons
        .iter()
        .map(|(loc, (left_ast, right_ast))| {
            let (_, left_t) = left_ast.loc();
            let (_, right_t) = right_ast.loc();
            let l_reason = reason_of_t(left_t);
            let r_reason = reason_of_t(right_t);
            let Ok(left_conc_t) =
                FlowJs::singleton_concrete_type_for_inspection(cx, l_reason, left_t)
            else {
                return Ok(None);
            };
            let Ok(right_conc_t) =
                FlowJs::singleton_concrete_type_for_inspection(cx, r_reason, right_t)
            else {
                return Ok(None);
            };
            // the reason after concretization will contain more information.
            let l_singleton_reason = reason_of_t(&left_conc_t);
            let r_singleton_reason = reason_of_t(&right_conc_t);

            fn has_null_type<'cx>(cx: &mut Context<'cx>, t: &Type) -> bool {
                match type_filter::not_null(cx, t.dupe()) {
                    type_filter::FilterResult { changed: true, .. } => true,
                    _ => false,
                }
            }
            // not_maybe will return emptyT if null or undefined is passed in.
            // emptyT is a subtype of any type, so we don't need to check the AST node
            // of `left` or `right`.
            fn filter_maybe_and_check_is_subtyping<'cx>(
                cx: &mut Context<'cx>,
                left: &Type,
                right: &Type,
            ) -> Result<bool, JobError> {
                let left_filtered = type_filter::not_maybe(cx, left.dupe()).type_;
                let right_filtered = type_filter::not_maybe(cx, right.dupe()).type_;
                let left_expanded = drop_generic(left.dupe());
                let right_expanded = drop_generic(right.dupe());
                let succeeds_1 =
                    FlowJs::speculative_subtyping_succeeds(cx, &right_filtered, &left_expanded)?;
                if succeeds_1 {
                    return Ok(true);
                }
                FlowJs::speculative_subtyping_succeeds(cx, &left_filtered, &right_expanded)
            }
            let banned = |info| StrictComparisonResult {
                left: flow_js_utils::type_reference_with_reason_for_error(left_t, l_reason.dupe()),
                right: flow_js_utils::type_reference_with_reason_for_error(
                    right_t,
                    r_reason.dupe(),
                ),
                primary_loc: loc.dupe(),
                info,
            };
            Ok(match (left_conc_t.deref(), right_conc_t.deref()) {
                (
                    TypeInner::AnyT(_, _)
                    | TypeInner::ImplicitInstantiationTvar(box ImplicitInstantiationTvarData {
                        kind: ImplicitInstantiationTvarKind::Placeholder,
                        ..
                    }),
                    _,
                )
                | (
                    _,
                    TypeInner::AnyT(_, _)
                    | TypeInner::ImplicitInstantiationTvar(box ImplicitInstantiationTvarData {
                        kind: ImplicitInstantiationTvarKind::Placeholder,
                        ..
                    }),
                ) => None,
                (TypeInner::DefT(_, d), _) if matches!(d.deref(), DefTInner::VoidT) => None,
                (_, TypeInner::DefT(_, d)) if matches!(d.deref(), DefTInner::VoidT) => None,
                (TypeInner::DefT(_, d1), TypeInner::DefT(_, d2))
                    if matches!(d1.deref(), DefTInner::EmptyT)
                        && matches!(d2.deref(), DefTInner::EmptyT) =>
                {
                    None
                }
                (TypeInner::DefT(_, d), _) if matches!(d.deref(), DefTInner::EmptyT) => {
                    Some(banned(StrictComparisonInfo::Empty {
                        empty: flow_js_utils::type_reference_with_reason_for_error(
                            &left_conc_t,
                            l_singleton_reason.dupe(),
                        ),
                    }))
                }
                (_, TypeInner::DefT(_, d)) if matches!(d.deref(), DefTInner::EmptyT) => {
                    Some(banned(StrictComparisonInfo::Empty {
                        empty: flow_js_utils::type_reference_with_reason_for_error(
                            &right_conc_t,
                            r_singleton_reason.dupe(),
                        ),
                    }))
                }
                (_, TypeInner::DefT(_, d))
                    if matches!(d.deref(), DefTInner::NullT)
                        && !has_null_type(cx, &left_conc_t) =>
                {
                    Some(banned(StrictComparisonInfo::Null {
                        null_loc: r_singleton_reason.loc().dupe(),
                        other: flow_js_utils::type_reference_with_reason_for_error(
                            &left_conc_t,
                            l_singleton_reason.dupe(),
                        ),
                    }))
                }
                (TypeInner::DefT(_, d), _)
                    if matches!(d.deref(), DefTInner::NullT)
                        && !has_null_type(cx, &right_conc_t) =>
                {
                    Some(banned(StrictComparisonInfo::Null {
                        null_loc: l_singleton_reason.loc().dupe(),
                        other: flow_js_utils::type_reference_with_reason_for_error(
                            &right_conc_t,
                            r_singleton_reason.dupe(),
                        ),
                    }))
                }
                _ => {
                    if filter_maybe_and_check_is_subtyping(cx, &left_conc_t, &right_conc_t)? {
                        None
                    } else {
                        Some(banned(StrictComparisonInfo::General {
                            left: flow_js_utils::type_reference_with_reason_for_error(
                                &left_conc_t,
                                l_singleton_reason.dupe(),
                            ),
                            right: flow_js_utils::type_reference_with_reason_for_error(
                                &right_conc_t,
                                r_singleton_reason.dupe(),
                            ),
                        }))
                    }
                }
            })
        })
        .collect::<Result<Vec<_>, JobError>>()?
        .into_iter()
        .flatten()
        .collect())
}

fn detect_invalid_strict_comparison<'cx>(cx: &mut Context<'cx>) -> Result<(), JobError> {
    let all_strict_comparisons = cx.get_all_strict_comparisons();
    let all_strict_comparisons: Vec<_> = all_strict_comparisons.iter().cloned().collect();
    for result in check_strict_comparison(cx, &all_strict_comparisons)? {
        flow_js::add_output_non_speculating(
            cx,
            ErrorMessage::EComparison(Box::new(EComparisonData {
                r1: result.left,
                r2: result.right,
                loc_opt: Some(result.primary_loc),
                strict_comparison_opt: Some(result.info),
            })),
        );
    }
    Ok(())
}

fn detect_unnecessary_optional_chains<'cx>(cx: &mut Context<'cx>) {
    for (loc, lhs_expression) in cx.unnecessary_optional_chains().iter() {
        flow_js::add_output_non_speculating(
            cx,
            ErrorMessage::EUnnecessaryOptionalChain(Box::new((loc.dupe(), lhs_expression.dupe()))),
        );
    }
}

fn detect_unused_promises<'cx>(cx: &mut Context<'cx>) -> Result<(), JobError> {
    for (loc, t, async_) in cx.maybe_unused_promises().iter() {
        let t = tvar_resolver::resolved_t(
            |r| any_t::make(AnySource::Untyped, r.dupe()),
            false,
            cx,
            t.dupe(),
        );
        let reason = flow_common::reason::mk_reason(None, loc.dupe());
        let concrete_types = flow_js_utils::flow_js_result_to_job_error(
            FlowJs::possible_concrete_types_for_unused_promise_check(cx, &reason, &t),
        )?;
        if concrete_types.iter().any(|t| {
            if let TypeInner::DefT(_, def_t) = t.deref()
                && let DefTInner::InstanceT(inst_t) = def_t.deref()
            {
                flow_js_utils::is_builtin_class_id("Promise", inst_t.inst.class_id.dupe(), cx)
            } else {
                false
            }
        }) {
            flow_js::add_output_non_speculating(
                cx,
                ErrorMessage::EUnusedPromise {
                    loc: loc.dupe(),
                    async_: *async_,
                },
            );
        }
    }
    Ok(())
}

fn enforce_optimize<'cx>(cx: &mut Context<'cx>, loc: &ALoc, t: &Type) -> Result<(), JobError> {
    let reason = flow_common::reason::mk_reason(
        VirtualReasonDesc::RType(FlowSmolStr::new_inline("$Flow$EnforceOptimized")),
        loc.dupe(),
    );
    let internal_t = Type::new(TypeInner::NominalT {
        reason: reason.dupe(),
        nominal_type: Rc::new(NominalType::new(NominalTypeInner {
            nominal_id: nominal::Id::InternalEnforceUnionOptimized,
            underlying_t: nominal::UnderlyingT::FullyOpaque,
            lower_t: None,
            upper_t: None,
            nominal_type_args: Rc::from(Vec::<(
                flow_common::subst_name::SubstName,
                Reason,
                Type,
                Option<Polarity>,
            )>::new()),
        })),
    });
    flow_js::flow_t_non_speculating(cx, (&internal_t, t))
}

fn check_union_opt<'cx>(cx: &mut Context<'cx>) -> Result<(), JobError> {
    let mut result = Ok(());
    cx.iter_union_opt(|cx, loc, t| {
        if result.is_ok() {
            result = enforce_optimize(cx, loc, t);
        }
    });
    result
}

fn detect_import_export_errors<'cx>(
    cx: &mut Context<'cx>,
    file_sig: &FileSig,
    program: &ast::Program<ALoc, ALoc>,
    metadata: &Metadata,
) {
    strict_es6_import_export::detect_errors(cx, file_sig, program, metadata);
    for msg in module_exports_checker::check_program(file_sig, program) {
        flow_js_utils::add_output_non_speculating(cx, msg);
    }
    #[cfg(fbcode_build)]
    if cx.package_boundary() && !cx.is_global_lib_context() {
        detect_package_boundary_imports(cx, file_sig);
    }
    // The UMD global is built from the builtins copy of this module, which cannot resolve
    // imports, so it is not declared.
    if !cx.is_global_lib_context()
        && has_dts_ext(cx.file().as_str())
        && !file_sig.requires().is_empty()
    {
        for stmt in program.statements.iter() {
            if let ast::statement::StatementInner::NamespaceExportDeclaration { loc, .. } = &**stmt
            {
                flow_js_utils::add_output_non_speculating(
                    cx,
                    ErrorMessage::EUnsupportedSyntax(Box::new((
                        loc.dupe(),
                        UnsupportedSyntax::NamespaceExportDeclaration(
                            NamespaceExportDeclarationKind::ModuleWithImports,
                        ),
                    ))),
                );
            }
        }
    }
}

#[cfg(fbcode_build)]
fn detect_package_boundary_imports<'cx>(cx: &mut Context<'cx>, file_sig: &FileSig) {
    for (specifier, locs) in file_sig.require_loc_map() {
        let provider = match cx.find_require(&specifier) {
            ResolvedRequire::TypedModule(f) => match f(cx) {
                Ok(m) => m.module_reason.def_loc().source().map(|file| file.dupe()),
                Err(_) => None,
            },
            ResolvedRequire::UncheckedModule(loc) => loc.source().map(|file| file.dupe()),
            ResolvedRequire::MissingModule | ResolvedRequire::GlobalLibdefModule(_) => None,
        };
        let Some(provider) = provider else {
            continue;
        };
        let Some(boundary) =
            flow_facebook_package_boundary::crossed_boundary(cx.file().as_str(), provider.as_str())
        else {
            continue;
        };
        for loc in locs {
            flow_js_utils::add_output_non_speculating(
                cx,
                ErrorMessage::EPackageBoundaryImport(Box::new((
                    ALoc::of_loc(loc),
                    FlowSmolStr::new(boundary),
                ))),
            );
        }
    }
}

fn detect_non_voidable_properties<'cx>(cx: &mut Context<'cx>) {
    // This function approximately checks whether VoidT can flow to the provided
    // type without actually creating the flow so as not to disturb type inference.
    // Even though this is happening post-merge, it is possible to encounter an
    // unresolved tvar, in which case it conservatively returns false.
    fn is_voidable<'cx>(cx: &mut Context<'cx>, seen_ids: &mut TvarSeenSet<i32>, t: &Type) -> bool {
        match t.deref() {
            TypeInner::OpenT(tvar) => {
                let id = tvar.id() as i32;
                // tvar is recursive: conservatively assume it is non-voidable
                if seen_ids.contains(&id) {
                    false
                } else {
                    match flow_js_utils::possible_types(cx, id).as_slice() {
                        // tvar has no lower bounds: we conservatively assume it's non-voidable
                        // except in the special case when it also has no upper bounds
                        [] => flow_js_utils::possible_uses(cx, id).is_empty(),
                        // tvar is resolved: look at voidability of the resolved type
                        [t] => seen_ids.with_added(id, |seen_ids| is_voidable(cx, seen_ids, t)),
                        // tvar is unresolved: conservatively assume it is non-voidable
                        _ => false,
                    }
                }
            }
            // a union is voidable if any of its members are voidable
            TypeInner::UnionT(_, rep) => rep.members_iter().any(|t| is_voidable(cx, seen_ids, t)),
            // an intersection is voidable if all of its members are voidable
            TypeInner::IntersectionT(_, rep) => {
                rep.members_iter().all(|t| is_voidable(cx, seen_ids, t))
            }
            // trivially voidable
            TypeInner::MaybeT(_, _)
            | TypeInner::OptionalT { .. }
            | TypeInner::AnyT(_, _)
            | TypeInner::ImplicitInstantiationTvar(box ImplicitInstantiationTvarData {
                kind: ImplicitInstantiationTvarKind::Placeholder,
                ..
            }) => true,
            TypeInner::DefT(_, def_t) => matches!(
                def_t.deref(),
                DefTInner::VoidT
                    | DefTInner::MixedT(MixedFlavor::MixedEverything | MixedFlavor::MixedNonNull)
            ),
            // conservatively assume all other types are non-voidable
            _ => false,
        }
    }

    fn check_properties<'cx>(
        cx: &mut Context<'cx>,
        property_map_id: properties::Id,
        errors: &BTreeMap<FlowSmolStr, Vec<property_assignment::Error<ALoc>>>,
    ) {
        let pmap = cx.find_props(property_map_id);
        for (name, errs) in errors.iter() {
            let should_error = match pmap.get(&Name::new(name.dupe())) {
                Some(prop) => match &**prop {
                    PropertyInner::Field(fd) => {
                        !is_voidable(cx, &mut TvarSeenSet::new(), &fd.type_)
                    }
                    _ => true,
                },
                _ => true,
            };
            if should_error {
                for err in errs {
                    flow_js::add_output_non_speculating(
                        cx,
                        ErrorMessage::EUninitializedInstanceProperty(err.loc.dupe(), err.desc),
                    );
                }
            }
        }
    }

    for check in cx.voidable_checks().clone().iter() {
        check_properties(
            cx,
            check.public_property_map.dupe(),
            &check.errors.public_property_errors,
        );
        check_properties(
            cx,
            check.private_property_map.dupe(),
            &check.errors.private_property_errors,
        );
    }
}

fn check_polarity_fn<'cx>(cx: &mut Context<'cx>) -> Result<(), JobError> {
    for (tparams, polarity, t) in cx.post_inference_polarity_checks().clone().iter() {
        flow_js_utils::flow_js_result_to_job_error(check_polarity::check_polarity(
            cx, tparams, *polarity, t,
        ))?;
    }
    Ok(())
}

fn check_general_post_inference_validations<'cx>(cx: &mut Context<'cx>) -> Result<(), JobError> {
    for (t, use_t) in cx.take_post_inference_validation_flows().iter() {
        flow_js::flow_non_speculating(cx, (t, use_t))?
    }
    for f in cx.take_post_inference_validation_callbacks().iter() {
        f(cx)?
    }
    Ok(())
}

fn check_interface_merge_conflicts<'cx>(cx: &mut Context<'cx>) -> Result<(), JobError> {
    for (use_op, tparam_subst_map, bad_t, good_t) in cx.interface_merge_unify_tasks().iter() {
        let bad_t = type_subst::subst(
            cx,
            Some(use_op.dupe()),
            true,
            false,
            type_subst::Purpose::Normal,
            tparam_subst_map,
            bad_t.dupe(),
        );
        flow_js::unify_non_speculating(cx, Some(use_op.dupe()), &bad_t, good_t)?;
    }
    Ok(())
}

fn check_react_rules_fn<'cx>(
    cx: &mut Context<'cx>,
    tast: &ast::Program<ALoc, (ALoc, Type)>,
) -> Result<(), JobError> {
    react_rules::check_react_rules(cx, tast)
}

fn check_multiplatform_conformance<'cx>(
    cx: &mut Context<'cx>,
    ast: &ast::Program<ALoc, ALoc>,
    tast: &ast::Program<ALoc, (ALoc, Type)>,
) -> Result<(), JobError> {
    let prog_aloc = &ast.loc;
    let filename = cx.file().dupe();
    let file_options = cx.file_options();
    let file_loc = ALoc::of_loc(Loc {
        source: Some(filename.dupe()),
        start: flow_parser::loc::Position { line: 0, column: 0 },
        end: flow_parser::loc::Position { line: 0, column: 0 },
    });
    match files::relative_interface_mref_of_possibly_platform_specific_file(
        &file_options,
        &filename,
    ) {
        Some(imported_interface_module_name) => {
            let specifier =
                FlowImportSpecifier::userland(FlowSmolStr::new(&imported_interface_module_name));
            match cx.find_require(&specifier) {
                // It's ok if a platform speicific implementation file doesn't have an interface.
                // It just makes the module non-importable without platform extension.
                ResolvedRequire::MissingModule
                | ResolvedRequire::GlobalLibdefModule(_)
                | ResolvedRequire::UncheckedModule(_) => {}
                ResolvedRequire::TypedModule(interface_module_f) => {
                    let get_exports_t = |cx: &mut Context<'cx>,
                                         is_common_interface_module: bool,
                                         reason: Reason,
                                         m_opt: Option<&ModuleType>|
                     -> Type {
                        match m_opt {
                            Some(m) => {
                                if is_common_interface_module {
                                    if let (Some(avail), Some(mod_avail)) =
                                        (cx.available_platforms(), &m.module_available_platforms)
                                    {
                                        if PlatformSet::no_overlap(avail, mod_avail) {
                                            // If the current module's platform has no overlap with the common interface
                                            // file's platforms, then the common interface file is irrelevant. We give it an
                                            // any type to make conformance check always passing.
                                            return any_t::make(AnySource::Untyped, reason);
                                        }
                                    }
                                }
                                // For conformance test, we only care about the value part
                                let strictness_kind = cx.type_strictness_kind();
                                let (values_t, _) =
                                    flow_js_utils::import_module_ns_t_kit::on_module_t(
                                        cx,
                                        is_common_interface_module,
                                        reason.dupe(),
                                        false,
                                        strictness_kind,
                                        m,
                                    )
                                    .unwrap();
                                values_t
                            }
                            None => any_t::make(AnySource::Untyped, reason),
                        }
                    };
                    let interface_t = {
                        let reason = flow_common::reason::mk_reason(None, prog_aloc.dupe());
                        let m_result = interface_module_f(cx);
                        get_exports_t(cx, true, reason, m_result.ok().as_ref())
                    };
                    let (self_sig_loc, self_module_type) =
                        module_info_analyzer::analyze_program(cx, tast)?;
                    let self_t = get_exports_t(
                        cx,
                        false,
                        self_module_type.module_reason.dupe(),
                        Some(&self_module_type),
                    );
                    // We need to fully resolve the type to prevent tvar widening.
                    tvar_resolver::resolve(
                        cx,
                        tvar_resolver::default_no_lowers,
                        true,
                        &interface_t,
                    );
                    tvar_resolver::resolve(cx, tvar_resolver::default_no_lowers, true, &self_t);
                    let use_op = UseOp::Op(Arc::new(RootUseOp::ConformToCommonInterface(
                        Box::new(ConformToCommonInterfaceData {
                            self_sig_loc,
                            self_module_loc: prog_aloc.dupe(),
                            originate_from_import: false,
                        }),
                    )));
                    flow_js::flow_non_speculating(
                        cx,
                        (&self_t, &UseT::new(UseTInner::UseT(use_op, interface_t))),
                    )?
                }
            }
        }
        None => {
            match platform_set::platform_specific_implementation_mrefs_of_possibly_interface_file(
                &file_options,
                cx.available_platforms(),
                &filename,
            ) {
                None => {}
                Some((
                    unconditional_extensions,
                    grouped_extensions_with_conditional_extensions,
                )) => {
                    let module_exists = |cx: &mut Context<'cx>, mref: &str| -> bool {
                        match cx
                            .find_require(&FlowImportSpecifier::userland(FlowSmolStr::new(mref)))
                        {
                            ResolvedRequire::TypedModule(_)
                            | ResolvedRequire::UncheckedModule(_) => true,
                            ResolvedRequire::MissingModule
                            | ResolvedRequire::GlobalLibdefModule(_) => false,
                        }
                    };
                    if !cx.has_explicit_supports_platform()
                        && unconditional_extensions
                            .iter()
                            .all(|m| !module_exists(cx, m))
                        && grouped_extensions_with_conditional_extensions.iter().all(
                            |(grouped, conditional)| {
                                !module_exists(cx, grouped)
                                    && conditional.iter().all(|m| !module_exists(cx, m))
                            },
                        )
                    {
                        // We are fine if no implementation file exist.
                        // The .js.flow file might be declaring a builtin module.
                    } else {
                        for name in &unconditional_extensions {
                            if !module_exists(cx, name) {
                                flow_js_utils::add_output_non_speculating(
                                    cx,
                                    ErrorMessage::EPlatformSpecificImplementationModuleLookupFailed(
                                        Box::new(
                                            EPlatformSpecificImplementationModuleLookupFailedData {
                                                loc: file_loc.dupe(),
                                                name: FlowSmolStr::new(name),
                                            },
                                        ),
                                    ),
                                );
                            }
                        }
                        for (grouped, conditional) in
                            &grouped_extensions_with_conditional_extensions
                        {
                            if !module_exists(cx, grouped) {
                                for name in conditional {
                                    if !module_exists(cx, name) {
                                        flow_js_utils::add_output_non_speculating(
                                            cx,
                                            ErrorMessage::EPlatformSpecificImplementationModuleLookupFailed(Box::new(EPlatformSpecificImplementationModuleLookupFailedData {
                                                loc: file_loc.dupe(), name:  FlowSmolStr::new(name),
                                            }))
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

fn check_spread_prop_keys<'cx>(cx: &mut Context<'cx>, tast: &ast::Program<ALoc, (ALoc, Type)>) {
    if !cx.ban_spread_key_props() {
        return;
    }
    fn find_key_prop<'cx>(
        cx: &mut Context<'cx>,
        seen: &mut TvarSeenSet<i32>,
        spread: &ALoc,
        t: &Type,
    ) {
        match t.deref() {
            TypeInner::OpenT(tvar) if seen.contains(&(tvar.id() as i32)) => {}
            TypeInner::OpenT(tvar) => {
                let id = tvar.id() as i32;
                seen.with_added(id, |seen| {
                    for t in flow_js_utils::possible_types(cx, id) {
                        find_key_prop(cx, seen, spread, &t);
                    }
                });
            }
            TypeInner::AnnotT(_, t, _) => find_key_prop(cx, seen, spread, t),
            TypeInner::UnionT(_, rep) => {
                for t in rep.members_iter() {
                    find_key_prop(cx, seen, spread, t);
                }
            }
            TypeInner::IntersectionT(_, rep) => {
                for t in rep.members_iter() {
                    find_key_prop(cx, seen, spread, t);
                }
            }
            TypeInner::OptionalT { type_, .. } => find_key_prop(cx, seen, spread, type_),
            TypeInner::MaybeT(_, ty) => find_key_prop(cx, seen, spread, ty),
            TypeInner::DefT(r, def_t) => {
                if let DefTInner::ObjT(obj_t) = def_t.deref() {
                    let prop = cx.get_prop(
                        obj_t.props_tmap.dupe(),
                        &Name::new(FlowSmolStr::new_inline("key")),
                    );
                    if let Some(prop) = prop {
                        let loc = property::first_loc(&prop).unwrap_or_else(|| r.loc().dupe());
                        flow_js::add_output_non_speculating(
                            cx,
                            ErrorMessage::EKeySpreadProp(Box::new(EKeySpreadPropData {
                                spread: spread.dupe(),
                                loc,
                            })),
                        );
                    }
                }
            }
            _ => {}
        }
    }
    struct SpreadPropKeyChecker<'a, 'cx> {
        cx: &'a mut Context<'cx>,
    }
    impl<'ast> ast_visitor::AstVisitor<'ast, ALoc, (ALoc, Type), &'ast ALoc, !>
        for SpreadPropKeyChecker<'_, '_>
    {
        fn normalize_loc(loc: &'ast ALoc) -> &'ast ALoc {
            loc
        }
        fn normalize_type(type_: &'ast (ALoc, Type)) -> &'ast ALoc {
            &type_.0
        }
        fn jsx_spread_attribute(
            &mut self,
            attr: &'ast ast::jsx::SpreadAttribute<ALoc, (ALoc, Type)>,
        ) -> Result<(), !> {
            let (spread, ty) = attr.argument.loc();
            find_key_prop(self.cx, &mut TvarSeenSet::new(), spread, ty);
            ast_visitor::jsx_spread_attribute_default(self, attr)
        }
    }
    let mut checker = SpreadPropKeyChecker { cx };
    let Ok(()) = checker.program(tast);
}

fn check_match_exhaustiveness<'cx>(
    cx: &mut Context<'cx>,
    tast: &ast::Program<ALoc, (ALoc, Type)>,
) -> Result<(), JobError> {
    if !cx.enable_pattern_matching() {
        return Ok(());
    }
    struct MatchExhaustivenessChecker<'a, 'cx> {
        cx: &'a mut Context<'cx>,
    }
    impl<'ast> ast_visitor::AstVisitor<'ast, ALoc, (ALoc, Type), &'ast ALoc, JobError>
        for MatchExhaustivenessChecker<'_, '_>
    {
        fn normalize_loc(loc: &'ast ALoc) -> &'ast ALoc {
            loc
        }
        fn normalize_type(type_: &'ast (ALoc, Type)) -> &'ast ALoc {
            &type_.0
        }
        fn match_<B>(
            &mut self,
            loc: &'ast ALoc,
            m: &'ast ast::match_::Match<ALoc, (ALoc, Type), B>,
            on_case_body: impl FnMut(&mut Self, &'ast B) -> Result<(), JobError>,
        ) -> Result<(), JobError> {
            let match_loc = &m.match_keyword_loc;
            let (_, arg_t) = m.arg.loc();
            let patterns: Vec<_> = m
                .cases
                .iter()
                .map(|case| (case.pattern.clone(), case.guard.is_some()))
                .collect();
            exhaustive::analyze(
                self.cx,
                &|cx, loc| flow_typing_utils::type_env::is_global_var(cx, loc),
                match_loc.dupe(),
                &patterns,
                arg_t,
            )?;
            ast_visitor::match_default(self, loc, m, on_case_body)
        }
    }
    let mut checker = MatchExhaustivenessChecker { cx };
    checker.program(tast)
}

fn emit_refinement_information_as_errors<'cx>(cx: &mut Context<'cx>) {
    fn emit_refined_locations_info<'cx>(cx: &mut Context<'cx>) {
        for (refined_loc, refining_locs) in cx.refined_locations().clone().iter() {
            flow_js_utils::add_output_non_speculating(
                cx,
                ErrorMessage::EDevOnlyRefinedLocInfo(Box::new(EDevOnlyRefinedLocInfoData {
                    refined_loc: refined_loc.dupe(),
                    refining_locs: refining_locs.iter().map(|l| l.dupe()).collect(),
                })),
            );
        }
    }
    fn emit_invalidated_locations_info<'cx>(cx: &mut Context<'cx>) {
        for (read_loc, invalidation_info) in cx.aggressively_invalidated_locations().clone().iter()
        {
            flow_js_utils::add_output_non_speculating(
                cx,
                ErrorMessage::EDevOnlyInvalidatedRefinementInfo(Box::new(
                    EDevOnlyInvalidatedRefinementInfoData {
                        read_loc: read_loc.dupe(),
                        invalidation_info: invalidation_info
                            .iter()
                            .map(|(k, v)| (k.dupe(), *v))
                            .collect(),
                    },
                )),
            );
        }
    }
    if cx.dev_only_refinement_info_as_errors() {
        emit_refined_locations_info(cx);
        emit_invalidated_locations_info(cx);
    }
}

fn check_assert_operator<'cx>(
    cx: &mut Context<'cx>,
    tast: &ast::Program<ALoc, (ALoc, Type)>,
) -> Result<(), JobError> {
    if !cx.assert_operator_enabled() || cx.is_ts_file() {
        return Ok(());
    }
    fn check_specialized_assert_operator<'cx>(
        cx: &mut Context<'cx>,
        op_reason: &Reason,
        expr: &ast::expression::Expression<ALoc, (ALoc, Type)>,
    ) {
        let obj_expression = ExpressionReferenceData {
            loc: expr.loc().0.dupe(),
            kind: flow_js_utils::expression_reference_kind_for_error(expr),
        };
        match expr.deref() {
            ast::expression::ExpressionInner::Member { loc: _, inner }
                if let ast::expression::member::Property::PropertyIdentifier(id) =
                    &inner.property =>
            {
                let (_, obj_t) = inner.object.loc();
                let name = &id.name;
                type_assertions::check_specialized_assert_operator_property(
                    cx,
                    op_reason,
                    &obj_expression,
                    obj_t,
                    name,
                );
            }
            ast::expression::ExpressionInner::OptionalMember { loc: _, inner }
                if let ast::expression::member::Property::PropertyIdentifier(id) =
                    &inner.member.property =>
            {
                let (_, obj_t) = inner.member.object.loc();
                let name = &id.name;
                type_assertions::check_specialized_assert_operator_property(
                    cx,
                    op_reason,
                    &obj_expression,
                    obj_t,
                    name,
                );
            }
            ast::expression::ExpressionInner::Member { loc: _, inner }
                if let ast::expression::member::Property::PropertyExpression(prop_expr) =
                    &inner.property =>
            {
                let (_, obj_t) = inner.object.loc();
                let (_, prop_t) = prop_expr.loc();
                type_assertions::check_specialized_assert_operator_lookup(
                    cx,
                    op_reason,
                    &obj_expression,
                    obj_t,
                    prop_t,
                );
            }
            ast::expression::ExpressionInner::OptionalMember { loc: _, inner }
                if let ast::expression::member::Property::PropertyExpression(prop_expr) =
                    &inner.member.property =>
            {
                let (_, obj_t) = inner.member.object.loc();

                let (_, prop_t) = prop_expr.loc();
                type_assertions::check_specialized_assert_operator_lookup(
                    cx,
                    op_reason,
                    &obj_expression,
                    obj_t,
                    prop_t,
                );
            }
            _ => {
                flow_js_utils::add_output_non_speculating(
                    cx,
                    ErrorMessage::EIllegalAssertOperator(Box::new(EIllegalAssertOperatorData {
                        op_loc: op_reason.loc().dupe(),
                        obj: obj_expression,
                        specialized: true,
                    })),
                );
            }
        }
    }
    fn check_assert_operator_useful<'cx>(
        cx: &mut Context<'cx>,
        op_reason: &Reason,
        expr: &ast::expression::Expression<ALoc, (ALoc, Type)>,
    ) -> Result<(), JobError> {
        let obj_expression = ExpressionReferenceData {
            loc: expr.loc().0.dupe(),
            kind: flow_js_utils::expression_reference_kind_for_error(expr),
        };
        let (_, t) = expr.loc();
        let legal = type_assertions::check_assert_operator_nullable(cx, t)?
            || match expr.deref() {
                ast::expression::ExpressionInner::Member { loc: _, inner } => {
                    let (_, obj_t) = inner.object.loc();
                    type_assertions::check_assert_operator_implicitly_nullable(cx, obj_t)?
                }
                ast::expression::ExpressionInner::OptionalMember { loc: _, inner } => {
                    let (_, obj_t) = inner.member.object.loc();
                    type_assertions::check_assert_operator_implicitly_nullable(cx, obj_t)?
                }
                _ => false,
            };
        if !legal {
            flow_js_utils::add_output_non_speculating(
                cx,
                ErrorMessage::EIllegalAssertOperator(Box::new(EIllegalAssertOperatorData {
                    op_loc: op_reason.loc().dupe(),
                    obj: obj_expression,
                    specialized: false,
                })),
            );
        }
        Ok(())
    }
    fn check<'cx>(
        cx: &mut Context<'cx>,
        op_reason: &Reason,
        expr: &ast::expression::Expression<ALoc, (ALoc, Type)>,
    ) -> Result<(), JobError> {
        check_assert_operator_useful(cx, op_reason, expr)?;
        if cx.assert_operator_specialized() {
            check_specialized_assert_operator(cx, op_reason, expr);
        }
        Ok(())
    }

    struct AssertOperatorChecker<'a, 'cx> {
        cx: &'a mut Context<'cx>,
    }
    impl<'ast> ast_visitor::AstVisitor<'ast, ALoc, (ALoc, Type), &'ast ALoc, JobError>
        for AssertOperatorChecker<'_, '_>
    {
        fn normalize_loc(loc: &'ast ALoc) -> &'ast ALoc {
            loc
        }
        fn normalize_type(type_: &'ast (ALoc, Type)) -> &'ast ALoc {
            &type_.0
        }
        fn expression(
            &mut self,
            expr: &'ast ast::expression::Expression<ALoc, (ALoc, Type)>,
        ) -> Result<(), JobError> {
            match expr.deref() {
                ast::expression::ExpressionInner::Unary { loc: _, inner }
                    if inner.operator == ast::expression::UnaryOperator::Nonnull =>
                {
                    let op_reason = reason::mk_typed_expression_reason(expr);
                    check(self.cx, &op_reason, &inner.argument)?;
                }
                ast::expression::ExpressionInner::OptionalCall { loc: _, inner }
                    if let ast::expression::OptionalCallKind::AssertNonnull { assertion_loc } =
                        &inner.optional =>
                {
                    let target = &inner.call.callee;
                    let op_reason =
                        reason::mk_typed_expression_reason(target).reposition(assertion_loc.dupe());
                    check(self.cx, &op_reason, target)?;
                }
                ast::expression::ExpressionInner::OptionalMember { loc: _, inner }
                    if let ast::expression::OptionalMemberKind::AssertNonnull { assertion_loc } =
                        &inner.optional =>
                {
                    let target = &inner.member.object;
                    let op_reason =
                        reason::mk_typed_expression_reason(target).reposition(assertion_loc.dupe());
                    check(self.cx, &op_reason, target)?;
                }
                _ => {}
            }
            ast_visitor::expression_default(self, expr)
        }
    }

    let mut checker = AssertOperatorChecker { cx };
    checker.program(tast)?;
    Ok(())
}

fn convert_type_to_type_desc_in_errors<'cx>(
    cx: &mut Context<'cx>,
    file_sig: Arc<FileSig>,
    typed_ast: &ast::Program<ALoc, (ALoc, Type)>,
) {
    const ERROR_NORMALIZATION_MAX_SIZE: usize = 250;

    let errors = cx.errors();
    if errors.is_empty() {
        return;
    }
    let genv = crate::ty_normalizer_flow::mk_genv(
        flow_typing_ty_normalizer::env::Options {
            optimize_types: false,
            max_type_size: Some(ERROR_NORMALIZATION_MAX_SIZE),
            ..flow_typing_ty_normalizer::env::Options::default()
        },
        Some(typed_ast),
        file_sig,
    );
    type TypeOrTypeDesc = type_or_type_desc::TypeOrTypeDescT<ALoc>;
    // Errors from the same failed operation (e.g. every branch of a failed union speculation)
    // reference the same types, so each distinct type is normalized once.
    let mut descs: BTreeMap<Type, type_or_type_desc::ErrorTypeDesc<ALoc>> = BTreeMap::new();
    let mut convert = |tod: TypeOrTypeDesc| -> TypeOrTypeDesc {
        match tod {
            TypeOrTypeDesc::Type(t) => {
                let desc = match descs.get(&t) {
                    Some(desc) => desc.clone(),
                    None => {
                        let desc = flow_typing_ty_normalizer::no_flow::type_to_desc_for_errors(
                            cx, &genv, &t,
                        );
                        descs.insert(t, desc.clone());
                        desc
                    }
                };
                TypeOrTypeDesc::TypeDesc(desc)
            }
            TypeOrTypeDesc::TypeDesc(d) => TypeOrTypeDesc::TypeDesc(d),
        }
    };
    let converted = errors.map(|err| FlowError::convert_type_to_type_desc(&mut convert, err));
    cx.reset_errors(converted);
}

pub fn get_lint_severities(
    metadata: &Metadata,
    strict_mode: &StrictModeSettings,
    lint_severities: LintSettings<Severity>,
) -> LintSettings<Severity> {
    if metadata.overridable.strict || metadata.overridable.strict_local {
        strict_mode.fold(lint_severities, |lint_kind, mut settings| {
            settings.set_value(lint_kind, (Severity::Err, None));
            settings
        })
    } else {
        lint_severities
    }
}

// Post-merge errors.
//
// At this point, all dependencies have been merged and the component has been
// linked together. Any constraints should have already been evaluated, which
// means we can complain about things that either haven't happened yet, or
// which require complete knowledge of tvar bounds.

pub fn post_merge_checks<'cx>(
    cx: &mut Context<'cx>,
    file_sig: Arc<FileSig>,
    ast: &ast::Program<ALoc, ALoc>,
    tast: &ast::Program<ALoc, (ALoc, Type)>,
    metadata: &Metadata,
) -> Result<(), JobError> {
    force_lazy_tvars(cx)?;
    check_react_rules_fn(cx, tast)?;
    if !cx.is_global_lib_context() {
        check_multiplatform_conformance(cx, ast, tast)?;
    }
    check_polarity_fn(cx)?;
    check_general_post_inference_validations(cx)?;
    check_interface_merge_conflicts(cx)?;
    detect_sketchy_null_checks(cx, tast);
    detect_non_voidable_properties(cx);
    detect_test_prop_misses(cx);
    detect_unnecessary_optional_chains(cx);
    detect_constant_conditions(cx)?;
    detect_import_export_errors(cx, &file_sig, ast, metadata);
    detect_invalid_strict_comparison(cx)?;
    detect_unused_promises(cx)?;
    check_union_opt(cx)?;
    check_spread_prop_keys(cx, tast);
    check_match_exhaustiveness(cx, tast)?;
    check_assert_operator(cx, tast)?;
    emit_refinement_information_as_errors(cx);
    convert_type_to_type_desc_in_errors(cx, file_sig, tast);
    Ok(())
}

// Check will lazily create types for the checked file's dependencies. These
// types are created in the dependency's context and need to be copied into the
// checked file's context.
//
// This visitor walks a type in the dependency's context (src_cx) and copies
// any tvars, property maps, evaluated types, etc. into the check file's context
// (dst_cx).
//
// When calculating a direct dependency's types, we might also need to construct
// types for a transitive dependency. These types are similarly created in the
// transitive dependency's context, then copied into the dependency's context,
// and so on.
//
// Finally, due to cycles, it's possile that src_cx and dst_cx share the same
// component cx, and thus have the same tvar graph, property maps, etc. Happily,
// this does not complicate the implementation, as the mem checks and early
// returns on each method override are sufficient.
//
// Crucially, this copying process is shallow. We only copy what is necessary to
// interpret a given type.

// The visitor walks the type with the context switched to `src`, and switches to `dst` to copy
// into it.
struct CopierVisitor {
    src: ContextId,
    dst: ContextId,
}

impl CopierVisitor {
    fn new(src: ContextId, dst: ContextId) -> Self {
        CopierVisitor { src, dst }
    }
}

impl type_visitor::TypeVisitor<()> for CopierVisitor {
    // Copying a tvar produces a FullyResolved tvar in the dst cx, which
    // contains an unevaluated thunk. The laziness here makes the copying
    // shallow. Note that the visitor stops at root tvars here and only resumes
    // if the thunk is forced.
    fn tvar<'cx>(&mut self, cx: &mut Context<'cx>, pole: Polarity, _acc: (), _r: &Reason, id: u32) {
        let id = id as i32;
        if cx.switch_to(self.dst).graph().get(&id).is_some() {
            return;
        }
        let (root_id, constraints) = cx.find_constraints(id);
        if id == root_id {
            let state = match constraints {
                Constraints::Unresolved(_) | Constraints::Resolved(_) => {
                    panic!("unexpected unresolved constraint")
                }
                Constraints::FullyResolved(s) => {
                    let src = self.src;
                    s.copy(
                        move |dst_cx: &mut Context<'_>, src_state: &ForcingState<Cx>| {
                            let dst = dst_cx.id();
                            let mut src_cx = dst_cx.switch_to(src);
                            let t = src_state.force(&mut src_cx, |src_cx, r| {
                                src_cx.on_cyclic_tvar_error(r.dupe())
                            });
                            CopierVisitor::new(src, dst).type_(&mut src_cx, pole, (), &t);
                            t
                        },
                    )
                }
            };
            cx.switch_to(self.dst)
                .graph_mut()
                .insert_fresh(id, Node::create_root(Constraints::FullyResolved(state)));
        } else {
            cx.switch_to(self.dst)
                .graph_mut()
                .insert_fresh(id, Node::create_goto(root_id));
            self.tvar(cx, pole, (), _r, root_id as u32);
        }
    }

    fn props<'cx>(&mut self, cx: &mut Context<'cx>, pole: Polarity, _acc: (), id: properties::Id) {
        if cx.switch_to(self.dst).has_property_map(&id) {
            return;
        }
        let props = cx.find_props(id.dupe());
        cx.switch_to(self.dst).add_property_map(id.dupe(), props);
        type_visitor::props_default(self, cx, pole, (), id);
    }

    fn call_prop<'cx>(&mut self, cx: &mut Context<'cx>, pole: Polarity, _acc: (), id: i32) {
        if cx.switch_to(self.dst).has_call_prop(&id) {
            return;
        }
        let t = cx.find_call(id);
        cx.switch_to(self.dst).add_call_prop(id, t);
        type_visitor::call_prop_default(self, cx, pole, (), id);
    }

    fn exports<'cx>(&mut self, cx: &mut Context<'cx>, pole: Polarity, _acc: (), id: exports::Id) {
        if cx.switch_to(self.dst).has_export_map(&id) {
            return;
        }
        let map = cx.find_exports(id.dupe());
        cx.switch_to(self.dst).add_export_map(id.dupe(), map);
        type_visitor::exports_default(self, cx, pole, (), id);
    }

    fn eval_id<'cx>(&mut self, cx: &mut Context<'cx>, pole: Polarity, _acc: (), id: eval::Id) {
        match cx.evaluated().get(&id).duped() {
            None => {}
            Some(t) => {
                {
                    let mut dst_cx = cx.switch_to(self.dst);
                    if dst_cx.evaluated().contains_key(&id) {
                        return;
                    }
                    let mut evaluated = dst_cx.evaluated();
                    evaluated.insert(id.dupe(), t);
                    dst_cx.set_evaluated(evaluated);
                }
                type_visitor::eval_id_default(self, cx, pole, (), id);
            }
        }
    }
}

/// `dst_cx` is the context the module is copied into, and `src` the one `f` computes it in.
pub fn copy_into<'cx>(
    dst_cx: &mut Context<'cx>,
    src: ContextId,
    f: &dyn Fn(&mut Context<'cx>) -> Result<ModuleType, Type>,
) -> Result<ModuleType, Type> {
    let dst = dst_cx.id();
    let mut src_cx = dst_cx.switch_to(src);
    let m = f(&mut src_cx);
    let mut copier = CopierVisitor::new(src, dst);
    match m {
        Ok(m) => {
            copier.export_types(&mut src_cx, Polarity::Positive, (), &m.module_export_types);
            Ok(m)
        }
        Err(e) => {
            copier.type_(&mut src_cx, Polarity::Positive, (), &e);
            Err(e)
        }
    }
}

pub fn copied<'cx>(dst_cx: &mut Context<'cx>, src: ContextId, t: &Type) -> Type {
    let mut copier = CopierVisitor::new(src, dst_cx.id());
    copier.type_(&mut dst_cx.switch_to(src), Polarity::Positive, (), t);
    t.dupe()
}

pub fn module_type_copied<'cx>(
    dst_cx: &mut Context<'cx>,
    src: ContextId,
    m: &ModuleType,
) -> ModuleType {
    let mut copier = CopierVisitor::new(src, dst_cx.id());
    copier.export_types(
        &mut dst_cx.switch_to(src),
        Polarity::Positive,
        (),
        &m.module_export_types,
    );
    m.dupe()
}

fn merge_libs_from_ordered_asts(
    sig_opts: &TypeSigOptions,
    ordered_asts: &[(
        &ast::Program<Loc, Loc>,
        flow_common::type_strictness::TypeStrictnessKind,
    )],
    global_augmentation_asts: &[(
        &ast::Program<Loc, Loc>,
        flow_common::type_strictness::TypeStrictnessKind,
        &FileSig,
    )],
) -> (
    flow_error::ErrorSet,
    compact_table::Table<Loc>,
    flow_type_sig::packed_type_sig::Builtins<Loc>,
) {
    let arenas = bumpalo::Bump::new();
    let (builtin_errors, builtin_locs, builtins) = type_sig_utils::parse_and_pack_builtins(
        sig_opts,
        &arenas,
        ordered_asts,
        global_augmentation_asts,
    );
    let builtin_errors: Vec<FlowError<ALoc>> = builtin_errors
        .into_iter()
        .filter_map(|err| match err {
            type_sig::Errno::SigError(e) => {
                let e = e.map(&mut (), |_, l| ALoc::of_loc(builtin_locs.get(*l).dupe()));
                let msg = ErrorMessage::ESignatureVerification(e);
                let loc_of_msg = msg.loc_of_msg();
                let source_file = loc_of_msg
                    .as_ref()
                    .and_then(|loc| ALoc::source(loc))
                    .unwrap();
                Some(flow_error::error_of_msg(source_file.dupe(), msg))
            }
            type_sig::Errno::BindingValidationError(e) => {
                let e = e.map(&mut (), |_, l| ALoc::of_loc(builtin_locs.get(*l).dupe()));
                let msg = ErrorMessage::ESignatureBindingValidation(e);
                let loc_of_msg = msg.loc_of_msg();
                let source_file = loc_of_msg
                    .as_ref()
                    .and_then(|loc| ALoc::source(loc))
                    .unwrap();
                Some(flow_error::error_of_msg(source_file.dupe(), msg))
            }
            type_sig::Errno::CheckError => None,
        })
        .collect();
    let builtin_error_set = flow_error::ErrorSet::from_iter(builtin_errors);
    (builtin_error_set, builtin_locs, builtins)
}

pub fn merge_lib_files(
    sig_opts: &TypeSigOptions,
    all_unordered_libs: Arc<BTreeSet<FlowSmolStr>>,
    ordered_asts: &[(
        flow_common::type_strictness::TypeStrictnessKind,
        Arc<ast::Program<Loc, Loc>>,
    )],
    global_augmentation_asts: &[(
        flow_common::type_strictness::TypeStrictnessKind,
        Arc<ast::Program<Loc, Loc>>,
        Arc<FileSig>,
    )],
) -> (flow_error::ErrorSet, MasterContext) {
    let global_libdefs: Arc<BTreeSet<FileKey>> = Arc::new(
        ordered_asts
            .iter()
            .filter_map(|(_, ast)| ast.loc.source.dupe())
            .collect(),
    );
    let builtin_leader_file_key = ordered_asts
        .first()
        .and_then(|(_, ast)| ast.loc.source.dupe());
    let ordered_asts: Vec<_> = ordered_asts
        .iter()
        .map(|(strictness_kind, ast)| (ast.as_ref(), *strictness_kind))
        .collect();
    let global_augmentation_asts: Vec<_> = global_augmentation_asts
        .iter()
        .map(|(strictness_kind, ast, file_sig)| (ast.as_ref(), *strictness_kind, file_sig.as_ref()))
        .collect();
    let (builtin_errors, builtin_locs, builtins) =
        merge_libs_from_ordered_asts(sig_opts, &ordered_asts, &global_augmentation_asts);
    match builtin_leader_file_key {
        None => (builtin_errors, MasterContext::EmptyMasterContext),
        Some(builtin_leader_file_key) => (
            builtin_errors,
            MasterContext::NonEmptyMasterContext {
                builtin_leader_file_key,
                all_unordered_libs,
                global_libdefs,
                builtins: BuiltinsGroup {
                    builtin_locs: Arc::new(builtin_locs),
                    builtins: Arc::new(builtins),
                },
            },
        ),
    }
}

/// Merges the builtins into a context of `arena`. Contexts made with the returned function read
/// the builtins from that context, so they must belong to a store that contains `arena`.
pub fn mk_builtins(
    metadata: &Metadata,
    master_cx: &MasterContext,
    arena: &mut ContextArena,
) -> MkBuiltins {
    match master_cx {
        MasterContext::EmptyMasterContext => Rc::new(|_cx: &mut Context<'_>| Builtins::empty()),
        MasterContext::NonEmptyMasterContext {
            builtin_leader_file_key,
            all_unordered_libs,
            global_libdefs: _,
            builtins,
        } => {
            let (values, types, modules, umd_values) = type_sig_merge::merge_builtins(
                arena,
                builtin_leader_file_key.dupe(),
                builtins.builtin_locs.dupe(),
                builtins.builtins.dupe(),
            );
            let values: FlowOrdMap<_, _> = values.into_iter().collect();
            let types: FlowOrdMap<_, _> = types.into_iter().collect();
            let modules: FlowOrdMap<_, _> = modules.into_iter().collect();
            let umd_values: FlowOrdMap<_, _> = umd_values.into_iter().collect();
            let source_cx = {
                let values = values.dupe();
                let types = types.dupe();
                let modules = modules.dupe();
                let ccx = arena.add_component();
                arena.add_context(
                    ccx,
                    {
                        let mut m = metadata.clone();
                        m.overridable.checked = false;
                        m
                    },
                    builtin_leader_file_key.dupe(),
                    all_unordered_libs.dupe(),
                    {
                        let builtin_leader_file_key = builtin_leader_file_key.dupe();
                        Rc::new(LazyCell::new(Box::new(move || {
                            Rc::new(ALocTable::empty(builtin_leader_file_key))
                        })
                            as Box<dyn FnOnce() -> Rc<ALocTable>>))
                    },
                    Rc::new(|_cx: &mut Context<'_>, _: &FlowImportSpecifier| {
                        ResolvedRequire::MissingModule
                    }),
                    Rc::new(move |_cx: &mut Context<'_>| {
                        Builtins::of_name_map(values.dupe(), types.dupe(), modules.dupe())
                    }),
                    flow_utils_concurrency::check_budget::CheckBudget::new(None),
                )
            };
            Rc::new(move |_dst_cx: &mut Context<'_>| {
                Builtins::of_name_map_with_readers(
                    Rc::new(move |dst_cx: &mut Context<'_>, lazy_val: &LazyVal<Cx>| {
                        let (loc, t) = lazy_val.get_forced(&mut dst_cx.switch_to(source_cx));
                        (loc.dupe(), copied(dst_cx, source_cx, t))
                    }),
                    Rc::new(
                        move |dst_cx: &mut Context<'_>, lazy_module: &LazyModule<Cx>| {
                            let (r, m) = {
                                let mut src_cx = dst_cx.switch_to(source_cx);
                                let (r, lazy_module_type) = lazy_module.get_forced(&mut src_cx);
                                (r.dupe(), lazy_module_type.get_forced(&mut src_cx).dupe())
                            };
                            (r, module_type_copied(dst_cx, source_cx, &m))
                        },
                    ),
                    values.dupe(),
                    types.dupe(),
                    modules.dupe(),
                    umd_values.dupe(),
                )
            })
        }
    }
}
