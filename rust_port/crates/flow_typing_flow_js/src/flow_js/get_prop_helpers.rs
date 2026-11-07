/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use flow_typing_errors::error_message::EPrivateLookupFailedData;
use flow_typing_errors::error_message::EPropNotFoundInLookupData;
use flow_typing_flow_common::flow_js_utils::CgLookupArgs;
use flow_typing_flow_js_env::FlowJsEnv;
use flow_typing_type::type_::GenericTData;
use flow_typing_type::type_::LookupTData;
use flow_typing_type::type_::MethodTData;
use flow_typing_type::type_::ReadElemData;
use flow_typing_type::type_::WriteElemData;

use super::helpers::*;
use super::*;

impl flow_js_utils::GetPropHelper for FlowJs {
    type R = Box<dyn for<'b> FnOnce(&mut Context<'b>, Tvar) -> Result<(), FlowJsException>>;

    fn error_type<'cx>(
        _cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        trace: DepthTrace,
        reason: Reason,
    ) -> Result<Self::R, FlowJsException> {
        let env = env.dupe();
        Ok(Box::new(move |cx, tout| {
            rec_flow_t(
                cx,
                &env,
                trace,
                unknown_use(),
                (&any_t::error(reason), &Type::new(TypeInner::OpenT(tout))),
            )
        }))
    }

    fn return_<'cx>(
        _cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        use_op: UseOp,
        trace: DepthTrace,
        t: Type,
    ) -> Result<Self::R, FlowJsException> {
        let env = env.dupe();
        Ok(Box::new(move |cx, tout| {
            rec_flow_t(
                cx,
                &env,
                trace,
                use_op,
                (&t, &Type::new(TypeInner::OpenT(tout))),
            )
        }))
    }

    fn dict_read_check<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        trace: DepthTrace,
        use_op: &UseOp,
        pair: (&Type, &Type),
    ) -> Result<(), FlowJsException> {
        rec_flow_t(cx, env, trace, use_op.dupe(), pair)
    }

    fn reposition<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        trace: Option<DepthTrace>,
        loc: ALoc,
        t: Type,
    ) -> Result<Type, FlowJsException> {
        helpers::reposition(cx, env, trace, loc, None, None, t)
    }

    fn cg_lookup<'cx>(
        _cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        trace: DepthTrace,
        obj_t: Type,
        method_accessible: bool,
        super_t: Type,
        args: CgLookupArgs,
    ) -> Result<Self::R, FlowJsException> {
        let env = env.dupe();
        let CgLookupArgs {
            reason_op,
            lookup_kind,
            indexer_fallback,
            propref,
            use_op,
            ids,
            access_strictness_kind: _,
        } = args;
        Ok(Box::new(move |cx, tout| {
            rec_flow(
                cx,
                &env,
                trace,
                (
                    &super_t,
                    &UseT::new(UseTInner::LookupT(Box::new(LookupTData {
                        reason: reason_op,
                        lookup_kind: Box::new(lookup_kind),
                        indexer_fallback,
                        try_ts_on_failure: Rc::from([]),
                        propref: Box::new(propref),
                        lookup_action: Box::new(LookupAction::ReadProp(Box::new(ReadPropData {
                            use_op,
                            obj_t,
                            tout,
                        }))),
                        method_accessible,
                        ids: Some(ids),
                        ignore_dicts: false,
                    }))),
                ),
            )
        }))
    }

    fn cg_get_prop<'cx>(
        _cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        trace: DepthTrace,
        t: Type,
        args: (
            UseOp,
            Reason,
            Option<i32>,
            (Reason, flow_common::reason::Name),
        ),
        _access_strictness_kind: TypeStrictnessKind,
    ) -> Result<Self::R, FlowJsException> {
        let env = env.dupe();
        let (use_op, access_reason, id, (prop_reason, name)) = args;
        let env = env.dupe();
        Ok(Box::new(move |cx, tout| {
            rec_flow(
                cx,
                &env,
                trace,
                (
                    &t,
                    &UseT::new(UseTInner::GetPropT(Box::new(GetPropTData {
                        use_op,
                        reason: access_reason,
                        id,
                        from_annot: false,
                        skip_optional: false,
                        propref: Box::new(flow_typing_type::type_util::mk_named_prop(
                            prop_reason,
                            false,
                            name,
                        )),
                        tout: Box::new(tout),
                        hint: hint_unavailable(),
                    }))),
                ),
            )
        }))
    }

    fn mk_react_dro<'cx>(
        cx: &mut Context<'cx>,
        _env: &FlowJsEnv,
        use_op: UseOp,
        dro: &ReactDro,
        t: Type,
    ) -> Type {
        mk_react_dro(cx, use_op, dro.clone(), t)
    }

    fn key_overlaps_with_indexer() -> Option<
        for<'b> fn(
            &mut Context<'b>,
            &FlowJsEnv,
            &Type,
            &Type,
        ) -> Result<bool, flow_utils_concurrency::job_error::JobError>,
    > {
        Some(speculative_subtyping_succeeds_non_speculating)
    }
}

