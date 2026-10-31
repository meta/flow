/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

//! env_resolution - corresponds to OCaml's Env_resolution module in typing
//!
//! Resolves environment entries during type checking. Handles hint resolution,
//! predicate function resolution, and component resolution in the name definition
//! ordering graph.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::ops::Deref;
use std::rc::Rc;
use std::sync::Arc;

use dupe::Dupe;
use dupe::IterDupedExt;
use dupe::OptionDupedExt;
use flow_aloc::ALoc;
use flow_analysis::bindings::Kind;
use flow_common::enclosing_context::EnclosingContext;
use flow_common::files;
use flow_common::flow_import_specifier;
use flow_common::polarity::Polarity;
use flow_common::reason;
use flow_common::reason::Reason;
use flow_common::subst_name::SubstName;
use flow_data_structure_wrapper::ord_map::FlowOrdMap;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_env_builder::env_api;
use flow_env_builder::name_def_types;
use flow_env_builder::name_def_types::AnnotationData;
use flow_env_builder::name_def_types::Binding;
use flow_env_builder::name_def_types::ClassDefData;
use flow_env_builder::name_def_types::ComponentDefData;
use flow_env_builder::name_def_types::ContextualData;
use flow_env_builder::name_def_types::DeclaredClassDefData;
use flow_env_builder::name_def_types::Def;
use flow_env_builder::name_def_types::EmptyArrayData;
use flow_env_builder::name_def_types::FunctionDefData;
use flow_env_builder::name_def_types::FunctionValueData;
use flow_env_builder::name_def_types::Import;
use flow_env_builder::name_def_types::ImportData;
use flow_env_builder::name_def_types::MatchCasePatternData;
use flow_env_builder::name_def_types::MatchCaseRootData;
use flow_env_builder::name_def_types::MemberAssignData;
use flow_env_builder::name_def_types::ObjectValueData;
use flow_env_builder::name_def_types::OpAssignData;
use flow_env_builder::name_def_types::RecordDefData;
use flow_env_builder::name_def_types::Root;
use flow_env_builder::name_def_types::ScopeKind;
use flow_env_builder::name_def_types::TypeParamData;
use flow_env_builder::selector;
use flow_env_builder_resolver::name_def_ordering;
use flow_parser::ast;
use flow_parser::ast_utils;
use flow_parser::loc_sig::LocSig;
use flow_typing_context::Context;
use flow_typing_context::Cx;
use flow_typing_context::TypingMode;
use flow_typing_errors::flow_error::ErrorSet;
use flow_typing_errors::intermediate_error_types::ExpectedModulePurpose;
use flow_typing_flow_common::flow_js_utils;
use flow_typing_flow_common::flow_js_utils::FlowJsException;
use flow_typing_flow_common::type_subst;
use flow_typing_flow_js::flow_js;
use flow_typing_flow_js::tvar_resolver;
use flow_typing_loc_env::loc_env::PredFuncLazy;
use flow_typing_loc_env::match_pattern_ir;
use flow_typing_statement::statement;
use flow_typing_statement::type_annotation;
use flow_typing_statement::type_annotation::BindsSingleValue;
use flow_typing_type::type_;
use flow_typing_type::type_::AnySource;
use flow_typing_type::type_::ArrayATData;
use flow_typing_type::type_::DefT;
use flow_typing_type::type_::DefTInner;
use flow_typing_type::type_::Tvar;
use flow_typing_type::type_::Type;
use flow_typing_type::type_::TypeInner;
use flow_typing_type::type_::UseT;
use flow_typing_type::type_::any_t;
use flow_typing_type::type_util;
use flow_typing_utils::abnormal::CheckExprError;
use flow_typing_utils::exhaustive;
use flow_typing_utils::type_env;
use flow_typing_utils::type_hint;
use flow_typing_utils::type_operation_utils;
use flow_utils_concurrency::job_error::JobError;

fn mk_tparams_map<'cx>(
    cx: &mut Context<'cx>,
    tparams_map: &FlowOrdMap<ALoc, FlowSmolStr>,
) -> FlowOrdMap<SubstName, Type> {
    let env = cx.environment();
    let mut subst_map = FlowOrdMap::new();
    for (l, _) in tparams_map.iter() {
        if let Some((name, _, ty)) = env.tparams.get_ordinary(l) {
            subst_map.insert(name.dupe(), ty.dupe());
        }
    }
    subst_map
}

fn try_cache_result<'cx, T>(
    cx: &mut Context<'cx>,
    target_loc: Option<ALoc>,
    check: impl FnOnce(&mut Context<'cx>) -> Result<T, CheckExprError>,
    cache: impl FnOnce(&mut Context<'cx>, &T),
) -> Result<T, CheckExprError> {
    if !matches!(
        cx.typing_mode(),
        flow_typing_context::TypingMode::CheckingMode
    ) {
        let original_errors = cx.errors();
        cx.reset_errors(ErrorSet::new());
        let (produced_uncacheable_result, result) =
            cx.run_in_synthesis_mode(false, target_loc, check);
        let can_cache = !produced_uncacheable_result && cx.errors().is_empty();
        cx.reset_errors(original_errors);
        let e = result?;
        if can_cache {
            cache(cx, &e);
        }
        Ok(e)
    } else {
        let e = check(cx)?;
        cache(cx, &e);
        Ok(e)
    }
}

// Helper to incrementally build PatternUnion state from a previous pattern location.
// Returns the pattern_union_state to use for the current pattern.
fn get_pattern_union_state_from_prev<'cx>(
    cx: &mut Context<'cx>,
    prev_pattern_loc: Option<ALoc>,
) -> Result<(match_pattern_ir::pattern_union::PatternUnion, usize), JobError> {
    match prev_pattern_loc {
        // First pattern: start with empty PatternUnion
        None => Ok((match_pattern_ir::pattern_union::empty(), 0)),
        Some(prev_loc) => {
            let node_cache = cx.node_cache();
            match node_cache.get_match_pattern_union(&prev_loc) {
                Some(prev_pattern_union_state) => {
                    let prev_pattern = node_cache
                        .get_match_pattern(&prev_loc)
                        .expect("match pattern cache missing");
                    exhaustive::pattern_union_builder::add_pattern(
                        cx,
                        &|cx, loc| type_env::is_global_var(cx, loc),
                        false,
                        (
                            prev_pattern_union_state.0,
                            prev_pattern_union_state.1 as usize,
                        ),
                        (&prev_pattern.0, prev_pattern.1),
                        false,
                    )
                }
                None => panic!("ERROR: pattern_union cache missing - dependency ordering issue"),
            }
        }
    }
}

fn expression<'cx>(
    cx: &mut Context<'cx>,
    encl_ctx: Option<EnclosingContext>,
    decl: Option<ast::VariableKind>,
    as_const: Option<bool>,
    exp: &ast::expression::Expression<ALoc, ALoc>,
) -> Result<Type, CheckExprError> {
    let target_loc = match cx.typing_mode() {
        TypingMode::SynthesisMode { target_loc } => target_loc.clone(),
        _ => None,
    };
    let exp_clone = exp.dupe();
    let result = try_cache_result(
        cx,
        target_loc,
        move |cx| statement::expression(encl_ctx, decl, as_const, cx, &exp_clone),
        |cx, e| {
            cx.node_cache_mut().set_expression(e.dupe());
        },
    )?;
    let (_loc, t) = result.loc().dupe();
    Ok(t)
}

fn make_hooklike<'cx>(cx: &mut Context<'cx>, t: Type) -> Type {
    if cx.hook_compatibility() {
        match t.deref() {
            TypeInner::DefT(_, def_t) if matches!(def_t.deref(), type_::DefTInner::TypeT(..)) => t,
            _ => {
                let reason = type_util::reason_of_t(&t).dupe();
                let reason_inner = reason.dupe();
                flow_js_utils::map_on_resolved_type(cx, reason.dupe(), t, move |cx, t| {
                    let reason_for_tvar = reason_inner.dupe();
                    tvar_resolver::mk_tvar_and_fully_resolve_no_wrap_where(
                        cx,
                        reason_inner,
                        |cx, _tvar_reason, tvar_id| {
                            let tvar = Tvar::new(reason_for_tvar.dupe(), tvar_id as u32);
                            let use_t = UseT::new(type_::UseTInner::HooklikeT(Box::new(tvar)));
                            flow_js::flow_non_speculating(cx, (&t, &use_t))?;
                            Ok(())
                        },
                    )
                })
            }
        }
    } else {
        t
    }
}

/// `binds_single_value` says whether the annotation is the type of a position
/// that binds exactly one value. A variable declaration does, and is the only
/// statement position that may spell out a `unique symbol`.
fn resolve_annotation<'cx>(
    cx: &mut Context<'cx>,
    tparams_map: &FlowOrdMap<ALoc, FlowSmolStr>,
    react_deep_read_only: Option<(ALoc, name_def_types::DroAnnot)>,
    anno: &ast::types::Annotation<ALoc, ALoc>,
    binds_single_value: BindsSingleValue,
) -> Result<Type, JobError> {
    let tparams_map = mk_tparams_map(cx, tparams_map);
    let (t, anno_typed) =
        type_annotation::mk_type_available_annotation(cx, tparams_map, anno, binds_single_value)?;
    let t = match react_deep_read_only {
        Some((param_loc, kind)) => {
            let enabled = match kind {
                name_def_types::DroAnnot::Hook => {
                    cx.react_rule_enabled(flow_common::options::ReactRule::DeepReadOnlyProps)
                }
                name_def_types::DroAnnot::Comp => {
                    cx.react_rule_enabled(flow_common::options::ReactRule::DeepReadOnlyProps)
                }
            };
            if enabled {
                let reason = type_util::reason_of_t(&t).dupe();
                let dro_type = match kind {
                    name_def_types::DroAnnot::Hook => type_::DroType::HookArg,
                    name_def_types::DroAnnot::Comp => type_::DroType::Props,
                };
                let react_dro = type_::ReactDro(param_loc, dro_type);
                let destructor = Rc::new(type_::Destructor::ReactDRO(Box::new(react_dro)));
                let eval_id = type_::eval::Id::generate_id();
                flow_js_exception_to_job_error(
                    flow_js::FlowJs::mk_possibly_evaluated_destructor_for_annotations(
                        cx,
                        type_::unknown_use(),
                        &reason,
                        &t,
                        &destructor,
                        eval_id,
                    ),
                )?
            } else {
                t
            }
        }
        _ => t,
    };
    if matches!(
        cx.typing_mode(),
        flow_typing_context::TypingMode::CheckingMode
    ) {
        cx.node_cache_mut().set_annotation(anno_typed);
    }
    Ok(t)
}

fn synthesizable_expression<'cx>(
    cx: &mut Context<'cx>,
    encl_ctx: EnclosingContext,
    exp: &ast::expression::Expression<ALoc, ALoc>,
) -> Result<Type, CheckExprError> {
    use ast::expression::ExpressionInner;
    Ok(match exp.deref() {
        ExpressionInner::Identifier { loc, inner } => {
            statement::identifier(encl_ctx, cx, inner, loc.dupe())?
        }
        ExpressionInner::StringLiteral { loc, inner } => {
            statement::string_literal(cx, encl_ctx, loc.dupe(), inner)?
        }
        ExpressionInner::BooleanLiteral { loc, inner } => {
            statement::boolean_literal(cx, encl_ctx, loc.dupe(), inner)
        }
        ExpressionInner::NullLiteral { loc, .. } => statement::null_literal(loc.dupe()),
        ExpressionInner::NumberLiteral { loc, inner } => {
            statement::number_literal(cx, encl_ctx, loc.dupe(), inner)
        }
        ExpressionInner::BigIntLiteral { loc, inner } => {
            statement::bigint_literal(cx, encl_ctx, loc.dupe(), inner)
        }
        ExpressionInner::RegExpLiteral { loc, .. } => statement::regexp_literal(cx, loc.dupe())?,
        ExpressionInner::ModuleRefLiteral { loc, inner } => {
            let (t, _lit) = statement::module_ref_literal(cx, loc.dupe(), inner)?;
            t
        }
        ExpressionInner::AsExpression { inner, .. } => resolve_annotation(
            cx,
            &FlowOrdMap::default(),
            None,
            &inner.annot,
            BindsSingleValue::No,
        )?,
        ExpressionInner::TypeCast { inner, .. } => resolve_annotation(
            cx,
            &FlowOrdMap::default(),
            None,
            &inner.annot,
            BindsSingleValue::No,
        )?,
        ExpressionInner::Member { loc, inner }
            if let ast::expression::member::Property::PropertyIdentifier(id) = &inner.property =>
        {
            let ploc = &id.loc;
            let name = &id.name;
            let t = synthesizable_expression(cx, encl_ctx, &inner.object)?;
            match flow_typing_statement::refinement::get(false, cx, exp, loc.dupe())? {
                Some(t) => t,
                None => {
                    let expr_reason = reason::mk_expression_reason(exp);
                    let prop_reason = reason::mk_reason(
                        reason::VirtualReasonDesc::RProperty(Some(reason::Name::new(name.dupe()))),
                        ploc.dupe(),
                    );
                    // TODO(jmbrown) This feels incorrect
                    let use_op = type_::VirtualUseOp::Op(Arc::new(
                        type_::VirtualRootUseOp::GetProperty(expr_reason.dupe()),
                    ));
                    statement::get_prop(
                        EnclosingContext::NoContext,
                        cx,
                        expr_reason,
                        use_op,
                        type_env::get_hint(cx, loc.dupe()),
                        t,
                        prop_reason,
                        name,
                    )?
                }
            }
        }
        _ => return expression(cx, Some(encl_ctx), None, None, exp),
    })
}

fn mk_selector_reason_has_default<'cx>(
    cx: &mut Context<'cx>,
    loc: &ALoc,
    selector: &selector::Selector<ALoc, ALoc>,
) -> Result<(type_::Selector, Reason, bool), CheckExprError> {
    use flow_env_builder::selector::Selector;
    Ok(match selector {
        Selector::Elem {
            index: n,
            has_default,
        } => {
            let key = Type::new(TypeInner::DefT(
                reason::mk_reason(reason::VirtualReasonDesc::RNumber, loc.dupe()),
                DefT::new(DefTInner::SingletonNumT {
                    from_annot: false,
                    value: type_::NumberLiteral(*n as f64, FlowSmolStr::from(n.to_string())),
                }),
            ));
            (
                type_::Selector::Elem(key),
                reason::mk_reason(None, loc.dupe()),
                *has_default,
            )
        }
        Selector::Prop {
            prop,
            prop_loc,
            has_default,
        } => (
            type_::Selector::Prop(prop.dupe(), *has_default),
            reason::mk_reason(
                reason::VirtualReasonDesc::RProperty(Some(reason::Name::new(prop.dupe()))),
                prop_loc.dupe(),
            ),
            *has_default,
        ),
        Selector::ArrRest(n) => (
            type_::Selector::ArrRest(*n as i32),
            reason::mk_reason(reason::VirtualReasonDesc::RArrayPatternRestProp, loc.dupe()),
            false,
        ),
        Selector::ObjRest {
            used_props,
            after_computed: _,
        } => {
            // TODO: eveyrthing after a computed prop should be optional
            (
                type_::Selector::ObjRest(used_props.to_vec().into()),
                reason::mk_reason(
                    reason::VirtualReasonDesc::RObjectPatternRestProp,
                    loc.dupe(),
                ),
                false,
            )
        }
        Selector::Computed {
            expression: exp,
            has_default,
        } => {
            let t = expression(cx, Some(EnclosingContext::IndexContext), None, None, exp)?;
            (
                type_::Selector::Elem(t),
                reason::mk_reason(reason::VirtualReasonDesc::RProperty(None), loc.dupe()),
                *has_default,
            )
        }
        Selector::Default => (
            type_::Selector::Default,
            reason::mk_reason(None, loc.dupe()),
            false,
        ),
    })
}

fn synthesize_expression_for_instantiation<'cx>(
    cx: &mut Context<'cx>,
    target_loc: Option<ALoc>,
    e: &ast::expression::Expression<ALoc, ALoc>,
) -> Result<ast::expression::Expression<ALoc, (ALoc, Type)>, CheckExprError> {
    let e_clone = e.dupe();
    try_cache_result(
        cx,
        target_loc,
        move |cx| statement::expression(None, None, None, cx, &e_clone),
        |cx, e| {
            cx.node_cache_mut().set_expression(e.dupe());
        },
    )
}

