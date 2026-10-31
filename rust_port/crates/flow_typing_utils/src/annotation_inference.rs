/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::convert::Infallible;
use std::ops::Deref;
use std::rc::Rc;

use dupe::Dupe;
use flow_aloc::ALoc;
use flow_common::reason::Name;
use flow_common::reason::Reason;
use flow_common::reason::mk_id;
use flow_data_structure_wrapper::ord_set::FlowOrdSet;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_typing_context::Context;
use flow_typing_context::ContextId;
use flow_typing_context::Cx;
use flow_typing_context::LazyType;
use flow_typing_context::elab_open_memo_key;
use flow_typing_errors::error_message::EAnnotationInferenceData;
use flow_typing_errors::error_message::EMissingTypeArgsData;
use flow_typing_errors::error_message::EPropNotFoundInLookupData;
use flow_typing_errors::error_message::EnumInvalidMemberAccessData;
use flow_typing_errors::error_message::EnumMemberAccess;
use flow_typing_errors::error_message::EnumMemberUsedAsTypeData;
use flow_typing_errors::error_message::EnumReferenceData;
use flow_typing_errors::error_message::ErrorMessage;
use flow_typing_errors::error_message::IncompatibleUpperData;
use flow_typing_flow_common::flow_js_utils;
use flow_typing_flow_common::flow_js_utils::CgLookupArgs;
use flow_typing_flow_common::flow_js_utils::FlowJsException;
use flow_typing_flow_common::flow_js_utils::get_prop_t_kit::IndexerFallbackMode;
use flow_typing_flow_js::intersection_kit::Normalization;
use flow_typing_flow_js::intersection_kit::Normalizer;
use flow_typing_flow_js::object_kit::slice_of_keys;
use flow_typing_flow_js::object_kit::split_mapped_type_keys;
use flow_typing_flow_js::slice_utils;
use flow_typing_flow_js_env::FlowJsEnv;
use flow_typing_type::type_;
use flow_typing_type::type_::ArrayATData;
use flow_typing_type::type_::DepthTrace;
use flow_typing_type::type_::Destructor;
use flow_typing_type::type_::DestructorSpreadTypeData;
use flow_typing_type::type_::ROArrayATData;
use flow_typing_type::type_::ThisInstanceTData;
use flow_typing_type::type_::ThisTypeAppTData;
use flow_typing_type::type_::TupleATData;
use flow_typing_type::type_::Type;
use flow_typing_type::type_::TypeAppTData;
use flow_typing_type::type_::TypeInner;
use flow_typing_type::type_::TypeMap;
use flow_typing_type::type_::TypeTKind;
use flow_typing_type::type_::UseOp;
use flow_typing_type::type_::aconstraint::AConstraintInner;
use flow_typing_type::type_::aconstraint::AnnotArithTData;
use flow_typing_type::type_::aconstraint::AnnotConcretizeForImportsExportsFn;
use flow_typing_type::type_::aconstraint::AnnotDeepReadOnlyTData;
use flow_typing_type::type_::aconstraint::AnnotGetPropTData;
use flow_typing_type::type_::aconstraint::AnnotGetTypeFromNamespaceTData;
use flow_typing_type::type_::aconstraint::AnnotLookupTData;
use flow_typing_type::type_::aconstraint::AnnotObjKitTData;
use flow_typing_type::type_::aconstraint::AnnotSpecializeTData;
use flow_typing_type::type_::aconstraint::AnnotationInferenceOperation;
use flow_typing_type::type_::aconstraint::Op;
use flow_typing_type::type_::aconstraint::OpInner;
use flow_typing_type::type_::constraint::Constraints;
use flow_typing_type::type_::constraint::forcing_state::ForcingState;
use flow_typing_type::type_::constraint::forcing_state::ModuleTypeForcingState;
use flow_typing_type::type_::properties;
use flow_typing_type::type_::type_collector::TypeCollector;
use flow_typing_type::type_util;

use crate::avar;

/// The contribution of an indexer key to the key set of an `AnnotGetKeysT`.
/// The key goes through `AnnotGetKeysDictKeyT` rather than being classified
/// here, so that a key that is still an unresolved annotation is elaborated
/// once it resolves, mirroring the dispatch (`GetKeysDictKeyT`) path.
fn elab_dict_key<'cx>(
    cx: &mut Context<'cx>,
    seen: &FlowOrdSet<i32>,
    reason_op: &Reason,
    dict_key: Type,
) -> Type {
    elab_t(
        cx,
        Some(seen.dupe()),
        dict_key,
        Op::new(OpInner::AnnotGetKeysDictKeyT(reason_op.dupe())),
    )
}

fn object_like_op(op: &OpInner<Cx>) -> bool {
    match op {
        OpInner::AnnotSpecializeT(_)
        | OpInner::AnnotThisSpecializeT { .. }
        | OpInner::AnnotUseTTypeT { .. }
        | OpInner::AnnotConcretizeForImportsExports(_, _)
        | OpInner::AnnotConcretizeForCJSExtractNamedExportsAndTypeExports(_)
        | OpInner::AnnotConcretizeForInspection { .. }
        | OpInner::AnnotConcretizeForClassExtends { .. }
        | OpInner::AnnotImportTypeofT { .. }
        | OpInner::AnnotAssertExportIsTypeT { .. }
        | OpInner::AnnotElemT { .. }
        | OpInner::AnnotGetStaticsT(_)
        | OpInner::AnnotMixinT(_)
        | OpInner::AnnotObjKitT(_)
        | OpInner::AnnotObjTestProtoT(_)
        | OpInner::AnnotArithT(_)
        | OpInner::AnnotUnaryArithT { .. }
        | OpInner::AnnotNotT(_)
        | OpInner::AnnotObjKeyMirror(_)
        | OpInner::AnnotGetKeysT(_)
        | OpInner::AnnotGetKeysDictKeyT(_)
        | OpInner::AnnotGetEnumT(_)
        | OpInner::AnnotDeepReadOnlyT(_)
        | OpInner::AnnotToStringT { .. }
        | OpInner::AnnotFilterOptionalT(_) => false,

        OpInner::AnnotGetTypeFromNamespaceT(_)
        | OpInner::AnnotGetPropT(_)
        | OpInner::AnnotGetElemT { .. }
        | OpInner::AnnotLookupT(_)
        | OpInner::AnnotObjRestT { .. }
        | OpInner::AnnotGetValuesT(_) => true,
    }
}

fn primitive_promoting_op(op: &OpInner<Cx>) -> bool {
    match op {
        OpInner::AnnotGetPropT(_) | OpInner::AnnotGetElemT { .. } | OpInner::AnnotLookupT(_) => {
            true
        }
        // TODO: enumerate all use types
        _ => false,
    }
}

fn get_fully_resolved_type_state<'cx>(cx: &mut Context<'cx>, id: i32) -> ForcingState<Cx> {
    let (_, constraints) = cx.find_constraints(id);
    match constraints {
        Constraints::FullyResolved(s) => s,
        Constraints::Resolved(_) | Constraints::Unresolved(_) => {
            panic!("unexpected unresolved constraint in annotation inference")
        }
    }
}
fn get_fully_resolved_type<'cx>(cx: &mut Context<'cx>, id: i32) -> Type {
    let s = get_fully_resolved_type_state(cx, id);
    let t = cx.force_fully_resolved_tvar(&s);
    flow_js_utils::invalid_cyclic_type_validation::validate_type_sig_type(cx, &t);
    t
}

fn get_builtin_typeapp<'cx>(
    cx: &mut Context<'cx>,
    reason: Reason,
    x: &str,
    targs: Vec<Type>,
) -> Type {
    let t = flow_js_utils::lookup_builtin_type(cx, x, reason.dupe());
    type_util::typeapp(false, false, reason, t, targs)
}

// Errors created with [error_unsupported] are actually reported. Compare this to
// errors created with Flow_js_utils.add_output which are recorded in the context
// of the source of the annotations, and are therefore ignored. This function reports
// them on [Context::annot_error_dst].
//
// The only kind of errors that are reported here are "unsupported" cases. These
// are mostly cases that rely on subtyping, which is not implemented here; most
// commonly evaluating call-like EvalTs and speculation.
fn error_unsupported_operation(
    cx: &mut Context,
    target: &Type,
    operation: AnnotationInferenceOperation,
    operation_reason: Reason,
) -> Type {
    let target_reason = type_util::reason_of_t(target);
    let target_loc = target_reason
        .annot_loc()
        .unwrap_or_else(|| target_reason.def_loc())
        .dupe();
    let genv = flow_typing_ty_normalizer::no_flow::mk_default_genv(None);
    let target_desc =
        flow_typing_ty_normalizer::no_flow::type_to_desc_for_errors(cx, &genv, target);
    let operation_loc = operation_reason
        .annot_loc()
        .unwrap_or_else(|| operation_reason.def_loc())
        .dupe();
    let msg = ErrorMessage::EAnnotationInference(Box::new(EAnnotationInferenceData {
        loc: operation_reason.loc().dupe(),
        operation,
        operation_loc,
        target_loc,
        target_desc,
    }));
    flow_js_utils::add_annot_inference_error(cx, msg);
    type_::any_t::error(operation_reason)
}

fn error_unsupported(cx: &mut Context, target: &Type, op: &Op<Cx>) -> Type {
    error_unsupported_operation(cx, target, op.error_operation(), op.reason())
}

pub(crate) fn error_recursive(cx: &mut Context, reason: &Reason) -> Type {
    let loc = reason.loc().dupe();
    let msg = ErrorMessage::ETrivialRecursiveDefinition(loc);
    flow_js_utils::add_annot_inference_error(cx, msg);
    type_::any_t::error(reason.dupe())
}

pub fn force_module_type_thunk(
    src_id: ContextId,
    s: ModuleTypeForcingState<Cx>,
) -> flow_typing_context::ModuleTypeThunk {
    Rc::new(move |cx: &mut Context<'_>| {
        let mut src_cx = cx.switch_to(src_id);
        s.force(&mut src_cx, |src_cx, r| Err(error_recursive(src_cx, r)))
    })
}

fn get_lazy_module_type_or_any_src(
    resolved_require: &flow_typing_context::ResolvedRequire,
) -> Rc<flow_lazy::Lazy<Cx, Result<type_::ModuleType, type_::AnySource>>> {
    use flow_typing_context::ResolvedRequire;
    use flow_typing_type::type_::AnyErrorKind;
    use flow_typing_type::type_::AnySource;

    match resolved_require {
        ResolvedRequire::TypedModule(f) => {
            let f = f.clone();
            Rc::new(flow_lazy::Lazy::new(move |cx: &mut Context<'_>| {
                match f(cx) {
                    Err(_) => Err(AnySource::AnyError(None)),
                    Ok(module_type) => Ok(module_type),
                }
            }))
        }
        ResolvedRequire::UncheckedModule(_) => {
            Rc::new(flow_lazy::Lazy::new(|_cx: &mut Context<'_>| {
                Err(AnySource::Untyped)
            }))
        }
        ResolvedRequire::MissingModule | ResolvedRequire::GlobalLibdefModule(_) => {
            Rc::new(flow_lazy::Lazy::new(|_cx: &mut Context<'_>| {
                Err(AnySource::AnyError(Some(AnyErrorKind::UnresolvedName)))
            }))
        }
    }
}

fn error_internal_reason(cx: &mut Context, msg: &str, reason_op: Reason) -> Type {
    let loc = reason_op.loc().dupe();
    let err_msg = ErrorMessage::EInternal(Box::new((
        loc,
        flow_typing_errors::error_message::InternalError::UnexpectedAnnotationInference(msg.into()),
    )));
    flow_js_utils::add_annot_inference_error(cx, err_msg);
    type_::any_t::error(reason_op)
}

fn error_internal(cx: &mut Context, msg: &str, op: &Op<Cx>) -> Type {
    let reason_op = op.reason();
    error_internal_reason(cx, msg, reason_op)
}

fn dummy_trace() -> DepthTrace {
    DepthTrace::dummy_trace()
}

// Repositioning does not seem to have any perceptible impact in annotation
// inference. Instead of replicating the convoluted implementation of Flow_js
// here, we just return the same type intact.
pub fn reposition(_cx: &Context, _loc: ALoc, t: Type) -> Type {
    t
}

// Instantiation
struct AnnotInstantiationHelper;

impl flow_js_utils::InstantiationHelper for AnnotInstantiationHelper {
    // We will not be solving implicit instantiation problems here. The only case
    // where we will need to use this function is when a PolyT needs to be used
    // as a monomorphic type. In this case, the only sensible thing to do is to
    // use the bound of each parameter as the argument to the intantiation.
    fn mk_targ<'cx>(
        _cx: &mut Context<'cx>,
        typeparam: &type_::TypeParam,
        _reason_op: &Reason,
        _reason_tapp: &Reason,
    ) -> Type {
        typeparam.bound.dupe()
    }

    fn is_subtype<'cx>(
        _cx: &mut Context<'cx>,
        _env: &FlowJsEnv,
        _trace: DepthTrace,
        _use_op: UseOp,
        _t1: Type,
        _t2: Type,
    ) -> Result<(), FlowJsException> {
        Ok(())
    }

    fn unify<'cx>(
        _cx: &mut Context<'cx>,
        _env: &FlowJsEnv,
        _trace: DepthTrace,
        _use_op: UseOp,
        _t1: Type,
        _t2: Type,
    ) -> Result<(), FlowJsException> {
        Ok(())
    }

    fn reposition<'cx>(
        _cx: &mut Context<'cx>,
        _env: &FlowJsEnv,
        _trace: Option<DepthTrace>,
        _loc: ALoc,
        t: Type,
    ) -> Result<Type, FlowJsException> {
        Ok(t)
    }
}

fn instantiate_poly<'cx>(
    cx: &mut Context<'cx>,
    use_op: UseOp,
    reason_op: Reason,
    reason_tapp: Reason,
    tparams_loc: ALoc,
    ids: Vec<type_::TypeParam>,
    t: Type,
) -> Type {
    let xs = vec1::Vec1::try_from_vec(ids).expect("tparams should be non-empty");
    let (t, _) = flow_js_utils::instantiation_kit::instantiate_poly::<AnnotInstantiationHelper>(
        cx,
        dummy_trace(),
        use_op,
        &reason_op,
        &reason_tapp,
        false,
        tparams_loc,
        &xs,
        t,
    )
    // Annotation inference is never speculative
    .unwrap();
    t
}

fn mk_typeapp_of_poly<'cx>(
    cx: &mut Context<'cx>,
    use_op: UseOp,
    reason_op: Reason,
    reason_tapp: Reason,
    id: type_::poly::Id,
    tparams_loc: ALoc,
    xs: Vec<type_::TypeParam>,
    t: Type,
    ts: Rc<[Type]>,
) -> Type {
    let xs = vec1::Vec1::try_from_vec(xs).expect("tparams should be non-empty");
    flow_js_utils::instantiation_kit::mk_typeapp_of_poly::<AnnotInstantiationHelper>(
        cx,
        dummy_trace(),
        use_op,
        &reason_op,
        &reason_tapp,
        id,
        tparams_loc,
        &xs,
        t,
        ts,
    )
    // Annotation inference is never speculative
    .unwrap()
}

fn with_concretized_type<'cx>(
    cx: &mut Context<'cx>,
    r: Reason,
    f: AnnotConcretizeForImportsExportsFn<Cx>,
    t: Type,
) -> Type {
    elab_t(
        cx,
        None,
        t,
        Op::new(OpInner::AnnotConcretizeForImportsExports(r, f)),
    )
}

/// Evaluates deferred heads (type applications, computed wrappers) of an
/// annotation type without applying any further operation, so that callers
/// which inspect a merged type directly observe the same shape that property
/// lookup would navigate into. Already-concrete types pass through unchanged.
pub fn concretize_type<'cx>(cx: &mut Context<'cx>, reason: Reason, type_: Type) -> Type {
    with_concretized_type(
        cx,
        reason,
        Rc::new(|_cx: &mut Context<'_>, type_| type_),
        type_,
    )
}

// GetProp

fn cg_lookup_<'cx>(
    cx: &mut Context<'cx>,
    use_op: &UseOp,
    t: Type,
    reason_op: &Reason,
    propref: &type_::PropRef,
    objt: &Type,
    indexer_fallback: Option<Box<type_::IndexerFallbackData>>,
) -> Type {
    elab_t(
        cx,
        None,
        t,
        Op::new(OpInner::AnnotLookupT(Box::new(AnnotLookupTData {
            reason: reason_op.dupe(),
            use_op: use_op.dupe(),
            prop_ref: propref.clone(),
            type_: objt.dupe(),
            indexer_fallback,
        }))),
    )
}

/// The annotation-inference counterpart of `flow_js`'s `strict_lookup_failed`: the
/// prototype chain is exhausted with no declared property. Report that, then recover.
///
/// If an indexer matched the access on the way up, no declared property was ever required,
/// so recovery yields the indexer's value instead of `any` — and for a TS-related access
/// there is nothing to report at all.
fn annot_lookup_failed<'cx>(
    cx: &mut Context<'cx>,
    reason_op: &Reason,
    reason_prop: &Reason,
    name: &Name,
    use_op: &UseOp,
    propref: &type_::PropRef,
    objt: &Type,
    indexer_fallback: Option<&type_::IndexerFallbackData>,
) -> Type {
    use flow_typing_flow_common::flow_js_utils::get_prop_t_kit;

    if !indexer_fallback.is_some_and(|fallback| fallback.suppress_missing_error) {
        flow_js_utils::add_output_non_speculating(
            cx,
            flow_typing_errors::error_message::ErrorMessage::EPropNotFoundInLookup(Box::new(
                EPropNotFoundInLookupData {
                    prop_loc: reason_prop.loc().dupe(),
                    object: flow_js_utils::type_reference_with_reason_for_error(
                        objt,
                        type_util::reason_of_t(objt).dupe(),
                    ),
                    prop_name: Some(name.dupe()),
                    use_op: use_op.dupe(),
                    suggestion: None,
                },
            )),
        );
    }
    match indexer_fallback {
        Some(fallback) => get_prop_t_kit::perform_read_prop_action::<AnnotGetPropHelper>(
            cx,
            &FlowJsEnv::entry(),
            &dummy_trace(),
            use_op.dupe(),
            propref,
            type_::property::property_type(&fallback.property),
            reason_op,
            &None,
        )
        // Annotation inference is never speculative
        .unwrap(),
        None => type_::any_t::error_of_kind(type_::AnyErrorKind::UnresolvedName, reason_op.dupe()),
    }
}

