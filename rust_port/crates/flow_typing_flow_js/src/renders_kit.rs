/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::collections::BTreeSet;
use std::ops::Deref;
use std::rc::Rc;
use std::sync::Arc;

use dupe::Dupe;
use flow_aloc::ALoc;
use flow_common::reason::Reason;
use flow_common::reason::VirtualReasonDesc;
use flow_typing_context::Context;
use flow_typing_errors::error_message::EIncompatibleTypesWithUseOpData;
use flow_typing_errors::error_message::ErrorMessage;
use flow_typing_errors::intermediate_error_types::ExpectedModulePurpose;
use flow_typing_flow_common::flow_js_utils;
use flow_typing_flow_common::flow_js_utils::FlowJsException;
use flow_typing_flow_js_env::FlowJsEnv;
use flow_typing_type::type_::ArrType;
use flow_typing_type::type_::ArrayATData;
use flow_typing_type::type_::CanonicalRendersForm;
use flow_typing_type::type_::ComponentKind;
use flow_typing_type::type_::DefT;
use flow_typing_type::type_::DefTInner;
use flow_typing_type::type_::DepthTrace;
use flow_typing_type::type_::ImplicitInstantiationTvarData;
use flow_typing_type::type_::ImplicitInstantiationTvarKind;
use flow_typing_type::type_::NominalType;
use flow_typing_type::type_::ROArrayATData;
use flow_typing_type::type_::ReactAbstractComponentTData;
use flow_typing_type::type_::RendersVariant;
use flow_typing_type::type_::TupleATData;
use flow_typing_type::type_::Type;
use flow_typing_type::type_::TypeInner;
use flow_typing_type::type_::UseOp;
use flow_typing_type::type_::VirtualFrameUseOp;
use flow_typing_type::type_::VirtualUseOp;
use flow_typing_type::type_::type_collector;
use flow_typing_type::type_::unknown_use;
use flow_typing_type::type_util;

use crate::flow_js::FlowJs;

fn reconstruct_render_type(reason: &Reason, form: &CanonicalRendersForm) -> Type {
    Type::new(TypeInner::DefT(
        reason.dupe(),
        DefT::new(DefTInner::RendersT(Rc::new(form.clone()))),
    ))
}

fn frame_renders_compat(use_op: UseOp) -> UseOp {
    VirtualUseOp::Frame(
        Arc::new(VirtualFrameUseOp::RendersCompatibility),
        Arc::new(use_op),
    )
}

fn incompatible_renders_error(lower: &Type, upper: &Type, use_op: UseOp) -> ErrorMessage<ALoc> {
    ErrorMessage::EIncompatibleTypesWithUseOp(Box::new(EIncompatibleTypesWithUseOpData {
        lower_loc: type_util::loc_of_t(lower).dupe(),
        lower_def_loc: type_util::ref_loc_of_t(lower).dupe(),
        upper_loc: type_util::loc_of_t(upper).dupe(),
        upper_def_loc: type_util::ref_loc_of_t(upper).dupe(),
        lower_desc: flow_js_utils::type_or_type_desc_for_error(lower),
        upper_desc: flow_js_utils::type_or_type_desc_for_error(upper),
        use_op,
        explanation: None,
        example: None,
        branches: vec![],
    }))
}