fn synthesize_jsx_children_for_instantiation<'cx>(
    cx: &mut Context<'cx>,
    children: &(ALoc, Vec<ast::jsx::Child<ALoc, ALoc>>),
) -> Result<
    (
        Vec<type_::UnresolvedParam>,
        (ALoc, Vec<ast::jsx::Child<ALoc, (ALoc, Type)>>),
    ),
    CheckExprError,
> {
    let children_clone = children.clone();
    try_cache_result(
        cx,
        None,
        move |cx| statement::collapse_children(cx, &children_clone),
        |cx, result| {
            cx.node_cache_mut().set_jsx_children(result.clone());
        },
    )
}

fn synth_arg_list<'cx>(
    cx: &mut Context<'cx>,
    target_loc: Option<ALoc>,
    arg_list: &ast::expression::ArgList<ALoc, ALoc>,
) -> Result<Vec<(ALoc, type_::CallArg)>, JobError> {
    let arguments = &arg_list.arguments;
    arguments
        .iter()
        .map(|arg| -> Result<_, JobError> {
            use ast::expression::ExpressionOrSpread;
            match arg {
                ExpressionOrSpread::Expression(e) => {
                    let result = check_expr_error_to_job_error(
                        synthesize_expression_for_instantiation(cx, target_loc.dupe(), e),
                    )?;
                    let (loc, t) = result.loc().dupe();
                    Ok((loc, type_::CallArg::arg(t)))
                }
                ExpressionOrSpread::Spread(spread) => {
                    let e = &spread.argument;
                    let result = check_expr_error_to_job_error(
                        synthesize_expression_for_instantiation(cx, target_loc.dupe(), e),
                    )?;
                    let (loc, t) = result.loc().dupe();
                    Ok((loc, type_::CallArg::spread_arg(t)))
                }
            }
        })
        .collect()
}

fn flow_js_exception_to_job_error<T>(r: Result<T, FlowJsException>) -> Result<T, JobError> {
    match r {
        Ok(v) => Ok(v),
        Err(FlowJsException::WorkerCanceled(c)) => Err(JobError::Canceled(c)),
        Err(FlowJsException::TimedOut(t)) => Err(JobError::TimedOut(t)),
        Err(FlowJsException::DebugThrow { loc }) => Err(JobError::DebugThrow { loc }),
        Err(err) => panic!("Non speculating: {:?}", err),
    }
}

fn check_expr_error_to_job_error<T>(r: Result<T, CheckExprError>) -> Result<T, JobError> {
    match r {
        Ok(v) => Ok(v),
        Err(CheckExprError::Canceled(c)) => Err(JobError::Canceled(c)),
        Err(CheckExprError::TimedOut(t)) => Err(JobError::TimedOut(t)),
        Err(CheckExprError::DebugThrow { loc }) => Err(JobError::DebugThrow { loc }),
        Err(err) => panic!("Non speculating: {:?}", err),
    }
}

fn resolve_hint<'cx>(
    cx: &mut Context<'cx>,
    loc: ALoc,
    hint: flow_env_builder::name_def_types::AstHint,
) -> type_hint::ConcrHint {
    use flow_env_builder::name_def_types::ArrayElementPatternHint;
    use flow_env_builder::name_def_types::HintNode;
    use flow_env_builder::name_def_types::ObjectPropPatternHint;
    use flow_typing_type::type_::TypeInner;

    fn resolve_hint_node<'cx>(
        cx: &mut Context<'cx>,
        loc: ALoc,
        hint_node: HintNode,
    ) -> Result<Type, JobError> {
        Ok(match hint_node {
            HintNode::AnnotationHint(tparams_map, anno) => {
                resolve_annotation(cx, &tparams_map, None, &anno, BindsSingleValue::No)?
            }
            HintNode::ValueHint(encl_ctx, exp) => {
                check_expr_error_to_job_error(resolve_write_expression(cx, encl_ctx, &exp))?
            }
            HintNode::ProvidersHint(locs) => {
                if locs.len() == 1 {
                    let loc = locs.first().dupe();
                    type_env::checked_find_loc_env_write(
                        cx,
                        flow_env_builder::env_api::DefLocType::OrdinaryNameLoc,
                        loc,
                    )
                } else {
                    let mut iter = locs.iter();
                    let l1 = iter.next().unwrap().dupe();
                    let l2 = iter.next().unwrap().dupe();
                    let rest: Vec<ALoc> = iter.duped().collect();
                    let t1 = type_env::checked_find_loc_env_write(
                        cx,
                        flow_env_builder::env_api::DefLocType::OrdinaryNameLoc,
                        l1.dupe(),
                    );
                    let t2 = type_env::checked_find_loc_env_write(
                        cx,
                        flow_env_builder::env_api::DefLocType::OrdinaryNameLoc,
                        l2,
                    );
                    let ts: Vec<Type> = rest
                        .into_iter()
                        .map(|loc| {
                            type_env::checked_find_loc_env_write(
                                cx,
                                flow_env_builder::env_api::DefLocType::OrdinaryNameLoc,
                                loc,
                            )
                        })
                        .collect();
                    Type::new(TypeInner::UnionT(
                        reason::mk_reason(None, l1),
                        type_::union_rep::make(
                            None,
                            type_::union_rep::UnionKind::ProvidersKind,
                            t1,
                            t2,
                            ts.into(),
                        ),
                    ))
                }
            }
            HintNode::WriteLocHint(kind, hint_loc) => {
                type_env::checked_find_loc_env_write(cx, kind, hint_loc)
            }
            HintNode::StringLiteralType(name) => Type::new(TypeInner::DefT(
                reason::mk_reason(
                    reason::VirtualReasonDesc::RIdentifier(name.dupe()),
                    loc.dupe(),
                ),
                DefT::new(DefTInner::SingletonStrT {
                    from_annot: true,
                    value: name,
                }),
            )),
            HintNode::ReactFragmentType => flow_js_exception_to_job_error(
                flow_js_utils::import_export_utils::get_implicitly_imported_react_type(
                    cx,
                    loc.dupe(),
                    &|cx, reason, t| {
                        flow_js::FlowJs::singleton_concretize_type_for_imports_exports(
                            cx, &reason, &t,
                        )
                    },
                    ExpectedModulePurpose::ReactModuleForJSXFragment,
                ),
            )?,
            HintNode::ReactNodeType => {
                let react_node_reason = reason::mk_reason(
                    reason::VirtualReasonDesc::RType("React.Node".into()),
                    loc.dupe(),
                );
                flow_typing_tvar::mk_fully_resolved_lazy(
                    cx,
                    react_node_reason.dupe(),
                    false,
                    move |cx: &mut Context<'_>| {
                        flow_js::get_builtin_react_type_non_speculating(
                            cx,
                            &react_node_reason,
                            None,
                            ExpectedModulePurpose::ReactModuleForReactNodeType,
                        )
                    },
                )
            }
            HintNode::AnyErrorHint(reason) => type_::any_t::error(reason),
            HintNode::ComposedArrayPatternHint(hint_loc, elements) => {
                let array_reason = reason::mk_reason(None, hint_loc.dupe());
                let is_empty = elements.is_empty();
                let elem_spread_list: flow_data_structure_wrapper::list::FlowOcamlList<
                    type_::UnresolvedParam,
                > = elements
                    .into_iter()
                    .map(|el| match el {
                        ArrayElementPatternHint::ArrayElementPatternHint(h) => {
                            let t = resolve_hint_node(cx, loc.dupe(), h)?;
                            let elem_reason = reason::mk_reason(None, hint_loc.dupe());
                            Ok(type_::UnresolvedParam::UnresolvedArg(Box::new(
                                type_::UnresolvedArgData(
                                    type_util::mk_tuple_element(
                                        elem_reason,
                                        t,
                                        None,
                                        false,
                                        flow_common::polarity::Polarity::Neutral,
                                    ),
                                    None,
                                ),
                            )))
                        }
                        ArrayElementPatternHint::ArrayRestElementPatternHint(h) => {
                            Ok(type_::UnresolvedParam::UnresolvedSpreadArg(
                                resolve_hint_node(cx, loc.dupe(), h)?,
                            ))
                        }
                    })
                    .collect::<Result<_, JobError>>()?;
                flow_typing_tvar::mk_where(cx, array_reason.dupe(), |cx, tout| {
                    let reason_op = array_reason.dupe();
                    let element_reason = reason_op.dupe().replace_desc(
                        reason::VirtualReasonDesc::RInferredUnionElemArray {
                            instantiable: false,
                            is_empty,
                        },
                    );
                    let elem_t = flow_typing_tvar::mk(cx, element_reason);
                    flow_js_exception_to_job_error(flow_js::FlowJs::resolve_spread_list(
                        cx,
                        type_::unknown_use(),
                        &reason_op,
                        elem_spread_list,
                        type_::SpreadResolve::ResolveSpreadsToArrayLiteral {
                            literal: false,
                            fresh: false,
                            id: reason::mk_id() as i32,
                            kind: type_::ArrayLiteralKind::Mutable,
                            elem_t,
                            tout: tout.dupe(),
                        },
                    ))?;
                    Ok::<(), JobError>(())
                })?
            }
            HintNode::ComposedObjectPatternHint(hint_loc, properties) => {
                let acc = properties.into_iter().try_fold(
                    statement::object_expression_acc::ObjectExpressionAcc::empty(),
                    |acc, prop| -> Result<_, JobError> {
                        Ok(match prop {
                            ObjectPropPatternHint::ObjectPropPatternHint(n, l, h) => {
                                let t = resolve_hint_node(cx, loc.dupe(), h)?;
                                acc.add_prop(move |props_map| {
                                    props_map.insert(
                                        reason::Name::new(n),
                                        type_::Property::new(type_::PropertyInner::Field(
                                            Box::new(type_::FieldData {
                                                preferred_def_locs: None,
                                                key_loc: Some(l),
                                                type_: t,
                                                polarity: flow_common::polarity::Polarity::Neutral,
                                            }),
                                        )),
                                    );
                                })
                            }
                            ObjectPropPatternHint::ObjectSpreadPropPatternHint(h) => {
                                let t = resolve_hint_node(cx, loc.dupe(), h)?;
                                acc.add_spread(t)
                            }
                        })
                    },
                )?;
                let obj_reason = reason::mk_reason(None, hint_loc.dupe());
                let default_proto = Type::new(TypeInner::ObjProtoT(obj_reason.dupe()));
                flow_typing_tvar::mk_fully_resolved_lazy(
                    cx,
                    obj_reason.dupe(),
                    false,
                    move |cx: &mut Context<'_>| {
                        acc.mk_object_from_spread_acc(
                            cx,
                            obj_reason.dupe(),
                            obj_reason,
                            false,
                            false,
                            default_proto,
                            false,
                        )
                    },
                )
            }
        })
    }

    let loc_clone = loc.dupe();
    let map_base_hint =
        Rc::new(move |cx: &mut Context<'_>, h| resolve_hint_node(cx, loc_clone.dupe(), h));

    let map_targs = Rc::new(
        move |cx: &mut Context<'_>, targs: Option<ast::expression::CallTypeArgs<ALoc, ALoc>>| {
            statement::convert_call_targs_opt_prime(cx, targs.as_ref())
        },
    );

    let map_arg_list = Rc::new(
        move |_cx: &mut Context<'_>,
              arg_list: ast::expression::ArgList<ALoc, ALoc>|
              -> type_hint::ConcrArgListFn {
            let l = arg_list.loc.dupe();
            let arg_list = std::sync::Arc::new(arg_list);
            Rc::new(move |cx: &mut Context<'_>, target_loc: Option<ALoc>| {
                if let Some(result) = cx
                    .hint_map_arglist_cache()
                    .get(&(l.dupe(), target_loc.dupe()))
                    .cloned()
                {
                    Ok(result)
                } else {
                    let result = synth_arg_list(cx, target_loc.dupe(), &arg_list)?;
                    cx.hint_map_arglist_cache_mut()
                        .insert((l.dupe(), target_loc), result.clone());
                    Ok(result)
                }
            })
        },
    );

    type PropsAndChildren = (
        std::sync::Arc<[ast::jsx::OpeningAttribute<ALoc, ALoc>]>,
        (ALoc, Vec<ast::jsx::Child<ALoc, ALoc>>),
    );
    let map_jsx = Rc::new(
        move |cx: &mut Context<'_>,
              jsx_reason: Reason,
              name: FlowSmolStr,
              (props, children): PropsAndChildren|
              -> type_hint::ConcrJsxProps {
            let key_locs: Vec<ALoc> = props
                .iter()
                .map(|attr| match attr {
                    ast::jsx::OpeningAttribute::Attribute(attr) => attr.loc.dupe(),
                    ast::jsx::OpeningAttribute::SpreadAttribute(spread) => spread.loc.dupe(),
                })
                .collect();
            let children_loc = children.0.dupe();
            let cache_key = (jsx_reason.dupe(), name.to_string(), key_locs, children_loc);

            if let Some(result) = cx.hint_map_jsx_cache().get(&cache_key).duped() {
                result
            } else {
                let original_errors = cx.errors();
                cx.reset_errors(ErrorSet::new());
                let props_clone = props.dupe();
                let children_clone = children.clone();
                let jsx_reason_clone = jsx_reason.dupe();
                let name_clone = name.dupe();
                let lazy_props: type_hint::ConcrJsxProps =
                    Rc::new(flow_lazy::Lazy::new(move |cx: &mut Context<'_>| {
                        let (jsx_props_t, _, _, _) =
                            check_expr_error_to_job_error(statement::jsx_mk_props(
                                cx,
                                jsx_reason_clone.dupe(),
                                &|_hint, cx, expr| {
                                    synthesize_expression_for_instantiation(cx, None, expr)
                                },
                                &|cx, children| {
                                    synthesize_jsx_children_for_instantiation(cx, children)
                                },
                                &name_clone,
                                &props_clone,
                                &children_clone,
                            ))?;
                        Ok(jsx_props_t)
                    }));
                cx.reset_errors(original_errors);
                cx.hint_map_jsx_cache_mut()
                    .insert(cache_key, lazy_props.dupe());
                lazy_props
            }
        },
    );

    hint.map::<Cx, _, _, _, _>(cx, &map_base_hint, &map_targs, &map_arg_list, &map_jsx)
}

fn resolve_hints<'cx>(
    cx: &mut Context<'cx>,
    loc: ALoc,
    hints: &[flow_env_builder::name_def_types::AstHint],
) -> Vec<type_hint::ConcrHint> {
    hints
        .iter()
        .map(|h| resolve_hint(cx, loc.dupe(), h.clone()))
        .collect()
}

pub fn lazily_resolve_hints<'cx>(
    _cx: &mut Context<'cx>,
    loc: ALoc,
    hints: &[flow_env_builder::name_def_types::AstHint],
) -> type_::LazyHintT<Cx> {
    let has_hint = !hints.is_empty();
    let hints_owned: Vec<flow_env_builder::name_def_types::AstHint> = hints.to_vec();
    let lazy_hint = move |cx: &mut Context<'_>,
                          expected_only: bool,
                          skip_optional: Option<bool>,
                          r: Reason|
          -> Result<type_::HintEvalResult, JobError> {
        let resolved = resolve_hints(cx, loc.dupe(), &hints_owned);
        type_hint::evaluate_hints(cx, expected_only, skip_optional, &r, resolved)
    };
    type_::LazyHintT(has_hint, Rc::new(lazy_hint))
}