struct AnnotGetPropHelper;

impl flow_js_utils::GetPropHelper for AnnotGetPropHelper {
    type R = Type;

    fn error_type<'cx>(
        _cx: &mut Context<'cx>,
        _env: &FlowJsEnv,
        _trace: DepthTrace,
        reason: Reason,
    ) -> Result<Type, FlowJsException> {
        Ok(type_::any_t::error(reason))
    }

    fn return_<'cx>(
        cx: &mut Context<'cx>,
        _env: &FlowJsEnv,
        _use_op: UseOp,
        _trace: DepthTrace,
        t: Type,
    ) -> Result<Type, FlowJsException> {
        // We could have just returned `t` here. The OpenT indirection is for compatibility
        // with Flow_js. Specifically, without the OpenT the transformation in
        // https://github.com/meta/flow/blob/8c3825a1be188e9ade4ad4ed515361bb28c65d8a/src/typing/flow_js.ml#L1744-L1755
        // would fire, causing a divergence in the behavior of this module and Flow_js.
        match t.deref() {
            TypeInner::OpenT(_) => Ok(t),
            _ => {
                let reason = type_util::reason_of_t(&t).dupe();
                Ok(flow_typing_tvar::mk_fully_resolved(cx, reason, t))
            }
        }
    }

    fn dict_read_check<'cx>(
        _cx: &mut Context<'cx>,
        _env: &FlowJsEnv,
        _trace: DepthTrace,
        _use_op: &UseOp,
        _pair: (&Type, &Type),
    ) -> Result<(), FlowJsException> {
        // We will not be doing subtyping checks in annotation inference.
        Ok(())
    }

    fn reposition<'cx>(
        cx: &mut Context<'cx>,
        _env: &FlowJsEnv,
        _trace: Option<DepthTrace>,
        loc: ALoc,
        t: Type,
    ) -> Result<Type, FlowJsException> {
        Ok(reposition(cx, loc, t))
    }

    fn cg_lookup<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        _trace: DepthTrace,
        obj_t: Type,
        _method_accessible: bool,
        super_t: Type,
        args: CgLookupArgs,
    ) -> Result<Type, FlowJsException> {
        debug_assert!(!env.speculating(), "annotation inference never speculates");
        let CgLookupArgs {
            reason_op,
            indexer_fallback,
            propref,
            use_op,
            ..
        } = args;
        Ok(cg_lookup_(
            cx,
            &use_op,
            super_t,
            &reason_op,
            &propref,
            &obj_t,
            indexer_fallback,
        ))
    }

    fn cg_get_prop<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        _trace: DepthTrace,
        t: Type,
        args: (UseOp, Reason, Option<i32>, (Reason, Name)),
    ) -> Result<Type, FlowJsException> {
        debug_assert!(!env.speculating(), "annotation inference never speculates");
        let (use_op, access_reason, _, (prop_reason, name)) = args;
        Ok(elab_t(
            cx,
            None,
            t,
            Op::new(OpInner::AnnotGetPropT(Box::new(AnnotGetPropTData {
                reason: access_reason,
                use_op,
                from_annot: false,
                prop_ref: type_util::mk_named_prop(prop_reason, false, name),
            }))),
        ))
    }

    fn mk_react_dro<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        _use_op: UseOp,
        dro: &type_::ReactDro,
        t: Type,
    ) -> Type {
        debug_assert!(!env.speculating(), "annotation inference never speculates");
        let reason = type_util::reason_of_t(&t).dupe();
        elab_t(
            cx,
            None,
            t,
            Op::new(OpInner::AnnotDeepReadOnlyT(Box::new(
                AnnotDeepReadOnlyTData {
                    reason: reason.dupe(),
                    loc: dro.0.dupe(),
                    dro_type: dro.1.clone(),
                },
            ))),
        )
    }

    fn key_overlaps_with_indexer() -> Option<
        fn(
            &mut Context,
            &FlowJsEnv,
            &Type,
            &Type,
        ) -> Result<bool, flow_utils_concurrency::job_error::JobError>,
    > {
        None
    }
}

/// Ensures that the annotation constraint associated with [id] has been resolved.
/// If the respective constraint is already resolved then it returns immediately.
/// Otherwise, it resolves [id] immediately to the 'any' type. In the case of an
/// [AnnotOp] constraint that depends on [dep_id] we also update the "dependents" set
/// of [dep_id], so that we don't attempt to resolve [id] once again when [dep_id]
/// gets resolved.
fn ensure_annot_resolved<'cx>(cx: &mut Context<'cx>, reason: Reason, id: i32) -> Type {
    match cx.find_avar_opt(id) {
        None => get_fully_resolved_type(cx, id),
        Some(constraint) => match constraint.deref() {
            AConstraintInner::AnnotUnresolved { .. } => {
                let t = error_recursive(cx, &reason);
                resolve_id(cx, reason, id, t.dupe());
                t
            }
            AConstraintInner::AnnotOp { id: dep_id, .. } => {
                let dep_id = *dep_id;
                let dep_constraint = cx.find_avar(dep_id);
                dep_constraint.update_deps(|mut deps| {
                    deps.remove(&id);
                    deps
                });
                let t = error_recursive(cx, &reason);
                resolve_id(cx, reason, id, t.dupe());
                t
            }
        },
    }
}

fn mk_lazy_tvar<'cx>(
    cx: &mut Context<'cx>,
    reason: Reason,
    f: impl for<'a> FnOnce(&mut Context<'a>, i32) + 'static,
) -> Type {
    let id = mk_id() as i32;
    let tvar = Type::new(TypeInner::OpenT(type_::Tvar::new(reason.dupe(), id as u32)));
    let reason2 = reason.dupe();
    let cx_id = cx.id();
    let lazy_fn = move |forcing_cx: &mut Context<'_>| {
        let cx2 = &mut *forcing_cx.switch_to(cx_id);
        avar::unresolved_with_id(cx2, id, reason2.dupe());
        f(cx2, id);
        // Before forcing the type constraint of [id] we need to make sure the
        // respective annotation constraint has been processed. If not we infer
        // the empty type.
        ensure_annot_resolved(cx2, reason2.dupe(), id)
    };
    let forcing_state = ForcingState::<Cx>::of_lazy_t(reason.dupe(), lazy_fn);
    let node = flow_utils_union_find::Node::create_root(Constraints::FullyResolved(forcing_state));
    cx.add_fresh_tvar(id, node);
    tvar
}

pub fn mk_sig_tvar<'cx>(cx: &mut Context<'cx>, reason: Reason, resolved: LazyType) -> Type {
    let reason2 = reason.dupe();
    mk_lazy_tvar(cx, reason.dupe(), move |cx: &mut Context<'_>, id| {
        let t = resolved.get_forced(cx).dupe();
        resolve_id(cx, reason2, id, t);
    })
}

/// Resolves the annotation tvar [id] to the type [t].
/// - If [t] is a concrete type, we mark [id] as a resolved annotation tvar and
///   record it as fully resolved in the type graph.
/// - If [t] is an `OpenT` with id [id2], we first ensure [id2] is resolved, then
///   resolve [id] to the resulting type.
fn resolve_id<'cx>(cx: &mut Context<'cx>, reason: Reason, id: i32, t: Type) {
    match cx.find_avar_opt(id) {
        None => {
            // The avar is already resolved. This happens when the avar is recursively
            // reachable and becomes resolved to any.
        }
        Some(constraints1) => {
            let t = match t.deref() {
                TypeInner::OpenT(tvar) => {
                    ensure_annot_resolved(cx, reason.dupe(), tvar.id() as i32)
                }
                _ => t,
            };
            cx.remove_avar(id);
            if cx.graph().get(&id).is_some()
                && !get_fully_resolved_type_state(cx, id).already_forced_with_cyclic_error()
            {
                // It is possible that this tvar is cyclic in a bad way, and at this point,
                // the ensure_annot_resolved above has already discovered that and resolve the
                // tvar to any. If we still unconditionally try to add the tvar here, it will
                // undo the work of turning the bad tvar to any.
                //
                // e.g. Consider OpenT(id=1). 1=AnnotT(OpenT(id=1)).
                // ensure_annot_resolved, while visiting the nested OpenT in Annot, will resolve
                // the tvar to any and error, but if we unconditionally add the tvar with
                // t = AnnotT(OpenT(id=1)), the work is undone.
                cx.add_tvar(
                    id,
                    flow_utils_union_find::Node::create_root(Constraints::FullyResolved(
                        ForcingState::of_non_lazy_t(t.dupe()),
                    )),
                );
            }
            let dependents1 = constraints1.deps();
            resolve_dependent_set(cx, reason, &dependents1.borrow(), t);
        }
    }
}

fn resolve_dependent_set<'cx>(
    cx: &mut Context<'cx>,
    reason: Reason,
    dependents: &FlowOrdSet<i32>,
    t: Type,
) {
    cx.iter_annot_dependent_set(
        |cx, id, op| {
            let result = elab_t(cx, None, t.dupe(), op.dupe());
            resolve_id(cx, reason.dupe(), id, result);
        },
        dependents,
    );
}

fn elab_open<'cx>(
    cx: &mut Context<'cx>,
    seen: &FlowOrdSet<i32>,
    reason: Reason,
    id: i32,
    op: Op<Cx>,
) -> Type {
    if seen.contains(&id) {
        return error_recursive(cx, &reason);
    }
    match cx.find_avar_opt(id) {
        None => {
            // [id] may refer to a lazily resolved constraint (e.g. created through
            // [mk_lazy_tvar]). To protect against trying to force recursive lazy
            // structures, we introduce a lazy indirection around the resulting
            // constraint. An example that would have cause this unwanted behavior is
            //
            //   declare var x: {
            //     p: number;
            //     q: typeof (x.p);
            //   };
            //
            // This lazy indirection allows the type of `x` to be resolved, before we
            // attempt to force the constraint for `x.p`.
            //
            // Share one indirection per demand: re-demands (e.g. while
            // forcing) reuse it instead of expanding forever.
            let memo_key = elab_open_memo_key(&op, id);
            if let Some(key) = &memo_key
                && let Some(t) = cx.elab_open_memo_get(key)
            {
                return t;
            }
            let seen2 = FlowOrdSet::singleton(id);
            let op2 = op.dupe();
            let resolved: LazyType = Rc::new(flow_lazy::Lazy::new(move |cx: &mut Context<'_>| {
                let t = get_fully_resolved_type(cx, id);
                elab_t(cx, Some(seen2.dupe()), t, op2.dupe())
            }));
            let t = mk_sig_tvar(cx, op.reason(), resolved);
            if let Some(key) = memo_key {
                cx.elab_open_memo_insert(key, t.dupe());
            }
            t
        }
        Some(constraint) => match constraint.deref() {
            AConstraintInner::AnnotUnresolved { .. } | AConstraintInner::AnnotOp { .. } => {
                let fresh_id = avar::constrained(cx, op, id);
                Type::new(TypeInner::OpenT(type_::Tvar::new(reason, fresh_id as u32)))
            }
        },
    }
}

pub fn elab_t<'cx>(
    cx: &mut Context<'cx>,
    seen: Option<FlowOrdSet<i32>>,
    t: Type,
    op: Op<Cx>,
) -> Type {
    let seen = seen.unwrap_or_default();
    match (t.deref(), op.deref()) {
        (
            TypeInner::EvalT {
                type_: inner_t,
                defer_use_t,
                ..
            },
            _,
        ) => {
            let use_op = defer_use_t.0.dupe();
            let reason = defer_use_t.1.dupe();
            match defer_use_t.2.deref() {
                // The sig merge counterpart of normalization in `eval_destructor`, with
                // members concretized by `AnnotConcretizeForInspection`.
                Destructor::NormalizeIntersection { check_exact } => {
                    let TypeInner::IntersectionT(_, _) = inner_t.deref() else {
                        return elab_t(cx, None, inner_t.dupe(), op);
                    };
                    let result = Normalizer::new(&FlowJsEnv::entry(), &reason).normalize(
                        cx,
                        inner_t,
                        |cx, _, member| {
                            let collector = TypeCollector::create();
                            elab_t(
                                cx,
                                Some(seen.dupe()),
                                member.dupe(),
                                Op::new(OpInner::AnnotConcretizeForInspection {
                                    reason: type_util::reason_of_t(member).dupe(),
                                    collector: collector.clone(),
                                }),
                            );
                            Ok::<Vec<Type>, Infallible>(collector.collect_to_vec())
                        },
                    );
                    let result = match result {
                        Ok(result) => result,
                        Err(never) => match never {},
                    };
                    let normalized = match result {
                        Normalization::Normalized(normalized) => normalized,
                        Normalization::InvalidExact(normalized, invalid) => {
                            if *check_exact {
                                flow_js_utils::add_annot_inference_error(
                                    cx,
                                    ErrorMessage::EInvalidExactObjectIntersection(invalid),
                                );
                            }
                            normalized
                        }
                        Normalization::Unchanged => inner_t.dupe(),
                    };
                    elab_t(cx, Some(seen), normalized, op)
                }
                Destructor::ReadOnlyType => {
                    let t = make_readonly(cx, use_op, reason, inner_t.dupe());
                    elab_t(cx, None, t, op)
                }
                Destructor::ReactDRO(box react_dro) => {
                    let t = elab_t(
                        cx,
                        None,
                        inner_t.dupe(),
                        Op::new(OpInner::AnnotDeepReadOnlyT(Box::new(
                            AnnotDeepReadOnlyTData {
                                reason: reason.dupe(),
                                loc: react_dro.0.dupe(),
                                dro_type: react_dro.1.clone(),
                            },
                        ))),
                    );
                    elab_t(cx, None, t, op)
                }
                Destructor::ExactType => {
                    let t = make_exact(cx, reason, inner_t.dupe());
                    elab_t(cx, None, t, op)
                }
                Destructor::PartialType => {
                    let t = make_partial(cx, use_op, reason, inner_t.dupe());
                    elab_t(cx, None, t, op)
                }
                Destructor::RequiredType => {
                    let t = make_required(cx, use_op, reason, inner_t.dupe());
                    elab_t(cx, None, t, op)
                }
                Destructor::SpreadType(box DestructorSpreadTypeData(
                    target,
                    todo_rev,
                    head_slice,
                )) => {
                    let acc: flow_data_structure_wrapper::list::FlowOcamlList<
                        flow_typing_type::type_::object::spread::AccElement,
                    > = match head_slice {
                        Some(x) => flow_data_structure_wrapper::list::FlowOcamlList::unit(
                            flow_typing_type::type_::object::spread::AccElement::InlineSlice(
                                x.clone(),
                            ),
                        ),
                        None => flow_data_structure_wrapper::list::FlowOcamlList::new(),
                    };
                    let state = type_::object::spread::State {
                        todo_rev: todo_rev.dupe(),
                        acc,
                        spread_id: flow_common::reason::mk_id() as i32,
                        union_t: None,
                        curr_resolve_idx: 0,
                    };
                    let t =
                        object_spread(cx, use_op, reason, target.clone(), state, inner_t.dupe());
                    elab_t(cx, None, t, op)
                }
                Destructor::RestType(options, r) => {
                    let state = type_::object::rest::State::One(r.dupe());
                    let t = object_rest_internal(
                        cx,
                        use_op,
                        reason,
                        options.clone(),
                        state,
                        inner_t.dupe(),
                    );
                    elab_t(cx, None, t, op)
                }
                Destructor::TypeMap(TypeMap::ObjectKeyMirror) => {
                    let t = elab_t(
                        cx,
                        None,
                        inner_t.dupe(),
                        Op::new(OpInner::AnnotObjKeyMirror(reason)),
                    );
                    elab_t(cx, None, t, op)
                }
                Destructor::ValuesType => {
                    let t = elab_t(
                        cx,
                        None,
                        inner_t.dupe(),
                        Op::new(OpInner::AnnotGetValuesT(reason)),
                    );
                    elab_t(cx, None, t, op)
                }
                Destructor::PropertyType { name } => {
                    let reason_op = reason.dupe().replace_desc(
                        flow_common::reason::VirtualReasonDesc::RProperty(Some(name.dupe())),
                    );
                    let t = elab_t(
                        cx,
                        None,
                        inner_t.dupe(),
                        Op::new(OpInner::AnnotGetPropT(Box::new(AnnotGetPropTData {
                            reason: reason_op,
                            use_op,
                            from_annot: true,
                            prop_ref: type_::PropRef::Named {
                                reason: reason.dupe(),
                                name: name.dupe(),
                                from_indexed_access: true,
                            },
                        }))),
                    );
                    elab_t(cx, None, t, op)
                }
                Destructor::ElementType { index_type } => {
                    let t = elab_t(
                        cx,
                        None,
                        inner_t.dupe(),
                        Op::new(OpInner::AnnotGetElemT {
                            reason,
                            use_op,
                            key: index_type.dupe(),
                        }),
                    );
                    elab_t(cx, None, t, op)
                }
                Destructor::EnumType => {
                    let t = elab_t(
                        cx,
                        None,
                        inner_t.dupe(),
                        Op::new(OpInner::AnnotGetEnumT(reason)),
                    );
                    elab_t(cx, None, t, op)
                }
                Destructor::MappedType(box type_::DestructorMappedTypeData {
                    property_type,
                    name_type: None,
                    mapped_type_flags,
                    homomorphic,
                    distributive_tparam_name,
                }) => {
                    let (property_type, _, homomorphic) =
                        flow_js_utils::substitute_mapped_type_distributive_tparams(
                            cx,
                            Some(use_op.dupe()),
                            distributive_tparam_name.clone(),
                            property_type.dupe(),
                            None,
                            homomorphic.dupe(),
                            inner_t.dupe(),
                        );
                    let selected_keys_opt = match homomorphic {
                        type_::MappedTypeHomomorphicFlag::Unspecialized => {
                            let mapped = mapped_type_of_keys(
                                cx,
                                use_op,
                                reason,
                                inner_t.dupe(),
                                &property_type,
                                mapped_type_flags,
                                &op,
                            );
                            return elab_t(cx, None, mapped, op);
                        }
                        type_::MappedTypeHomomorphicFlag::Homomorphic => None,
                        type_::MappedTypeHomomorphicFlag::SemiHomomorphic(keys) => Some(keys),
                    };
                    let mapped = object_kit(
                        cx,
                        use_op,
                        reason,
                        type_::object::ResolveTool::Resolve(type_::object::Resolve::Next),
                        type_::object::Tool::ObjectMap(Box::new(
                            type_::object::ObjectToolObjectMapData {
                                prop_type: property_type,
                                name_type: None,
                                mapped_type_flags: *mapped_type_flags,
                                selected_keys_opt,
                            },
                        )),
                        inner_t.dupe(),
                    );
                    elab_t(cx, None, mapped, op)
                }
                _ => error_unsupported(cx, &t, &op),
            }
        }
        (
            TypeInner::OpenT(tvar),
            OpInner::AnnotConcretizeForInspection { .. }
            | OpInner::AnnotConcretizeForClassExtends { .. },
        ) => {
            let t = elab_open(cx, &seen, tvar.reason().dupe(), tvar.id() as i32, op);
            if let TypeInner::OpenT(tvar2) = t.deref() {
                // Force the type so that the concretized results are eagarly added to the collector
                get_fully_resolved_type(cx, tvar2.id() as i32);
            }
            t
        }
        (TypeInner::OpenT(tvar), _) => {
            elab_open(cx, &seen, tvar.reason().dupe(), tvar.id() as i32, op)
        }
        (TypeInner::AnnotT(r, inner_t, _), _) => {
            let t = reposition(cx, r.loc().dupe(), inner_t.dupe());
            elab_t(cx, Some(seen), t, op)
        }
        // The remaining cases are dispatched to elab_t_concrete.
        // This split avoids an extremely long match expression.
        _ => elab_t_concrete(cx, seen, t, op),
    }
}