pub fn rec_renders_to_renders<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    use_op: UseOp,
    (reasonl, l): (&Reason, &CanonicalRendersForm),
    (reasonu, u): (&Reason, &CanonicalRendersForm),
) -> Result<(), FlowJsException> {
    match (l, u) {
        (
            CanonicalRendersForm::IntrinsicRenders(n1),
            CanonicalRendersForm::IntrinsicRenders(n2),
        ) => {
            if n1 == n2 {
                Ok(())
            } else {
                flow_js_utils::add_output_with_env(
                    cx,
                    env,
                    incompatible_renders_error(
                        &reconstruct_render_type(reasonl, l),
                        &reconstruct_render_type(reasonu, u),
                        frame_renders_compat(use_op),
                    ),
                )
            }
        }
        (
            CanonicalRendersForm::IntrinsicRenders(_),
            CanonicalRendersForm::StructuralRenders {
                renders_variant: _,
                renders_structural_type: t,
            },
        ) => {
            if !FlowJs::speculative_subtyping_succeeds_with_flow_errors(
                cx,
                env,
                &reconstruct_render_type(reasonl, l),
                t,
            )? {
                flow_js_utils::add_output_with_env(
                    cx,
                    env,
                    incompatible_renders_error(
                        &reconstruct_render_type(reasonl, l),
                        &reconstruct_render_type(reasonu, u),
                        frame_renders_compat(use_op),
                    ),
                )
            } else {
                Ok(())
            }
        }
        (
            CanonicalRendersForm::IntrinsicRenders(_),
            CanonicalRendersForm::NominalRenders { .. },
        )
        | (_, CanonicalRendersForm::IntrinsicRenders(_)) => flow_js_utils::add_output_with_env(
            cx,
            env,
            incompatible_renders_error(
                &reconstruct_render_type(reasonl, l),
                &reconstruct_render_type(reasonu, u),
                frame_renders_compat(use_op),
            ),
        ),
        (
            CanonicalRendersForm::NominalRenders {
                renders_id: id1,
                renders_name: name_1,
                renders_super,
            },
            CanonicalRendersForm::NominalRenders {
                renders_id: id2,
                renders_name: name_2,
                renders_super: _,
            },
        ) => {
            if id1 == id2
                || type_util::nominal_id_have_same_logical_module(
                    &cx.file_options(),
                    cx.projects_options(),
                    (id1, Some(name_1.as_ref())),
                    (id2, Some(name_2.as_ref())),
                )
            {
                Ok(())
            } else {
                // We reposition the super using l's reason for better error messages
                let repositioned_super = FlowJs::reposition_reason(
                    cx,
                    env,
                    Some(trace),
                    reasonl,
                    Some(true),
                    renders_super,
                )?;
                FlowJs::rec_flow_t_with_env(
                    cx,
                    env,
                    trace,
                    frame_renders_compat(use_op),
                    &repositioned_super,
                    &reconstruct_render_type(reasonu, u),
                )?;
                Ok(())
            }
        }
        (
            CanonicalRendersForm::NominalRenders {
                renders_id: _,
                renders_name: _,
                renders_super,
            },
            CanonicalRendersForm::StructuralRenders {
                renders_variant:
                    RendersVariant::RendersNormal
                    | RendersVariant::RendersMaybe
                    | RendersVariant::RendersStar,
                renders_structural_type: t,
            },
        ) => {
            if !FlowJs::speculative_subtyping_succeeds_with_flow_errors(
                cx,
                env,
                &reconstruct_render_type(reasonl, l),
                t,
            )? {
                let u_type = reconstruct_render_type(reasonu, u);
                let repositioned_super = FlowJs::reposition_reason(
                    cx,
                    env,
                    Some(trace),
                    reasonl,
                    Some(true),
                    renders_super,
                )?;
                FlowJs::rec_flow_t_with_env(
                    cx,
                    env,
                    trace,
                    frame_renders_compat(use_op),
                    &repositioned_super,
                    &u_type,
                )?;
            }
            Ok(())
        }
        (
            CanonicalRendersForm::DefaultRenders,
            CanonicalRendersForm::StructuralRenders {
                renders_variant: _,
                renders_structural_type: t,
            },
        ) => {
            FlowJs::rec_flow_t_with_env(
                cx,
                env,
                trace,
                frame_renders_compat(use_op),
                &reconstruct_render_type(reasonl, l),
                t,
            )?;
            Ok(())
        }
        (
            CanonicalRendersForm::StructuralRenders {
                renders_variant: RendersVariant::RendersMaybe | RendersVariant::RendersStar,
                renders_structural_type: _,
            }
            | CanonicalRendersForm::DefaultRenders,
            CanonicalRendersForm::NominalRenders { .. },
        ) => flow_js_utils::add_output_with_env(
            cx,
            env,
            incompatible_renders_error(
                &reconstruct_render_type(reasonl, l),
                &reconstruct_render_type(reasonu, u),
                frame_renders_compat(use_op),
            ),
        ),
        (
            CanonicalRendersForm::StructuralRenders {
                renders_variant: RendersVariant::RendersNormal,
                renders_structural_type,
            },
            _,
        ) => {
            FlowJs::rec_flow_t_with_env(
                cx,
                env,
                trace,
                use_op,
                renders_structural_type,
                &reconstruct_render_type(reasonu, u),
            )?;
            Ok(())
        }
        (
            CanonicalRendersForm::StructuralRenders {
                renders_variant: RendersVariant::RendersMaybe,
                renders_structural_type,
            },
            CanonicalRendersForm::StructuralRenders {
                renders_variant,
                renders_structural_type: _,
            },
        ) => {
            match renders_variant {
                RendersVariant::RendersNormal => {
                    let u_type = reconstruct_render_type(reasonu, u);
                    let reason = reasonl
                        .dupe()
                        .replace_desc(VirtualReasonDesc::RRendersNothing);
                    let null_t =
                        Type::new(TypeInner::DefT(reason.dupe(), DefT::new(DefTInner::NullT)));
                    FlowJs::rec_flow_t_with_env(cx, env, trace, use_op.dupe(), &null_t, &u_type)?;
                    let void_t =
                        Type::new(TypeInner::DefT(reason.dupe(), DefT::new(DefTInner::VoidT)));
                    FlowJs::rec_flow_t_with_env(cx, env, trace, use_op.dupe(), &void_t, &u_type)?;
                    let false_t = Type::new(TypeInner::DefT(
                        reason,
                        DefT::new(DefTInner::SingletonBoolT {
                            value: false,
                            from_annot: false,
                        }),
                    ));
                    FlowJs::rec_flow_t_with_env(cx, env, trace, use_op.dupe(), &false_t, &u_type)?;
                }
                RendersVariant::RendersMaybe => {}
                RendersVariant::RendersStar => {}
            }
            rec_renders_to_renders(
                cx,
                env,
                trace,
                use_op,
                (
                    reasonl,
                    &CanonicalRendersForm::StructuralRenders {
                        renders_variant: RendersVariant::RendersNormal,
                        renders_structural_type: renders_structural_type.dupe(),
                    },
                ),
                (reasonu, u),
            )
        }
        (
            CanonicalRendersForm::StructuralRenders {
                renders_variant: RendersVariant::RendersStar,
                renders_structural_type: t,
            },
            CanonicalRendersForm::StructuralRenders {
                renders_variant,
                renders_structural_type: _,
            },
        ) => {
            match renders_variant {
                RendersVariant::RendersNormal | RendersVariant::RendersMaybe => {
                    let renders_star = reconstruct_render_type(reasonl, l);
                    let roa = FlowJs::get_builtin_typeapp_with_env(
                        cx,
                        env,
                        reasonl,
                        None,
                        "ReadonlyArray",
                        vec![renders_star],
                    );
                    FlowJs::rec_flow_t_with_env(
                        cx,
                        env,
                        trace,
                        use_op.dupe(),
                        &roa,
                        &reconstruct_render_type(reasonu, u),
                    )?;
                }
                RendersVariant::RendersStar => {}
            }
            rec_renders_to_renders(
                cx,
                env,
                trace,
                use_op,
                (
                    reasonl,
                    &CanonicalRendersForm::StructuralRenders {
                        renders_variant: RendersVariant::RendersMaybe,
                        renders_structural_type: t.dupe(),
                    },
                ),
                (reasonu, u),
            )
        }
        (
            CanonicalRendersForm::IntrinsicRenders(_)
            | CanonicalRendersForm::NominalRenders { .. }
            | CanonicalRendersForm::StructuralRenders { .. }
            | CanonicalRendersForm::DefaultRenders,
            CanonicalRendersForm::DefaultRenders,
        ) => Ok(()),
    }
}