pub fn resolve_pred_func<'cx>(
    _cx: &mut Context<'cx>,
    info: &env_api::PredFuncInfo<ALoc>,
) -> PredFuncLazy<Cx> {
    let callee_info = info.clone();
    let callee = Rc::new(flow_lazy::Lazy::new(move |cx: &mut Context<'_>| {
        let loc = callee_info.callee.loc();
        // [callee] might be a member access expression. Type it separately in
        // synthesis mode so resolving predicate metadata does not cache or duplicate errors.
        Ok(type_env::with_class_stack(
            cx,
            callee_info.class_stack.dupe(),
            |cx| {
                // Synthesis mode to avoid caching
                let (_, (t, new_errors)) = cx.run_in_synthesis_mode_with_errors(None, |cx| {
                    expression(cx, None, None, None, &callee_info.callee)
                });
                if new_errors.is_lint_only_errorset() {
                    match t {
                        Ok(t) => t,
                        Err(_) => any_t::at(AnySource::AnyError(None), loc.dupe()),
                    }
                } else {
                    any_t::at(AnySource::AnyError(None), loc.dupe())
                }
            },
        ))
    }));

    let info = info.clone();
    let callee_for_full = callee.dupe();
    let full = Rc::new(flow_lazy::Lazy::new(move |cx: &mut Context<'_>| {
        let use_op = type_::VirtualUseOp::Op(Arc::new(type_::VirtualRootUseOp::FunCall(Box::new(
            type_::FunCallData {
                op: reason::mk_expression_reason(&info.call_expr),
                fn_: reason::mk_expression_reason(&info.callee),
                args: reason::mk_initial_arguments_reason(&info.arguments).into(),
                implicit_instantiation_reference: type_::ImplicitInstantiationReferenceKind::Call(
                    Some(reason::code_desc_of_expression(false, &info.callee).into()),
                ),
                local: true,
            },
        ))));
        let callee_t = callee_for_full.try_get_forced(cx)?.dupe();
        let targs = statement::convert_call_targs_opt_prime(cx, info.targs.as_ref())?;
        let arguments = &info.arguments.arguments;
        let argts: Rc<[type_::CallArg]> = arguments
            .iter()
            .map(|e| -> Result<_, JobError> {
                let (t, _) = check_expr_error_to_job_error(statement::expression_or_spread(cx, e))?;
                Ok(t)
            })
            .collect::<Result<_, JobError>>()?;
        let ex_loc = info.call_expr.loc();
        Ok(type_::PredFuncallInfo(
            use_op,
            ex_loc.dupe(),
            callee_t,
            targs.map(Rc::from),
            argts,
        ))
    }));

    PredFuncLazy { callee, full }
}

fn resolve_annotated_function<'cx>(
    cx: &mut Context<'cx>,
    scope_kind: ScopeKind,
    bind_this: bool,
    statics: &BTreeMap<FlowSmolStr, env_api::EnvKey<ALoc>>,
    namespace_types: &BTreeMap<FlowSmolStr, env_api::EnvKey<ALoc>>,
    hook_like: bool,
    reason: Reason,
    tparams_map: &FlowOrdMap<ALoc, FlowSmolStr>,
    function_loc: ALoc,
    function_: &ast::function::Function<ALoc, ALoc>,
) -> Result<Type, CheckExprError> {
    let sig_loc = function_.sig_loc.dupe();
    let effect_ = &function_.effect_;
    if (scope_kind == ScopeKind::ComponentOrHookBody
        || scope_kind == ScopeKind::AsyncComponentOrHookBody)
        && *effect_ == ast::function::Effect::Hook
    {
        flow_js_utils::add_output_non_speculating(
            cx,
            flow_typing_errors::error_message::ErrorMessage::ENestedHook(reason.loc().dupe()),
        );
    }
    if function_.async_ && *effect_ == ast::function::Effect::Hook && !cx.async_component_syntax() {
        flow_js_utils::add_output_non_speculating(
            cx,
            flow_typing_errors::error_message::ErrorMessage::EUnsupportedSyntax(Box::new((
                function_loc.dupe(),
                flow_typing_errors::intermediate_error_types::UnsupportedSyntax::AsyncHookSyntax,
            ))),
        );
    }
    let tparams_map = mk_tparams_map(cx, tparams_map);
    let default_this = flow_js_utils::default_this_type(cx, bind_this, function_)
        .unwrap_or_else(|_| type_::mixed_t::at(function_loc.dupe()));
    let sig_data = statement::mk_func_sig(
        cx,
        false,
        flow_typing_statement::type_annotation::MethodKind::FunctionKind,
        statics,
        &tparams_map,
        reason.dupe(),
        true,
        function_,
    )?;
    let (ref func_sig, ref reconstruct, ref deferred_tg_check) = sig_data;
    cx.node_cache_mut().set_function_sig(
        sig_loc,
        (
            func_sig.clone(),
            reconstruct.clone(),
            deferred_tg_check.clone(),
        ),
    );
    let t = flow_typing_statement::func_sig::functiontype(
        cx,
        !bind_this, // arrow
        Some(function_loc),
        default_this,
        func_sig,
    );
    let t = if *effect_ != ast::function::Effect::Hook && hook_like {
        make_hooklike(cx, t)
    } else {
        t
    };
    Ok(wrap_function_with_namespace_types(
        cx,
        function_.id.as_ref(),
        t,
        namespace_types,
    ))
}

fn namespace_member_polarity(cx: &Context<'_>, key: &env_api::EnvKey<ALoc>) -> Polarity {
    if cx
        .environment()
        .var_info
        .scopes
        .def_of_use_opt(&key.loc)
        .is_some_and(|def| {
            def.is(&key.loc) && matches!(def.kind, Kind::Const | Kind::DeclaredConst)
        })
    {
        Polarity::Positive
    } else {
        Polarity::Neutral
    }
}

fn resolve_function_statics<'cx>(
    cx: &mut Context<'cx>,
    reason: Reason,
    statics: &BTreeMap<FlowSmolStr, env_api::EnvKey<ALoc>>,
) -> Type {
    let props: type_::properties::PropertiesMap = statics
        .iter()
        .map(|(name, env_key)| {
            let expr_t =
                type_env::checked_find_loc_env_write(cx, env_key.def_loc_type, env_key.loc.dupe());
            let field =
                type_::Property::new(type_::PropertyInner::Field(Box::new(type_::FieldData {
                    preferred_def_locs: None,
                    key_loc: Some(env_key.loc.dupe()),
                    type_: expr_t,
                    polarity: namespace_member_polarity(cx, env_key),
                })));
            (reason::Name::new(name.dupe()), field)
        })
        .collect();
    flow_typing_flow_common::obj_type::mk_with_proto(
        cx,
        reason.dupe(),
        type_::ObjKind::Inexact,
        None,
        None,
        Some(props),
        None,
        Type::new(TypeInner::FunProtoT(reason)),
        false,
    )
}

fn extend_class_statics(
    cx: &mut Context<'_>,
    class_t: &Type,
    values: &BTreeMap<FlowSmolStr, env_api::EnvKey<ALoc>>,
) {
    if values.is_empty() {
        return;
    }
    if let Some(statics) = type_util::statics(class_t)
        && let TypeInner::DefT(_, def) = statics.deref()
        && let DefTInner::ObjT(object) = def.deref()
    {
        let mut props = cx.find_props(object.props_tmap.dupe());
        for (name, key) in values {
            let name = reason::Name::new(name.dupe());
            if !props.contains_key(&name) {
                let t = type_env::checked_find_loc_env_write(cx, key.def_loc_type, key.loc.dupe());
                props.insert(
                    name,
                    type_::Property::new(type_::PropertyInner::Field(Box::new(type_::FieldData {
                        preferred_def_locs: None,
                        key_loc: Some(key.loc.dupe()),
                        type_: t,
                        polarity: namespace_member_polarity(cx, key),
                    }))),
                );
            }
        }
        cx.add_property_map(object.props_tmap.dupe(), props);
    }
}

fn replace_function_statics(function_t: Type, statics_t: &Type) -> Type {
    match function_t.deref() {
        TypeInner::DefT(reason, def_t) => match def_t.deref() {
            DefTInner::FunT(_, fun_t) => Type::new(TypeInner::DefT(
                reason.dupe(),
                DefT::new(DefTInner::FunT(statics_t.dupe(), fun_t.dupe())),
            )),
            DefTInner::PolyT(box type_::PolyTData {
                kind: poly_kind,
                tparams_loc,
                tparams,
                t_out,
                id,
                strictness_kind,
            }) => match t_out.deref() {
                TypeInner::DefT(inner_reason, inner_def_t)
                    if let DefTInner::FunT(_, fun_t) = inner_def_t.deref() =>
                {
                    let new_t_out = Type::new(TypeInner::DefT(
                        inner_reason.dupe(),
                        DefT::new(DefTInner::FunT(statics_t.dupe(), fun_t.dupe())),
                    ));
                    Type::new(TypeInner::DefT(
                        reason.dupe(),
                        DefT::new(DefTInner::PolyT(Box::new(type_::PolyTData {
                            kind: *poly_kind,
                            tparams_loc: tparams_loc.dupe(),
                            tparams: tparams.dupe(),
                            t_out: new_t_out,
                            id: id.dupe(),
                            strictness_kind: *strictness_kind,
                        }))),
                    ))
                }
                _ => function_t.dupe(),
            },
            _ => function_t.dupe(),
        },
        _ => function_t.dupe(),
    }
}

fn wrap_with_namespace_types<'cx>(
    cx: &mut Context<'cx>,
    namespace_name: &FlowSmolStr,
    namespace_loc: ALoc,
    values_type: Type,
    namespace_types: &BTreeMap<FlowSmolStr, env_api::EnvKey<ALoc>>,
) -> Type {
    if namespace_types.is_empty() {
        values_type
    } else {
        let types = namespace_types
            .iter()
            .map(|(name, env_key)| {
                let type_ = type_env::checked_find_loc_env_write(
                    cx,
                    env_key.def_loc_type,
                    env_key.loc.dupe(),
                );
                (
                    reason::Name::new(name.dupe()),
                    type_::NamedSymbol::new(Some(env_key.loc.dupe()), None, type_, None),
                )
            })
            .collect();
        flow_js_utils::namespace_type_with_values_type(
            cx,
            flow_common::flow_symbol::Symbol::mk_namespace_symbol(
                namespace_name.dupe(),
                namespace_loc,
            ),
            values_type,
            &types,
        )
    }
}

fn wrap_function_with_namespace_types<'cx>(
    cx: &mut Context<'cx>,
    id: Option<&ast::Identifier<ALoc, ALoc>>,
    values_type: Type,
    namespace_types: &BTreeMap<FlowSmolStr, env_api::EnvKey<ALoc>>,
) -> Type {
    if namespace_types.is_empty() {
        values_type
    } else {
        let id = id.expect("merged function namespace requires a named function");
        wrap_with_namespace_types(cx, &id.name, id.loc.dupe(), values_type, namespace_types)
    }
}

fn resolve_declared_function<'cx>(
    cx: &mut Context<'cx>,
    declarations: &[(ALoc, ast::statement::DeclareFunction<ALoc, ALoc>)],
    statics: &BTreeMap<FlowSmolStr, env_api::EnvKey<ALoc>>,
    namespace_types: &BTreeMap<FlowSmolStr, env_api::EnvKey<ALoc>>,
) -> Result<Type, JobError> {
    let first_decl = declarations
        .first()
        .expect("declared function definitions should be non-empty");
    let statics_t = resolve_function_statics(
        cx,
        reason::mk_annot_reason(
            reason::VirtualReasonDesc::RFunctionType,
            first_decl.0.dupe(),
        ),
        statics,
    );
    let mut types = declarations
        .iter()
        .map(|(_, declaration)| {
            let effect = match &*declaration.annot.annotation {
                ast::types::TypeInner::Function { inner, .. } => inner.effect,
                _ => ast::function::Effect::Arbitrary,
            };
            let t = replace_function_statics(
                resolve_annotation(
                    cx,
                    &FlowOrdMap::default(),
                    None,
                    &declaration.annot,
                    BindsSingleValue::No,
                )?,
                &statics_t,
            );
            Ok(match declaration.id.as_ref() {
                Some(id)
                    if effect != ast::function::Effect::Hook
                        && flow_parser::ast_utils::hook_name(&id.name) =>
                {
                    make_hooklike(cx, t)
                }
                _ => t,
            })
        })
        .collect::<Result<Vec<_>, JobError>>()?
        .into_iter();
    let function_t = match (types.next(), types.next()) {
        (Some(t0), None) => t0,
        (Some(t0), Some(t1)) => Type::new(TypeInner::IntersectionT(
            type_util::reason_of_t(&t0).dupe().replace_desc(None),
            type_::inter_rep::make(t0, t1, types.collect::<Vec<_>>().into()),
        )),
        (None, _) => panic!("declared function definitions should be non-empty"),
    };
    Ok(wrap_function_with_namespace_types(
        cx,
        first_decl.1.id.as_ref(),
        function_t,
        namespace_types,
    ))
}

fn resolve_annotated_component<'cx>(
    cx: &mut Context<'cx>,
    scope_kind: ScopeKind,
    reason: Reason,
    tparams_map: &FlowOrdMap<ALoc, FlowSmolStr>,
    component_loc: ALoc,
    component: &ast::statement::ComponentDeclaration<ALoc, ALoc>,
) -> Result<Type, JobError> {
    if !cx.component_syntax() {
        flow_js_utils::add_output_non_speculating(
            cx,
            flow_typing_errors::error_message::ErrorMessage::EUnsupportedSyntax(Box::new((
                component_loc.dupe(),
                flow_typing_errors::intermediate_error_types::UnsupportedSyntax::ComponentSyntax,
            ))),
        );
        Ok(any_t::at(AnySource::AnyError(None), component_loc))
    } else {
        if scope_kind == ScopeKind::ComponentOrHookBody
            || scope_kind == ScopeKind::AsyncComponentOrHookBody
        {
            flow_js_utils::add_output_non_speculating(
                cx,
                flow_typing_errors::error_message::ErrorMessage::ENestedComponent(
                    reason.loc().dupe(),
                ),
            );
        }
        if component.async_ && !cx.async_component_syntax() {
            flow_js_utils::add_output_non_speculating(
                cx,
                flow_typing_errors::error_message::ErrorMessage::EUnsupportedSyntax(Box::new((
                    component_loc.dupe(),
                    flow_typing_errors::intermediate_error_types::UnsupportedSyntax::AsyncComponentSyntax,
                ))),
            );
        }
        let tparams_map = mk_tparams_map(cx, tparams_map);
        let tparams_map: BTreeMap<SubstName, Type> = tparams_map
            .iter()
            .map(|(k, v)| (k.dupe(), v.dupe()))
            .collect();
        let sig_loc = component.sig_loc.dupe();
        let body = &component.body;
        let sig_data = statement::mk_component_sig(cx, &tparams_map, reason.dupe(), component)?;
        let (ref component_sig, _) = sig_data;
        cx.node_cache_mut()
            .set_component_sig(sig_loc, sig_data.clone());
        let in_annotation = body.is_none();
        flow_typing_statement::component_sig::component_type::<
            flow_typing_statement::component_params::DeclarationConfig,
        >(
            cx,
            in_annotation,
            &component_sig.reason,
            component_sig.tparams.clone(),
            &component_sig.cparams.params,
            component_sig.cparams.rest.as_ref(),
            component_sig.renders_t.dupe(),
            component_sig.id_opt.as_ref().map(|(loc, name)| (loc, name)),
        )
    }
}

fn binding_has_annot(binding: &Binding) -> bool {
    match binding {
        Binding::Root(Root::Annotation(_)) => true,
        Binding::Hooklike(b) | Binding::Select { parent: (_, b), .. } => binding_has_annot(b),
        _ => false,
    }
}