fn general_error<'cx>(cx: &mut Context<'cx>, t: &Type, op: &Op<Cx>) -> Type {
    use flow_typing_type::type_::any_t;

    let reason_op = op.reason();
    let upper = IncompatibleUpperData {
        loc: reason_op.loc().dupe(),
        kind: flow_typing_errors::error_message::UpperKind::IncompatibleUnclassified(
            op.string_of_operation().into(),
        ),
    };
    let use_op = op.use_op();
    flow_js_utils::add_output_non_speculating(
        cx,
        flow_js_utils::incompatible_type_error(t, upper, use_op),
    );
    any_t::error(reason_op)
}

fn elab_t_concrete<'cx>(cx: &mut Context<'cx>, seen: FlowOrdSet<i32>, t: Type, op: Op<Cx>) -> Type {
    use flow_typing_type::type_::DefTInner;
    use flow_typing_type::type_::EnumInfoInner;
    use flow_typing_type::type_::PolyTData;
    use flow_typing_type::type_::any_t;

    match (t.deref(), op.deref()) {
        // *******************************************************************
        // UseT TypeT (runtime types derive static types through annotation)
        // *******************************************************************
        // First handle catch-all cases of subtyping_kit.rs
        (TypeInner::MaybeT(_, _), OpInner::AnnotUseTTypeT { .. })
        | (TypeInner::OptionalT { .. }, OpInner::AnnotUseTTypeT { .. }) => {
            error_unsupported(cx, &t, &op)
        }
        (
            TypeInner::ThisTypeAppT(box ThisTypeAppTData {
                reason,
                type_,
                this_t,
                targs,
            }),
            OpInner::AnnotUseTTypeT { .. },
        ) => {
            let reason_tapp = reason.dupe();
            let reason_op = op.reason();
            let tc = specialize_class(
                cx,
                type_.dupe(),
                reason_op,
                reason_tapp.dupe(),
                targs.clone(),
                op.error_operation(),
            );
            let t = this_specialize(cx, reason_tapp, this_t.dupe(), tc);
            elab_t(cx, Some(seen), t, op)
        }
        (
            TypeInner::TypeAppT(box TypeAppTData {
                reason,
                use_op,
                type_,
                targs,
                from_value,
                ..
            }),
            OpInner::AnnotUseTTypeT { .. },
        ) => {
            // NOTE omitting TypeAppExpansion.push_unless_loop check.
            let reason_op = op.reason();
            let t = mk_typeapp_instance(
                cx,
                use_op.dupe(),
                reason_op,
                reason.dupe(),
                *from_value,
                type_.dupe(),
                targs.clone(),
                op.error_operation(),
            );
            elab_t(cx, Some(seen), t, op)
        }
        (TypeInner::DefT(def_reason, def_t), OpInner::AnnotUseTTypeT { reason, kind }) => {
            match def_t.deref() {
                DefTInner::PolyT(box PolyTData { tparams, t_out, .. })
                    if matches!(kind, flow_typing_type::type_::TypeTKind::RenderTypeKind)
                        && matches!(
                            t_out.deref(),
                            TypeInner::DefT(_, dt) if matches!(dt.deref(), DefTInner::ReactAbstractComponentT(_))
                        ) =>
                {
                    let mut subst_map = flow_data_structure_wrapper::ord_map::FlowOrdMap::new();
                    for tparam in tparams.iter() {
                        subst_map.insert(tparam.name.dupe(), any_t::untyped(reason.dupe()));
                    }
                    let t_ = flow_typing_flow_common::type_subst::subst(
                        cx,
                        None,
                        true,
                        false,
                        flow_typing_flow_common::type_subst::Purpose::Normal,
                        &subst_map,
                        t_out.dupe(),
                    );
                    elab_t(cx, Some(seen), t_, op)
                }
                DefTInner::PolyT(box PolyTData {
                    kind: poly_kind,
                    tparams_loc,
                    tparams,
                    t_out,
                    id,
                    strictness_kind,
                }) if matches!(kind, flow_typing_type::type_::TypeTKind::RenderTypeKind)
                    && let TypeInner::DefT(_, inner) = t_out.deref()
                    && let DefTInner::TypeT(TypeTKind::ImportTypeofKind, inner_t) =
                        inner.deref()
                    && let TypeInner::AnnotT(_, t_out_mono, _) = inner_t.deref() =>
                {
                    // `renders` over `import typeof` of a generic component must
                    // behave like `renders` over the component itself. Unwrap the
                    // alias to a polymorphic component and recurse so the direct
                    // renders arm above applies.
                    let poly = type_util::poly_type_of_tparam_list(
                        id.dupe(),
                        tparams_loc.dupe(),
                        tparams.dupe(),
                        t_out_mono.dupe(),
                        *strictness_kind,
                        *poly_kind,
                    );
                    elab_t(cx, Some(seen), poly, op)
                }
                DefTInner::PolyT(box PolyTData {
                    kind: poly_kind,
                    tparams_loc,
                    tparams,
                    t_out,
                    id,
                    strictness_kind,
                }) if let TypeInner::DefT(_, inner) = t_out.deref()
                    && let DefTInner::TypeT(TypeTKind::ImportTypeofKind, inner_t) =
                        inner.deref()
                    && let TypeInner::AnnotT(_, t_out_mono, _) = inner_t.deref() =>
                {
                    // `import typeof` of a generic function, class, or component is a
                    // parameterized alias over the typeof of the body. Using it bare
                    // must behave like `typeof v` (keep the polymorphism) rather than
                    // reporting missing-type-arg, so rebuild the polymorphic typeof
                    // from the alias pieces.
                    let poly = type_util::poly_type_of_tparam_list(
                        id.dupe(),
                        tparams_loc.dupe(),
                        tparams.dupe(),
                        t_out_mono.dupe(),
                        *strictness_kind,
                        *poly_kind,
                    );
                    type_util::typeof_annotation(reason.dupe(), poly, None)
                }
                DefTInner::PolyT(box PolyTData {
                    kind: _,
                    tparams_loc,
                    tparams: ids,
                    t_out,
                    id,
                    strictness_kind: _,
                }) if !ids.is_empty() && ids.iter().all(|tp| tp.default.is_some()) => {
                    // Treat missing type args the same as empty type args (Foo = Foo<>) when all
                    // parameters have defaults. Otherwise, fall through to EMissingTypeArgs.
                    let reason_tapp = def_reason.dupe();
                    let t = mk_typeapp_of_poly(
                        cx,
                        type_::unknown_use(),
                        reason.dupe(),
                        reason_tapp,
                        id.dupe(),
                        tparams_loc.dupe(),
                        ids.to_vec(),
                        t_out.dupe(),
                        Rc::from([]),
                    );
                    elab_t(cx, Some(seen), t, op)
                }
                DefTInner::PolyT(box PolyTData {
                    tparams_loc,
                    tparams,
                    ..
                }) => {
                    flow_js_utils::add_output_non_speculating(
                        cx,
                        flow_typing_errors::error_message::ErrorMessage::EMissingTypeArgs(
                            Box::new(EMissingTypeArgsData {
                                loc: reason.loc().dupe(),
                                type_: flow_js_utils::type_reference_with_reason_for_error(
                                    &t,
                                    def_reason.dupe(),
                                ),
                                arity_loc: tparams_loc.dupe(),
                                min_arity: tparams.iter().filter(|tp| tp.default.is_none()).count()
                                    as i32,
                                max_arity: tparams.len() as i32,
                            }),
                        ),
                    );
                    any_t::error(reason.dupe())
                }
                DefTInner::ClassT(instance) => match instance.deref() {
                    TypeInner::ThisInstanceT(box ThisInstanceTData {
                        reason: r,
                        instance: i,
                        is_this,
                        subst_name,
                    }) => {
                        let fixed = flow_js_utils::fix_this_instance(
                            cx,
                            reason.dupe(),
                            r.dupe(),
                            i,
                            *is_this,
                            subst_name.dupe(),
                        );
                        let c = Type::new(TypeInner::DefT(
                            def_reason.dupe(),
                            type_::DefT::new(DefTInner::ClassT(fixed)),
                        ));
                        elab_t(cx, Some(seen), c, op)
                    }
                    //  a class value annotation becomes the instance type
                    _ => reposition(cx, reason.loc().dupe(), instance.dupe()),
                },
                // a component syntax value annotation becomes an element of that component
                DefTInner::ReactAbstractComponentT(_) => {
                    get_builtin_typeapp(cx, reason.dupe(), "React$RendersExactly", vec![t.dupe()])
                }
                DefTInner::TypeT(_, l) => l.dupe(),
                // A `unique symbol` value used in a type position resolves to its
                // own `unique symbol` type, so a value-level symbol binding can
                // key a type-body member (`interface I { [s]: T }`). Distinct
                // symbols stay distinct.
                DefTInner::UniqueSymbolT(_) => reposition(cx, reason.loc().dupe(), t.dupe()),
                // an enum object value annotation becomes the enum value type
                DefTInner::EnumObjectT { enum_value_t, .. } => enum_value_t.dupe(),
                DefTInner::EnumValueT(enum_info) => {
                    flow_js_utils::add_output_non_speculating(
                        cx,
                        flow_typing_errors::error_message::ErrorMessage::EEnumError(
                            flow_typing_errors::error_message::EnumErrorKind::EnumMemberUsedAsType(
                                Box::new(EnumMemberUsedAsTypeData {
                                    loc: reason.loc().dupe(),
                                    description_name: flow_js_utils::description_name_for_error(
                                        reason,
                                    ),
                                    enum_: EnumReferenceData {
                                        loc: type_util::ref_loc_of_t(&t).dupe(),
                                        name: enum_info.enum_name().map(Dupe::dupe),
                                    },
                                }),
                            ),
                        ),
                    );
                    any_t::error(reason.dupe())
                }
                _ => {
                    let reason_use = reason.dupe();
                    flow_js_utils::add_output_non_speculating(
                        cx,
                        flow_typing_errors::error_message::ErrorMessage::EValueUsedAsType {
                            reference: flow_js_utils::value_as_type_reference_for_error(
                                &reason_use,
                            ),
                            value: flow_js_utils::type_reference_at_loc_for_error(
                                &t,
                                reason_use.loc().dupe(),
                            ),
                        },
                    );
                    any_t::error(reason_use)
                }
            }
        }
        // a TS enum used as a bare type is the union of its member literal types; any
        // other namespace value used as a type is an error (catch-all below)
        (TypeInner::NamespaceT(ns), OpInner::AnnotUseTTypeT { reason, .. })
            if flow_js_utils::is_ts_enum_symbol(&ns.namespace_symbol) =>
        {
            flow_js_utils::ts_enum_member_union(cx, reason.dupe(), ns.types_tmap.dupe())
        }
        (
            TypeInner::AnyT(_, type_::AnySource::AnyError(_)),
            OpInner::AnnotUseTTypeT { reason, .. },
        ) => {
            // Short-circuit as we already error on the unresolved name.
            any_t::error(reason.dupe())
        }
        (TypeInner::AnyT(_, _), OpInner::AnnotUseTTypeT { reason, .. }) => {
            let reason_use = reason.dupe();
            flow_js_utils::add_output_non_speculating(
                cx,
                flow_typing_errors::error_message::ErrorMessage::EAnyValueUsedAsType {
                    reference: flow_js_utils::value_as_type_reference_for_error(&reason_use),
                    value: flow_js_utils::type_reference_at_loc_for_error(
                        &t,
                        reason_use.loc().dupe(),
                    ),
                },
            );
            any_t::error(reason_use)
        }
        (_, OpInner::AnnotUseTTypeT { reason, .. }) => {
            let reason_use = reason.dupe();
            flow_js_utils::add_output_non_speculating(
                cx,
                flow_typing_errors::error_message::ErrorMessage::EValueUsedAsType {
                    reference: flow_js_utils::value_as_type_reference_for_error(&reason_use),
                    value: flow_js_utils::type_reference_at_loc_for_error(
                        &t,
                        reason_use.loc().dupe(),
                    ),
                },
            );
            any_t::error(reason_use)
        }
        // *****************
        //  `import typeof`
        // *****************
        (_, OpInner::AnnotImportTypeofT { reason, name }) => {
            // Annotation inference is never speculative
            flow_js_utils::import_typeof_t_kit::on_concrete_type(
                cx,
                reason.dupe(),
                name.as_str(),
                &t,
            )
        }
        // ****************
        //  Module exports
        // ****************
        (_, OpInner::AnnotAssertExportIsTypeT { name, .. }) => {
            // Annotation inference is never speculative
            flow_js_utils::assert_export_is_type_t_kit::on_concrete_type(cx, name.dupe(), t)
                .unwrap()
        }
        (_, OpInner::AnnotConcretizeForCJSExtractNamedExportsAndTypeExports(_)) => t,
        // **********************************
        // Wildcards (idx, maybe, optional)
        // **********************************
        (TypeInner::OptionalT { type_, .. }, OpInner::AnnotFilterOptionalT(_)) => {
            elab_t(cx, Some(seen), type_.dupe(), op)
        }
        (TypeInner::MaybeT(r, inner), OpInner::AnnotFilterOptionalT(reason)) => {
            let inner = elab_t(cx, Some(seen), inner.dupe(), op.dupe());
            type_util::union_of_ts(reason.dupe(), vec![type_::null::why(r.dupe()), inner], None)
        }
        (TypeInner::MaybeT(_, _), _) | (TypeInner::OptionalT { .. }, _) => {
            // These are rare in practice. Will consider adding support if we hit this error case.
            error_unsupported(cx, &t, &op)
        }
        // *******************
        //  Type applications
        // *******************
        (
            TypeInner::TypeAppT(box TypeAppTData {
                reason,
                type_,
                targs,
                ..
            }),
            OpInner::AnnotConcretizeForClassExtends { this_t, .. },
        ) => {
            let reason_tapp = reason.dupe();
            let reason_op = op.reason();
            let tc = specialize_class(
                cx,
                type_.dupe(),
                reason_op,
                reason_tapp.dupe(),
                Some(targs.dupe()),
                op.error_operation(),
            );
            let t = this_specialize(cx, reason_tapp, this_t.dupe(), tc);
            elab_t(cx, Some(seen), t, op)
        }
        (
            TypeInner::ThisTypeAppT(box ThisTypeAppTData {
                reason,
                type_,
                this_t,
                targs,
            }),
            _,
        ) => {
            let reason_tapp = reason.dupe();
            let reason_op = op.reason();
            let tc = specialize_class(
                cx,
                type_.dupe(),
                reason_op,
                reason_tapp.dupe(),
                targs.clone(),
                op.error_operation(),
            );
            let t = this_specialize(cx, reason_tapp, this_t.dupe(), tc);
            elab_t(cx, Some(seen), t, op)
        }
        (
            TypeInner::TypeAppT(box TypeAppTData {
                reason,
                use_op,
                type_,
                targs,
                from_value,
                ..
            }),
            _,
        ) => {
            let reason_op = op.reason();
            let t = mk_typeapp_instance(
                cx,
                use_op.dupe(),
                reason_op,
                reason.dupe(),
                *from_value,
                type_.dupe(),
                targs.clone(),
                op.error_operation(),
            );
            elab_t(cx, Some(seen), t, op)
        }
        (_, OpInner::AnnotConcretizeForImportsExports(_, f)) => f(cx, t),
        // *****************
        //  Filter optional
        // *****************
        (TypeInner::DefT(r, def_t), OpInner::AnnotFilterOptionalT(_))
            if matches!(def_t.deref(), DefTInner::VoidT) =>
        {
            type_::empty_t::why(r.dupe())
        }
        (TypeInner::UnionT(_, rep), OpInner::AnnotFilterOptionalT(reason)) => {
            let ts: Vec<Type> = rep
                .members_iter()
                .map(|member| elab_t(cx, Some(seen.dupe()), member.dupe(), op.dupe()))
                .collect();
            type_util::union_of_ts(reason.dupe(), ts, None)
        }
        (_, OpInner::AnnotFilterOptionalT(_)) => t,
        // **************
        //  Opaque types
        // **************
        (TypeInner::NominalT { nominal_type, .. }, OpInner::AnnotToStringT { reason, .. })
            if nominal_type.upper_t.is_some() =>
        {
            let upper_t = nominal_type.upper_t.as_ref().unwrap().dupe();
            elab_t(
                cx,
                Some(seen),
                upper_t,
                Op::new(OpInner::AnnotToStringT {
                    orig_t: Some(t.dupe()),
                    reason: reason.dupe(),
                }),
            )
        }
        (
            TypeInner::NominalT {
                reason,
                nominal_type,
            },
            _,
        ) if let type_::nominal::UnderlyingT::OpaqueWithLocal { t: inner_t } =
            &nominal_type.underlying_t
            && reason.loc().source() == reason.def_loc().source() =>
        {
            elab_t(cx, Some(seen), inner_t.dupe(), op)
        }
        (TypeInner::NominalT { nominal_type, .. }, _)
            if let type_::nominal::UnderlyingT::CustomError(
                box type_::nominal::CustomErrorData { t: inner_t, .. },
            ) = &nominal_type.underlying_t =>
        {
            elab_t(cx, Some(seen), inner_t.dupe(), op)
        }
        // ******
        //  Keys
        // ******
        (TypeInner::KeysT(_, _), OpInner::AnnotToStringT { .. }) => t,
        // Mirrors the dispatch pass-through: re-elaborating the key set can
        // repeatedly flatten a large dictionary key set.
        (TypeInner::KeysT(_, _), OpInner::AnnotGetKeysDictKeyT(_)) => t,
        (TypeInner::KeysT(reason, inner), _) => {
            let keys_t = elab_t(
                cx,
                Some(seen.dupe()),
                inner.dupe(),
                Op::new(OpInner::AnnotGetKeysT(reason.dupe())),
            );
            elab_t(cx, Some(seen), keys_t, op)
        }
        (TypeInner::DefT(_, def_t), OpInner::AnnotGetKeysT(reason_op))
            if let DefTInner::ObjT(obj) = def_t.deref() =>
        {
            let dict_t = flow_typing_flow_common::obj_type::get_dict_opt(&obj.flags.obj_kind);
            let props = cx.find_props(obj.props_tmap.dupe());
            // Type-level `keyof` includes `unique symbol` keys.
            let mut keylist =
                flow_js_utils::keylist_of_props(&FlowJsEnv::entry(), &props, reason_op, true);
            if let Some(dict) = dict_t {
                keylist.push(elab_dict_key(cx, &seen, reason_op, dict.key.dupe()));
            }
            type_util::union_of_ts(reason_op.dupe(), keylist, None)
        }
        (TypeInner::DefT(_, def_t), OpInner::AnnotGetKeysT(reason_op))
            if let DefTInner::InstanceT(inst_t) = def_t.deref() =>
        {
            let (prop_ids, dict_keys) = flow_js_utils::key_sources_of_instance_t(
                |reason, t| -> Result<Vec<Type>, flow_utils_concurrency::job_error::JobError> {
                    let collector = TypeCollector::create();
                    elab_t(
                        cx,
                        Some(seen.dupe()),
                        t.dupe(),
                        Op::new(OpInner::AnnotConcretizeForInspection {
                            reason: reason.dupe(),
                            collector: collector.clone(),
                        }),
                    );
                    Ok(collector.collect_to_vec())
                },
                inst_t,
            )
            .expect("annotation_inference::AnnotGetKeysT concretize closure is infallible");
            let keylist = flow_js_utils::keylist_of_prop_ids(
                cx,
                &FlowJsEnv::entry(),
                &prop_ids,
                reason_op,
                true,
            );
            let keylist = dict_keys.into_iter().fold(keylist, |mut keylist, key| {
                keylist.push(elab_dict_key(cx, &seen, reason_op, key));
                keylist
            });
            type_util::union_of_ts(reason_op.dupe(), keylist, None)
        }
        // keyof any -> any: any in any out.
        (TypeInner::AnyT(_, src), OpInner::AnnotGetKeysT(reason_op)) => {
            type_::any_t::why(*src, reason_op.dupe())
        }
        // **********
        //  $Values
        // **********
        (TypeInner::DefT(_, def_t), OpInner::AnnotGetValuesT(reason))
            if let DefTInner::ObjT(obj) = def_t.deref() =>
        {
            flow_js_utils::get_values_type_of_obj_t(cx, obj, reason.dupe())
        }
        (TypeInner::DefT(_, def_t), OpInner::AnnotGetValuesT(reason))
            if let DefTInner::InstanceT(inst_t) = def_t.deref() =>
        {
            flow_js_utils::get_values_type_of_instance_t(
                cx,
                inst_t.inst.own_props.dupe(),
                inst_t.inst.inst_dict.as_ref(),
                reason.dupe(),
            )
        }
        (TypeInner::DefT(_, def_t), OpInner::AnnotGetValuesT(reason))
            if let DefTInner::ArrT(arr) = def_t.deref() =>
        {
            let elem_t = type_::elemt_of_arrtype(arr.as_ref());
            type_util::mod_reason_of_t(&|_| reason.dupe(), &elem_t)
        }
        (TypeInner::AnyT(_, src), OpInner::AnnotGetValuesT(reason)) => {
            any_t::why(src.clone(), reason.dupe())
        }
        // ******************************
        //  Union and intersection types
        // ******************************
        (TypeInner::UnionT(_, _), OpInner::AnnotObjKitT(data)) => object_kit_concrete(
            cx,
            data.use_op.dupe(),
            &op,
            data.reason.dupe(),
            data.resolve_tool.clone(),
            data.tool.clone(),
            t,
        ),
        (TypeInner::UnionT(_, rep), _) => {
            let reason = op.reason();
            let ts: Vec<Type> = rep
                .members_iter()
                .map(|member| elab_t(cx, Some(seen.dupe()), member.dupe(), op.dupe()))
                .collect();
            type_util::union_of_ts(reason, ts, None)
        }
        (TypeInner::IntersectionT(_, _), OpInner::AnnotObjKitT(data)) => object_kit_concrete(
            cx,
            data.use_op.dupe(),
            &op,
            data.reason.dupe(),
            data.resolve_tool.clone(),
            data.tool.clone(),
            t,
        ),
        (
            TypeInner::IntersectionT(_, _),
            OpInner::AnnotConcretizeForInspection { collector, .. }
            | OpInner::AnnotConcretizeForClassExtends { collector, .. },
        ) => {
            collector.add(t.dupe());
            t
        }
        (TypeInner::IntersectionT(_, _), _) => error_unsupported(cx, &t, &op),
        // *************************
        //  ConcretizeForInspection
        // *************************
        (
            _,
            OpInner::AnnotConcretizeForInspection { collector, .. }
            | OpInner::AnnotConcretizeForClassExtends { collector, .. },
        ) => {
            collector.add(t.dupe());
            t
        }
        // ***********
        //  Unary not
        // ***********
        (_, OpInner::AnnotNotT(reason)) => {
            match t.deref() {
                // any propagation
                TypeInner::AnyT(_, _) => return t,
                TypeInner::DefT(_, def_t) => {
                    match def_t.deref() {
                        DefTInner::BoolGeneralT
                        | DefTInner::StrGeneralT(flow_typing_type::type_::Literal::AnyLiteral)
                        | DefTInner::NumGeneralT(flow_typing_type::type_::Literal::AnyLiteral) => {
                            return type_::bool_module_t::at(reason.loc().dupe());
                        }
                        // !x when x is falsy
                        DefTInner::SingletonBoolT { value: false, .. }
                        | DefTInner::NullT
                        | DefTInner::VoidT => {
                            let reason = reason.dupe().replace_desc(
                                flow_common::reason::VirtualReasonDesc::RBooleanLit(true),
                            );
                            return Type::new(TypeInner::DefT(
                                reason,
                                type_::DefT::new(DefTInner::SingletonBoolT {
                                    value: true,
                                    from_annot: false,
                                }),
                            ));
                        }
                        DefTInner::SingletonStrT { value, .. } if value.is_empty() => {
                            let reason = reason.dupe().replace_desc(
                                flow_common::reason::VirtualReasonDesc::RBooleanLit(true),
                            );
                            return Type::new(TypeInner::DefT(
                                reason,
                                type_::DefT::new(DefTInner::SingletonBoolT {
                                    value: true,
                                    from_annot: false,
                                }),
                            ));
                        }
                        DefTInner::SingletonNumT { value, .. } if value.0 == 0.0 => {
                            let reason = reason.dupe().replace_desc(
                                flow_common::reason::VirtualReasonDesc::RBooleanLit(true),
                            );
                            return Type::new(TypeInner::DefT(
                                reason,
                                type_::DefT::new(DefTInner::SingletonBoolT {
                                    value: true,
                                    from_annot: false,
                                }),
                            ));
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
            // !x when x is truthy
            let reason = reason
                .dupe()
                .replace_desc(flow_common::reason::VirtualReasonDesc::RBooleanLit(false));
            Type::new(TypeInner::DefT(
                reason,
                type_::DefT::new(DefTInner::SingletonBoolT {
                    value: false,
                    from_annot: false,
                }),
            ))
        }
        // ********
        //  Mixins
        // ********
        (TypeInner::AnyT(_, src), OpInner::AnnotMixinT(r)) => any_t::why(*src, r.dupe()),
        (_, OpInner::AnnotMixinT(r)) => {
            let make_mixin_instance = |inst_r: &Reason,
                                       inst: &type_::InstanceT,
                                       is_this: bool,
                                       this_name: &flow_common::subst_name::SubstName,
                                       class_r: &Reason,
                                       r: &Reason|
             -> Type {
                let static_ = Type::new(TypeInner::ObjProtoT(r.dupe()));
                let super_ = Type::new(TypeInner::ObjProtoT(r.dupe()));
                let instance = type_::InstanceT::new(type_::InstanceTInner {
                    static_,
                    super_,
                    implements: Rc::from([]),
                    inst: inst.inst.clone(),
                });
                Type::new(TypeInner::DefT(
                    class_r.dupe(),
                    type_::DefT::new(DefTInner::ClassT(Type::new(TypeInner::ThisInstanceT(
                        Box::new(ThisInstanceTData {
                            reason: inst_r.dupe(),
                            instance,
                            is_this,
                            subst_name: this_name.clone(),
                        }),
                    )))),
                ))
            };

            if let TypeInner::DefT(class_r, def_t) = t.deref() {
                match def_t.deref() {
                    DefTInner::ClassT(inner) => {
                        if let TypeInner::ThisInstanceT(box ThisInstanceTData {
                            reason: inst_r,
                            instance: i,
                            is_this,
                            subst_name,
                        }) = inner.deref()
                        {
                            make_mixin_instance(inst_r, i, *is_this, subst_name, class_r, r)
                        } else {
                            error_unsupported(cx, &t, &op)
                        }
                    }
                    DefTInner::PolyT(box PolyTData {
                        tparams_loc,
                        kind: poly_kind,
                        tparams: xs,
                        t_out,
                        strictness_kind,
                        ..
                    }) => {
                        if let TypeInner::DefT(class_r2, inner_def_t) = t_out.deref() {
                            if let DefTInner::ClassT(inner) = inner_def_t.deref() {
                                if let TypeInner::ThisInstanceT(box ThisInstanceTData {
                                    reason: inst_r,
                                    instance: i,
                                    is_this,
                                    subst_name,
                                }) = inner.deref()
                                {
                                    let t_out = make_mixin_instance(
                                        inst_r, i, *is_this, subst_name, class_r2, r,
                                    );
                                    type_util::poly_type(
                                        type_::poly::Id::generate_id(),
                                        tparams_loc.dupe(),
                                        vec1::Vec1::try_from_vec(xs.to_vec())
                                            .expect("tparams should be non-empty"),
                                        t_out,
                                        *strictness_kind,
                                        *poly_kind,
                                    )
                                } else {
                                    error_unsupported(cx, &t, &op)
                                }
                            } else {
                                error_unsupported(cx, &t, &op)
                            }
                        } else {
                            error_unsupported(cx, &t, &op)
                        }
                    }
                    _ => error_unsupported(cx, &t, &op),
                }
            } else {
                error_unsupported(cx, &t, &op)
            }
        }

        // ********************
        //  Type specialization
        // ********************
        (TypeInner::DefT(_, def_t), OpInner::AnnotSpecializeT(data))
            if let DefTInner::PolyT(box PolyTData {
                kind: _,
                tparams_loc,
                tparams: xs,
                t_out,
                id,
                strictness_kind: _,
            }) = def_t.deref() =>
        {
            let ts: Rc<[Type]> = data
                .types
                .as_ref()
                .map(|rc| rc.dupe())
                .unwrap_or_else(|| Rc::from([]));
            mk_typeapp_of_poly(
                cx,
                data.use_op.dupe(),
                data.reason.dupe(),
                data.reason2.dupe(),
                id.clone(),
                tparams_loc.dupe(),
                xs.to_vec(),
                t_out.dupe(),
                ts,
            )
        }
        (TypeInner::DefT(_, def_t), OpInner::AnnotSpecializeT(data))
            if data.types.is_none() && matches!(def_t.deref(), DefTInner::ClassT(_)) =>
        {
            t
        }
        (TypeInner::AnyT(_, _), OpInner::AnnotSpecializeT(_)) => t,
        // Mirrors the [SpecializeT] rule for a value with a construct
        // signature or a `prototype`: it is class-like, so extending it is
        // allowed and there is nothing to specialize. Without this a
        // signature-verified file falls through to [general_error] and every
        // consumer of the class sees [any].
        (TypeInner::DefT(_, def_t), OpInner::AnnotSpecializeT(data))
            if data.types.is_none() && matches!(def_t.deref(), DefTInner::InstanceT(_)) =>
        {
            let concretize =
                |cx: &mut Context<'_>, t: &Type| -> Result<Vec<Type>, FlowJsException> {
                    let collector = TypeCollector::create();
                    elab_t(
                        cx,
                        Some(seen.dupe()),
                        t.dupe(),
                        Op::new(OpInner::AnnotConcretizeForInspection {
                            reason: type_util::reason_of_t(t).dupe(),
                            collector: collector.clone(),
                        }),
                    );
                    Ok(collector.collect_to_vec())
                };
            let inheritable = flow_js_utils::is_inheritable(&concretize, cx, &t)
                .expect("annotation_inference concretize closure is infallible");
            if inheritable {
                t
            } else {
                general_error(cx, &t, &op)
            }
        }
        (
            TypeInner::DefT(_, def_t),
            OpInner::AnnotThisSpecializeT {
                reason,
                type_: this,
            },
        ) if let DefTInner::ClassT(instance) = def_t.deref()
            && let TypeInner::ThisInstanceT(box ThisInstanceTData {
                reason: r,
                instance: i,
                is_this: _,
                subst_name,
            }) = instance.deref() =>
        {
            let this_type = this.dupe();
            let map = flow_data_structure_wrapper::ord_map::FlowOrdMap::from_iter(std::iter::once(
                (subst_name.clone(), this_type),
            ));
            let i = flow_typing_flow_common::type_subst::subst_instance_type(
                cx,
                None,
                false,
                false,
                flow_typing_flow_common::type_subst::Purpose::Normal,
                &map,
                i,
            );
            reposition(
                cx,
                reason.loc().dupe(),
                Type::new(TypeInner::DefT(
                    r.dupe(),
                    type_::DefT::new(DefTInner::InstanceT(Rc::new(i))),
                )),
            )
        }
        (
            TypeInner::ThisInstanceT(box ThisInstanceTData {
                reason: r,
                instance: i,
                is_this: _,
                subst_name,
            }),
            OpInner::AnnotThisSpecializeT {
                reason,
                type_: this,
            },
        ) => {
            let this_type = this.dupe();
            let map = flow_data_structure_wrapper::ord_map::FlowOrdMap::from_iter(std::iter::once(
                (subst_name.clone(), this_type),
            ));
            let i = flow_typing_flow_common::type_subst::subst_instance_type(
                cx,
                None,
                false,
                false,
                flow_typing_flow_common::type_subst::Purpose::Normal,
                &map,
                i,
            );
            reposition(
                cx,
                reason.loc().dupe(),
                Type::new(TypeInner::DefT(
                    r.dupe(),
                    type_::DefT::new(DefTInner::InstanceT(Rc::new(i))),
                )),
            )
        }
        // this-specialization of non-this-abstracted classes is a no-op
        (TypeInner::DefT(_, def_t), OpInner::AnnotThisSpecializeT { reason, .. })
            if let DefTInner::ClassT(instance) = def_t.deref() =>
        {
            reposition(cx, reason.loc().dupe(), instance.dupe())
        }
        // Mirrors the [ThisSpecializeT] rule for a value with a construct
        // signature: the instance the derived class inherits from is what the
        // first signature returns, carrying the constructor object's statics
        // and construct signature.
        (
            TypeInner::DefT(_, def_t),
            OpInner::AnnotThisSpecializeT {
                reason,
                type_: this,
            },
        ) if matches!(def_t.deref(), DefTInner::InstanceT(_)) => {
            let concretize = |cx: &mut Context<'_>,
                              t: &Type,
                              this_t: Option<&Type>|
             -> Result<Vec<Type>, FlowJsException> {
                let collector = TypeCollector::create();
                let concretize_op = match this_t {
                    Some(this_t) => OpInner::AnnotConcretizeForClassExtends {
                        reason: type_util::reason_of_t(t).dupe(),
                        collector: collector.clone(),
                        this_t: this_t.dupe(),
                    },
                    None => OpInner::AnnotConcretizeForInspection {
                        reason: type_util::reason_of_t(t).dupe(),
                        collector: collector.clone(),
                    },
                };
                elab_t(cx, Some(seen.dupe()), t.dupe(), Op::new(concretize_op));
                Ok(collector.collect_to_vec())
            };
            let base = flow_js_utils::construct_base_instance(&concretize, cx, &t, this)
                .expect("annotation_inference concretize closure is infallible");
            match base {
                Some(base) => reposition(cx, reason.loc().dupe(), base),
                // No instance to inherit after all; the specialize step above has
                // already reported it.
                None => any_t::error(reason.dupe()),
            }
        }
        (TypeInner::AnyT(_, _), OpInner::AnnotThisSpecializeT { reason, .. }) => {
            reposition(cx, reason.loc().dupe(), t)
        }

        // *********************
        //  Type instantiation
        // *********************
        (TypeInner::DefT(reason_tapp, def_t), _)
            if let DefTInner::PolyT(box PolyTData {
                tparams_loc,
                tparams: ids,
                t_out,
                ..
            }) = def_t.deref() =>
        {
            let use_op = type_::unknown_use();
            let reason_op = op.reason();
            let t = instantiate_poly(
                cx,
                use_op,
                reason_op,
                reason_tapp.dupe(),
                tparams_loc.dupe(),
                ids.to_vec(),
                t_out.dupe(),
            );
            elab_t(cx, Some(seen), t, op)
        }
        (
            TypeInner::ThisInstanceT(box ThisInstanceTData {
                reason: r,
                instance: i,
                is_this,
                subst_name,
            }),
            _,
        ) => {
            let reason = op.reason();
            let fixed = flow_js_utils::fix_this_instance(
                cx,
                reason,
                r.dupe(),
                i,
                *is_this,
                subst_name.dupe(),
            );
            elab_t(cx, Some(seen), fixed, op)
        }
        // ***************************
        //  React Abstract Components
        // ***************************
        (TypeInner::DefT(reason, def_t), _)
            if matches!(def_t.deref(), DefTInner::ReactAbstractComponentT(_))
                && matches!(
                    op.deref(),
                    OpInner::AnnotGetPropT(_) | OpInner::AnnotGetElemT { .. }
                ) =>
        {
            let statics = flow_js_utils::lookup_builtin_type(
                cx,
                "React$AbstractComponentStatics",
                reason.dupe(),
            );
            elab_t(cx, Some(seen), statics, op)
        }
        // ***************
        //  ObjTestProtoT
        // ***************
        (TypeInner::AnyT(_, src), OpInner::AnnotObjTestProtoT(reason_op)) => {
            any_t::why(src.clone(), reason_op.dupe())
        }
        (TypeInner::DefT(_, def_t), OpInner::AnnotObjTestProtoT(reason_op))
            if matches!(def_t.deref(), DefTInner::NullT) =>
        {
            type_::null_proto::why(reason_op.dupe())
        }
        (_, OpInner::AnnotObjTestProtoT(reason_op)) => {
            if flow_js_utils::object_like(&t) {
                reposition(cx, reason_op.loc().dupe(), t)
            } else {
                flow_js_utils::add_output_non_speculating(
                    cx,
                    flow_js_utils::invalid_prototype_error(reason_op.loc().dupe(), &t),
                );
                type_::obj_proto::why(reason_op.dupe())
            }
        }
        // *************
        //  Get statics
        // *************
        (TypeInner::DefT(_, def_t), OpInner::AnnotGetStaticsT(reason_op))
            if let DefTInner::InstanceT(inst) = def_t.deref() =>
        {
            reposition(cx, reason_op.loc().dupe(), inst.static_.dupe())
        }
        (TypeInner::AnyT(_, src), OpInner::AnnotGetStaticsT(reason_op)) => {
            any_t::why(src.clone(), reason_op.dupe())
        }
        (TypeInner::ObjProtoT(_), OpInner::AnnotGetStaticsT(reason_op)) => {
            // ObjProtoT not only serves as the instance type of the root class, but
            // also as the statics of the root class.
            reposition(cx, reason_op.loc().dupe(), t)
        }

        // *************
        //  LookupT pt1
        // *************
        (TypeInner::AnyT(_, _), OpInner::AnnotLookupT(data)) => any_t::untyped(data.reason.dupe()),
        (_, OpInner::AnnotLookupT(data)) => {
            let AnnotLookupTData {
                reason: reason_op,
                use_op,
                prop_ref: propref,
                type_: objt,
                indexer_fallback,
            } = data.as_ref();
            use flow_typing_flow_common::flow_js_utils::get_prop_t_kit;
            use flow_typing_type::type_::property;

            match t.deref() {
                TypeInner::DefT(_, def_t) if let DefTInner::InstanceT(inst_t) = def_t.deref() => {
                    match propref {
                        type_::PropRef::Named { .. } => {
                            let react_dro = match objt.deref() {
                                TypeInner::DefT(_, d) => match d.deref() {
                                    DefTInner::ObjT(o) => o.flags.react_dro.clone(),
                                    _ => None,
                                },
                                _ => None,
                            };
                            let trace = dummy_trace();
                            let (property, candidate) =
                                get_prop_t_kit::get_instance_prop_for_lookup::<AnnotGetPropHelper>(
                                    cx,
                                    &FlowJsEnv::entry(),
                                    &trace,
                                    use_op,
                                    true,
                                    &inst_t.inst,
                                    propref,
                                    reason_op,
                                    IndexerFallbackMode::PropertyAndIndexedAccess,
                                    &t,
                                )
                                // Annotation inference is never speculative
                                .unwrap();
                            match property {
                                Some((p, _)) => {
                                    get_prop_t_kit::perform_read_prop_action::<AnnotGetPropHelper>(
                                        cx,
                                        &FlowJsEnv::entry(),
                                        &trace,
                                        use_op.dupe(),
                                        propref,
                                        property::property_type(&p),
                                        reason_op,
                                        &react_dro,
                                    )
                                    // Annotation inference is never speculative
                                    .unwrap()
                                }
                                // A candidate found closer to the access wins.
                                None => cg_lookup_(
                                    cx,
                                    use_op,
                                    inst_t.super_.dupe(),
                                    reason_op,
                                    propref,
                                    objt,
                                    indexer_fallback.clone().or(candidate),
                                ),
                            }
                        }
                        type_::PropRef::Computed(_) => error_unsupported(cx, &t, &op),
                    }
                }
                TypeInner::DefT(_, def_t) if let DefTInner::ObjT(o) = def_t.deref() => {
                    let react_dro = match objt.deref() {
                        TypeInner::DefT(_, d) => match d.deref() {
                            DefTInner::ObjT(o) => o.flags.react_dro.clone(),
                            _ => None,
                        },
                        _ => None,
                    };
                    let trace = dummy_trace();
                    match get_prop_t_kit::get_obj_prop::<AnnotGetPropHelper>(
                        cx,
                        &FlowJsEnv::entry(),
                        &trace,
                        &type_::unknown_use(),
                        false,
                        true,
                        o.as_ref(),
                        propref,
                        reason_op,
                    )
                    // Annotation inference is never speculative
                    .unwrap()
                    {
                        Some((p, _)) => {
                            get_prop_t_kit::perform_read_prop_action::<AnnotGetPropHelper>(
                                cx,
                                &FlowJsEnv::entry(),
                                &trace,
                                use_op.dupe(),
                                propref,
                                p,
                                reason_op,
                                &react_dro,
                            )
                            // Annotation inference is never speculative
                            .unwrap()
                        }
                        None => cg_lookup_(
                            cx,
                            use_op,
                            o.proto_t.dupe(),
                            reason_op,
                            propref,
                            objt,
                            indexer_fallback.clone(),
                        ),
                    }
                }
                // ***************
                //  LookupT pt2
                // ***************
                TypeInner::ObjProtoT(_)
                    if let flow_typing_type::type_::PropRef::Named { name, .. } = propref
                        && flow_js_utils::is_object_prototype_method(name) =>
                {
                    flow_js_utils::lookup_builtin_value(cx, "Object", reason_op.dupe())
                }
                TypeInner::FunProtoT(_)
                    if let flow_typing_type::type_::PropRef::Named { name, .. } = propref
                        && flow_js_utils::is_function_prototype(name) =>
                {
                    flow_js_utils::lookup_builtin_value(cx, "Function", reason_op.dupe())
                }
                TypeInner::DefT(_, def_t)
                    if matches!(def_t.deref(), DefTInner::NullT)
                        && let type_::PropRef::Named {
                            reason: reason_prop,
                            name,
                            ..
                        } = propref =>
                {
                    annot_lookup_failed(
                        cx,
                        reason_op,
                        reason_prop,
                        name,
                        use_op,
                        propref,
                        objt,
                        indexer_fallback.as_deref(),
                    )
                }
                TypeInner::ObjProtoT(_) | TypeInner::FunProtoT(_)
                    if let type_::PropRef::Named {
                        reason: reason_prop,
                        name,
                        ..
                    } = propref =>
                {
                    annot_lookup_failed(
                        cx,
                        reason_op,
                        reason_prop,
                        name,
                        use_op,
                        propref,
                        objt,
                        indexer_fallback.as_deref(),
                    )
                }
                _ => elab_t_wildcard_op(cx, seen, t, op),
            }
        }

        // ******
        //  DRO
        // ******
        (_, OpInner::AnnotDeepReadOnlyT(data)) => {
            use flow_typing_type::type_::ArrType;
            use flow_typing_type::type_::ArrayATData;
            use flow_typing_type::type_::ReactDro;
            use flow_typing_type::type_::TupleATData;

            let dro = Some(ReactDro(data.loc.dupe(), data.dro_type.clone()));

            match t.deref() {
                TypeInner::DefT(r, def_t) => match def_t.deref() {
                    DefTInner::ObjT(o) => {
                        let mut o = (**o).clone();
                        o.flags.react_dro = dro;
                        Type::new(TypeInner::DefT(
                            r.dupe(),
                            type_::DefT::new(DefTInner::ObjT(Rc::new(o))),
                        ))
                    }
                    DefTInner::ArrT(arr) => match arr.deref() {
                        ArrType::TupleAT(box TupleATData {
                            elem_t,
                            elements,
                            arity,
                            inexact,
                            rest,
                            react_dro: _,
                            strictness_kind,
                            ..
                        }) => Type::new(TypeInner::DefT(
                            r.dupe(),
                            type_::DefT::new(DefTInner::ArrT(Rc::new(ArrType::TupleAT(Box::new(
                                TupleATData {
                                    react_dro: dro,
                                    elem_t: elem_t.dupe(),
                                    elements: elements.dupe(),
                                    arity: *arity,
                                    inexact: *inexact,
                                    rest: rest.dupe(),
                                    strictness_kind: *strictness_kind,
                                    literal: arr.literal(),
                                    fresh: arr.fresh(),
                                },
                            ))))),
                        )),
                        ArrType::ArrayAT(box ArrayATData {
                            elem_t,
                            tuple_view,
                            react_dro: _,
                            strictness_kind,
                            ..
                        }) => Type::new(TypeInner::DefT(
                            r.dupe(),
                            type_::DefT::new(DefTInner::ArrT(Rc::new(ArrType::ArrayAT(Box::new(
                                ArrayATData {
                                    react_dro: dro,
                                    elem_t: elem_t.dupe(),
                                    tuple_view: tuple_view.clone(),
                                    strictness_kind: *strictness_kind,
                                    literal: arr.literal(),
                                    fresh: arr.fresh(),
                                },
                            ))))),
                        )),
                        ArrType::ROArrayAT(box ROArrayATData {
                            elem_t: inner_t, ..
                        }) => Type::new(TypeInner::DefT(
                            r.dupe(),
                            type_::DefT::new(DefTInner::ArrT(Rc::new(ArrType::ROArrayAT(
                                Box::new(ROArrayATData {
                                    elem_t: inner_t.dupe(),
                                    react_dro: dro,
                                    literal: arr.literal(),
                                }),
                            )))),
                        )),
                    },
                    _ => t,
                },
                _ => t,
            }
        }

        // **********
        //  ObjRestT
        // **********
        (TypeInner::AnyT(_, src), OpInner::AnnotObjRestT { reason, .. }) => {
            any_t::why(src.clone(), reason.dupe())
        }
        (
            _,
            OpInner::AnnotObjRestT {
                reason: reason_op,
                keys: xs,
            },
        ) => {
            match t.deref() {
                TypeInner::DefT(reason_obj, def_t) if let DefTInner::ObjT(o) = def_t.deref() => {
                    flow_js_utils::objt_to_obj_rest(
                        cx,
                        &FlowJsEnv::entry(),
                        o.props_tmap.dupe(),
                        Some(Rc::from([])),
                        o.flags.obj_kind.clone(),
                        reason_op,
                        reason_obj,
                        xs,
                        true,
                    )
                    // Annotation inference is never speculative
                    .unwrap()
                }
                TypeInner::DefT(_, def_t) if matches!(def_t.deref(), DefTInner::InstanceT(_)) => {
                    // This implementation relies on unsealed objects and set-prop logic that is
                    // hard to implement in annotation inference.
                    error_unsupported(cx, &t, &op)
                }
                TypeInner::ObjProtoT(_) => flow_typing_flow_common::obj_type::mk_with_proto(
                    cx,
                    reason_op.dupe(),
                    type_::ObjKind::Exact,
                    None,
                    None,
                    None,
                    None,
                    t,
                    true,
                ),
                TypeInner::DefT(_, def_t)
                    if matches!(def_t.deref(), DefTInner::NullT | DefTInner::VoidT) =>
                {
                    flow_typing_flow_common::obj_type::mk(
                        type_::ObjKind::Exact,
                        cx,
                        reason_op.dupe(),
                        true,
                    )
                }
                TypeInner::NamespaceT(ns) => elab_t(cx, Some(seen), ns.values_type.dupe(), op),
                _ => {
                    let reason_op_catch = op.reason();
                    let upper = IncompatibleUpperData {
                        loc: reason_op_catch.loc().dupe(),
                        kind:
                            flow_typing_errors::error_message::UpperKind::IncompatibleUnclassified(
                                op.string_of_operation().into(),
                            ),
                    };
                    let use_op = op.use_op();
                    flow_js_utils::add_output_non_speculating(
                        cx,
                        flow_js_utils::incompatible_type_error(&t, upper, use_op),
                    );
                    any_t::error(reason_op_catch)
                }
            }
        }

        // **********************************
        //  Namespace and type qualification
        // **********************************
        (TypeInner::NamespaceT(ns), OpInner::AnnotGetTypeFromNamespaceT(data)) => {
            let AnnotGetTypeFromNamespaceTData {
                reason,
                use_op,
                prop_ref,
            } = data.as_ref();
            let (prop_ref_reason, prop_name) = prop_ref;
            match cx
                .find_props(ns.types_tmap.dupe())
                .get(prop_name)
                .and_then(type_::property::read_t)
            {
                Some(prop) => prop,
                None => elab_t(
                    cx,
                    Some(seen),
                    ns.values_type.dupe(),
                    Op::new(OpInner::AnnotGetPropT(Box::new(AnnotGetPropTData {
                        reason: reason.dupe(),
                        use_op: use_op.dupe(),
                        from_annot: false,
                        prop_ref: type_util::mk_named_prop(
                            prop_ref_reason.dupe(),
                            false,
                            prop_name.dupe(),
                        ),
                    }))),
                ),
            }
        }
        (TypeInner::NamespaceT(ns), _) => elab_t(cx, Some(seen), ns.values_type.dupe(), op),
        (_, OpInner::AnnotGetTypeFromNamespaceT(data)) => {
            let AnnotGetTypeFromNamespaceTData {
                reason,
                use_op,
                prop_ref,
            } = data.as_ref();
            let (prop_ref_reason, prop_name) = prop_ref;
            elab_t(
                cx,
                Some(seen),
                t,
                Op::new(OpInner::AnnotGetPropT(Box::new(AnnotGetPropTData {
                    reason: reason.dupe(),
                    use_op: use_op.dupe(),
                    from_annot: false,
                    prop_ref: type_util::mk_named_prop(
                        prop_ref_reason.dupe(),
                        false,
                        prop_name.dupe(),
                    ),
                }))),
            )
        }
        // **********
        //  GetPropT
        // **********
        (TypeInner::AnyT(_, _), OpInner::AnnotGetPropT(data)) => Type::new(TypeInner::AnyT(
            data.reason.dupe(),
            type_::AnySource::Untyped,
        )),
        (_, OpInner::AnnotGetPropT(data)) => {
            let AnnotGetPropTData {
                reason: reason_op,
                use_op,
                from_annot,
                prop_ref: propref,
            } = data.as_ref();
            use flow_typing_flow_common::flow_js_utils::get_prop_t_kit;

            match t.deref() {
                TypeInner::DefT(_, def_t) if let DefTInner::InstanceT(inst_t) = def_t.deref() => {
                    match propref {
                        type_::PropRef::Named { .. } => {
                            let trace = dummy_trace();
                            get_prop_t_kit::read_instance_prop::<AnnotGetPropHelper>(
                                cx,
                                &FlowJsEnv::entry(),
                                &trace,
                                use_op,
                                &t,
                                None,
                                false,
                                inst_t.super_.dupe(),
                                type_::LookupKind::Strict(t.dupe()),
                                &type_::hint_unavailable(),
                                false,
                                &inst_t.inst,
                                propref,
                                reason_op,
                            )
                            // Annotation inference is never speculative
                            .unwrap()
                        }
                        type_::PropRef::Computed(_) => error_unsupported(cx, &t, &op),
                    }
                }
                TypeInner::DefT(reason_obj, def_t) if let DefTInner::ObjT(o) = def_t.deref() => {
                    if let type_::PropRef::Named { name, .. } = propref
                        && name.matches_str("constructor")
                    {
                        type_::unsoundness::why(
                            type_::UnsoundnessKind::Constructor,
                            reason_op.dupe(),
                        )
                    } else {
                        let trace = dummy_trace();
                        get_prop_t_kit::read_obj_prop::<AnnotGetPropHelper>(
                            cx,
                            &FlowJsEnv::entry(),
                            &trace,
                            use_op.dupe(),
                            *from_annot,
                            false,
                            o.as_ref(),
                            propref,
                            reason_obj.dupe(),
                            reason_op.dupe(),
                            None,
                        )
                        // Annotation inference is never speculative
                        .unwrap()
                    }
                }
                TypeInner::DefT(reason, def_t)
                    if let DefTInner::ClassT(instance) = def_t.deref()
                        && let type_::PropRef::Named { name, .. } = propref
                        && name.matches_str("prototype") =>
                {
                    reposition(cx, reason.loc().dupe(), instance.dupe())
                }
                // *********
                //  Enums
                // *********
                TypeInner::DefT(_, def_t)
                    if let DefTInner::EnumObjectT {
                        enum_value_t,
                        enum_info,
                    } = def_t.deref()
                        && let EnumInfoInner::ConcreteEnum(enum_concrete_info) =
                            enum_info.deref().deref()
                        && let type_::PropRef::Named {
                            reason: prop_reason,
                            name,
                            ..
                        } = propref =>
                {
                    let trace = dummy_trace();
                    let access = (
                        use_op.dupe(),
                        reason_op.dupe(),
                        None,
                        (prop_reason.dupe(), name.dupe()),
                    );
                    flow_js_utils::get_prop_t_kit::on_enum_object_t::<AnnotGetPropHelper>(
                        cx,
                        &FlowJsEnv::entry(),
                        &trace,
                        t.dupe(),
                        enum_value_t.dupe(),
                        enum_concrete_info,
                        &access,
                    )
                    // Annotation inference is never speculative
                    .unwrap()
                }
                // **********************
                //  GetPropT (for arrays)
                // **********************
                TypeInner::DefT(reason, def_t)
                    if let DefTInner::ArrT(arr) = def_t.deref()
                        && let flow_typing_type::type_::ArrType::TupleAT(box TupleATData {
                            arity,
                            inexact,
                            rest,
                            ..
                        }) = arr.deref()
                        && let type_::PropRef::Named { name, .. } = propref
                        && name == &Name::new("length") =>
                {
                    let trace = dummy_trace();
                    flow_js_utils::get_prop_t_kit::on_array_length::<AnnotGetPropHelper>(
                        cx,
                        &FlowJsEnv::entry(),
                        &trace,
                        reason.dupe(),
                        *inexact || rest.is_some(),
                        *arity,
                        reason_op.loc().dupe(),
                    )
                    // Annotation inference is never speculative
                    .unwrap()
                }
                TypeInner::DefT(reason, def_t) if let DefTInner::ArrT(arr) = def_t.deref() => {
                    use flow_typing_type::type_::ArrType;
                    match arr.as_ref() {
                        ArrType::ArrayAT(box ArrayATData { elem_t, .. }) => {
                            let arr_t = get_builtin_typeapp(
                                cx,
                                reason.dupe(),
                                "Array",
                                vec![elem_t.dupe()],
                            );
                            elab_t(cx, Some(seen), arr_t, op)
                        }
                        ArrType::TupleAT(box TupleATData { .. })
                        | ArrType::ROArrayAT(box ROArrayATData { .. }) => {
                            let elem_t = type_::elemt_of_arrtype(arr.as_ref());
                            let arr_t = get_builtin_typeapp(
                                cx,
                                reason.dupe(),
                                "ReadonlyArray",
                                vec![elem_t],
                            );
                            elab_t(cx, Some(seen), arr_t, op)
                        }
                    }
                }
                _ => elab_t_wildcard_op(cx, seen, t, op),
            }
        }
        // ************
        //  Object Kit
        // ************
        (_, OpInner::AnnotObjKitT(data)) => object_kit_concrete(
            cx,
            data.use_op.dupe(),
            &op,
            data.reason.dupe(),
            data.resolve_tool.clone(),
            data.tool.clone(),
            t,
        ),
        // ******************
        //  GetElemT / ElemT
        // ******************
        (TypeInner::DefT(_, def_t), OpInner::AnnotGetElemT { reason, .. })
            if matches!(
                def_t.deref(),
                DefTInner::StrGeneralT(_) | DefTInner::SingletonStrT { .. }
            ) =>
        {
            // NOTE bypassing check that index is a number
            type_::str_module_t::why(reason.dupe())
        }
        (
            TypeInner::AnyT(_, _),
            OpInner::AnnotGetElemT {
                reason,
                use_op,
                key,
            },
        ) => {
            let key_type = key.dupe();
            elab_t(
                cx,
                Some(seen),
                key_type,
                Op::new(OpInner::AnnotElemT {
                    reason: reason.dupe(),
                    use_op: use_op.dupe(),
                    from_annot: false,
                    source: t.dupe(),
                }),
            )
        }
        (
            TypeInner::DefT(_, def_t),
            OpInner::AnnotGetElemT {
                reason,
                use_op,
                key,
            },
        ) if matches!(
            def_t.deref(),
            DefTInner::ObjT(_) | DefTInner::ArrT(_) | DefTInner::InstanceT(_)
        ) =>
        {
            let key_type = key.dupe();
            elab_t(
                cx,
                Some(seen),
                key_type,
                Op::new(OpInner::AnnotElemT {
                    reason: reason.dupe(),
                    use_op: use_op.dupe(),
                    from_annot: false,
                    source: t.dupe(),
                }),
            )
        }
        (
            _,
            OpInner::AnnotElemT {
                reason: reason_op,
                use_op,
                from_annot,
                source,
            },
        ) => {
            let source_type = source.dupe();
            match (t.deref(), source.deref()) {
                (_, TypeInner::DefT(_, source_def_t))
                    if matches!(
                        source_def_t.deref(),
                        DefTInner::ObjT(_) | DefTInner::InstanceT(_)
                    ) =>
                {
                    let prop_ref = flow_js_utils::propref_for_elem_t(cx, &t);
                    elab_t(
                        cx,
                        Some(seen),
                        source_type,
                        Op::new(OpInner::AnnotGetPropT(Box::new(AnnotGetPropTData {
                            reason: reason_op.dupe(),
                            from_annot: *from_annot,
                            use_op: use_op.dupe(),
                            prop_ref,
                        }))),
                    )
                }
                (_, TypeInner::AnyT(_, _)) => {
                    let value = type_::any_t::untyped(reason_op.dupe());
                    reposition(cx, reason_op.loc().dupe(), value)
                }
                (TypeInner::AnyT(_, _), TypeInner::DefT(_, source_def_t))
                    if let DefTInner::ArrT(arrtype) = source_def_t.deref() =>
                {
                    let value = type_::elemt_of_arrtype(arrtype.as_ref());
                    reposition(cx, reason_op.loc().dupe(), value)
                }
                (TypeInner::DefT(_, elem_def_t), TypeInner::DefT(reason_tup, source_def_t))
                    if matches!(
                        elem_def_t.deref(),
                        DefTInner::NumGeneralT(_) | DefTInner::SingletonNumT { .. }
                    ) && let DefTInner::ArrT(arrtype) = source_def_t.deref() =>
                {
                    let (value, _, _, _) = flow_js_utils::array_elem_check(
                        cx,
                        &FlowJsEnv::entry(),
                        false,
                        *from_annot,
                        &source_type,
                        &t,
                        use_op.dupe(),
                        reason_op,
                        reason_tup,
                        arrtype.as_ref(),
                    )
                    // Annotation inference is never speculative
                    .unwrap();
                    reposition(cx, reason_op.loc().dupe(), value)
                }
                _ => general_error(cx, &t, &op),
            }
        }
        (TypeInner::DefT(_, def_t), OpInner::AnnotObjKeyMirror(reason_op))
            if let DefTInner::ObjT(o) = def_t.deref() =>
        {
            flow_js_utils::obj_key_mirror(cx, &FlowJsEnv::entry(), o, reason_op)
        }
        (_, OpInner::AnnotObjKeyMirror(_)) => error_unsupported(cx, &t, &op),
        (TypeInner::DefT(_, def_t), OpInner::AnnotGetEnumT(reason))
            if let DefTInner::EnumValueT(enum_info) = def_t.deref() =>
        {
            Type::new(TypeInner::DefT(
                reason.dupe(),
                type_::DefT::new(DefTInner::EnumObjectT {
                    enum_value_t: t.dupe(),
                    enum_info: enum_info.clone(),
                }),
            ))
        }
        (_, OpInner::AnnotGetEnumT(_)) => error_unsupported(cx, &t, &op),
        // *********************
        //  Opaque types (pt 2)
        // *********************
        (TypeInner::NominalT { nominal_type, .. }, _) if nominal_type.upper_t.is_some() => elab_t(
            cx,
            Some(seen),
            nominal_type.upper_t.as_ref().unwrap().dupe(),
            op,
        ),
        // ************************
        //  Binary arith operators
        // ************************
        (_, OpInner::AnnotArithT(data)) => {
            let AnnotArithTData {
                reason,
                flip,
                rhs_t,
                kind,
            } = data.as_ref();
            let rhs_type = rhs_t.dupe();
            if flow_js_utils::needs_resolution(&rhs_type) || flow_js_utils::is_generic(&rhs_type) {
                elab_t(
                    cx,
                    Some(seen),
                    rhs_type,
                    Op::new(OpInner::AnnotArithT(Box::new(AnnotArithTData {
                        reason: reason.dupe(),
                        flip: !flip,
                        rhs_t: t.dupe(),
                        kind: kind.clone(),
                    }))),
                )
            } else {
                let (lhs_t, rhs_t) = if *flip {
                    (&rhs_type, &t)
                } else {
                    (&t, &rhs_type)
                };
                // Annotation inference is never speculative
                flow_js_utils::flow_arith(
                    cx,
                    &FlowJsEnv::entry(),
                    reason.dupe(),
                    lhs_t,
                    rhs_t,
                    kind.clone(),
                )
                .unwrap()
            }
        }
        // ***********************
        //  Unary arith operators
        // ***********************
        (_, OpInner::AnnotUnaryArithT { reason, kind }) => {
            // Annotation inference is never speculative
            flow_js_utils::flow_unary_arith(
                cx,
                &FlowJsEnv::entry(),
                &t,
                reason.dupe(),
                kind.clone(),
            )
            .unwrap()
        }
        // ***************************
        //  Singleton primitive types
        // ***************************
        (TypeInner::DefT(reason, def_t), _)
            if let DefTInner::NumericStrKeyT(num_lit) = def_t.deref() =>
        {
            let s = &num_lit.1;
            let new_t = Type::new(TypeInner::DefT(
                reason.dupe(),
                type_::DefT::new(DefTInner::SingletonStrT {
                    value: s.dupe(),
                    from_annot: false,
                }),
            ));
            elab_t(cx, Some(seen), new_t, op)
        }
        (TypeInner::NullProtoT(reason), _) => {
            let new_t = Type::new(TypeInner::DefT(
                reason.dupe(),
                type_::DefT::new(DefTInner::NullT),
            ));
            elab_t(cx, Some(seen), new_t, op)
        }
        // ******************
        //  Function Statics
        // ******************
        (TypeInner::DefT(reason, def_t), _)
            if let DefTInner::FunT(static_, _) = def_t.deref()
                && object_like_op(op.deref()) =>
        {
            let static_ = reposition(cx, reason.loc().dupe(), static_.dupe());
            elab_t(cx, Some(seen), static_, op)
        }
        // ***************
        //  Class statics
        // ***************
        (TypeInner::DefT(reason, def_t), _)
            if let DefTInner::ClassT(instance) = def_t.deref()
                && object_like_op(op.deref()) =>
        {
            let statics = get_statics(cx, reason.dupe(), instance.dupe());
            elab_t(cx, Some(seen), statics, op)
        }
        (
            TypeInner::DefT(_, def_t),
            OpInner::AnnotGetElemT {
                reason: reason_op,
                use_op: _,
                key: elem,
            },
        ) if let DefTInner::EnumObjectT { enum_info, .. } = def_t.deref() => {
            let elem_type = elem.dupe();
            let reason = type_util::reason_of_t(&elem_type);
            flow_js_utils::add_output_non_speculating(
                cx,
                flow_typing_errors::error_message::ErrorMessage::EEnumError(
                    flow_typing_errors::error_message::EnumErrorKind::EnumInvalidMemberAccess(
                        Box::new(EnumInvalidMemberAccessData {
                            member: EnumMemberAccess::Computed(
                                flow_js_utils::type_reference_for_error(&elem_type),
                            ),
                            member_loc: reason.loc().dupe(),
                            enum_: EnumReferenceData {
                                loc: type_util::ref_loc_of_t(&t).dupe(),
                                name: enum_info.enum_name().map(Dupe::dupe),
                            },
                        }),
                    ),
                ),
            );
            any_t::error(reason_op.dupe())
        }
        // **************************************
        //  Object, function, etc. library calls
        // **************************************
        (TypeInner::ObjProtoT(reason), _) => {
            let obj_proto = get_builtin_type(cx, reason.dupe(), Some(true), "Object");
            elab_t(cx, Some(seen), obj_proto, op)
        }
        (TypeInner::FunProtoT(reason), _) => {
            let fun_proto = get_builtin_type(cx, reason.dupe(), Some(true), "Function");
            elab_t(cx, Some(seen), fun_proto, op)
        }
        // ***********
        //  ToStringT
        // ***********
        (TypeInner::DefT(_, def_t), OpInner::AnnotToStringT { .. })
            if matches!(
                def_t.deref(),
                DefTInner::StrGeneralT(_) | DefTInner::SingletonStrT { .. }
            ) =>
        {
            t
        }
        (_, OpInner::AnnotToStringT { reason, .. }) => type_::str_module_t::why(reason.dupe()),
        // ***********************************
        //  The indexer half of a key set
        // ***********************************
        // A symbol-valued indexer key contributes to `keyof` as that same symbol
        // type. Every other key stringifies.
        (TypeInner::DefT(_, def_t), OpInner::AnnotGetKeysDictKeyT(_))
            if matches!(
                def_t.deref(),
                DefTInner::UniqueSymbolT(_) | DefTInner::SymbolT
            ) =>
        {
            t
        }
        (_, OpInner::AnnotGetKeysDictKeyT(reason)) => elab_t(
            cx,
            Some(seen),
            t,
            Op::new(OpInner::AnnotToStringT {
                orig_t: None,
                reason: reason.dupe(),
            }),
        ),
        // **********************
        //  Promoting primitives
        // **********************
        (TypeInner::DefT(reason, def_t), _)
            if primitive_promoting_op(op.deref())
                && matches!(
                    def_t.deref(),
                    DefTInner::StrGeneralT(_)
                        | DefTInner::SingletonStrT { .. }
                        | DefTInner::NumGeneralT(_)
                        | DefTInner::SingletonNumT { .. }
                        | DefTInner::BoolGeneralT
                        | DefTInner::SingletonBoolT { .. }
                        | DefTInner::SymbolT
                        | DefTInner::UniqueSymbolT(_)
                ) =>
        {
            let name = match def_t.deref() {
                DefTInner::StrGeneralT(_) | DefTInner::SingletonStrT { .. } => "String",
                DefTInner::NumGeneralT(_) | DefTInner::SingletonNumT { .. } => "Number",
                DefTInner::BoolGeneralT | DefTInner::SingletonBoolT { .. } => "Boolean",
                DefTInner::SymbolT | DefTInner::UniqueSymbolT(_) => "Symbol",
                _ => unreachable!(),
            };
            let builtin = get_builtin_type(cx, reason.dupe(), Some(true), name);
            elab_t(cx, Some(seen), builtin, op)
        }
        (TypeInner::DefT(reason, def_t), _)
            if matches!(
                def_t.deref(),
                DefTInner::MixedT(type_::MixedFlavor::MixedFunction)
            ) && matches!(
                op.deref(),
                OpInner::AnnotGetPropT(_) | OpInner::AnnotLookupT(_)
            ) =>
        {
            elab_t(
                cx,
                Some(seen),
                Type::new(TypeInner::FunProtoT(reason.dupe())),
                op,
            )
        }
        (_, _) => general_error(cx, &t, &op),
    }
}

fn elab_t_wildcard_op<'cx>(
    cx: &mut Context<'cx>,
    seen: FlowOrdSet<i32>,
    t: Type,
    op: Op<Cx>,
) -> Type {
    use flow_typing_type::type_::DefTInner;

    // ******************
    //  Function Statics
    // ******************
    if let TypeInner::DefT(reason, def_t) = t.deref()
        && let DefTInner::FunT(static_, _) = def_t.deref()
        && object_like_op(op.deref())
    {
        let static_ = reposition(cx, reason.loc().dupe(), static_.dupe());
        return elab_t(cx, Some(seen), static_, op);
    }
    // ***************
    //  Class statics
    // ***************
    if let TypeInner::DefT(reason, def_t) = t.deref()
        && let DefTInner::ClassT(instance) = def_t.deref()
        && object_like_op(op.deref())
    {
        let statics = get_statics(cx, reason.dupe(), instance.dupe());
        return elab_t(cx, Some(seen), statics, op);
    }
    // **************************************
    //  Object, function, etc. library calls
    // **************************************
    if let TypeInner::ObjProtoT(reason) = t.deref() {
        let obj_proto = get_builtin_type(cx, reason.dupe(), Some(true), "Object");
        return elab_t(cx, Some(seen), obj_proto, op);
    }
    if let TypeInner::FunProtoT(reason) = t.deref() {
        let fun_proto = get_builtin_type(cx, reason.dupe(), Some(true), "Function");
        return elab_t(cx, Some(seen), fun_proto, op);
    }
    // **********************
    //  Promoting primitives
    // **********************
    if let TypeInner::DefT(reason, def_t) = t.deref()
        && primitive_promoting_op(op.deref())
    {
        let name = match def_t.deref() {
            DefTInner::StrGeneralT(_) | DefTInner::SingletonStrT { .. } => Some("String"),
            DefTInner::NumGeneralT(_) | DefTInner::SingletonNumT { .. } => Some("Number"),
            DefTInner::BoolGeneralT | DefTInner::SingletonBoolT { .. } => Some("Boolean"),
            DefTInner::SymbolT | DefTInner::UniqueSymbolT(_) => Some("Symbol"),
            _ => None,
        };
        if let Some(name) = name {
            let builtin = get_builtin_type(cx, reason.dupe(), Some(true), name);
            return elab_t(cx, Some(seen), builtin, op);
        }
    }
    if let TypeInner::DefT(reason, def_t) = t.deref()
        && matches!(
            def_t.deref(),
            DefTInner::MixedT(type_::MixedFlavor::MixedFunction)
        )
        && matches!(
            op.deref(),
            OpInner::AnnotGetPropT(_) | OpInner::AnnotLookupT(_)
        )
    {
        return elab_t(
            cx,
            Some(seen),
            Type::new(TypeInner::FunProtoT(reason.dupe())),
            op,
        );
    }
    general_error(cx, &t, &op)
}

pub fn get_builtin_type<'cx>(
    cx: &mut Context<'cx>,
    reason: Reason,
    use_desc: Option<bool>,
    name: &str,
) -> Type {
    let use_desc = use_desc.unwrap_or(false);
    let t = flow_js_utils::lookup_builtin_type(cx, name, reason.dupe());
    let reason_type = type_util::reason_of_t(&t);
    mk_instance_raw(cx, None, reason, Some(use_desc), reason_type.dupe(), t)
}

pub fn specialize<'cx>(
    cx: &mut Context<'cx>,
    t: Type,
    use_op: UseOp,
    reason_op: Reason,
    reason_tapp: Reason,
    ts: Option<Rc<[Type]>>,
    operation: AnnotationInferenceOperation,
) -> Type {
    elab_t(
        cx,
        None,
        t,
        Op::new(OpInner::AnnotSpecializeT(Box::new(AnnotSpecializeTData {
            use_op,
            reason: reason_op,
            reason2: reason_tapp,
            types: ts,
            operation,
        }))),
    )
}