fn possibly_promoted_render_types_of_react_element_type<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    elem_reason: &Reason,
    opq: &NominalType,
) -> Result<(Vec<Type>, bool), FlowJsException> {
    let on_concretized_types = |ts: Vec<Type>| -> (Vec<Type>, bool) {
        let mut result_ts: Vec<Type> = Vec::new();
        let mut has_failed = false;
        for t in ts {
            match t.deref() {
                TypeInner::AnyT(..)
                | TypeInner::ImplicitInstantiationTvar(box ImplicitInstantiationTvarData {
                    kind: ImplicitInstantiationTvarKind::Placeholder,
                    ..
                }) => {
                    result_ts.push(t);
                }
                TypeInner::DefT(_, def_t) if matches!(&**def_t, DefTInner::RendersT(_)) => {
                    result_ts.push(t);
                }
                _ => {
                    has_failed = true;
                }
            }
        }
        (result_ts, has_failed)
    };

    let concretize_component_renders_and_check =
        |cx: &mut Context<'cx>, component_t: &Type| -> Result<(Vec<Type>, bool), FlowJsException> {
            let extracted = FlowJs::run_render_extractor_with_env(
                cx,
                env,
                unknown_use(),
                elem_reason,
                component_t,
            )?;
            let concrete = FlowJs::possible_concrete_types_for_inspection_with_env(
                cx,
                env,
                elem_reason,
                &extracted,
            )?;
            Ok(on_concretized_types(concrete))
        };

    if let (Some(upper_t), [(_, _, component_t, _), ..]) =
        (&opq.upper_t, &opq.nominal_type_args[..])
        && let TypeInner::DefT(_, def_t) = upper_t.deref()
        && matches!(def_t.deref(), DefTInner::ObjT(_))
    {
        // We only want to promote if this is actually a React of a component, otherwise we want
        // to flow the original object to the tout.
        //
        // We perform a speculative subtyping check and then use ComponentRenders to
        // extract the render type of the component. This type gets concretized, and we continue
        // with renders subtyping if we get a RendersT from ComponentRenders, otherwise we error,
        // as we've already checked for structural compatibility in subtyping kit.
        let top_abstract_component = {
            let config = Type::new(TypeInner::DefT(
                elem_reason.dupe(),
                DefT::new(DefTInner::EmptyT),
            ));
            let renders = FlowJs::get_builtin_react_type_with_env(
                cx,
                env,
                None,
                elem_reason,
                None,
                ExpectedModulePurpose::ReactModuleForReactNodeType,
            )?;
            Type::new(TypeInner::DefT(
                elem_reason.dupe(),
                DefT::new(DefTInner::ReactAbstractComponentT(Box::new(
                    ReactAbstractComponentTData {
                        config,
                        renders,
                        component_kind: ComponentKind::Structural,
                    },
                ))),
            ))
        };
        if FlowJs::speculative_subtyping_succeeds_with_flow_errors(
            cx,
            env,
            component_t,
            &top_abstract_component,
        )? {
            concretize_component_renders_and_check(cx, component_t)
        } else if FlowJs::speculative_subtyping_succeeds_with_flow_errors(
            cx,
            env,
            component_t,
            &Type::new(TypeInner::DefT(
                elem_reason.dupe(),
                DefT::new(DefTInner::SingletonStrT {
                    from_annot: true,
                    value: "svg".into(),
                }),
            )),
        )? {
            Ok((
                vec![Type::new(TypeInner::DefT(
                    elem_reason.dupe(),
                    DefT::new(DefTInner::RendersT(Rc::new(
                        CanonicalRendersForm::IntrinsicRenders("svg".into()),
                    ))),
                ))],
                false,
            ))
        } else {
            Ok((vec![], true))
        }
    } else {
        Ok((vec![], true))
    }
}