fn resolve_binding<'cx>(
    cx: &mut Context<'cx>,
    def_scope_kind: ScopeKind,
    reason: Reason,
    loc: ALoc,
    b: &Binding,
) -> Result<Type, flow_typing_utils::abnormal::CheckExprError> {
    match b {
        Binding::Root(Root::Annotation(box AnnotationData {
            tparams_map,
            optional,
            has_default_expression,
            param_loc,
            annot,
            react_deep_read_only,
            concrete: _,
        })) => {
            let t = resolve_annotation(
                cx,
                tparams_map,
                match (param_loc, react_deep_read_only) {
                    (Some(param_loc), Some(name_def_types::DroAnnot::Comp)) => {
                        Some((param_loc.dupe(), name_def_types::DroAnnot::Comp))
                    }
                    (Some(param_loc), Some(name_def_types::DroAnnot::Hook)) => {
                        Some((param_loc.dupe(), name_def_types::DroAnnot::Hook))
                    }
                    _ => None,
                },
                &ast::types::Annotation {
                    loc: annot.0.dupe(),
                    annotation: annot.1.dupe(),
                },
                // A function parameter carries a `param_loc`, and one annotation
                // is shared by every call, so it may never spell out a `unique
                // symbol`. Everything else here names a binding and is
                // converted permissively: the def side cannot tell a variable
                // statement from a destructuring pattern or a loop head, so the
                // statement that owns the declaration reports on those.
                match param_loc {
                    Some(_) => BindsSingleValue::No,
                    None => BindsSingleValue::Yes(None),
                },
            )?;
            if let Some(param_loc) = param_loc {
                type_env::bind_function_param(cx, t.dupe(), param_loc.dupe());
            }
            if *optional && !*has_default_expression {
                Ok(type_util::optional(t, None, false))
            } else {
                Ok(t)
            }
        }
        Binding::Root(Root::Value(box name_def_types::Value {
            hints: _,
            expr,
            decl_kind,
            as_const,
        })) => expression(cx, None, *decl_kind, Some(*as_const), expr),
        Binding::Root(Root::MatchCaseRoot(box MatchCaseRootData {
            case_match_root_loc,
            root_pattern_loc,
            prev_pattern_loc,
        })) => {
            let unfiltered_t = type_env::var_ref(
                None,
                cx,
                None,
                &FlowSmolStr::new_inline("<match_root>"),
                case_match_root_loc.dupe(),
            )?;
            let pattern_union_state =
                get_pattern_union_state_from_prev(cx, prev_pattern_loc.dupe())?;
            // Optimization: if the previous value_left was only inexhaustible (e.g., just `string`),
            // filtering won't change it, so we can reuse it directly.
            let value_left = prev_pattern_loc.as_ref().and_then(|prev_loc| {
                cx.node_cache()
                    .get_match_pattern_value_union(prev_loc)
                    .filter(|vu| vu.is_only_inexhaustible())
            });
            let value_left = match value_left {
                Some(value_left) => value_left,
                None => {
                    let pattern_union =
                        exhaustive::pattern_union_builder::finalize(pattern_union_state.0.clone());
                    exhaustive::filter_by_pattern_union(cx, &unfiltered_t, &pattern_union)?
                }
            };
            let node_cache = cx.node_cache_mut();
            node_cache.set_match_pattern_value_union(root_pattern_loc.dupe(), value_left.dupe());
            node_cache.set_match_pattern_union(
                root_pattern_loc.dupe(),
                (pattern_union_state.0, pattern_union_state.1 as i32),
            );
            Ok(value_left.to_type(type_util::reason_of_t(&unfiltered_t).dupe()))
        }
        Binding::Root(Root::ObjectValue(box ObjectValueData {
            obj_loc,
            obj,
            synthesizable: name_def_types::ObjectSynthKind::ObjectSynthesizable { .. },
        })) => {
            let resolve_prop = |cx: &mut Context<'cx>,
                                bind_this: bool,
                                prop_loc: ALoc,
                                fn_loc: ALoc,
                                fn_: &ast::function::Function<ALoc, ALoc>|
             -> Result<Type, CheckExprError> {
                let reason = reason::func_reason(false, false, prop_loc);
                resolve_annotated_function(
                    cx,
                    def_scope_kind,
                    bind_this,
                    &BTreeMap::new(),
                    &BTreeMap::new(),
                    false,
                    reason,
                    &FlowOrdMap::default(),
                    fn_loc,
                    fn_,
                )
            };

            fn mk_expression<'cx>(
                cx: &mut Context<'cx>,
                def_scope_kind: ScopeKind,
                resolve_prop: &dyn Fn(
                    &mut Context<'cx>,
                    bool,
                    ALoc,
                    ALoc,
                    &ast::function::Function<ALoc, ALoc>,
                ) -> Result<Type, CheckExprError>,
                reason: &Reason,
                expr: &ast::expression::Expression<ALoc, ALoc>,
            ) -> Result<Type, CheckExprError> {
                Ok(match expr.deref() {
                    ast::expression::ExpressionInner::StringLiteral { .. }
                    | ast::expression::ExpressionInner::NumberLiteral { .. }
                    | ast::expression::ExpressionInner::NullLiteral { .. }
                    | ast::expression::ExpressionInner::BooleanLiteral { .. }
                    | ast::expression::ExpressionInner::BigIntLiteral { .. }
                    | ast::expression::ExpressionInner::RegExpLiteral { .. }
                    | ast::expression::ExpressionInner::ModuleRefLiteral { .. }
                    | ast::expression::ExpressionInner::Identifier { .. }
                    | ast::expression::ExpressionInner::TypeCast { .. }
                    | ast::expression::ExpressionInner::AsConstExpression { .. }
                    | ast::expression::ExpressionInner::AsExpression { .. }
                    | ast::expression::ExpressionInner::Member { .. } => {
                        synthesizable_expression(cx, EnclosingContext::NoContext, expr)?
                    }
                    ast::expression::ExpressionInner::Function { loc, inner: fn_ } => {
                        let sig_loc = fn_.sig_loc.dupe();
                        resolve_prop(cx, true, sig_loc, loc.dupe(), fn_)?
                    }
                    ast::expression::ExpressionInner::ArrowFunction { loc, inner: fn_ } => {
                        let sig_loc = fn_.sig_loc.dupe();
                        resolve_prop(cx, false, sig_loc, loc.dupe(), fn_)?
                    }
                    ast::expression::ExpressionInner::Object { loc, inner: obj } => {
                        mk_obj(cx, def_scope_kind, resolve_prop, reason, loc.dupe(), obj)?
                    }
                    ast::expression::ExpressionInner::Array { loc, inner: arr }
                        if arr.elements.is_empty() =>
                    {
                        let (_, elem_t) = statement::empty_array(cx, loc.dupe())?;
                        Type::new(TypeInner::DefT(
                            reason.dupe(),
                            DefT::new(DefTInner::ArrT(Rc::new(type_::ArrType::ArrayAT(Box::new(
                                ArrayATData {
                                    elem_t,
                                    tuple_view: Some(type_::empty_tuple_view()),
                                    react_dro: None,
                                    strictness_kind: cx.type_strictness_kind(),
                                    literal: false,
                                    fresh: false,
                                },
                            ))))),
                        ))
                    }
                    ast::expression::ExpressionInner::Array { loc, inner: arr } => {
                        let elements = &arr.elements;
                        let is_empty = elements.is_empty();
                        // TODO merge code with statement.rs implementation

                        let elem_spread_list: flow_data_structure_wrapper::list::FlowOcamlList<
                            type_::UnresolvedParam,
                        > = elements
                            .iter()
                            .map(|e| -> Result<type_::UnresolvedParam, CheckExprError> {
                                use ast::expression::ArrayElement;
                                match e {
                                    ArrayElement::Expression(e) => {
                                        let t = mk_expression(
                                            cx,
                                            def_scope_kind,
                                            resolve_prop,
                                            reason,
                                            e,
                                        )?;
                                        let e_loc = e.loc();
                                        let elem_reason = reason::mk_reason(None, e_loc.dupe());
                                        Ok(type_::UnresolvedParam::UnresolvedArg(Box::new(
                                            type_::UnresolvedArgData(
                                                type_util::mk_tuple_element(
                                                    elem_reason,
                                                    t,
                                                    None,
                                                    false,
                                                    flow_common::polarity::Polarity::Neutral,
                                                ),
                                                None,
                                            ),
                                        )))
                                    }
                                    ArrayElement::Hole(hole_loc) => {
                                        let t = type_::empty_t::at(loc.dupe());
                                        let elem_reason = reason::mk_reason(None, hole_loc.dupe());
                                        Ok(type_::UnresolvedParam::UnresolvedArg(Box::new(
                                            type_::UnresolvedArgData(
                                                type_util::mk_tuple_element(
                                                    elem_reason,
                                                    t,
                                                    None,
                                                    false,
                                                    flow_common::polarity::Polarity::Neutral,
                                                ),
                                                None,
                                            ),
                                        )))
                                    }
                                    ArrayElement::Spread(spread) => {
                                        let t = synthesizable_expression(
                                            cx,
                                            EnclosingContext::NoContext,
                                            &spread.argument,
                                        )?;
                                        Ok(type_::UnresolvedParam::UnresolvedSpreadArg(t))
                                    }
                                }
                            })
                            .collect::<Result<_, _>>()?;
                        let arr_reason =
                            reason::mk_reason(reason::VirtualReasonDesc::RArrayLit, loc.dupe());
                        flow_typing_tvar::mk_where(cx, arr_reason.dupe(), |cx, tout| {
                            let reason_op = arr_reason.dupe();
                            let element_reason = reason_op.dupe().replace_desc(
                                reason::VirtualReasonDesc::RInferredUnionElemArray {
                                    instantiable: false,
                                    is_empty,
                                },
                            );
                            let elem_t = flow_typing_tvar::mk(cx, element_reason);
                            flow_js_exception_to_job_error(flow_js::FlowJs::resolve_spread_list(
                                cx,
                                type_::unknown_use(),
                                &reason_op,
                                elem_spread_list,
                                type_::SpreadResolve::ResolveSpreadsToArrayLiteral {
                                    literal: false,
                                    fresh: false,
                                    id: reason::mk_id() as i32,
                                    kind: type_::ArrayLiteralKind::Mutable,
                                    elem_t,
                                    tout: tout.dupe(),
                                },
                            ))?;
                            Ok::<(), JobError>(())
                        })?
                    }
                    ast::expression::ExpressionInner::Assignment { .. }
                    | ast::expression::ExpressionInner::Binary { .. }
                    | ast::expression::ExpressionInner::Call { .. }
                    | ast::expression::ExpressionInner::Class { .. }
                    | ast::expression::ExpressionInner::Conditional { .. }
                    | ast::expression::ExpressionInner::Import { .. }
                    | ast::expression::ExpressionInner::JSXElement { .. }
                    | ast::expression::ExpressionInner::JSXFragment { .. }
                    | ast::expression::ExpressionInner::Logical { .. }
                    | ast::expression::ExpressionInner::Match { .. }
                    | ast::expression::ExpressionInner::MetaProperty { .. }
                    | ast::expression::ExpressionInner::New { .. }
                    | ast::expression::ExpressionInner::OptionalCall { .. }
                    | ast::expression::ExpressionInner::OptionalMember { .. }
                    | ast::expression::ExpressionInner::Record { .. }
                    | ast::expression::ExpressionInner::Sequence { .. }
                    | ast::expression::ExpressionInner::Super { .. }
                    | ast::expression::ExpressionInner::TaggedTemplate { .. }
                    | ast::expression::ExpressionInner::TemplateLiteral { .. }
                    | ast::expression::ExpressionInner::This { .. }
                    | ast::expression::ExpressionInner::TSSatisfies { .. }
                    | ast::expression::ExpressionInner::Unary { .. }
                    | ast::expression::ExpressionInner::Update { .. }
                    | ast::expression::ExpressionInner::Yield { .. } => {
                        panic!("Object not synthesizable")
                    }
                })
            }

            fn mk_obj<'cx>(
                cx: &mut Context<'cx>,
                def_scope_kind: ScopeKind,
                resolve_prop: &dyn Fn(
                    &mut Context<'cx>,
                    bool,
                    ALoc,
                    ALoc,
                    &ast::function::Function<ALoc, ALoc>,
                ) -> Result<Type, CheckExprError>,
                reason: &Reason,
                obj_loc: ALoc,
                obj: &ast::expression::Object<ALoc, ALoc>,
            ) -> Result<Type, CheckExprError> {
                let obj_reason =
                    reason::mk_reason(reason::VirtualReasonDesc::RObjectLit, obj_loc.dupe());
                let obj_proto = Type::new(TypeInner::ObjProtoT(obj_reason.dupe()));
                let acc = obj.properties.iter().try_fold(
                    statement::object_expression_acc::ObjectExpressionAcc::empty(),
                    |acc, prop| -> Result<_, CheckExprError> {
                        use ast::expression::object::*;
                        Ok(match prop {
                            Property::SpreadProperty(SpreadProperty { argument: exp, .. })
                                if matches!(
                                    exp.deref(),
                                    ast::expression::ExpressionInner::Identifier { .. }
                                ) =>
                            {
                                let spread =
                                    synthesizable_expression(cx, EnclosingContext::NoContext, exp)?;
                                acc.add_spread(spread)
                            }
                            Property::NormalProperty(NormalProperty::Method {
                                loc: prop_loc,
                                key,
                                value: (fn_loc, fn_),
                            }) => {
                                let t =
                                    resolve_prop(cx, false, prop_loc.dupe(), fn_loc.dupe(), fn_)?;
                                match key {
                                    Key::Identifier(id) => {
                                        let name_loc = id.loc.dupe();
                                        let name = id.name.dupe();
                                        acc.add_prop(move |props_map| {
                                            props_map.insert(
                                                reason::Name::new(name),
                                                type_::Property::new(
                                                    type_::PropertyInner::Method {
                                                        key_loc: Some(name_loc.dupe()),
                                                        type_: t,
                                                    },
                                                ),
                                            );
                                        })
                                    }
                                    Key::StringLiteral((name_loc, lit)) => {
                                        let name = lit.value.dupe();
                                        acc.add_prop(move |props_map| {
                                            props_map.insert(
                                                reason::Name::new(name),
                                                type_::Property::new(
                                                    type_::PropertyInner::Method {
                                                        key_loc: Some(name_loc.dupe()),
                                                        type_: t,
                                                    },
                                                ),
                                            );
                                        })
                                    }
                                    _ => panic!("Object not synthesizable"),
                                }
                            }
                            Property::NormalProperty(NormalProperty::Init {
                                key, value, ..
                            }) => {
                                let t =
                                    mk_expression(cx, def_scope_kind, resolve_prop, reason, value)?;
                                match key {
                                    Key::Identifier(id) => {
                                        let name_loc = id.loc.dupe();
                                        let name = id.name.dupe();
                                        acc.add_prop(move |props_map| {
                                            props_map.insert(
                                                reason::Name::new(name),
                                                type_::Property::new(type_::PropertyInner::Field(Box::new(type_::FieldData {
                                                    preferred_def_locs: None,
                                                    key_loc: Some(name_loc.dupe()),
                                                    type_: t,
                                                    polarity:
                                                        flow_common::polarity::Polarity::Neutral,
                                                }))),
                                            );
                                        })
                                    }
                                    Key::StringLiteral((name_loc, lit)) => {
                                        let name = lit.value.dupe();
                                        acc.add_prop(move |props_map| {
                                            props_map.insert(
                                                reason::Name::new(name),
                                                type_::Property::new(type_::PropertyInner::Field(Box::new(type_::FieldData {
                                                    preferred_def_locs: None,
                                                    key_loc: Some(name_loc.dupe()),
                                                    type_: t,
                                                    polarity:
                                                        flow_common::polarity::Polarity::Neutral,
                                                }))),
                                            );
                                        })
                                    }
                                    _ => panic!("Object not synthesizable"),
                                }
                            }
                            _ => panic!("Object not synthesizable"),
                        })
                    },
                )?;
                Ok(acc.mk_object_from_spread_acc(
                    cx,
                    obj_reason.dupe(),
                    obj_reason,
                    false,
                    false,
                    obj_proto,
                    false,
                )?)
            }

            mk_obj(
                cx,
                def_scope_kind,
                &resolve_prop,
                &reason,
                obj_loc.dupe(),
                obj,
            )
        }
        Binding::Root(Root::ObjectValue(box ObjectValueData {
            obj_loc,
            obj,
            synthesizable: _,
        })) => {
            let expr = ast::expression::Expression::new(ast::expression::ExpressionInner::Object {
                loc: obj_loc.dupe(),
                inner: obj.clone().into(),
            });
            expression(cx, None, None, None, &expr)
        }
        Binding::Root(Root::FunctionValue(box FunctionValueData {
            hints: _,
            synthesizable_from_annotation: name_def_types::FunctionSynthKind::FunctionSynthesizable,
            function_loc,
            function_,
            statics,
            arrow,
            tparams_map,
        })) => {
            let tparams_map = mk_tparams_map(cx, tparams_map);
            let sig_loc = function_.sig_loc.dupe();
            let async_ = function_.async_;
            let generator = function_.generator;
            let reason_fun = reason::func_reason(
                async_,
                generator,
                if *arrow {
                    function_loc.dupe()
                } else {
                    sig_loc.dupe()
                },
            );
            let default_this = flow_js_utils::default_this_type(cx, !*arrow, function_)
                .unwrap_or_else(|_| type_::mixed_t::at(function_loc.dupe()));
            let sig_data = statement::mk_func_sig(
                cx,
                false,
                flow_typing_statement::type_annotation::MethodKind::FunctionKind,
                statics,
                &tparams_map,
                reason_fun,
                true,
                function_,
            )?;
            let (ref func_sig, ref reconstruct, ref deferred_tg_check) = sig_data;
            let t = flow_typing_statement::func_sig::functiontype(
                cx,
                *arrow,
                Some(function_loc.dupe()),
                default_this,
                func_sig,
            );
            cx.node_cache_mut().set_function_sig(
                sig_loc,
                (
                    func_sig.clone(),
                    reconstruct.clone(),
                    deferred_tg_check.clone(),
                ),
            );
            Ok(t)
        }
        Binding::Root(Root::FunctionValue(box FunctionValueData {
            hints: _,
            synthesizable_from_annotation: _,
            function_loc,
            function_,
            statics,
            arrow,
            tparams_map: _,
        })) => {
            let id = &function_.id;
            let async_ = function_.async_;
            let generator = function_.generator;
            let sig_loc = function_.sig_loc.dupe();
            let reason_fun = reason::func_reason(
                async_,
                generator,
                if *arrow {
                    function_loc.dupe()
                } else {
                    sig_loc.dupe()
                },
            );
            let func: (Type, ast::function::Function<ALoc, (ALoc, Type)>) = if *arrow {
                statement::mk_arrow(cx, statics, reason_fun, function_)?
            } else {
                statement::mk_function(
                    cx,
                    true,
                    None,
                    statics,
                    reason_fun,
                    function_loc.dupe(),
                    function_,
                )?
            };
            let (func_type, func_ast) = func;
            let cache = cx.node_cache_mut();
            match id {
                Some(id) => {
                    cache.set_function(id.loc.dupe(), (func_type.dupe(), func_ast.clone()));
                }
                None => {
                    cache.set_function(function_loc.dupe(), (func_type.dupe(), func_ast.clone()));
                }
            }
            let expr = ast::expression::Expression::new(if *arrow {
                ast::expression::ExpressionInner::ArrowFunction {
                    loc: (function_loc.dupe(), func_type.dupe()),
                    inner: func_ast.into(),
                }
            } else {
                ast::expression::ExpressionInner::Function {
                    loc: (function_loc.dupe(), func_type.dupe()),
                    inner: func_ast.into(),
                }
            });
            cache.set_expression(expr);
            Ok(func_type)
        }
        Binding::Root(Root::EmptyArray(box EmptyArrayData {
            array_providers,
            arr_loc,
        })) => {
            let (elem_t, tuple_view, arr_reason) = if !array_providers.is_empty() {
                let ts: Vec<Type> = array_providers
                    .iter()
                    .map(|provider_loc| {
                        type_env::checked_find_loc_env_write(
                            cx,
                            env_api::DefLocType::ArrayProviderLoc,
                            provider_loc.dupe(),
                        )
                    })
                    .collect();
                let elem_t = flow_typing_tvar::mk_where(
                    cx,
                    reason::mk_reason(None, loc.dupe()),
                    |cx, tvar| -> Result<(), JobError> {
                        for t in &ts {
                            let use_t = UseT::new(type_::UseTInner::UseT(
                                type_::unknown_use(),
                                tvar.dupe(),
                            ));
                            flow_js::flow_non_speculating(cx, (t, &use_t))?
                        }
                        Ok(())
                    },
                )?;
                (elem_t, None, reason.dupe())
            } else {
                let elem_t = type_::empty_t::make(reason::mk_reason(None, loc.dupe()));
                flow_js_utils::add_output_non_speculating(
                    cx,
                    flow_typing_errors::error_message::ErrorMessage::EEmptyArrayNoProvider {
                        loc: loc.dupe(),
                    },
                );
                (
                    elem_t,
                    Some(type_::empty_tuple_view()),
                    reason.dupe().replace_desc(None),
                )
            };
            let t = Type::new(TypeInner::DefT(
                arr_reason,
                DefT::new(DefTInner::ArrT(Rc::new(type_::ArrType::ArrayAT(Box::new(
                    ArrayATData {
                        elem_t,
                        tuple_view,
                        react_dro: None,
                        strictness_kind: cx.type_strictness_kind(),
                        literal: array_providers.is_empty(),
                        fresh: false,
                    },
                ))))),
            ));
            let exp: ast::expression::Expression<ALoc, (ALoc, Type)> =
                ast::expression::Expression::new(ast::expression::ExpressionInner::Array {
                    loc: (arr_loc.dupe(), t.dupe()),
                    inner: ast::expression::Array {
                        elements: vec![].into(),
                        trailing_comma: false,
                        comments: None,
                    }
                    .into(),
                });
            cx.node_cache_mut().set_expression(exp);
            Ok(t)
        }
        Binding::Root(Root::Contextual(box ContextualData {
            reason: ctx_reason,
            hints,
            optional,
            default_expression,
        })) => {
            let param_loc = ctx_reason.loc().dupe();
            let type_::LazyHintT(has_hint, lazy_hint) = lazily_resolve_hints(cx, loc.dupe(), hints);
            let t = match lazy_hint(cx, false, None, ctx_reason.dupe())? {
                type_::HintEvalResult::HintAvailable(t, _) => {
                    let t = if default_expression.is_some() {
                        flow_typing_tvar::mk_where(
                            cx,
                            ctx_reason.dupe(),
                            |cx, tout| -> Result<(), JobError> {
                                let use_t = UseT::new(type_::UseTInner::FilterOptionalT(
                                    type_::unknown_use(),
                                    tout.dupe(),
                                ));
                                flow_js::flow_non_speculating(cx, (&t, &use_t))?;
                                Ok(())
                            },
                        )?
                    } else {
                        t
                    };
                    type_util::mod_reason_of_t(&|_| ctx_reason.dupe(), &t)
                }
                type_::HintEvalResult::NoHint
                | type_::HintEvalResult::DecompositionError
                | type_::HintEvalResult::EncounteredPlaceholder(_) => {
                    if has_hint {
                        flow_js_utils::add_output_non_speculating(
                            cx,
                            flow_typing_errors::error_message::ErrorMessage::EMissingParameterAnnotation {
                                loc: param_loc.dupe(),
                                hint_available: true,
                            },
                        );
                    }
                    any_t::make(
                        AnySource::AnyError(Some(type_::AnyErrorKind::MissingAnnotation)),
                        ctx_reason.dupe(),
                    )
                }
            };
            if hints.is_empty() {
                flow_js_utils::add_output_non_speculating(
                    cx,
                    flow_typing_errors::error_message::ErrorMessage::EMissingParameterAnnotation {
                        loc: param_loc.dupe(),
                        hint_available: false,
                    },
                );
            }
            type_env::bind_function_param(cx, t.dupe(), param_loc);
            if *optional && default_expression.is_none() {
                Ok(type_util::optional(t, None, false))
            } else {
                Ok(t)
            }
        }
        Binding::Root(Root::UnannotatedParameter(unannotated_reason)) => {
            let t = any_t::make(
                AnySource::AnyError(Some(type_::AnyErrorKind::MissingAnnotation)),
                unannotated_reason.dupe(),
            );
            type_env::bind_function_param(cx, t.dupe(), unannotated_reason.loc().dupe());
            flow_js_utils::add_output_non_speculating(
                cx,
                flow_typing_errors::error_message::ErrorMessage::EMissingParameterAnnotation {
                    loc: unannotated_reason.loc().dupe(),
                    hint_available: false,
                },
            );
            Ok(t)
        }
        Binding::Root(Root::CatchUnannotated) => {
            if cx.use_unknown_in_catch_variables() {
                Ok(type_::mixed_t::at(loc))
            } else {
                let r = reason::mk_reason(None, loc);
                Ok(any_t::make(
                    AnySource::AnyError(Some(type_::AnyErrorKind::MissingAnnotation)),
                    r,
                ))
            }
        }
        Binding::Root(Root::DeclareVariableMissingAnnotationAndInit) => {
            let r = reason::mk_reason(None, loc);
            Ok(any_t::make(
                AnySource::AnyError(Some(type_::AnyErrorKind::MissingAnnotation)),
                r,
            ))
        }
        Binding::Root(Root::For(box (kind, (_, exp_inner)))) => {
            let right_t = expression(
                cx,
                Some(EnclosingContext::OtherTestContext),
                None,
                None,
                exp_inner,
            )?;
            Ok(match kind {
                name_def_types::ForKind::In => {
                    type_operation_utils::type_assertions::assert_for_in_rhs(cx, &right_t)?;
                    type_::str_module_t::at(loc)
                }
                name_def_types::ForKind::Of { await_ } => {
                    // TODO: loc should be loc of loop
                    let reason = reason::mk_reason(None, loc);
                    statement::for_of_elemt(cx, right_t, reason, *await_)?
                }
            })
        }
        Binding::Hooklike(binding) => {
            let t = resolve_binding(cx, def_scope_kind, reason, loc, binding)?;
            Ok(make_hooklike(cx, t))
        }
        Binding::Select {
            selector,
            parent: (parent_loc, binding),
        } => {
            let filtered_pattern_type = cx
                .node_cache()
                .get_match_pattern_value_union(parent_loc)
                .map(|vu| vu.select(cx, selector))
                .transpose()?
                .flatten()
                .map(|value_left| {
                    cx.node_cache_mut()
                        .set_match_pattern_value_union(loc.dupe(), value_left.clone());
                    value_left.to_type(reason.dupe())
                });
            Ok(match filtered_pattern_type {
                Some(t) => t,
                None => {
                    let refined_type = match selector {
                        selector::Selector::Prop { prop, prop_loc, .. } => {
                            let desc = reason::VirtualReasonDesc::RProperty(Some(
                                reason::Name::new(prop.dupe()),
                            ));
                            type_env::get_refinement(cx, desc, prop_loc.dupe())?
                        }
                        _ => None,
                    };
                    match refined_type {
                        // When we can get a refined value on a destructured property,
                        // we must be in an assignment position and the type must have been resolved.
                        Some(t) => t,
                        None => {
                            let t = type_env::checked_find_loc_env_write(
                                cx,
                                env_api::DefLocType::PatternLoc,
                                parent_loc.dupe(),
                            );
                            let annot = binding_has_annot(binding);
                            let (selector_ty, sel_reason, has_default) =
                                mk_selector_reason_has_default(cx, &loc, selector)?;
                            let eval_destruct = |cx: &mut Context<'_>,
                                                 reason: Reason,
                                                 t: Type,
                                                 selector: type_::Selector,
                                                 annot: bool|
                             -> Type {
                                let reason_c = reason.dupe();
                                let selector_c = selector.clone();
                                flow_js_utils::map_on_resolved_type(
                                    cx,
                                    reason.dupe(),
                                    t,
                                    move |cx, t| {
                                        let reason_c2 = reason_c.dupe();
                                        let selector_c2 = selector_c.clone();
                                        tvar_resolver::mk_tvar_and_fully_resolve_no_wrap_where(
                                            cx,
                                            reason_c.dupe(),
                                            |cx, tout_reason, tout_id| {
                                                let tout =
                                                    Tvar::new(tout_reason.dupe(), tout_id as u32);
                                                let id = reason::mk_id() as i32;
                                                match &selector_c2 {
                                                    type_::Selector::Prop(_, true) => {
                                                        // Concretize so eval_selector sees concrete types for the
                                                        // has_default exact-object check (lookup_ub vs getprop_ub).
                                                        for ct in flow_js_exception_to_job_error(
                                                            flow_js::FlowJs::possible_concrete_types_for_destructuring(
                                                                cx, &reason_c2, &t,
                                                            ),
                                                        )?
                                                        {
                                                            flow_js_exception_to_job_error(
                                                                flow_js::FlowJs::eval_selector(
                                                                    cx,
                                                                    None,
                                                                    annot,
                                                                    &reason_c2,
                                                                    &ct,
                                                                    &selector_c2,
                                                                    &tout,
                                                                    id,
                                                                ),
                                                            )?;
                                                        }
                                                    }
                                                    _ => {
                                                        // For other selectors, call eval_selector directly on the
                                                        // original type. The flow mechanism will handle union
                                                        // distribution, matching the old DestructuringT behavior.
                                                        flow_js_exception_to_job_error(
                                                            flow_js::FlowJs::eval_selector(
                                                                cx,
                                                                None,
                                                                annot,
                                                                &reason_c2,
                                                                &t,
                                                                &selector_c2,
                                                                &tout,
                                                                id,
                                                            ),
                                                        )?;
                                                    }
                                                }
                                                Ok::<(), JobError>(())
                                            },
                                        )
                                    },
                                )
                            };
                            let t =
                                eval_destruct(cx, sel_reason.dupe(), t, selector_ty.clone(), annot);
                            if has_default {
                                let (default_selector, default_reason, _) =
                                    mk_selector_reason_has_default(
                                        cx,
                                        &loc,
                                        &selector::Selector::Default,
                                    )?;
                                eval_destruct(cx, default_reason, t, default_selector, annot)
                            } else {
                                t
                            }
                        }
                    }
                }
            })
        }
    }
}