fn this_specialize<'cx>(cx: &mut Context<'cx>, reason: Reason, this: Type, t: Type) -> Type {
    elab_t(
        cx,
        None,
        t,
        Op::new(OpInner::AnnotThisSpecializeT {
            reason,
            type_: this,
        }),
    )
}

fn specialize_class<'cx>(
    cx: &mut Context<'cx>,
    c: Type,
    reason_op: Reason,
    reason_tapp: Reason,
    ts: Option<Rc<[Type]>>,
    operation: AnnotationInferenceOperation,
) -> Type {
    match ts {
        None => c,
        Some(ts) => specialize(
            cx,
            c,
            type_::unknown_use(),
            reason_op,
            reason_tapp,
            Some(ts),
            operation,
        ),
    }
}

pub(crate) fn mk_type_reference<'cx>(
    cx: &mut Context<'cx>,
    type_t_kind: TypeTKind,
    reason: Reason,
    c: Type,
) -> Type {
    let reason2 = reason.dupe();
    let type_t_kind2 = type_t_kind;
    let c2 = c.dupe();
    let tvar = mk_lazy_tvar(cx, reason.dupe(), move |cx, id| {
        let t = elab_t(
            cx,
            None,
            c2.dupe(),
            Op::new(OpInner::AnnotUseTTypeT {
                reason: reason2.dupe(),
                kind: type_t_kind2,
            }),
        );
        resolve_id(cx, reason2.dupe(), id, t);
    });
    Type::new(TypeInner::AnnotT(reason, tvar, false))
}