// Property lookup functions in objects and instances

pub(crate) fn prop_typo_suggestion<'cx>(
    cx: &Context<'cx>,
    ids: &[properties::Id],
    name: &str,
) -> Option<FlowSmolStr> {
    let possible_names: Vec<_> = ids
        .iter()
        .flat_map(|id| {
            cx.find_props(id.dupe())
                .iter()
                .filter_map(|(k, _)| k.as_smol_str_opt().map(Dupe::dupe))
                // A class's `constructor` method isn't a member, so a missing member of
                // that name is never its own typo.
                .filter(|k| k != name)
                .collect::<Vec<_>>()
        })
        .collect();
    let refs: Vec<&FlowSmolStr> = possible_names.iter().collect();
    flow_common_utils::utils_js::typo_suggestion(&refs, name)
}

/// `prop_typo_suggestion` for a property key that may be a symbol. A symbol key
/// has no string spelling to match against string property names, so it gets no
/// suggestion. Passing its empty string accessor instead would land in
/// `typo_suggestion`'s longest-name bucket and suggest every short property.
pub(crate) fn prop_typo_suggestion_for_name<'cx>(
    cx: &Context<'cx>,
    ids: &[properties::Id],
    name: &Name,
) -> Option<FlowSmolStr> {
    prop_typo_suggestion(cx, ids, name.as_str_opt()?)
}

pub(super) fn get_private_prop<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    allow_method_access: bool,
    trace: DepthTrace,
    l: &Type,
    reason_c: &Reason,
    instance: &InstType,
    use_op: &UseOp,
    reason_op: &Reason,
    prop_name: &FlowSmolStr,
    scopes: &[ClassBinding],
    is_static: bool,
    tout: &Tvar,
) -> Result<(), FlowJsException> {
    match scopes {
        [] => flow_js_utils::add_output_with_env(
            cx,
            env,
            ErrorMessage::EPrivateLookupFailed(Box::new(EPrivateLookupFailedData {
                loc: reason_op.loc().dupe(),
                object: flow_js_utils::type_reference_with_reason_for_error(l, reason_c.dupe()),
                prop_name: Name::new(prop_name.dupe()),
                use_op: use_op.dupe(),
            })),
        ),
        [scope, rest_scopes @ ..] => {
            if scope.class_binding_id != instance.class_id {
                get_private_prop(
                    cx,
                    env,
                    allow_method_access,
                    trace,
                    l,
                    reason_c,
                    instance,
                    use_op,
                    reason_op,
                    prop_name,
                    rest_scopes,
                    is_static,
                    tout,
                )
            } else {
                let name = Name::new(prop_name.dupe());
                let do_lookup =
                    |cx: &mut Context<'cx>, p: PropertyType| -> Result<(), FlowJsException> {
                        let action = LookupAction::ReadProp(Box::new(ReadPropData {
                            use_op: use_op.dupe(),
                            obj_t: l.dupe(),
                            tout: tout.dupe(),
                        }));
                        let propref = mk_named_prop(reason_op.dupe(), false, name.dupe());
                        perform_lookup_action(
                            cx,
                            env,
                            trace,
                            &propref,
                            &p,
                            None,
                            PropertySource::PropertyMapProperty,
                            l,
                            reason_op,
                            &action,
                            None,
                        )
                    };
                let field_maps = if is_static {
                    instance.class_private_static_fields.dupe()
                } else {
                    instance.class_private_fields.dupe()
                };
                let props = cx.find_props(field_maps);
                match props.get(&name) {
                    Some(p) => do_lookup(cx, property::property_type(p)),
                    None => {
                        let method_maps = if is_static {
                            instance.class_private_static_methods.dupe()
                        } else {
                            instance.class_private_methods.dupe()
                        };
                        let method_props = cx.find_props(method_maps);
                        match method_props.get(&name) {
                            Some(p) => do_lookup(
                                cx,
                                property::property_type(&flow_js_utils::method_property_for_read(
                                    allow_method_access,
                                    p.dupe(),
                                )),
                            ),
                            None => flow_js_utils::add_output_with_env(
                                cx,
                                env,
                                ErrorMessage::EPrivateLookupFailed(Box::new(
                                    EPrivateLookupFailedData {
                                        loc: reason_op.loc().dupe(),
                                        object: flow_js_utils::type_reference_with_reason_for_error(
                                            l,
                                            reason_c.dupe(),
                                        ),
                                        prop_name: name,
                                        use_op: use_op.dupe(),
                                    },
                                )),
                            ),
                        }
                    }
                }
            }
        }
    }
}