fn resolve_inferred_function<'cx>(
    cx: &mut Context<'cx>,
    scope_kind: ScopeKind,
    statics: &BTreeMap<FlowSmolStr, env_api::EnvKey<ALoc>>,
    namespace_types: &BTreeMap<FlowSmolStr, env_api::EnvKey<ALoc>>,
    needs_this_param: bool,
    id_loc: ALoc,
    reason: Reason,
    function_loc: ALoc,
    function_: &ast::function::Function<ALoc, ALoc>,
) -> Result<Type, CheckExprError> {
    let fn_ = statement::mk_function(
        cx,
        needs_this_param,
        None,
        statics,
        reason.dupe(),
        function_loc.dupe(),
        function_,
    )?;
    let (fun_type, _) = &fn_;
    let fun_type = fun_type.dupe();
    cx.node_cache_mut().set_function(id_loc, fn_);
    if (scope_kind == ScopeKind::ComponentOrHookBody
        || scope_kind == ScopeKind::AsyncComponentOrHookBody)
        && function_.effect_ == ast::function::Effect::Hook
    {
        flow_js_utils::add_output_non_speculating(
            cx,
            flow_typing_errors::error_message::ErrorMessage::ENestedHook(reason.loc().dupe()),
        );
    }
    if function_.async_
        && function_.effect_ == ast::function::Effect::Hook
        && !cx.async_component_syntax()
    {
        flow_js_utils::add_output_non_speculating(
            cx,
            flow_typing_errors::error_message::ErrorMessage::EUnsupportedSyntax(Box::new((
                function_loc.dupe(),
                flow_typing_errors::intermediate_error_types::UnsupportedSyntax::AsyncHookSyntax,
            ))),
        );
    }
    Ok(
        if function_.effect_ != ast::function::Effect::Hook
            && ast_utils::hook_function(function_).is_some()
        {
            let fun_type = make_hooklike(cx, fun_type);
            wrap_function_with_namespace_types(cx, function_.id.as_ref(), fun_type, namespace_types)
        } else {
            wrap_function_with_namespace_types(cx, function_.id.as_ref(), fun_type, namespace_types)
        },
    )
}

fn resolve_class<'cx>(
    cx: &mut Context<'cx>,
    id_loc: ALoc,
    reason: Reason,
    kind: &name_def_types::ClassKind,
    class_loc: ALoc,
    class_: &ast::class::Class<ALoc, ALoc>,
) -> Result<Type, CheckExprError> {
    let inst_kind = match kind {
        name_def_types::ClassKind::Class => type_::InstanceKind::ClassKind,
        name_def_types::ClassKind::Record { defaulted_props } => type_::InstanceKind::RecordKind {
            defaulted_props: defaulted_props.iter().duped().collect(),
        },
    };
    let sig_info =
        statement::mk_class_sig(cx, id_loc, class_loc.dupe(), inst_kind, reason, class_)?;
    let (class_t, class_t_internal, class_sig, reconstruct_fn) = sig_info;
    cx.node_cache_mut().set_class_sig(
        class_loc.dupe(),
        (
            class_t.dupe(),
            class_t_internal.dupe(),
            class_sig,
            reconstruct_fn,
        ),
    );
    type_env::bind_class_self_type(cx, class_t_internal, class_loc);
    Ok(class_t)
}