pub fn mk_instance<'cx>(
    cx: &mut Context<'cx>,
    type_t_kind: Option<TypeTKind>,
    instance_reason: Reason,
    use_desc: Option<bool>,
    c: Type,
) -> Type {
    mk_instance_raw(
        cx,
        type_t_kind,
        instance_reason.dupe(),
        use_desc,
        instance_reason,
        c,
    )
}

fn mk_instance_raw<'cx>(
    cx: &mut Context<'cx>,
    type_t_kind: Option<TypeTKind>,
    instance_reason: Reason,
    use_desc: Option<bool>,
    reason_type: Reason,
    c: Type,
) -> Type {
    let type_t_kind = type_t_kind.unwrap_or(TypeTKind::InstanceKind);
    let use_desc = use_desc.unwrap_or(false);
    let source = elab_t(
        cx,
        None,
        c,
        Op::new(OpInner::AnnotUseTTypeT {
            reason: reason_type,
            kind: type_t_kind,
        }),
    );
    Type::new(TypeInner::AnnotT(instance_reason, source, use_desc))
}

fn mk_typeapp_instance<'cx>(
    cx: &mut Context<'cx>,
    use_op: UseOp,
    reason_op: Reason,
    reason_tapp: Reason,
    from_value: bool,
    c: Type,
    ts: Rc<[Type]>,
    operation: AnnotationInferenceOperation,
) -> Type {
    let t = specialize(
        cx,
        c.dupe(),
        use_op,
        reason_op,
        reason_tapp.dupe(),
        Some(ts),
        operation,
    );
    if from_value {
        type_util::mod_reason_of_t(&|_| reason_tapp.dupe(), &t)
    } else {
        let reason_type = type_util::reason_of_t(&c);
        mk_instance_raw(cx, None, reason_tapp, None, reason_type.dupe(), t)
    }
}