pub(super) fn elem_action_on_obj<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    use_op: &UseOp,
    propref: PropRef,
    obj: &Type,
    reason_op: &Reason,
    action: &ElemAction<Cx>,
) -> Result<(), FlowJsException> {
    match action {
        ElemAction::ReadElem(box ReadElemData {
            id,
            from_annot,
            skip_optional,
            tout,
            ..
        }) => rec_flow(
            cx,
            env,
            trace,
            (
                obj,
                &UseT::new(UseTInner::GetPropT(Box::new(GetPropTData {
                    use_op: use_op.dupe(),
                    reason: reason_op.dupe(),
                    from_annot: *from_annot,
                    skip_optional: *skip_optional,
                    id: id.dupe(),
                    propref: Box::new(propref),
                    tout: Box::new(tout.dupe()),
                    hint: hint_unavailable(),
                }))),
            ),
        ),
        ElemAction::WriteElem(box WriteElemData { tin, tout, mode }) => {
            rec_flow(
                cx,
                env,
                trace,
                (
                    obj,
                    &UseT::new(UseTInner::SetPropT(
                        use_op.dupe(),
                        reason_op.dupe(),
                        Box::new(propref),
                        mode.clone(),
                        WriteCtx::Normal,
                        tin.dupe(),
                        None,
                    )),
                ),
            )?;
            if let Some(t) = tout {
                rec_flow_t(cx, env, trace, unknown_use(), (obj, t))?;
            }
            Ok(())
        }
        ElemAction::CallElem(reason_call, ft) => rec_flow(
            cx,
            env,
            trace,
            (
                obj,
                &UseT::new(UseTInner::MethodT(Box::new(MethodTData {
                    use_op: use_op.dupe(),
                    reason: reason_call.dupe(),
                    prop_reason: reason_op.dupe(),
                    propref: Box::new(propref),
                    method_action: ft.clone(),
                }))),
            ),
        ),
    }
}