fn resolve_record<'cx>(
    cx: &mut Context<'cx>,
    id_loc: ALoc,
    reason: Reason,
    record_loc: ALoc,
    defaulted_props: &BTreeSet<FlowSmolStr>,
    record: &ast::statement::RecordDeclaration<ALoc, ALoc>,
) -> Result<Type, CheckExprError> {
    let sig_info = statement::mk_record_sig(
        cx,
        id_loc,
        record_loc.dupe(),
        defaulted_props,
        reason,
        record,
    )?;
    let (record_t, record_t_internal, record_class_sig, record_reconstruct_fn) = sig_info;
    let record_cache_fn: Rc<
        dyn Fn(
            &mut Context<'_>,
            Type,
        ) -> Result<ast::statement::RecordDeclaration<ALoc, (ALoc, Type)>, JobError>,
    > = Rc::new(move |cx: &mut Context<'_>, t| record_reconstruct_fn(cx, t));
    cx.node_cache_mut().set_record_sig(
        record_loc.dupe(),
        (
            record_t.dupe(),
            record_t_internal.dupe(),
            record_class_sig,
            record_cache_fn,
        ),
    );
    type_env::bind_class_self_type(cx, record_t_internal, record_loc);
    Ok(record_t)
}

fn resolve_op_assign<'cx>(
    cx: &mut Context<'cx>,
    exp_loc: ALoc,
    lhs: &ast::pattern::Pattern<ALoc, ALoc>,
    assertion: bool,
    op: ast::expression::AssignmentOperator,
    rhs: &ast::expression::Expression<ALoc, ALoc>,
) -> Result<Type, CheckExprError> {
    let reason = reason::mk_reason(None, exp_loc.dupe());
    match op {
        ast::expression::AssignmentOperator::PlusAssign
        | ast::expression::AssignmentOperator::MinusAssign
        | ast::expression::AssignmentOperator::MultAssign
        | ast::expression::AssignmentOperator::ExpAssign
        | ast::expression::AssignmentOperator::DivAssign
        | ast::expression::AssignmentOperator::ModAssign
        | ast::expression::AssignmentOperator::LShiftAssign
        | ast::expression::AssignmentOperator::RShiftAssign
        | ast::expression::AssignmentOperator::RShift3Assign
        | ast::expression::AssignmentOperator::BitOrAssign
        | ast::expression::AssignmentOperator::BitXorAssign
        | ast::expression::AssignmentOperator::BitAndAssign => {
            // lhs (op)= rhs
            let lhs_patt = statement::assignment_lhs(cx, lhs)?;
            let (lhs_loc, lhs_t) = lhs_patt.loc();
            let lhs_t = if assertion {
                let reason = reason::mk_reason(None, lhs_loc.dupe());
                type_operation_utils::operators::non_maybe(cx, &reason, lhs_t)?
            } else {
                lhs_t.dupe()
            };
            let rhs_t = expression(cx, None, None, None, rhs)?;
            Ok(type_operation_utils::operators::arith(
                cx,
                &reason,
                &type_::arith_kind::ArithKind::of_assignment_operator(op),
                &lhs_t,
                &rhs_t,
            )?)
        }
        ast::expression::AssignmentOperator::AndAssign
        | ast::expression::AssignmentOperator::OrAssign
        | ast::expression::AssignmentOperator::NullishAssign => {
            let lhs_patt = statement::assignment_lhs(cx, lhs)?;
            let (_, lhs_t) = lhs_patt.loc();
            let (rhs_result, right_throws) =
                flow_typing_utils::abnormal::catch_expr_control_flow_exception(|| {
                    statement::expression(None, None, None, cx, rhs)
                })?;
            let rhs_t = rhs_result.loc().1.dupe();
            let rhs_t = if right_throws {
                type_::empty_t::at(exp_loc.dupe())
            } else {
                rhs_t
            };
            Ok(match op {
                ast::expression::AssignmentOperator::NullishAssign => {
                    type_operation_utils::operators::logical_nullish_coalesce(
                        cx, &reason, lhs_t, &rhs_t,
                    )?
                }
                ast::expression::AssignmentOperator::AndAssign => {
                    type_operation_utils::operators::logical_and(cx, &reason, lhs_t, &rhs_t)?
                }
                ast::expression::AssignmentOperator::OrAssign => {
                    type_operation_utils::operators::logical_or(cx, &reason, lhs_t, &rhs_t)?
                }
                _ => unreachable!("Bad conditional guard"),
            })
        }
    }
}

fn resolve_update<'cx>(
    cx: &mut Context<'cx>,
    id_loc: ALoc,
    exp_loc: ALoc,
    id_reason: Reason,
) -> Result<Type, JobError> {
    let reason = reason::mk_reason(None, exp_loc);
    let id_t = type_env::ref_entry_exn(type_env::LookupMode::ForValue, cx, id_loc, id_reason)?;
    type_operation_utils::operators::unary_arith(cx, &reason, &type_::UnaryArithKind::Update, &id_t)
}

fn resolve_type_alias<'cx>(
    cx: &mut Context<'cx>,
    loc: ALoc,
    alias: &ast::statement::TypeAlias<ALoc, ALoc>,
) -> Result<Type, JobError> {
    let (t, ast) = statement::type_alias(cx, loc.dupe(), alias)?;
    cx.node_cache_mut().set_alias(loc, (t.dupe(), ast));
    Ok(t)
}

fn resolve_opaque_type<'cx>(
    cx: &mut Context<'cx>,
    loc: ALoc,
    opaque: &ast::statement::OpaqueType<ALoc, ALoc>,
) -> Result<Type, JobError> {
    let (t, ast) = statement::opaque_type(cx, loc.dupe(), opaque)?;
    cx.node_cache_mut().set_opaque(loc, (t.dupe(), ast));
    Ok(t)
}

fn resolve_import<'cx>(
    cx: &mut Context<'cx>,
    id_loc: ALoc,
    import_reason: Reason,
    import_kind: ast::statement::ImportKind,
    module_name: &FlowSmolStr,
    source_loc: ALoc,
    import: &Import,
) -> Result<Type, JobError> {
    let module_name_userland = flow_import_specifier::Userland::from_smol_str(module_name.dupe());
    let source_module = {
        let import_kind_for_untyped_import_validation = match import_kind {
            ast::statement::ImportKind::ImportType => Some(type_::ImportKind::ImportType),
            ast::statement::ImportKind::ImportTypeof => Some(type_::ImportKind::ImportTypeof),
            ast::statement::ImportKind::ImportValue => Some(type_::ImportKind::ImportValue),
        };
        flow_js_exception_to_job_error(flow_js_utils::import_export_utils::get_module_type_or_any(
            cx,
            false,
            import_kind_for_untyped_import_validation,
            source_loc,
            module_name_userland.dupe(),
        ))?
    };
    Ok(match import {
        Import::Named {
            kind,
            remote,
            local,
        } => {
            let import_kind = kind.unwrap_or(import_kind);
            if files::has_ts_ext(cx.file())
                && import_kind == ast::statement::ImportKind::ImportValue
            {
                cx.add_ts_import_provenance(
                    id_loc.dupe(),
                    flow_import_specifier::FlowImportSpecifier::Userland(
                        module_name_userland.dupe(),
                    ),
                    remote.dupe(),
                );
            }
            let singleton_concretize =
                |cx: &mut Context<'cx>, reason: Reason, t: Type| -> Result<Type, FlowJsException> {
                    flow_js::FlowJs::singleton_concretize_type_for_imports_exports(cx, &reason, &t)
                };
            let (_, t) = flow_js_exception_to_job_error(
                flow_js_utils::import_export_utils::import_named_specifier_type(
                    cx,
                    import_reason,
                    &singleton_concretize,
                    &import_kind,
                    module_name_userland.dupe(),
                    &source_module,
                    remote,
                    local,
                ),
            )?;
            if ast_utils::hook_name(local.as_str()) {
                make_hooklike(cx, t)
            } else {
                t
            }
        }
        Import::Namespace(name) => {
            let namespace_symbol =
                flow_common::flow_symbol::Symbol::mk_namespace_symbol(name.dupe(), id_loc.dupe());
            let t = flow_js_exception_to_job_error(
                flow_js_utils::import_export_utils::import_namespace_specifier_type(
                    cx,
                    import_reason,
                    &import_kind,
                    module_name_userland.dupe(),
                    namespace_symbol,
                    &source_module,
                    id_loc.dupe(),
                ),
            )?;
            if ast_utils::hook_name(name.as_str()) {
                make_hooklike(cx, t)
            } else {
                t
            }
        }
        Import::Default(local_name) => {
            if files::has_ts_ext(cx.file())
                && import_kind == ast::statement::ImportKind::ImportValue
            {
                cx.add_ts_import_provenance(
                    id_loc.dupe(),
                    flow_import_specifier::FlowImportSpecifier::Userland(
                        module_name_userland.dupe(),
                    ),
                    FlowSmolStr::new_inline("default"),
                );
            }
            let singleton_concretize =
                |cx: &mut Context<'cx>, reason: Reason, t: Type| -> Result<Type, FlowJsException> {
                    flow_js::FlowJs::singleton_concretize_type_for_imports_exports(cx, &reason, &t)
                };
            let (_, t) = flow_js_exception_to_job_error(
                flow_js_utils::import_export_utils::import_default_specifier_type(
                    cx,
                    import_reason,
                    &singleton_concretize,
                    &import_kind,
                    module_name_userland.dupe(),
                    &source_module,
                    local_name,
                ),
            )?;
            if ast_utils::hook_name(local_name.as_str()) {
                make_hooklike(cx, t)
            } else {
                t
            }
        }
    })
}

// Interface/declare-class merging copies property maps from the absorbed
// declaration into the canonical declaration. Those copied property types may
// mention the absorbed declaration's local type parameters, so translate them
// into the canonical declaration's GenericTs before mutating the target maps.
fn subst_merge_props<'cx>(
    cx: &mut Context<'cx>,
    tparam_subst_map: &FlowOrdMap<SubstName, Type>,
    props: type_::properties::PropertiesMap,
) -> type_::properties::PropertiesMap {
    if tparam_subst_map.is_empty() {
        return props;
    }
    props.ident_map(|prop| {
        type_::property::map_t(
            |t| {
                type_subst::subst(
                    cx,
                    None,
                    true,
                    false,
                    type_subst::Purpose::Normal,
                    tparam_subst_map,
                    t.dupe(),
                )
            },
            prop,
        )
    })
}

fn merge_props_by_id<'cx>(
    cx: &mut Context<'cx>,
    target_own: &type_::properties::Id,
    target_proto: &type_::properties::Id,
    source_own: &type_::properties::Id,
    source_proto: &type_::properties::Id,
    tparam_subst_map: &FlowOrdMap<SubstName, Type>,
) {
    use flow_typing_type::type_::Property;
    use flow_typing_type::type_::PropertyInner;
    use flow_typing_type::type_::TypeInner;
    use flow_typing_type::type_::inter_rep;
    let target_own_map = cx.find_props(target_own.dupe());
    let source_own_map = subst_merge_props(cx, tparam_subst_map, cx.find_props(source_own.dupe()));
    let mut merged_own = target_own_map.clone();
    for (name, prop) in source_own_map.iter() {
        if !merged_own.contains_key(name) {
            merged_own.insert(name.dupe(), prop.dupe());
        }
    }
    cx.add_property_map(target_own.dupe(), merged_own);
    let target_proto_map = cx.find_props(target_proto.dupe());
    let source_proto_map =
        subst_merge_props(cx, tparam_subst_map, cx.find_props(source_proto.dupe()));
    let mut merged_proto = target_proto_map.clone();
    for (name, source_prop) in source_proto_map.iter() {
        match merged_proto.get(name).cloned() {
            Some(target_prop) => {
                if let (
                    PropertyInner::Method { key_loc, type_: t1 },
                    PropertyInner::Method { type_: t2, .. },
                ) = (&*target_prop, &**source_prop)
                {
                    let reason = type_util::reason_of_t(t1).dupe();
                    let combined = Type::new(TypeInner::IntersectionT(
                        reason,
                        inter_rep::make(t1.dupe(), t2.dupe(), std::rc::Rc::from(Vec::new())),
                    ));
                    merged_proto.insert(
                        name.dupe(),
                        Property::new(PropertyInner::Method {
                            key_loc: key_loc.dupe(),
                            type_: combined,
                        }),
                    );
                }
            }
            None => {
                merged_proto.insert(name.dupe(), source_prop.dupe());
            }
        }
    }
    cx.add_property_map(target_proto.dupe(), merged_proto);
}

fn merge_with_conflicts<'cx>(
    cx: &mut Context<'cx>,
    name_loc: &ALoc,
    merge_conflicts: &flow_data_structure_wrapper::ord_map::FlowOrdMap<ALoc, Vec<ALoc>>,
) {
    let is_good = merge_conflicts.contains_key(name_loc);
    let is_bad = !is_good
        && merge_conflicts
            .iter()
            .any(|(_good, bad_locs)| bad_locs.contains(name_loc));
    if is_good {
        if let Some(bad_locs) = merge_conflicts.get(name_loc) {
            if let Some((good_own, good_proto)) = cx.interface_prop_ids().get(name_loc).cloned() {
                for bad_name_loc in bad_locs {
                    if let Some((bad_own, bad_proto)) =
                        cx.interface_prop_ids().get(bad_name_loc).cloned()
                    {
                        let tparam_subst_map =
                            cx.interface_tparam_subst_map(bad_name_loc, name_loc);
                        merge_props_by_id(
                            cx,
                            &good_own,
                            &good_proto,
                            &bad_own,
                            &bad_proto,
                            &tparam_subst_map,
                        );
                    }
                }
            }
        }
    } else if is_bad {
        for (good_name_loc, bad_locs) in merge_conflicts.iter() {
            if bad_locs.contains(name_loc) {
                if let Some((good_own, good_proto)) =
                    cx.interface_prop_ids().get(good_name_loc).cloned()
                {
                    if let Some((bad_own, bad_proto)) =
                        cx.interface_prop_ids().get(name_loc).cloned()
                    {
                        let tparam_subst_map =
                            cx.interface_tparam_subst_map(name_loc, good_name_loc);
                        merge_props_by_id(
                            cx,
                            &good_own,
                            &good_proto,
                            &bad_own,
                            &bad_proto,
                            &tparam_subst_map,
                        );
                    }
                }
            }
        }
    }
}

fn resolve_interface<'cx>(
    cx: &mut Context<'cx>,
    loc: ALoc,
    inter: &ast::statement::Interface<ALoc, ALoc>,
) -> Result<Type, JobError> {
    let name_loc = inter.id.loc.dupe();
    let (t, ast) = statement::interface(cx, loc.dupe(), inter)?;
    cx.node_cache_mut().set_interface(loc, (t.dupe(), ast));
    // Interface declaration merging. The conflict map (good_name_loc -> [bad_name_locs])
    // built in env_builder drives everything. Prop IDs are registered in mk_interface_sig
    // (type_annotation.rs) when the InstanceT is created. Here we just look them up and merge.
    // If this interface is being absorbed by a `declare class`, route through the
    // declare-class conflict map so the class's InstanceT becomes the merge target.
    let env = cx.environment();
    let dc_conflicts = env.var_info.declare_class_interface_merge_conflicts.dupe();
    let absorbed_by_declare_class = dc_conflicts
        .iter()
        .any(|(_good, bad_locs)| bad_locs.contains(&name_loc));
    if absorbed_by_declare_class {
        merge_with_conflicts(cx, &name_loc, &dc_conflicts);
    } else {
        let conflicts = env.var_info.interface_merge_conflicts.dupe();
        merge_with_conflicts(cx, &name_loc, &conflicts);
    }
    Ok(t)
}

fn resolve_declare_class<'cx>(
    cx: &mut Context<'cx>,
    loc: ALoc,
    class_: &ast::statement::DeclareClass<ALoc, ALoc>,
) -> Result<Type, JobError> {
    let name_loc = class_.id.loc.dupe();
    let (t, ast) = statement::declare_class(cx, loc.dupe(), class_)?;
    cx.node_cache_mut().set_declared_class(loc, (t.dupe(), ast));
    // Declaration merging: a `declare class` and same-name `interface`(s) fold the
    // interface members into the class type. The conflict map built in env_builder
    // keys the canonical class loc to the bad interface locs. Both class and
    // interface prop_ids land in the shared registry from mk_declare_class_sig /
    // mk_interface_sig.
    let conflicts = cx
        .environment()
        .var_info
        .declare_class_interface_merge_conflicts
        .dupe();
    merge_with_conflicts(cx, &name_loc, &conflicts);
    Ok(t)
}

fn resolve_declare_component<'cx>(
    cx: &mut Context<'cx>,
    loc: ALoc,
    component: &ast::statement::DeclareComponent<ALoc, ALoc>,
) -> Result<Type, JobError> {
    let (t, ast) = statement::declare_component(cx, loc.dupe(), component)?;
    cx.node_cache_mut()
        .set_declared_component(loc, (t.dupe(), ast));
    Ok(t)
}