fn get_statics<'cx>(cx: &mut Context<'cx>, reason: Reason, t: Type) -> Type {
    elab_t(cx, None, t, Op::new(OpInner::AnnotGetStaticsT(reason)))
}

pub fn get_prop<'cx>(
    cx: &mut Context<'cx>,
    use_op: UseOp,
    reason: Reason,
    op_reason: Option<Reason>,
    name: Name,
    t: Type,
) -> Type {
    let op_reason = op_reason.unwrap_or_else(|| reason.dupe());
    elab_t(
        cx,
        None,
        t,
        Op::new(OpInner::AnnotGetPropT(Box::new(AnnotGetPropTData {
            reason: op_reason,
            use_op,
            from_annot: false,
            prop_ref: type_util::mk_named_prop(reason, false, name),
        }))),
    )
}

pub(crate) fn get_elem<'cx>(
    cx: &mut Context<'cx>,
    use_op: UseOp,
    reason: Reason,
    key: Type,
    t: Type,
) -> Type {
    elab_t(
        cx,
        None,
        t,
        Op::new(OpInner::AnnotGetElemT {
            reason,
            use_op,
            key,
        }),
    )
}

pub fn qualify_type<'cx>(
    cx: &mut Context<'cx>,
    use_op: UseOp,
    reason: Reason,
    op_reason: Reason,
    prop_name: Name,
    t: Type,
) -> Type {
    let op_reason2 = op_reason.dupe();
    let reason2 = reason.dupe();
    let use_op2 = use_op.dupe();
    let prop_name2 = prop_name.dupe();
    let t2 = t.dupe();
    mk_lazy_tvar(cx, op_reason.dupe(), move |cx, id| {
        let t = elab_t(
            cx,
            None,
            t2.dupe(),
            Op::new(OpInner::AnnotGetTypeFromNamespaceT(Box::new(
                AnnotGetTypeFromNamespaceTData {
                    reason: op_reason2.dupe(),
                    use_op: use_op2.dupe(),
                    prop_ref: (reason2.dupe(), prop_name2.dupe()),
                },
            ))),
        );
        resolve_id(cx, op_reason2, id, t);
    })
}