pub(super) fn write_computed_obj_prop<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    use_op: &UseOp,
    elem_t: &Type,
    obj_t: &Type,
    tin: &Type,
) -> Result<(), FlowJsException> {
    let reason = reason_of_t(elem_t);
    fn loop_<'cx>(
        cx: &mut Context<'cx>,
        env: &FlowJsEnv,
        trace: DepthTrace,
        use_op: &UseOp,
        elem_t: &Type,
        reason: &Reason,
        obj_t: &Type,
        tin: &Type,
    ) -> Result<(), FlowJsException> {
        match elem_t.deref() {
            TypeInner::DefT(reason, def_t)
                if matches!(def_t.deref(), DefTInner::SingletonStrT { .. }) =>
            {
                let loc = reason.loc().dupe();
                flow_js_utils::add_output_with_env(
                    cx,
                    env,
                    ErrorMessage::EInternal(Box::new((loc, InternalError::PropRefComputedLiteral))),
                )?;
                Ok(())
            }
            TypeInner::AnyT(_, _)
            | TypeInner::ImplicitInstantiationTvar(box ImplicitInstantiationTvarData {
                kind: ImplicitInstantiationTvarKind::Placeholder,
                ..
            }) => {
                let any = flow_js_utils::untyped_any_result(elem_t, reason.dupe());
                rec_flow_t(cx, env, trace, unknown_use(), (tin, &any))
            }
            TypeInner::DefT(_, def_t) if matches!(def_t.deref(), DefTInner::StrGeneralT(_)) => {
                flow_js_utils::add_output_with_env(
                    cx,
                    env,
                    ErrorMessage::EPropNotFoundInLookup(Box::new(EPropNotFoundInLookupData {
                        prop_name: None,
                        prop_loc: reason.loc().dupe(),
                        object: flow_js_utils::type_reference_with_reason_for_error(
                            obj_t,
                            reason_of_t(obj_t).dupe(),
                        ),
                        use_op: use_op.dupe(),
                        suggestion: None,
                    })),
                )?;
                Ok(())
            }
            TypeInner::TemplateLiteralT { .. } => {
                flow_js_utils::add_output_with_env(
                    cx,
                    env,
                    ErrorMessage::EPropNotFoundInLookup(Box::new(EPropNotFoundInLookupData {
                        prop_name: None,
                        prop_loc: reason.loc().dupe(),
                        object: flow_js_utils::type_reference_with_reason_for_error(
                            obj_t,
                            reason_of_t(obj_t).dupe(),
                        ),
                        use_op: use_op.dupe(),
                        suggestion: None,
                    })),
                )?;
                Ok(())
            }
            TypeInner::GenericT(box GenericTData { bound, .. }) => {
                loop_(cx, env, trace, use_op, bound, reason, obj_t, tin)
            }
            TypeInner::NominalT { nominal_type, .. } if let Some(upper) = &nominal_type.upper_t => {
                loop_(cx, env, trace, use_op, upper, reason, obj_t, tin)
            }
            TypeInner::DefT(reason, def_t)
                if let DefTInner::SingletonNumT {
                    value: NumberLiteral(value, _),
                    ..
                } = def_t.deref() =>
            {
                let kind =
                    flow_typing_errors::intermediate_error_types::InvalidObjKey::kind_of_num_value(
                        *value,
                    );
                flow_js_utils::add_output_with_env(
                    cx,
                    env,
                    ErrorMessage::EObjectComputedPropertyAssign(Box::new((
                        flow_js_utils::type_reference_with_reason_for_error(elem_t, reason.dupe()),
                        None,
                        kind,
                    ))),
                )?;
                Ok(())
            }
            _ => {
                flow_js_utils::add_output_with_env(
                    cx,
                    env,
                    ErrorMessage::EObjectComputedPropertyAssign(Box::new((
                        flow_js_utils::type_reference_with_reason_for_error(elem_t, reason.dupe()),
                        None,
                        flow_typing_errors::intermediate_error_types::InvalidObjKey::Other,
                    ))),
                )?;
                Ok(())
            }
        }
    }
    let ts = possible_concrete_types_for_computed_object_keys(cx, env, reason, elem_t)?;
    for t in &ts {
        loop_(cx, env, trace, use_op, t, reason, obj_t, tin)?;
    }
    Ok(())
}