fn resolve_declare_namespace<'cx>(
    cx: &mut Context<'cx>,
    loc: ALoc,
    ns: &ast::statement::DeclareNamespace<ALoc, ALoc>,
) -> Result<Type, JobError> {
    let (t, ast) = statement::declare_namespace(cx, loc.dupe(), ns)?;
    cx.node_cache_mut()
        .set_declared_namespace(loc, (t.dupe(), ast));
    Ok(t)
}

fn resolve_enum<'cx>(
    cx: &mut Context<'cx>,
    id_loc: ALoc,
    declared: bool,
    enum_reason: Reason,
    enum_loc: ALoc,
    name: &FlowSmolStr,
    enum_: &ast::statement::enum_declaration::Body<ALoc>,
) -> Type {
    if flow_common::files::has_ts_ext(cx.file()) {
        // TS enum: object value of literal members + union type (see ts_enum.rs).
        flow_typing_flow_common::ts_enum::mk_ts_enum_namespace(
            cx,
            declared,
            enum_reason,
            id_loc,
            name.dupe(),
            enum_,
        )
    } else if cx.enable_enums() {
        let enum_info = statement::mk_enum(cx, enum_reason.dupe(), id_loc, name, enum_);
        type_::mk_enum_object_type(
            enum_reason,
            type_::EnumInfo::new(type_::EnumInfoInner::ConcreteEnum(
                type_::EnumConcreteInfo::new(enum_info),
            ))
            .into(),
        )
    } else {
        flow_js_utils::add_output_non_speculating(
            cx,
            flow_typing_errors::error_message::ErrorMessage::EEnumError(
                flow_typing_errors::error_message::EnumErrorKind::EnumsNotEnabled(enum_loc),
            ),
        );
        any_t::error(enum_reason)
    }
}

fn resolve_type_param<'cx>(cx: &mut Context<'cx>, id_loc: &ALoc) -> Type {
    let env = cx.environment();
    let (_, _, t) = env.tparams.get_ordinary(id_loc).expect("tparam not found");
    let reason = type_util::reason_of_t(t).dupe();
    Type::new(TypeInner::DefT(
        reason,
        DefT::new(DefTInner::TypeT(type_::TypeTKind::TypeParamKind, t.dupe())),
    ))
}

fn resolve_chain_expression<'cx>(
    cx: &mut Context<'cx>,
    cond: EnclosingContext,
    exp: &ast::expression::Expression<ALoc, ALoc>,
) -> Result<Type, CheckExprError> {
    let (t, _, exp_typed) = statement::optional_chain(cond, cx, exp)?;
    cx.node_cache_mut().set_expression(exp_typed);
    Ok(t)
}

fn resolve_write_expression<'cx>(
    cx: &mut Context<'cx>,
    cond: EnclosingContext,
    exp: &ast::expression::Expression<ALoc, ALoc>,
) -> Result<Type, CheckExprError> {
    synthesizable_expression(cx, cond, exp)
}

fn resolve_match_pattern<'cx>(
    cx: &mut Context<'cx>,
    def_reason: Reason,
    case_match_root_loc: ALoc,
    has_guard: bool,
    prev_pattern_loc: Option<ALoc>,
    pattern: &ast::match_pattern::MatchPattern<ALoc, ALoc>,
) -> Result<Type, CheckExprError> {
    let typed_pattern = statement::match_pattern(cx, case_match_root_loc, has_guard, pattern)?;
    let pattern_loc = typed_pattern.loc().dupe();
    // Set the match pattern union cache for incremental PatternUnion building.
    // This allows subsequent patterns to depend on this one and build incrementally.
    let pattern_union_state = get_pattern_union_state_from_prev(cx, prev_pattern_loc)?;
    cx.node_cache_mut().set_match_pattern_union(
        pattern_loc,
        (pattern_union_state.0, pattern_union_state.1 as i32),
    );
    Ok(type_::mixed_t::why(def_reason))
}

fn resolve_generator_next<'cx>(
    cx: &mut Context<'cx>,
    reason: Reason,
    gen_info: Option<&name_def_types::GeneratorAnnot>,
) -> Result<Type, JobError> {
    match gen_info {
        None => {
            let r = reason.dupe().replace_desc(reason::VirtualReasonDesc::RVoid);
            Ok(type_::void::make(r))
        }
        Some(gen_info) => {
            let tparams_map = mk_tparams_map(cx, &gen_info.tparams_map);
            let (t, anno) = type_annotation::mk_type_available_annotation(
                cx,
                tparams_map,
                &ast::types::Annotation {
                    loc: gen_info.return_annot.0.dupe(),
                    annotation: gen_info.return_annot.1.dupe(),
                },
                BindsSingleValue::No,
            )?;
            cx.node_cache_mut().set_annotation(anno);
            let return_t = t;
            let gen_name = if gen_info.async_ {
                "AsyncGenerator"
            } else {
                "Generator"
            };
            flow_typing_tvar::mk_where(cx, reason.dupe(), |cx, next| {
                let yield_reason = reason.dupe().replace_desc(None);
                let return_reason = reason.dupe().replace_desc(None);
                let yield_t = any_t::placeholder(yield_reason);
                let return_targ = any_t::placeholder(return_reason);
                let t = flow_js::FlowJs::get_builtin_typeapp(
                    cx,
                    &reason,
                    None,
                    gen_name,
                    vec![yield_t, return_targ, next.dupe()],
                );
                let return_t_reason = type_util::reason_of_t(&return_t);
                let return_t_loc = return_t_reason.loc().dupe();
                let t = flow_js::reposition_non_speculating(cx, return_t_loc, t)?;
                flow_js::flow_t_non_speculating(cx, (&t, &return_t))?;
                Ok(())
            })
        }
    }
}

fn resolve<'cx>(
    cx: &mut Context<'cx>,
    def_kind: env_api::DefLocType,
    id_loc: ALoc,
    def: &Def,
    def_scope_kind: ScopeKind,
    class_stack: &name_def_types::ClassStack,
    def_reason: Reason,
) -> Result<(), flow_typing_utils::abnormal::CheckExprError> {
    {
        let env = cx.environment_mut();
        env.scope_kind = def_scope_kind;
        env.class_stack = class_stack.dupe();
    }
    let t = match def {
        Def::Binding(b) => {
            resolve_binding(cx, def_scope_kind, def_reason.dupe(), id_loc.dupe(), b)?
        }
        Def::MatchCasePattern(box MatchCasePatternData {
            case_match_root_loc,
            has_guard,
            pattern,
            prev_pattern_loc,
        }) => resolve_match_pattern(
            cx,
            def_reason,
            case_match_root_loc.dupe(),
            *has_guard,
            prev_pattern_loc.dupe(),
            &pattern.1,
        )?,
        Def::ExpressionDef(box name_def_types::ExpressionDef {
            cond_context,
            expr,
            chain: true,
            ..
        }) => resolve_chain_expression(cx, cond_context.dupe(), expr)?,
        Def::ExpressionDef(box name_def_types::ExpressionDef {
            cond_context,
            expr,
            chain: false,
            ..
        }) => resolve_write_expression(cx, cond_context.dupe(), expr)?,
        Def::Component(box ComponentDefData {
            component,
            component_loc,
            tparams_map,
        }) => resolve_annotated_component(
            cx,
            def_scope_kind,
            def_reason,
            tparams_map,
            component_loc.dupe(),
            component,
        )?,
        Def::Function(box FunctionDefData {
            function_,
            synthesizable_from_annotation: name_def_types::FunctionSynthKind::FunctionSynthesizable,
            arrow,
            has_this_def: _,
            function_loc,
            tparams_map,
            statics,
            namespace_types,
            hints: _,
        }) => {
            let hook_like = ast_utils::hook_function(function_).is_some();
            resolve_annotated_function(
                cx,
                def_scope_kind,
                !*arrow,
                statics,
                namespace_types,
                hook_like,
                def_reason,
                tparams_map,
                function_loc.dupe(),
                function_,
            )?
        }
        Def::Function(box FunctionDefData {
            function_,
            synthesizable_from_annotation: _,
            arrow,
            has_this_def: _,
            function_loc,
            tparams_map: _,
            statics,
            namespace_types,
            hints: _,
        }) => resolve_inferred_function(
            cx,
            def_scope_kind,
            statics,
            namespace_types,
            !*arrow,
            id_loc.dupe(),
            def_reason,
            function_loc.dupe(),
            function_,
        )?,
        Def::DeclaredFunction(box name_def_types::DeclaredFunctionDefData {
            declarations,
            statics,
            namespace_types,
        }) => resolve_declared_function(cx, declarations, statics, namespace_types)?,
        Def::Class(box ClassDefData {
            class_,
            class_loc,
            kind,
            this_super_write_locs: _,
            namespace_members,
        }) => {
            let class_t = resolve_class(
                cx,
                id_loc.dupe(),
                def_reason,
                kind,
                class_loc.dupe(),
                class_,
            )?;
            extend_class_statics(cx, &class_t, &namespace_members.values);
            if namespace_members.types.is_empty() {
                class_t
            } else {
                let (name, name_loc) = match &class_.id {
                    Some(id) => (id.name.dupe(), id.loc.dupe()),
                    None => (FlowSmolStr::new(""), class_loc.dupe()),
                };
                wrap_with_namespace_types(cx, &name, name_loc, class_t, &namespace_members.types)
            }
        }
        Def::Record(box RecordDefData {
            record,
            record_loc,
            this_super_write_locs: _,
            defaulted_props,
        }) => resolve_record(
            cx,
            id_loc.dupe(),
            def_reason,
            record_loc.dupe(),
            defaulted_props,
            record,
        )?,
        Def::MemberAssign(box MemberAssignData {
            member_loc: _,
            member: _,
            rhs,
        }) => expression(cx, None, None, None, &rhs.1)?,
        Def::OpAssign(box OpAssignData {
            exp_loc,
            lhs,
            op,
            rhs,
            assertion,
        }) => resolve_op_assign(cx, exp_loc.dupe(), &lhs.1, *assertion, *op, &rhs.1)?,
        Def::Update { exp_loc, op: _ } => {
            resolve_update(cx, id_loc.dupe(), exp_loc.dupe(), def_reason)?
        }
        Def::TypeAlias(loc, alias) => resolve_type_alias(cx, loc.dupe(), alias)?,
        Def::OpaqueType(loc, opaque) => resolve_opaque_type(cx, loc.dupe(), opaque)?,
        Def::Import(box ImportData {
            import_kind,
            source,
            source_loc,
            import,
        }) => resolve_import(
            cx,
            id_loc.dupe(),
            def_reason,
            *import_kind,
            source,
            source_loc.dupe(),
            import,
        )?,
        Def::Interface(loc, inter) => resolve_interface(cx, loc.dupe(), inter)?,
        Def::DeclaredClass(box DeclaredClassDefData {
            loc,
            decl,
            namespace_members,
        }) => {
            let class_t = resolve_declare_class(cx, loc.dupe(), decl)?;
            extend_class_statics(cx, &class_t, &namespace_members.values);
            let id_name = &decl.id.name;
            let id_loc = decl.id.loc.dupe();
            wrap_with_namespace_types(cx, id_name, id_loc, class_t, &namespace_members.types)
        }
        Def::DeclaredComponent(loc, comp) => resolve_declare_component(cx, loc.dupe(), comp)?,
        Def::Enum {
            loc: enum_loc,
            name,
            declared,
            body: enum_,
        } => resolve_enum(
            cx,
            id_loc.dupe(),
            *declared,
            def_reason,
            enum_loc.dupe(),
            name,
            enum_,
        ),
        Def::TypeParam(_) => resolve_type_param(cx, &id_loc),
        Def::GeneratorNext(gen_info) => {
            resolve_generator_next(cx, def_reason, (**gen_info).as_ref())?
        }
        Def::DeclaredNamespace(loc, ns) => resolve_declare_namespace(cx, loc.dupe(), ns)?,
        Def::MissingThisAnnot => any_t::at(AnySource::AnyError(None), id_loc.dupe()),
    };

    fn has_array_or_object_without_hint(expr: &ast::expression::Expression<ALoc, ALoc>) -> bool {
        match expr.deref() {
            ast::expression::ExpressionInner::Array { .. }
            | ast::expression::ExpressionInner::Object { .. } => true,
            ast::expression::ExpressionInner::Conditional { inner, .. } => {
                let ast::expression::Conditional {
                    consequent,
                    alternate,
                    ..
                } = inner.as_ref();
                has_array_or_object_without_hint(consequent)
                    || has_array_or_object_without_hint(alternate)
            }
            _ => false,
        }
    }
    let add_array_or_object_literal_declaration_tracking = match def {
        Def::Binding(box Binding::Root(Root::ObjectValue(_))) => true,
        Def::Binding(box Binding::Root(Root::EmptyArray(_))) => true,
        Def::Binding(box Binding::Root(Root::Value(box name_def_types::Value {
            hints: _,
            expr,
            decl_kind: Some(_),
            as_const: _,
        }))) if has_array_or_object_without_hint(expr) => true,
        _ => false,
    };
    flow_typing_debug::verbose::print_if_verbose_lazy(cx, None, None, None, || {
        vec![format!(
            "Setting variable at {} to {}",
            id_loc.debug_to_string(true),
            flow_typing_debug::dump_t(None, cx, &t),
        )]
    });
    type_env::resolve_env_entry(
        cx,
        t,
        def_kind,
        add_array_or_object_literal_declaration_tracking,
        id_loc,
    );
    Ok(())
}

fn entries_of_def(
    graph: &name_def_types::EnvEntriesMap,
    kind: env_api::DefLocType,
    loc: ALoc,
) -> env_api::EnvSet<ALoc> {
    let mut acc = env_api::EnvSet::empty();
    acc.insert(env_api::EnvKey::new(kind, loc.dupe()));

    fn add_from_bindings(acc: &mut env_api::EnvSet<ALoc>, b: &Binding) {
        match b {
            Binding::Root(Root::UnannotatedParameter(r)) => {
                acc.insert(env_api::EnvKey::new(
                    env_api::DefLocType::FunctionParamLoc,
                    r.loc().dupe(),
                ));
            }
            Binding::Root(Root::Annotation(box AnnotationData {
                param_loc: Some(l), ..
            })) => {
                acc.insert(env_api::EnvKey::new(
                    env_api::DefLocType::FunctionParamLoc,
                    l.dupe(),
                ));
            }
            Binding::Root(Root::Contextual(box ContextualData { reason, .. })) => {
                let l = reason.loc().dupe();
                acc.insert(env_api::EnvKey::new(
                    env_api::DefLocType::FunctionParamLoc,
                    l,
                ));
            }
            Binding::Root(Root::FunctionValue(box FunctionValueData {
                function_loc,
                arrow: false,
                function_,
                ..
            })) if function_.params.this_.is_none() => {
                acc.insert(env_api::EnvKey::new(
                    env_api::DefLocType::FunctionThisLoc,
                    function_loc.dupe(),
                ));
            }
            Binding::Root(Root::ObjectValue(box ObjectValueData {
                synthesizable:
                    name_def_types::ObjectSynthKind::ObjectSynthesizable { this_write_locs },
                ..
            })) => {
                for entry in this_write_locs.iter() {
                    acc.insert(entry.dupe());
                }
            }
            Binding::Hooklike(bind) => {
                add_from_bindings(acc, bind);
            }
            Binding::Root(_) => {}
            Binding::Select { .. } => {}
        }
    }
    if let Some((def, _, _, _, _)) = graph.get(&env_api::EnvKey::new(kind, loc)) {
        match def {
            Def::Binding(b) => {
                add_from_bindings(&mut acc, b);
            }
            Def::Class(box ClassDefData {
                this_super_write_locs,
                ..
            }) => {
                for entry in this_super_write_locs.iter() {
                    acc.insert(entry.dupe());
                }
            }
            Def::Record(box RecordDefData {
                this_super_write_locs,
                ..
            }) => {
                for entry in this_super_write_locs.iter() {
                    acc.insert(entry.dupe());
                }
            }
            Def::Function(box FunctionDefData {
                has_this_def: true,
                function_loc,
                function_,
                ..
            }) if function_.params.this_.is_none() => {
                acc.insert(env_api::EnvKey::new(
                    env_api::DefLocType::FunctionThisLoc,
                    function_loc.dupe(),
                ));
            }
            _ => {}
        }
    }
    acc
}