pub fn assert_export_is_type<'cx>(
    cx: &mut Context<'cx>,
    reason: Reason,
    name: &str,
    t: Type,
) -> Type {
    let reason2 = reason.dupe();
    let name2 = Name::new(name);
    let t2 = t.dupe();
    mk_lazy_tvar(cx, reason.dupe(), move |cx, id| {
        let t = elab_t(
            cx,
            None,
            t2.dupe(),
            Op::new(OpInner::AnnotAssertExportIsTypeT {
                reason: reason2.dupe(),
                name: name2.dupe(),
            }),
        );
        resolve_id(cx, reason2, id, t);
    })
}

pub(crate) fn cjs_require<'cx>(
    cx: &mut Context<'cx>,
    reason: Reason,
    namespace_symbol: flow_common::flow_symbol::Symbol,
    is_strict: bool,
    standard_cjs_esm_interop: bool,
    resolved_require: flow_typing_context::ResolvedRequire,
) -> Type {
    let reason2 = reason.dupe();
    let namespace_symbol2 = namespace_symbol.clone();
    let lz = get_lazy_module_type_or_any_src(&resolved_require);
    let resolved: LazyType = Rc::new(flow_lazy::Lazy::new(move |cx: &mut Context<'_>| {
        let result = lz.get_forced(cx).clone();
        match result {
            Ok(module_type) => {
                let (t, _) = flow_js_utils::cjs_require_t_kit::on_module_t(
                    cx,
                    |_cx, _loc, t| Ok(t),
                    reason2.dupe(),
                    namespace_symbol2.clone(),
                    is_strict,
                    standard_cjs_esm_interop,
                    &module_type,
                )
                .unwrap_or_else(|_| {
                    (
                        type_::any_t::error(reason2.dupe()),
                        flow_aloc::ALoc::default(),
                    )
                });
                t
            }
            Err(src) => type_::any_t::why(src, reason2.dupe()),
        }
    }));
    mk_sig_tvar(cx, reason, resolved)
}

pub(crate) fn lazy_cjs_extract_named_exports(
    reason: Reason,
    local_module: type_::ModuleType,
    t: Type,
) -> Rc<flow_lazy::Lazy<Cx, type_::ModuleType>> {
    let reason2 = reason.dupe();
    Rc::new(flow_lazy::Lazy::new(move |cx: &mut Context<'_>| {
        let reason3 = reason2.dupe();
        let concretize = |cx: &mut Context<'_>,
                          t: Type|
         -> Result<Type, flow_utils_concurrency::job_error::JobError> {
            let result = elab_t(
                cx,
                None,
                t,
                Op::new(
                    OpInner::AnnotConcretizeForCJSExtractNamedExportsAndTypeExports(reason3.dupe()),
                ),
            );
            Ok(match result.deref() {
                TypeInner::OpenT(tvar) => get_fully_resolved_type(cx, tvar.id() as i32),
                _ => result,
            })
        };
        // The concretize closure has no `?` call sites in its body, so
        // on_type cannot return Err here. .expect documents the
        // invariant — see annotation_inference::copy_type_exports for
        // the same pattern. Annotation inference is on the type-sig
        // pipeline, not the cancel-aware check pipeline.
        flow_js_utils::cjs_extract_named_exports_t_kit::on_type(
            cx,
            &concretize,
            reason2.dupe(),
            local_module.clone(),
            t.dupe(),
        )
        .expect(
            "annotation_inference::lazy_cjs_extract_named_exports concretize closure is infallible",
        )
    }))
}

pub(crate) fn import_typeof<'cx>(
    cx: &mut Context<'cx>,
    reason: Reason,
    name: &str,
    t: Type,
) -> Type {
    elab_t(
        cx,
        None,
        t,
        Op::new(OpInner::AnnotImportTypeofT {
            reason,
            name: name.into(),
        }),
    )
}

pub fn import_default<'cx>(
    cx: &mut Context<'cx>,
    reason: Reason,
    import_kind: type_::ImportKind,
    export_name: &str,
    module_name: flow_common::flow_import_specifier::Userland,
    is_strict: bool,
    resolved_require: flow_typing_context::ResolvedRequire,
) -> Type {
    let on_module_reason = reason.dupe();
    let export_name = export_name.to_owned();
    let on_module = move |cx: &mut Context<'_>, m: &type_::ModuleType| -> Type {
        let (_name_loc_opt, t) = flow_js_utils::import_default_t_kit::on_module_t(
            cx,
            &|cx, r, f, t| with_concretized_type(cx, r, f, t),
            on_module_reason.dupe(),
            import_kind.clone(),
            &export_name,
            module_name.clone(),
            is_strict,
            m,
        )
        .unwrap_or_else(|_| (None, type_::any_t::error(on_module_reason.dupe())));
        t
    };
    let why_reason = reason.dupe();
    let lz = get_lazy_module_type_or_any_src(&resolved_require);
    let resolved: LazyType = Rc::new(flow_lazy::Lazy::new(
        move |lazy_cx: &mut Context<'_>| match lz.get_forced(lazy_cx).clone() {
            Ok(m) => on_module(lazy_cx, &m),
            Err(src) => type_::any_t::why(src, why_reason.dupe()),
        },
    ));
    mk_sig_tvar(cx, reason, resolved)
}

pub fn import_named<'cx>(
    cx: &mut Context<'cx>,
    reason: Reason,
    import_kind: type_::ImportKind,
    export_name: &str,
    module_name: flow_common::flow_import_specifier::Userland,
    is_strict: bool,
    resolved_require: flow_typing_context::ResolvedRequire,
) -> Type {
    let reason2 = reason.dupe();
    let export_name2: FlowSmolStr = export_name.into();
    let module_name2 = module_name.clone();
    let on_module = move |cx: &mut Context<'_>, m: &type_::ModuleType| -> Type {
        let (_name_loc_opt, t) = flow_js_utils::import_named_t_kit::on_module_t(
            cx,
            &|cx, r, f, t| with_concretized_type(cx, r, f, t),
            reason2.dupe(),
            import_kind.clone(),
            &export_name2,
            module_name2.clone(),
            is_strict,
            m,
        )
        .unwrap_or_else(|_| (None, type_::any_t::error(reason2.dupe())));
        t
    };
    let lz = get_lazy_module_type_or_any_src(&resolved_require);
    let why_reason = reason.dupe();
    let resolved: LazyType = Rc::new(flow_lazy::Lazy::new(move |cx: &mut Context<'_>| {
        let result = lz.get_forced(cx).clone();
        match result {
            Ok(m) => on_module(cx, &m),
            Err(src) => type_::any_t::why(src, why_reason.dupe()),
        }
    }));
    mk_sig_tvar(cx, reason, resolved)
}

pub(crate) fn import_default_for_extends<'cx>(
    cx: &mut Context<'cx>,
    reason: Reason,
    import_kind: type_::ImportKind,
    local_name: &FlowSmolStr,
    module_name: flow_common::flow_import_specifier::Userland,
    is_strict: bool,
    resolved_require: flow_typing_context::ResolvedRequire,
) -> Type {
    let reason2 = reason.dupe();
    let local_name = local_name.dupe();
    let module_name2 = module_name.clone();
    let on_module = move |cx: &mut Context<'_>, m: &type_::ModuleType| -> Type {
        let (_name_loc_opt, t) = flow_js_utils::import_default_t_kit::on_module_t_for_extends(
            cx,
            reason2.dupe(),
            import_kind.clone(),
            local_name.as_str(),
            module_name2.clone(),
            is_strict,
            m,
        )
        .unwrap_or_else(|_| (None, type_::any_t::error(reason2.dupe())));
        t
    };
    let lz = get_lazy_module_type_or_any_src(&resolved_require);
    let why_reason = reason.dupe();
    let resolved: LazyType = Rc::new(flow_lazy::Lazy::new(move |cx: &mut Context<'_>| {
        let result = lz.get_forced(cx).clone();
        match result {
            Ok(m) => on_module(cx, &m),
            Err(src) => type_::any_t::why(src, why_reason.dupe()),
        }
    }));
    mk_sig_tvar(cx, reason, resolved)
}

pub(crate) fn import_named_for_extends<'cx>(
    cx: &mut Context<'cx>,
    reason: Reason,
    import_kind: type_::ImportKind,
    export_name: &FlowSmolStr,
    module_name: flow_common::flow_import_specifier::Userland,
    is_strict: bool,
    resolved_require: flow_typing_context::ResolvedRequire,
) -> Type {
    let reason2 = reason.dupe();
    let export_name2 = export_name.dupe();
    let module_name2 = module_name.clone();
    let on_module = move |cx: &mut Context<'_>, m: &type_::ModuleType| -> Type {
        let (_name_loc_opt, t) = flow_js_utils::import_named_t_kit::on_module_t_for_extends(
            cx,
            reason2.dupe(),
            import_kind.clone(),
            &export_name2,
            module_name2.clone(),
            is_strict,
            m,
        )
        .unwrap_or_else(|_| (None, type_::any_t::error(reason2.dupe())));
        t
    };
    let lz = get_lazy_module_type_or_any_src(&resolved_require);
    let why_reason = reason.dupe();
    let resolved: LazyType = Rc::new(flow_lazy::Lazy::new(move |cx: &mut Context<'_>| {
        let result = lz.get_forced(cx).clone();
        match result {
            Ok(m) => on_module(cx, &m),
            Err(src) => type_::any_t::why(src, why_reason.dupe()),
        }
    }));
    mk_sig_tvar(cx, reason, resolved)
}