pub(super) fn write_obj_prop<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    use_op: &UseOp,
    mode: &SetMode,
    o: &ObjType,
    propref: &PropRef,
    reason_obj: &Reason,
    reason_op: &Reason,
    tin: &Type,
    prop_tout: &Option<Type>,
) -> Result<(), FlowJsException> {
    let obj_t = Type::new(TypeInner::DefT(
        reason_obj.dupe(),
        DefT::new(DefTInner::ObjT(Rc::new(o.clone()))),
    ));
    let action = LookupAction::WriteProp(Box::new(WritePropData {
        use_op: use_op.dupe(),
        obj_t: obj_t.dupe(),
        prop_tout: prop_tout.dupe(),
        tin: tin.dupe(),
        write_ctx: WriteCtx::Normal,
        mode: mode.clone(),
    }));
    match flow_js_utils::get_prop_t_kit::get_obj_prop::<FlowJs>(
        cx, env, &trace, use_op, false, true, o, propref, reason_op,
    )? {
        Some((p, target_kind)) => perform_lookup_action(
            cx,
            env,
            trace,
            propref,
            &property::property_type(&p),
            Some((&p, o.strictness_kind)),
            target_kind,
            &obj_t,
            reason_op,
            &action,
            None,
        ),
        None => match propref {
            // An object isn't a class instance, so its constructor is the member of that name.
            PropRef::Constructor(reason) => write_obj_prop(
                cx,
                env,
                trace,
                use_op,
                mode,
                o,
                &mk_named_prop(reason.dupe(), false, Name::new("constructor")),
                reason_obj,
                reason_op,
                tin,
                prop_tout,
            ),
            PropRef::Named { reason, name, .. } if *name == Name::new("constructor") => {
                let reason = reason.dupe().replace_desc(VirtualReasonDesc::RFunction(
                    flow_common::reason::ReasonDescFunction::RNormal,
                ));
                let rest_param = Some(FunRestParam(
                    None,
                    reason.loc().dupe(),
                    empty_t::make(reason.dupe().replace_desc_new(None)),
                ));
                let mut funtype = mk_boundfunctiontype(
                    global_this(reason.dupe()),
                    None,
                    vec![],
                    rest_param,
                    reason.dupe(),
                    flow_common::error_ref::FunctionReferenceKind::Function,
                    None,
                    None,
                    any_t::untyped(reason.dupe()),
                );
                funtype.literal = true;
                let fn_t = Type::new(TypeInner::DefT(
                    reason.dupe(),
                    DefT::new(DefTInner::FunT(
                        dummy_static(reason.dupe()),
                        Rc::new(funtype),
                    )),
                ));
                rec_flow_t(cx, env, trace, use_op.dupe(), (tin, &fn_t))?;
                if let Some(t) = prop_tout {
                    rec_flow_t(cx, env, trace, unknown_use(), (&fn_t, t))?;
                }
                Ok(())
            }
            PropRef::Named {
                reason: reason_prop,
                name,
                ..
            } => {
                if obj_type::is_exact(&o.flags.obj_kind) {
                    let suggestion =
                        prop_typo_suggestion_for_name(cx, &[o.props_tmap.dupe()], name);
                    flow_js_utils::add_output_with_env(
                        cx,
                        env,
                        ErrorMessage::EPropNotFoundInLookup(Box::new(EPropNotFoundInLookupData {
                            prop_name: Some(name.dupe()),
                            prop_loc: reason_prop.loc().dupe(),
                            object: flow_js_utils::type_reference_with_reason_for_error(
                                &obj_t,
                                reason_obj.dupe(),
                            ),
                            use_op: use_op.dupe(),
                            suggestion,
                        })),
                    )
                } else {
                    rec_flow(
                        cx,
                        env,
                        trace,
                        (
                            &o.proto_t,
                            &UseT::new(UseTInner::LookupT(Box::new(LookupTData {
                                reason: reason_op.dupe(),
                                lookup_kind: Box::new(LookupKind::Strict(obj_t.dupe())),
                                indexer_fallback: None,
                                try_ts_on_failure: Rc::from([]),
                                propref: Box::new(propref.clone()),
                                lookup_action: Box::new(action),
                                ids: Some([o.props_tmap.dupe()].into_iter().collect()),
                                method_accessible: true,
                                ignore_dicts: false,
                            }))),
                        ),
                    )
                }
            }
            PropRef::Computed(elem_t) => {
                write_computed_obj_prop(cx, env, trace, use_op, elem_t, &obj_t, tin)
            }
        },
    }
}