fn entries_of_component(
    graph: &name_def_types::EnvEntriesMap,
    component: &name_def_ordering::OrderingResult,
) -> env_api::EnvSet<ALoc> {
    fn element_key(elt: &name_def_ordering::Element) -> (env_api::DefLocType, ALoc) {
        match elt {
            name_def_ordering::Element::Normal(key)
            | name_def_ordering::Element::Resolvable(key) => (key.def_loc_type, key.loc.dupe()),
            name_def_ordering::Element::Illegal(blame) => {
                (blame.payload.def_loc_type, blame.payload.loc.dupe())
            }
        }
    }

    match component {
        name_def_ordering::OrderingResult::Singleton(elt) => {
            let (kind, loc) = element_key(elt);
            entries_of_def(graph, kind, loc)
        }
        name_def_ordering::OrderingResult::ResolvableSCC(elts) => {
            let mut acc = env_api::EnvSet::empty();
            for elt in elts.iter() {
                let (kind, loc) = element_key(elt);
                for entry in entries_of_def(graph, kind, loc).iter() {
                    acc.insert(entry.dupe());
                }
            }
            acc
        }
        name_def_ordering::OrderingResult::IllegalSCC(elts) => {
            let mut acc = env_api::EnvSet::empty();
            for (blame, _) in elts.iter() {
                let (kind, loc) = element_key(&blame.payload);
                for entry in entries_of_def(graph, kind, loc).iter() {
                    acc.insert(entry.dupe());
                }
            }
            acc
        }
    }
}

fn init_type_param<'cx>(
    cx: &mut Context<'cx>,
    graph: &name_def_types::EnvEntriesMap,
    def_loc: ALoc,
) -> Result<(SubstName, type_::TypeParam, Type), JobError> {
    fn get_type_param<'cx>(
        cx: &mut Context<'cx>,
        graph: &name_def_types::EnvEntriesMap,
        l: ALoc,
    ) -> Result<(SubstName, type_::TypeParam, Type), JobError> {
        let cached = {
            let env = cx.environment();
            env.tparams.get_ordinary(&l).map(|entry| entry.dupe())
        };
        match cached {
            Some(entry) => Ok(entry),
            None => init_type_param(cx, graph, l),
        }
    }

    fn mk_tparams_map_from_graph<'cx>(
        cx: &mut Context<'cx>,
        graph: &name_def_types::EnvEntriesMap,
        tparams_map: &FlowOrdMap<ALoc, FlowSmolStr>,
    ) -> Result<FlowOrdMap<SubstName, Type>, JobError> {
        let mut subst_map = FlowOrdMap::new();
        for (l, _) in tparams_map.iter() {
            let (name, _, ty) = get_type_param(cx, graph, l.dupe())?;
            subst_map.insert(name, ty);
        }
        Ok(subst_map)
    }

    fn mk_synthetic_this<'cx>(cx: &mut Context<'cx>, reason: Reason) -> (type_::TypeParam, Type) {
        flow_typing_statement::class_sig::mk_this(type_::mixed_t::make(reason.dupe()), cx, reason)
    }
    let tparam_entry = match graph.get_ordinary(&def_loc) {
        Some((
            Def::TypeParam(box TypeParamData {
                tparams_map: tparams_locs,
                kind,
                tparam,
            }),
            _,
            _,
            _,
            _,
        )) => {
            let tparams_map = mk_tparams_map_from_graph(cx, graph, tparams_locs)?;
            let (_, tparam_inner) = tparam;
            let info = type_annotation::mk_type_param(cx, tparams_map, *kind, tparam_inner)?;
            let (_, ref tparam_result, ref t) = info;
            let result = (tparam_result.name.dupe(), tparam_result.dupe(), t.dupe());
            cx.node_cache_mut().set_tparam(info);
            result
        }
        Some((Def::Class(box ClassDefData { class_loc, .. }), _, _, reason, _)) => {
            let self_ = type_env::read_class_self_type(cx, class_loc.dupe());
            let (this_param, this_t) =
                flow_typing_statement::class_sig::mk_this(self_, cx, reason.dupe());
            (
                SubstName::name(FlowSmolStr::new("this")),
                this_param,
                this_t,
            )
        }
        Some((Def::Record(box RecordDefData { record_loc, .. }), _, _, reason, _)) => {
            let self_ = type_env::read_class_self_type(cx, record_loc.dupe());
            let (this_param, this_t) =
                flow_typing_statement::class_sig::mk_this(self_, cx, reason.dupe());
            (
                SubstName::name(FlowSmolStr::new("this")),
                this_param,
                this_t,
            )
        }
        Some((Def::Interface(_, _), _, _, reason, _)) => {
            // Interfaces have polymorphic [this]. The bound used here doesn't
            // need to match the interface's later [this_t] identity — [tparams_map]
            // lookups during convert just need a usable type so [extends this]
            // in a method's tparam constraint resolves rather than firing
            // [illegal-this]. The actual subtyping at use-sites goes through the
            // interface's own [this_t] via [check_implements] / [supertype]
            // substitution.
            let (this_param, this_t) = mk_synthetic_this(cx, reason.dupe());
            (
                SubstName::name(FlowSmolStr::new("this")),
                this_param,
                this_t,
            )
        }
        Some(_) => {
            panic!(
                "tparam_locs contain a non-tparam location: {:?}",
                def_loc.debug_to_string(true)
            )
        }
        None => {
            panic!(
                "init_type_param: tparams_locs entry {:?} has no name_def binding (likely a synthetic [this] whose AssigningWrite was not registered by the env_builder)",
                def_loc.debug_to_string(true)
            )
        }
    };
    cx.environment_mut()
        .tparams
        .insert(env_api::EnvKey::ordinary(def_loc), tparam_entry.dupe());
    Ok(tparam_entry)
}

fn resolve_component_type_params<'cx>(
    cx: &mut Context<'cx>,
    graph: &name_def_types::EnvEntriesMap,
    component: &name_def_ordering::OrderingResult,
) -> Result<(), JobError> {
    let resolve_illegal = |cx: &mut Context<'cx>, loc: ALoc, def: &name_def_types::NameDefEntry| {
        let (def, _, _, _, _) = def;
        match def {
            Def::TypeParam(box TypeParamData {
                tparam: (_, tparam),
                ..
            }) => {
                let name_loc = tparam.name.loc.dupe();
                let str_name = tparam.name.name.dupe();
                let name = SubstName::name(str_name.dupe());
                let tp_reason = reason::mk_annot_reason(
                    reason::VirtualReasonDesc::RType(str_name),
                    name_loc.dupe(),
                );
                let tp = type_::TypeParam::new(type_::TypeParamInner {
                    reason: tp_reason.dupe(),
                    name: name.dupe(),
                    bound: Type::new(TypeInner::DefT(
                        tp_reason.dupe(),
                        DefT::new(DefTInner::MixedT(type_::MixedFlavor::MixedEverything)),
                    )),
                    polarity: flow_common::polarity::Polarity::Neutral,
                    default: None,
                    is_this: false,
                    is_const: false,
                });
                cx.environment_mut().tparams.insert(
                    env_api::EnvKey::ordinary(loc.dupe()),
                    (name, tp, any_t::at(AnySource::AnyError(None), loc)),
                );
            }
            Def::Class(_) => {
                let name = SubstName::name(FlowSmolStr::new("this"));
                let tp_reason =
                    reason::mk_annot_reason(reason::VirtualReasonDesc::RThis, loc.dupe());
                let tp = type_::TypeParam::new(type_::TypeParamInner {
                    reason: tp_reason.dupe(),
                    name: name.dupe(),
                    bound: Type::new(TypeInner::DefT(
                        tp_reason.dupe(),
                        DefT::new(DefTInner::MixedT(type_::MixedFlavor::MixedEverything)),
                    )),
                    polarity: flow_common::polarity::Polarity::Neutral,
                    default: None,
                    is_this: true,
                    is_const: false,
                });
                cx.environment_mut().tparams.insert(
                    env_api::EnvKey::ordinary(loc.dupe()),
                    (name, tp, any_t::at(AnySource::AnyError(None), loc)),
                );
            }
            Def::Record(_) => {
                let name = SubstName::name(FlowSmolStr::new("this"));
                let tp_reason =
                    reason::mk_annot_reason(reason::VirtualReasonDesc::RThis, loc.dupe());
                let tp = type_::TypeParam::new(type_::TypeParamInner {
                    reason: tp_reason.dupe(),
                    name: name.dupe(),
                    bound: Type::new(TypeInner::DefT(
                        tp_reason.dupe(),
                        DefT::new(DefTInner::MixedT(type_::MixedFlavor::MixedEverything)),
                    )),
                    polarity: flow_common::polarity::Polarity::Neutral,
                    default: None,
                    is_this: true,
                    is_const: false,
                });
                cx.environment_mut().tparams.insert(
                    env_api::EnvKey::ordinary(loc.dupe()),
                    (name, tp, any_t::at(AnySource::AnyError(None), loc)),
                );
            }
            _ => {}
        }
    };

    let cyclic_type_param_locs = cx.environment().var_info.cyclic_type_param_locs.dupe();
    let seed_cyclic_type_param = |cx: &mut Context<'cx>, elt: &name_def_ordering::Element| {
        let key = match elt {
            name_def_ordering::Element::Normal(key)
            | name_def_ordering::Element::Resolvable(key) => key,
            name_def_ordering::Element::Illegal(_) => return,
        };
        if cyclic_type_param_locs.contains(&key.loc)
            && let Some(def_data) = graph.get(key)
        {
            resolve_illegal(cx, key.loc.dupe(), def_data);
        }
    };
    match component {
        name_def_ordering::OrderingResult::Singleton(elt) => seed_cyclic_type_param(cx, elt),
        name_def_ordering::OrderingResult::ResolvableSCC(elts) => {
            for elt in elts.iter() {
                seed_cyclic_type_param(cx, elt);
            }
        }
        name_def_ordering::OrderingResult::IllegalSCC(_) => {}
    }

    let resolve_element =
        |cx: &mut Context<'cx>, elt: &name_def_ordering::Element| -> Result<(), JobError> {
            match elt {
                name_def_ordering::Element::Illegal(blame) => {
                    let key = &blame.payload;
                    let loc = key.loc.dupe();
                    if let Some(def_data) = graph.get(key) {
                        resolve_illegal(cx, loc, def_data);
                    }
                }
                name_def_ordering::Element::Normal(key)
                | name_def_ordering::Element::Resolvable(key) => {
                    if let Some((def, _, _, _, _)) = graph.get(key)
                        && let Def::TypeParam(_) | Def::Class(_) | Def::Record(_) = def
                    {
                        init_type_param(cx, graph, key.loc.dupe())?;
                    }
                }
            }
            Ok(())
        };

    match component {
        name_def_ordering::OrderingResult::IllegalSCC(elts) => {
            for (blame, _) in elts.iter() {
                let key = match &blame.payload {
                    name_def_ordering::Element::Illegal(inner_blame) => &inner_blame.payload,
                    name_def_ordering::Element::Normal(key)
                    | name_def_ordering::Element::Resolvable(key) => key,
                };
                let loc = key.loc.dupe();
                if let Some(def_data) = graph.get(key) {
                    resolve_illegal(cx, loc, def_data);
                }
            }
        }
        name_def_ordering::OrderingResult::Singleton(elt) => resolve_element(cx, elt)?,
        name_def_ordering::OrderingResult::ResolvableSCC(elts) => {
            for elt in elts.iter() {
                resolve_element(cx, elt)?;
            }
        }
    }
    Ok(())
}

pub fn resolve_component<'cx>(
    cx: &mut Context<'cx>,
    graph: &name_def_types::EnvEntriesMap,
    component: &name_def_ordering::OrderingResult,
) -> Result<(), JobError> {
    cx.constraint_cache_mut().clear();
    cx.eval_repos_cache_mut().clear();

    let resolve_illegal = |cx: &mut Context<'cx>, entries: &env_api::EnvSet<ALoc>| {
        for entry in entries.iter() {
            let kind = entry.def_loc_type;
            let loc = entry.loc.dupe();
            type_env::resolve_env_entry(
                cx,
                any_t::at(AnySource::AnyError(None), loc.dupe()),
                kind,
                false,
                loc,
            );
        }
    };

    let resolve_element =
        |cx: &mut Context<'cx>, elt: &name_def_ordering::Element| -> Result<(), JobError> {
            match elt {
                name_def_ordering::Element::Illegal(blame) => {
                    let key = &blame.payload;
                    resolve_illegal(cx, &entries_of_def(graph, key.def_loc_type, key.loc.dupe()));
                }
                name_def_ordering::Element::Normal(key)
                | name_def_ordering::Element::Resolvable(key) => {
                    if let Some((def, scope_kind, class_stack, reason, _)) = graph.get(key) {
                        flow_typing_utils::abnormal::try_with_abnormal_exn(
                            || {
                                resolve(
                                    cx,
                                    key.def_loc_type,
                                    key.loc.dupe(),
                                    def,
                                    *scope_kind,
                                    class_stack,
                                    reason.dupe(),
                                )
                            },
                            // When there is an unhandled exception, it means that the initialization of the env slot
                            // won't be completed and will never be written in the new-env, so it's OK to do nothing.
                            |_| (),
                        )?;
                    }
                }
            }
            Ok(())
        };

    flow_typing_debug::verbose::print_if_verbose_lazy(cx, None, None, None, || {
        vec![format!(
            "Resolving component {}",
            name_def_ordering::string_of_component(graph, component),
        )]
    });

    let log_slow_to_check = |cx: &mut Context<'cx>,
                             f: &mut dyn FnMut(&mut Context<'cx>) -> Result<(), JobError>|
     -> Result<(), JobError> {
        let slow_to_check = cx.slow_to_check_logging();
        match slow_to_check.slow_components_logging_threshold {
            Some(threshold) => {
                let start_time = std::time::Instant::now();
                f(cx)?;
                let run_time = start_time.elapsed().as_secs_f64();
                if run_time > threshold {
                    flow_hh_logger::info!(
                        "[{}] Slow CHECK component {} ({} seconds)",
                        std::process::id(),
                        name_def_ordering::string_of_component(graph, component),
                        run_time,
                    );
                }
                Ok(())
            }
            None => f(cx),
        }
    };

    let mut f = |cx: &mut Context<'cx>| -> Result<(), JobError> {
        let entries_for_resolution = entries_of_component(graph, component);
        type_env::make_env_entries_under_resolution(cx, entries_for_resolution.dupe());

        resolve_component_type_params(cx, graph, component)?;

        match component {
            name_def_ordering::OrderingResult::IllegalSCC(_) => {
                resolve_illegal(cx, &entries_for_resolution)
            }
            name_def_ordering::OrderingResult::Singleton(elt) => resolve_element(cx, elt)?,
            name_def_ordering::OrderingResult::ResolvableSCC(elts) => {
                for elt in elts.iter() {
                    resolve_element(cx, elt)?;
                }
            }
        }

        flow_typing_debug::verbose::print_if_verbose_lazy(cx, None, None, None, || {
            vec!["Finished resolving component".to_string()]
        });
        flow_typing_debug::verbose::print_if_verbose_lazy(cx, None, None, None, || {
            vec!["Forcing all lazy tvars after resolving component".to_string()]
        });
        for entry in entries_for_resolution.iter() {
            let kind = entry.def_loc_type;
            let loc = entry.loc.dupe();
            let tvar_id = match cx.environment().find_write(kind, loc) {
                Some(type_entry) => match type_entry.t.deref() {
                    TypeInner::OpenT(tvar) => Some(tvar.id() as i32),
                    _ => None,
                },
                None => None,
            };
            if let Some(id) = tvar_id {
                let fully_resolved = cx.inspect_constraints(id, |root_id, constraints| {
                    if let type_::constraint::Constraints::FullyResolved(s) = constraints {
                        Some((root_id, s.clone()))
                    } else {
                        None
                    }
                });
                if let Some((root_id, s)) = fully_resolved {
                    cx.add_post_component_tvar_forcing_state(root_id, s);
                }
            }
        }
        cx.force_post_component_tvars();
        flow_typing_debug::verbose::print_if_verbose_lazy(cx, None, None, None, || {
            vec!["Forced all lazy tvars after resolving component".to_string()]
        });
        Ok(())
    };

    log_slow_to_check(cx, &mut f)
}