fn try_promote_render_type_from_react_element_type<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    use_op: UseOp,
    elem_t: &Type,
    elem_reason: &Reason,
    opq: &NominalType,
    renders_r: &Reason,
    upper_renders: &CanonicalRendersForm,
) -> Result<(), FlowJsException> {
    let (promoted_ts, has_failed) =
        possibly_promoted_render_types_of_react_element_type(cx, env, elem_reason, opq)?;
    let use_op = frame_renders_compat(use_op);
    if has_failed {
        flow_js_utils::add_output_with_env(
            cx,
            env,
            incompatible_renders_error(
                elem_t,
                &reconstruct_render_type(renders_r, upper_renders),
                use_op.dupe(),
            ),
        )?;
    }
    for promoted_l in promoted_ts {
        let repositioned =
            FlowJs::reposition_reason(cx, env, Some(trace), elem_reason, Some(true), &promoted_l)?;
        FlowJs::rec_flow_t_with_env(
            cx,
            env,
            trace,
            use_op.dupe(),
            &repositioned,
            &Type::new(TypeInner::DefT(
                renders_r.dupe(),
                DefT::new(DefTInner::RendersT(Rc::new(upper_renders.clone()))),
            )),
        )?;
    }
    Ok(())
}

pub fn non_renders_to_renders<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    use_op: UseOp,
    l: &Type,
    renders_r: &Reason,
    upper_renders: &CanonicalRendersForm,
) -> Result<(), FlowJsException> {
    match (l.deref(), upper_renders) {
        (
            TypeInner::DefT(_, def_t),
            CanonicalRendersForm::StructuralRenders {
                renders_variant: RendersVariant::RendersMaybe | RendersVariant::RendersStar,
                renders_structural_type: _,
            }
            | CanonicalRendersForm::DefaultRenders,
        ) if matches!(
            def_t.deref(),
            DefTInner::NullT | DefTInner::VoidT | DefTInner::SingletonBoolT { value: false, .. }
        ) =>
        {
            return Ok(());
        }
        (
            TypeInner::DefT(_, def_t),
            CanonicalRendersForm::StructuralRenders {
                renders_variant: RendersVariant::RendersStar,
                renders_structural_type: _,
            }
            | CanonicalRendersForm::DefaultRenders,
        ) if matches!(
            &**def_t,
            DefTInner::ArrT(arr) if matches!(arr.deref(),
                ArrType::ArrayAT(box ArrayATData { .. })
                | ArrType::TupleAT(box TupleATData { .. })
                | ArrType::ROArrayAT(box ROArrayATData { .. }))
        ) =>
        {
            if let DefTInner::ArrT(arr) = def_t.deref() {
                let t = match arr.deref() {
                    ArrType::ArrayAT(box ArrayATData { elem_t: t, .. })
                    | ArrType::TupleAT(box TupleATData { elem_t: t, .. })
                    | ArrType::ROArrayAT(box ROArrayATData { elem_t: t, .. }) => t,
                };
                FlowJs::rec_flow_t_with_env(
                    cx,
                    env,
                    trace,
                    use_op,
                    t,
                    &reconstruct_render_type(renders_r, upper_renders),
                )?;
                return Ok(());
            }
        }
        // Try to do structural subtyping. If that fails promote to a render type
        (
            TypeInner::NominalT {
                reason: reason_opaque,
                nominal_type: opq,
            },
            CanonicalRendersForm::IntrinsicRenders(_) | CanonicalRendersForm::NominalRenders { .. },
        ) if Some(&opq.nominal_id)
            == flow_js_utils::builtin_react_element_nominal_id(cx).as_ref() =>
        {
            try_promote_render_type_from_react_element_type(
                cx,
                env,
                trace,
                use_op,
                l,
                reason_opaque,
                opq,
                renders_r,
                upper_renders,
            )?;
            return Ok(());
        }
        (
            TypeInner::NominalT {
                reason: reason_opaque,
                nominal_type: opq,
            },
            CanonicalRendersForm::StructuralRenders {
                renders_variant: _,
                renders_structural_type: t,
            },
        ) if Some(&opq.nominal_id)
            == flow_js_utils::builtin_react_element_nominal_id(cx).as_ref() =>
        {
            if !FlowJs::speculative_subtyping_succeeds_with_flow_errors(cx, env, l, t)? {
                try_promote_render_type_from_react_element_type(
                    cx,
                    env,
                    trace,
                    use_op,
                    l,
                    reason_opaque,
                    opq,
                    renders_r,
                    upper_renders,
                )?;
            }
            return Ok(());
        }
        // given x <: y, x <: renders y. The only case in which this is not true is when `x` is a component reference,
        // Foo <: renders Foo fails in that case. Since the RHS is in its canonical form we know that we're safe
        // to Flow the LHS to the structural type on the RHS
        (
            _,
            CanonicalRendersForm::StructuralRenders {
                renders_variant: _,
                renders_structural_type: t,
            },
        ) => {
            FlowJs::rec_flow_t_with_env(cx, env, trace, frame_renders_compat(use_op), l, t)?;
            return Ok(());
        }
        (_, CanonicalRendersForm::DefaultRenders) => {
            let react_node = FlowJs::get_builtin_react_type_with_env(
                cx,
                env,
                None,
                renders_r,
                Some(true),
                ExpectedModulePurpose::ReactModuleForReactNodeType,
            )?;
            FlowJs::rec_flow_t_with_env(
                cx,
                env,
                trace,
                frame_renders_compat(use_op),
                l,
                &react_node,
            )?;
            return Ok(());
        }
        (
            TypeInner::AnyT(..)
            | TypeInner::ImplicitInstantiationTvar(box ImplicitInstantiationTvarData {
                kind: ImplicitInstantiationTvarKind::Placeholder,
                ..
            }),
            _,
        ) => return Ok(()),
        (_, _) => {}
    }
    flow_js_utils::add_output_with_env(
        cx,
        env,
        incompatible_renders_error(
            l,
            &reconstruct_render_type(renders_r, upper_renders),
            frame_renders_compat(use_op),
        ),
    )
}