pub fn import_ns<'cx>(
    cx: &mut Context<'cx>,
    reason: Reason,
    namespace_symbol: flow_common::flow_symbol::Symbol,
    is_strict: bool,
    resolved_require: flow_typing_context::ResolvedRequire,
) -> Type {
    let reason2 = reason.dupe();
    let namespace_symbol2 = namespace_symbol.clone();
    let lz = get_lazy_module_type_or_any_src(&resolved_require);
    let resolved: LazyType = Rc::new(flow_lazy::Lazy::new(move |cx: &mut Context<'_>| {
        let result = lz.get_forced(cx).clone();
        match result {
            Ok(m) => {
                let (values_type, types_tmap) = flow_js_utils::import_module_ns_t_kit::on_module_t(
                    cx,
                    false,
                    reason2.dupe(),
                    is_strict,
                    &m,
                )
                .unwrap_or_else(|_| {
                    (
                        type_::any_t::error(reason2.dupe()),
                        properties::Id::generate_id(),
                    )
                });
                Type::new(TypeInner::NamespaceT(Rc::new(type_::NamespaceType {
                    namespace_symbol: namespace_symbol2.clone(),
                    values_type,
                    types_tmap,
                })))
            }
            Err(src) => type_::any_t::why(src, reason2.dupe()),
        }
    }));
    mk_sig_tvar(cx, reason, resolved)
}

pub fn copy_named_exports<'cx>(
    cx: &mut Context<'cx>,
    source_module: Result<type_::ModuleType, Type>,
    target_module_type: &type_::ModuleType,
) {
    match source_module {
        Ok(m) => {
            flow_js_utils::copy_named_exports_t_kit::mod_module_t(cx, target_module_type, &m);
        }
        Err(_) => (),
    }
}

pub fn copy_type_exports<'cx>(
    cx: &mut Context<'cx>,
    source_module: Result<type_::ModuleType, Type>,
    reason: Reason,
    target_module_type: &type_::ModuleType,
) {
    match source_module {
        Ok(m) => {
            let reason2 = reason.dupe();
            let concretize_export_type =
                |cx: &mut Context<'cx>,
                 _r: Reason,
                 t: Type|
                 -> Result<Type, flow_utils_concurrency::job_error::JobError> {
                    let result = elab_t(
                        cx,
                        None,
                        t,
                        Op::new(
                            OpInner::AnnotConcretizeForCJSExtractNamedExportsAndTypeExports(
                                reason2.dupe(),
                            ),
                        ),
                    );
                    Ok(match result.deref() {
                        TypeInner::OpenT(tvar) => get_fully_resolved_type(cx, tvar.id() as i32),
                        _ => result,
                    })
                };
            // The closure above never propagates a JobError (no `?`-failing
            // call sites in its body), so mod_module_t cannot fail here.
            // .expect documents the invariant; this site does not need a
            // cancel/timeout boundary because annotation inference runs on
            // the type-sig pipeline, not the cancel-aware check pipeline.
            flow_js_utils::copy_type_exports_t_kit::mod_module_t(
                cx,
                concretize_export_type,
                reason,
                target_module_type,
                &m,
            )
            .expect("annotation_inference::copy_type_exports concretize closure is infallible");
        }
        Err(_) => (),
    }
}

pub fn mk_non_generic_render_type<'cx>(
    cx: &mut Context<'cx>,
    reason: Reason,
    renders_variant: type_::RendersVariant,
    t: Type,
) -> Type {
    let concretize = move |cx: &mut Context<'_>,
                           t: &Type|
          -> Result<Vec<Type>, flow_utils_concurrency::job_error::JobError> {
        let c = TypeCollector::create();
        elab_t(
            cx,
            None,
            t.dupe(),
            Op::new(OpInner::AnnotConcretizeForInspection {
                reason: type_util::reason_of_t(t).dupe(),
                collector: c.clone(),
            }),
        );
        Ok(c.collect_to_vec())
    };
    let is_iterable_for_better_error =
        |_cx: &mut Context<'_>,
         _t: &Type|
         -> Result<bool, flow_utils_concurrency::job_error::JobError> { Ok(false) };
    flow_js_utils::render_types::mk_non_generic_render_type(
        cx,
        reason,
        renders_variant,
        false,
        concretize,
        is_iterable_for_better_error,
        t,
    )
}

pub fn arith<'cx>(
    cx: &mut Context<'cx>,
    reason: Reason,
    lhs_t: Type,
    rhs_t: Type,
    kind: type_::arith_kind::ArithKind,
) -> Type {
    elab_t(
        cx,
        None,
        lhs_t,
        Op::new(OpInner::AnnotArithT(Box::new(AnnotArithTData {
            reason,
            flip: false,
            rhs_t,
            kind,
        }))),
    )
}

pub fn unary_arith<'cx>(
    cx: &mut Context<'cx>,
    reason: Reason,
    t: Type,
    kind: type_::UnaryArithKind,
) -> Type {
    elab_t(
        cx,
        None,
        t,
        Op::new(OpInner::AnnotUnaryArithT { reason, kind }),
    )
}

pub fn unary_not<'cx>(cx: &mut Context<'cx>, reason: Reason, t: Type) -> Type {
    elab_t(cx, None, t, Op::new(OpInner::AnnotNotT(reason)))
}

pub fn mixin<'cx>(cx: &mut Context<'cx>, reason: Reason, t: Type) -> Type {
    elab_t(cx, None, t, Op::new(OpInner::AnnotMixinT(reason)))
}

pub fn obj_rest<'cx>(cx: &mut Context<'cx>, reason: Reason, xs: Vec<FlowSmolStr>, t: Type) -> Type {
    elab_t(
        cx,
        None,
        t,
        Op::new(OpInner::AnnotObjRestT {
            reason,
            keys: xs.into(),
        }),
    )
}

pub fn arr_rest<'cx>(
    cx: &mut Context<'cx>,
    _use_op: UseOp,
    reason: Reason,
    _i: i32,
    t: Type,
) -> Type {
    error_unsupported_operation(cx, &t, AnnotationInferenceOperation::ArrayRest, reason)
}

fn object_kit_concrete<'cx>(
    cx: &mut Context<'cx>,
    use_op: UseOp,
    op: &Op<Cx>,
    reason: Reason,
    resolve_tool: type_::object::ResolveTool,
    tool: type_::object::Tool,
    t: Type,
) -> Type {
    use flow_typing_flow_common::flow_js_utils::FlowJsException;
    use flow_typing_type::type_::object;
    use flow_typing_type::type_::object::ObjectToolObjectMapData;
    use flow_typing_type::type_::object::ObjectToolReactConfigData;

    let add_output = |cx: &mut Context<'cx>,
                      env: &FlowJsEnv,
                      msg: flow_typing_errors::error_message::ErrorMessage<flow_aloc::ALoc>|
     -> Result<(), FlowJsException> {
        flow_js_utils::add_output_with_env(cx, env, msg)?;
        Ok(())
    };
    let return_ = |_cx: &mut Context<'cx>,
                   _use_op: UseOp,
                   t: Type|
     -> Result<Type, FlowJsException> { Ok(t) };
    let recurse = |cx: &mut Context<'cx>,
                   _env: &FlowJsEnv,
                   use_op: UseOp,
                   reason: &Reason,
                   resolve_tool: object::ResolveTool,
                   tool: &object::Tool,
                   x: Type|
     -> Result<Type, FlowJsException> {
        Ok(object_kit(
            cx,
            use_op,
            reason.dupe(),
            resolve_tool,
            tool.clone(),
            x,
        ))
    };
    let statics =
        |cx: &mut Context<'cx>,
         _env: &FlowJsEnv,
         reason: &Reason,
         t: &Type|
         -> Result<Type, FlowJsException> { Ok(get_statics(cx, reason.dupe(), t.dupe())) };
    let op2 = op.clone();
    let next = |cx: &mut Context<'cx>,
                use_op: UseOp,
                tool: &object::Tool,
                reason: &Reason,
                x: vec1::Vec1<object::Slice>|
     -> Result<Type, FlowJsException> {
        match tool {
            object::Tool::MakeExact => {
                slice_utils::object_make_exact(cx, &FlowJsEnv::entry(), reason, x)
            }
            object::Tool::Spread(box (options, state)) => slice_utils::object_spread(
                &|_cx, _use_op, _d1, _d2| Ok(()),
                &add_output,
                &return_,
                &|cx, env, use_op, reason, resolve_tool, tool, x| {
                    recurse(cx, env, use_op, reason, resolve_tool, &tool, x)
                },
                options,
                state.clone(),
                cx,
                use_op,
                reason,
                x,
            ),
            object::Tool::Rest(box (options, state)) => {
                let rest_return =
                    |_cx: &mut Context<'cx>,
                     _use_op: Box<dyn Fn(flow_common::polarity::Polarity) -> UseOp>,
                     _options: object::rest::MergeMode,
                     t: Type|
                     -> Result<Type, FlowJsException> { Ok(t) };
                let subt_check = |_use_op: UseOp,
                                  _cx: &mut Context<'cx>,
                                  (_t1, _t2): (&Type, &Type)|
                 -> Result<(), FlowJsException> { Ok(()) };
                slice_utils::object_rest(
                    &add_output,
                    &rest_return,
                    &|cx, env, use_op, reason, resolve_tool, tool, x| {
                        recurse(cx, env, use_op, reason, resolve_tool, &tool, x)
                    },
                    &subt_check,
                    options,
                    state,
                    cx,
                    use_op,
                    reason,
                    x,
                )
            }
            object::Tool::Partial => Ok(slice_utils::object_update_optionality(
                slice_utils::ObjectUpdateOptionalityKind::Partial,
                cx,
                reason,
                x,
            )),
            object::Tool::Required => Ok(slice_utils::object_update_optionality(
                slice_utils::ObjectUpdateOptionalityKind::Required,
                cx,
                reason,
                x,
            )),
            object::Tool::ReadOnly => Ok(slice_utils::object_read_only(cx, reason, x)),
            object::Tool::ReactConfig(box ObjectToolReactConfigData { .. }) => {
                Ok(error_internal(cx, "ReactConfig", &op2))
            }
            object::Tool::ReactCheckComponentConfig { props } => {
                slice_utils::check_component_config(
                    &add_output,
                    &return_,
                    props,
                    cx,
                    &FlowJsEnv::entry(),
                    use_op,
                    reason,
                    x,
                )
            }
            object::Tool::ObjectRep => Ok(error_internal(cx, "ObjectRep", &op2)),
            object::Tool::ObjectMap(box ObjectToolObjectMapData {
                prop_type,
                name_type: None,
                mapped_type_flags,
                selected_keys_opt,
            }) => {
                let selected_keys = match selected_keys_opt {
                    None => None,
                    Some(keys) => match partition_mapped_type_keys(cx, reason, keys.dupe()) {
                        Some(selected_keys) => Some(selected_keys),
                        None => return Ok(error_unsupported(cx, keys, &op2)),
                    },
                };
                let ts = x
                    .iter()
                    .map(|slice| {
                        map_mapped_type_slice(
                            cx,
                            &use_op,
                            reason,
                            prop_type,
                            mapped_type_flags,
                            selected_keys.as_ref(),
                            slice,
                        )
                    })
                    .collect();
                Ok(type_util::union_of_ts(reason.dupe(), ts, None))
            }
            object::Tool::ObjectMap(box ObjectToolObjectMapData { .. }) => {
                Ok(error_internal(cx, "ObjectMap", &op2))
            }
        }
    };
    slice_utils::run(
        &add_output,
        &return_,
        &next,
        &recurse,
        &statics,
        cx,
        use_op,
        &reason,
        resolve_tool,
        &tool,
        &t,
    )
    .unwrap()
}

/// Maps one object slice through a mapped type. Under `-?`, `void` is removed
/// from the deferred property type lazily, as checking does through a tvar.
fn map_mapped_type_slice<'cx>(
    cx: &mut Context<'cx>,
    use_op: &UseOp,
    reason: &Reason,
    property_type: &Type,
    mapped_type_flags: &type_::MappedTypeFlags,
    selected_keys: Option<&(Vec<(Name, Reason)>, Vec<Type>)>,
    slice: &type_::object::Slice,
) -> Type {
    let filter_optional = |cx: &mut Context<'cx>, t: Type| -> Result<Type, FlowJsException> {
        let op = Op::new(OpInner::AnnotFilterOptionalT(reason.dupe()));
        let filtered: LazyType = Rc::new(flow_lazy::Lazy::new(move |cx: &mut Context<'_>| {
            elab_t(cx, None, t.dupe(), op.dupe())
        }));
        Ok(mk_sig_tvar(cx, reason.dupe(), filtered))
    };
    slice_utils::map_object(
        &filter_optional,
        None,
        property_type,
        mapped_type_flags,
        cx,
        &FlowJsEnv::entry(),
        reason,
        use_op,
        selected_keys,
        slice,
    )
    .expect("should not raise: annotation inference's filter_optional never fails")
}

/// Splits mapped type keys into property names and indexer keys, as
/// `object_kit::partition_keys_and_indexer` does in checking. Checking
/// validates the keys by subtyping against `string | number | symbol`; here,
/// keys that are not structurally one of those are unsupported.
fn partition_mapped_type_keys<'cx>(
    cx: &mut Context<'cx>,
    reason: &Reason,
    keys: Type,
) -> Option<(Vec<(Name, Reason)>, Vec<Type>)> {
    use flow_typing_type::type_::DefTInner;

    let collector = TypeCollector::create();
    elab_t(
        cx,
        None,
        keys,
        Op::new(OpInner::AnnotConcretizeForInspection {
            reason: reason.dupe(),
            collector: collector.clone(),
        }),
    );
    let keys = collector.collect_to_vec();
    let all_valid = keys.iter().all(|t| match t.deref() {
        TypeInner::DefT(_, def_t) => matches!(
            def_t.deref(),
            DefTInner::SingletonStrT { .. }
                | DefTInner::UniqueSymbolT(_)
                | DefTInner::EmptyT
                | DefTInner::StrGeneralT(_)
                | DefTInner::NumGeneralT(_)
                | DefTInner::SingletonNumT { .. }
                | DefTInner::SymbolT
        ),
        TypeInner::AnyT(..) => true,
        _ => false,
    });
    all_valid.then(|| split_mapped_type_keys(keys))
}

/// Maps a non-homomorphic mapped type over its keys, as
/// `object_kit::mapped_type_of_keys` does in checking.
fn mapped_type_of_keys<'cx>(
    cx: &mut Context<'cx>,
    use_op: UseOp,
    reason: Reason,
    keys: Type,
    property_type: &Type,
    mapped_type_flags: &type_::MappedTypeFlags,
    op: &Op<Cx>,
) -> Type {
    let Some((keys_with_reasons, indexers)) = partition_mapped_type_keys(cx, &reason, keys.dupe())
    else {
        return error_unsupported(cx, &keys, op);
    };
    map_mapped_type_slice(
        cx,
        &use_op,
        &reason,
        property_type,
        mapped_type_flags,
        None,
        &slice_of_keys(&reason, &keys_with_reasons, &indexers),
    )
}

fn object_kit<'cx>(
    cx: &mut Context<'cx>,
    use_op: UseOp,
    reason: Reason,
    resolve_tool: type_::object::ResolveTool,
    tool: type_::object::Tool,
    t: Type,
) -> Type {
    elab_t(
        cx,
        None,
        t,
        Op::new(OpInner::AnnotObjKitT(Box::new(AnnotObjKitTData {
            reason,
            use_op,
            resolve_tool,
            tool,
        }))),
    )
}

pub fn object_spread<'cx>(
    cx: &mut Context<'cx>,
    use_op: UseOp,
    reason: Reason,
    target: type_::object::spread::Target,
    state: type_::object::spread::State,
    t: Type,
) -> Type {
    let resolve_tool = type_::object::ResolveTool::Resolve(type_::object::Resolve::Next);
    let tool = type_::object::Tool::Spread(Box::new((target, state)));
    object_kit(cx, use_op, reason, resolve_tool, tool, t)
}

fn object_rest_internal<'cx>(
    cx: &mut Context<'cx>,
    use_op: UseOp,
    reason: Reason,
    target: type_::object::rest::MergeMode,
    state: type_::object::rest::State,
    t: Type,
) -> Type {
    let resolve_tool = type_::object::ResolveTool::Resolve(type_::object::Resolve::Next);
    let tool = type_::object::Tool::Rest(Box::new((target, state)));
    object_kit(cx, use_op, reason, resolve_tool, tool, t)
}

fn make_readonly<'cx>(cx: &mut Context<'cx>, use_op: UseOp, reason: Reason, t: Type) -> Type {
    let resolve_tool = type_::object::ResolveTool::Resolve(type_::object::Resolve::Next);
    object_kit(
        cx,
        use_op,
        reason,
        resolve_tool,
        type_::object::Tool::ReadOnly,
        t,
    )
}

fn make_partial<'cx>(cx: &mut Context<'cx>, use_op: UseOp, reason: Reason, t: Type) -> Type {
    let resolve_tool = type_::object::ResolveTool::Resolve(type_::object::Resolve::Next);
    object_kit(
        cx,
        use_op,
        reason,
        resolve_tool,
        type_::object::Tool::Partial,
        t,
    )
}

fn make_required<'cx>(cx: &mut Context<'cx>, use_op: UseOp, reason: Reason, t: Type) -> Type {
    let resolve_tool = type_::object::ResolveTool::Resolve(type_::object::Resolve::Next);
    object_kit(
        cx,
        use_op,
        reason,
        resolve_tool,
        type_::object::Tool::Required,
        t,
    )
}

fn make_exact<'cx>(cx: &mut Context<'cx>, reason: Reason, t: Type) -> Type {
    let resolve_tool = type_::object::ResolveTool::Resolve(type_::object::Resolve::Next);
    object_kit(
        cx,
        type_::unknown_use(),
        reason,
        resolve_tool,
        type_::object::Tool::MakeExact,
        t,
    )
}

pub fn obj_test_proto<'cx>(cx: &mut Context<'cx>, reason: Reason, t: Type) -> Type {
    elab_t(cx, None, t, Op::new(OpInner::AnnotObjTestProtoT(reason)))
}