enum RenderTypeSynthesisState {
    IntermediateSynthesisState {
        normalized_render_type_collector: type_collector::TypeCollector,
        renders_variant: RendersVariant,
    },
    // If we encounter anything that we can't turn into render type.
    // e.g. arbitrary stuff in React.Node
    FailedSynthesisState,
}

pub(super) fn try_synthesize_render_type_with_env<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    drop_renders_any: bool,
    t: &Type,
) -> Result<Option<(RendersVariant, Vec<Type>)>, FlowJsException> {
    fn merge_renders_variant(v1: RendersVariant, v2: RendersVariant) -> RendersVariant {
        match (v1, v2) {
            (RendersVariant::RendersNormal, RendersVariant::RendersNormal) => {
                RendersVariant::RendersNormal
            }
            (RendersVariant::RendersNormal, RendersVariant::RendersMaybe) => {
                RendersVariant::RendersMaybe
            }
            (RendersVariant::RendersNormal, RendersVariant::RendersStar) => {
                RendersVariant::RendersStar
            }
            (RendersVariant::RendersMaybe, RendersVariant::RendersNormal) => {
                RendersVariant::RendersMaybe
            }
            (RendersVariant::RendersStar, RendersVariant::RendersNormal) => {
                RendersVariant::RendersStar
            }
            (RendersVariant::RendersMaybe, RendersVariant::RendersMaybe) => {
                RendersVariant::RendersMaybe
            }
            (RendersVariant::RendersMaybe, RendersVariant::RendersStar) => {
                RendersVariant::RendersStar
            }
            (RendersVariant::RendersStar, RendersVariant::RendersMaybe) => {
                RendersVariant::RendersStar
            }
            (RendersVariant::RendersStar, RendersVariant::RendersStar) => {
                RendersVariant::RendersStar
            }
        }
    }

    fn on_concretized_react_node_types<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        ts: Vec<Type>,
        ancestors: &mut BTreeSet<Type>,
        drop_renders_any: bool,
        mut state: RenderTypeSynthesisState,
    ) -> Result<RenderTypeSynthesisState, FlowJsException> {
        for t in ts {
            let (normalized_render_type_collector, renders_variant) = match &state {
                RenderTypeSynthesisState::FailedSynthesisState => {
                    return Ok(RenderTypeSynthesisState::FailedSynthesisState);
                }
                RenderTypeSynthesisState::IntermediateSynthesisState {
                    normalized_render_type_collector,
                    renders_variant,
                } => (normalized_render_type_collector, renders_variant),
            };
            if !ancestors.insert(t.dupe()) {
                return Ok(RenderTypeSynthesisState::FailedSynthesisState);
            }
            match t.deref() {
                TypeInner::GenericT(_) => {
                    normalized_render_type_collector.add(t.dupe());
                }
                TypeInner::AnyT(..)
                | TypeInner::ImplicitInstantiationTvar(box ImplicitInstantiationTvarData {
                    kind: ImplicitInstantiationTvarKind::Placeholder,
                    ..
                }) => {
                    if !drop_renders_any {
                        normalized_render_type_collector.add(t.dupe());
                    }
                }
                TypeInner::NominalT {
                    reason: nominal_elem_reason,
                    nominal_type: opq,
                } if Some(&opq.nominal_id)
                    == flow_js_utils::builtin_react_element_nominal_id(cx).as_ref() =>
                {
                    let (promoted_ts, has_failed) =
                        possibly_promoted_render_types_of_react_element_type(
                            cx,
                            env,
                            nominal_elem_reason,
                            opq,
                        )?;
                    if has_failed {
                        state = RenderTypeSynthesisState::FailedSynthesisState;
                    } else {
                        let current_state = std::mem::replace(
                            &mut state,
                            RenderTypeSynthesisState::FailedSynthesisState,
                        );
                        state = on_concretized_react_node_types(
                            cx,
                            env,
                            promoted_ts,
                            ancestors,
                            drop_renders_any,
                            current_state,
                        )?;
                    }
                }
                TypeInner::DefT(_, def_t) => match def_t.deref() {
                    DefTInner::RendersT(renders) => match renders.as_ref() {
                        CanonicalRendersForm::IntrinsicRenders(_)
                        | CanonicalRendersForm::NominalRenders { .. } => {
                            normalized_render_type_collector.add(t.dupe());
                        }
                        CanonicalRendersForm::DefaultRenders => {
                            state = RenderTypeSynthesisState::FailedSynthesisState;
                        }
                        CanonicalRendersForm::StructuralRenders {
                            renders_variant: renders_variant_prime,
                            renders_structural_type,
                        } => {
                            let concrete = FlowJs::possible_concrete_types_for_inspection_with_env(
                                cx,
                                env,
                                type_util::reason_of_t(renders_structural_type),
                                renders_structural_type,
                            )?;
                            let new_variant =
                                merge_renders_variant(*renders_variant, *renders_variant_prime);
                            let collector = match std::mem::replace(
                                &mut state,
                                RenderTypeSynthesisState::FailedSynthesisState,
                            ) {
                                RenderTypeSynthesisState::IntermediateSynthesisState {
                                    normalized_render_type_collector,
                                    ..
                                } => normalized_render_type_collector,
                                _ => unreachable!(),
                            };
                            state = on_concretized_react_node_types(
                                cx,
                                env,
                                concrete,
                                ancestors,
                                drop_renders_any,
                                RenderTypeSynthesisState::IntermediateSynthesisState {
                                    normalized_render_type_collector: collector,
                                    renders_variant: new_variant,
                                },
                            )?;
                        }
                    },
                    DefTInner::NullT
                    | DefTInner::VoidT
                    | DefTInner::SingletonBoolT { value: false, .. } => {
                        let new_variant =
                            merge_renders_variant(*renders_variant, RendersVariant::RendersMaybe);
                        let collector = match std::mem::replace(
                            &mut state,
                            RenderTypeSynthesisState::FailedSynthesisState,
                        ) {
                            RenderTypeSynthesisState::IntermediateSynthesisState {
                                normalized_render_type_collector,
                                ..
                            } => normalized_render_type_collector,
                            _ => unreachable!(),
                        };
                        state = RenderTypeSynthesisState::IntermediateSynthesisState {
                            normalized_render_type_collector: collector,
                            renders_variant: new_variant,
                        };
                    }
                    DefTInner::ArrT(arr) => {
                        let arr_elem_t = match arr.deref() {
                            ArrType::ArrayAT(box ArrayATData { elem_t, .. }) => elem_t,
                            ArrType::TupleAT(box TupleATData { elem_t, .. }) => elem_t,
                            ArrType::ROArrayAT(box ROArrayATData { elem_t: t, .. }) => t,
                        };
                        let concrete = FlowJs::possible_concrete_types_for_inspection_with_env(
                            cx,
                            env,
                            type_util::reason_of_t(arr_elem_t),
                            arr_elem_t,
                        )?;
                        let new_variant =
                            merge_renders_variant(*renders_variant, RendersVariant::RendersStar);
                        let collector = match std::mem::replace(
                            &mut state,
                            RenderTypeSynthesisState::FailedSynthesisState,
                        ) {
                            RenderTypeSynthesisState::IntermediateSynthesisState {
                                normalized_render_type_collector,
                                ..
                            } => normalized_render_type_collector,
                            _ => unreachable!(),
                        };
                        state = on_concretized_react_node_types(
                            cx,
                            env,
                            concrete,
                            ancestors,
                            drop_renders_any,
                            RenderTypeSynthesisState::IntermediateSynthesisState {
                                normalized_render_type_collector: collector,
                                renders_variant: new_variant,
                            },
                        )?;
                    }
                    _ => {
                        state = RenderTypeSynthesisState::FailedSynthesisState;
                    }
                },
                _ => {
                    state = RenderTypeSynthesisState::FailedSynthesisState;
                }
            }
            ancestors.remove(&t);
        }
        Ok(state)
    }

    let state = RenderTypeSynthesisState::IntermediateSynthesisState {
        normalized_render_type_collector: type_collector::TypeCollector::create(),
        renders_variant: RendersVariant::RendersNormal,
    };
    let concrete = FlowJs::possible_concrete_types_for_inspection_with_env(
        cx,
        env,
        type_util::reason_of_t(t),
        t,
    )?;
    match on_concretized_react_node_types(
        cx,
        env,
        concrete,
        &mut BTreeSet::new(),
        drop_renders_any,
        state,
    )? {
        RenderTypeSynthesisState::FailedSynthesisState => Ok(None),
        RenderTypeSynthesisState::IntermediateSynthesisState {
            normalized_render_type_collector,
            renders_variant,
        } => Ok(Some((
            renders_variant,
            normalized_render_type_collector
                .collect()
                .into_iter()
                .collect(),
        ))),
    }
}

pub fn try_synthesize_render_type<'cx>(
    cx: &mut Context<'cx>,
    drop_renders_any: bool,
    t: &Type,
) -> Result<Option<(RendersVariant, Vec<Type>)>, FlowJsException> {
    try_synthesize_render_type_with_env(cx, &FlowJsEnv::entry(), drop_renders_any, t)
}
