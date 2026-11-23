/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::ops::Deref;
use std::rc::Rc;

use dupe::Dupe;
use flow_common::reason::Name;
use flow_common::reason::Reason;
use flow_typing_context::Context;
use flow_typing_flow_common::flow_js_utils;
use flow_typing_flow_common::flow_js_utils::map_on_resolved_type;
use flow_typing_flow_common::flow_js_utils::value_to_type_reference_transform;
use flow_typing_flow_js::flow_js;
use flow_typing_flow_js::flow_js::FlowJs;
use flow_typing_flow_js::tvar_resolver;
use flow_typing_type::type_::Cont;
use flow_typing_type::type_::DefTInner;
use flow_typing_type::type_::GetPropTData;
use flow_typing_type::type_::PolyTData;
use flow_typing_type::type_::PropRef;
use flow_typing_type::type_::SpecializeTData;
use flow_typing_type::type_::Tvar;
use flow_typing_type::type_::Type;
use flow_typing_type::type_::TypeInner;
use flow_typing_type::type_::TypeTKind;
use flow_typing_type::type_::UseOp;
use flow_typing_type::type_::UseT;
use flow_typing_type::type_::UseTInner;
use flow_typing_type::type_::ValueToTypeReferenceTData;
use flow_typing_type::type_::hint_unavailable;
use flow_typing_type::type_::property;
use flow_typing_type::type_::type_collector::TypeCollector;
use flow_typing_type::type_::union_rep::UnionKind;
use flow_typing_type::type_::unknown_use;
use flow_typing_type::type_util;
use flow_typing_type::type_util::reason_of_t;
use flow_utils_concurrency::job_error::JobError;

pub fn specialize<'a>(
    cx: &mut Context<'a>,
    c: Type,
    use_op: UseOp,
    reason_op: Reason,
    reason_tapp: Reason,
    targs: Option<Vec<Type>>,
) -> Type {
    let reason = reason_of_t(&c).dupe();
    let reason_inner = reason.dupe();
    let f = move |cx: &mut Context<'_>, c: Type| -> Result<Type, JobError> {
        let collector = TypeCollector::create();
        let use_t = UseT::new(UseTInner::SpecializeT(Box::new(SpecializeTData {
            use_op,
            reason: reason_op,
            reason2: reason_tapp,
            targs: targs.map(Rc::from),
            upper: Some(Box::new(Cont::Collect(collector.dupe()))),
            from_value: true,
        })));
        flow_js::flow_non_speculating(cx, (&c, &use_t))?;
        Ok(collector
            .union_opt(reason_inner.dupe())
            .unwrap_or_else(|| tvar_resolver::default_no_lowers(&reason_inner)))
    };
    map_on_resolved_type(cx, reason, c, f)
}

/// Checks the type arguments of `c<targs>`, a type application written in the source, once
/// inference is done. Expanding a type application does not check them again. `targs` is
/// `None` for a reference to a class without type arguments, as in an `extends` clause. Unless
/// the application stays a value (`from_value`), this also checks that `c` can be used as a type,
/// as turning the expanded application into a type does.
pub fn validate_type_args<'a>(
    cx: &mut Context<'a>,
    use_op: UseOp,
    reason_tapp: Reason,
    c: Type,
    targs: Option<Rc<[Type]>>,
    from_value: bool,
) {
    let reason_op = reason_of_t(&c).dupe();
    cx.add_post_inference_validation_flow(
        c,
        UseT::new(UseTInner::SpecializeT(Box::new(SpecializeTData {
            use_op,
            reason: reason_op,
            reason2: reason_tapp,
            targs,
            upper: None,
            from_value,
        }))),
    );
}

pub fn mixin<'a>(cx: &mut Context<'a>, reason: Reason, i: Type) -> Type {
    let reason_inner = reason.dupe();
    let f = move |cx: &mut Context<'_>, i: Type| -> Result<Type, JobError> {
        let reason_for_mixin = reason_inner.dupe();
        let collector = TypeCollector::create();
        let use_t = UseT::new(UseTInner::MixinT(reason_for_mixin, collector.dupe()));
        flow_js::flow_non_speculating(cx, (&i, &use_t))?;
        Ok(type_util::union_of_ts_opt(
            reason_inner.dupe(),
            collector.collect_to_vec(),
            Some(UnionKind::ResolvedKind),
        )
        .unwrap_or_else(|| tvar_resolver::default_no_lowers(&reason_inner)))
    };
    map_on_resolved_type(cx, reason, i, f)
}

pub fn obj_test_proto<'a>(cx: &mut Context<'a>, reason: Reason, t: Type) -> Type {
    let reason_inner = reason.dupe();
    let f = move |cx: &mut Context<'_>, t: Type| -> Result<Type, JobError> {
        let reason_for_proto = reason_inner.dupe();
        let collector = TypeCollector::create();
        let use_t = UseT::new(UseTInner::ObjTestProtoT(reason_for_proto, collector.dupe()));
        flow_js::flow_non_speculating(cx, (&t, &use_t))?;
        let t = collector
            .union_opt(reason_inner.dupe())
            .unwrap_or_else(|| tvar_resolver::default_no_lowers(&reason_inner));
        Ok(tvar_resolver::resolved_t(
            tvar_resolver::default_no_lowers,
            true,
            cx,
            t,
        ))
    };
    map_on_resolved_type(cx, reason, t, f)
}

pub fn get_prop<'a>(
    cx: &mut Context<'a>,
    use_op: UseOp,
    reason: Reason,
    op_reason: Option<Reason>,
    name: Name,
    l: Type,
) -> Type {
    let op_reason = op_reason.unwrap_or_else(|| reason.dupe());
    let op_reason_inner = op_reason.dupe();
    let f = move |cx: &mut Context<'_>, l: Type| -> Result<Type, JobError> {
        let op_reason_for_flow = op_reason_inner.dupe();
        tvar_resolver::mk_tvar_and_fully_resolve_no_wrap_where(
            cx,
            op_reason_inner,
            move |cx, tout_reason, tout_id| {
                let tout = Tvar::new(tout_reason.dupe(), tout_id as u32);
                let use_t = UseT::new(UseTInner::GetPropT(Box::new(GetPropTData {
                    use_op,
                    reason: op_reason_for_flow,
                    id: None,
                    from_annot: false,
                    skip_optional: false,
                    propref: Box::new(PropRef::Named {
                        reason,
                        name,
                        from_indexed_access: false,
                    }),
                    tout: Box::new(tout),
                    hint: hint_unavailable(),
                })));
                flow_js::flow_non_speculating(cx, (&l, &use_t))
            },
        )
    };
    map_on_resolved_type(cx, op_reason, l, f)
}

pub fn qualify_type<'a>(
    cx: &mut Context<'a>,
    use_op: UseOp,
    reason: Reason,
    op_reason: Reason,
    prop_name: Name,
    l: Type,
) -> Type {
    let op_reason_inner = op_reason.dupe();
    let f = move |cx: &mut Context<'_>, l: Type| -> Result<Type, JobError> {
        let op_reason_for_flow = op_reason_inner.dupe();
        tvar_resolver::mk_tvar_and_fully_resolve_no_wrap_where(
            cx,
            op_reason_inner,
            move |cx, tout_reason, tout_id| {
                let tout = Tvar::new(tout_reason.dupe(), tout_id as u32);
                for t in flow_js_utils::flow_js_result_to_job_error(
                    FlowJs::possible_concrete_types_for_imports_exports(
                        cx,
                        &op_reason_for_flow,
                        &l,
                    ),
                )? {
                    let obj_t = if let TypeInner::NamespaceT(ns) = t.deref() {
                        let props = cx.find_props(ns.types_tmap.dupe());
                        if let Some(prop) = props.get(&prop_name).and_then(property::read_t) {
                            let prop_t = flow_js::reposition_non_speculating(
                                cx,
                                op_reason_for_flow.loc().dupe(),
                                prop,
                            )?;
                            let tout_t = Type::new(TypeInner::OpenT(tout.dupe()));
                            flow_js::flow_non_speculating(
                                cx,
                                (&prop_t, &UseT::new(UseTInner::UseT(use_op.dupe(), tout_t))),
                            )?;
                            continue;
                        }
                        ns.values_type.dupe()
                    } else {
                        t
                    };
                    let use_t = UseT::new(UseTInner::GetPropT(Box::new(GetPropTData {
                        use_op: use_op.dupe(),
                        reason: op_reason_for_flow.dupe(),
                        id: None,
                        from_annot: false,
                        skip_optional: false,
                        propref: Box::new(PropRef::Named {
                            reason: reason.dupe(),
                            name: prop_name.dupe(),
                            from_indexed_access: false,
                        }),
                        tout: Box::new(tout.dupe()),
                        hint: hint_unavailable(),
                    })));
                    flow_js::flow_non_speculating(cx, (&obj_t, &use_t))?;
                }
                Ok(())
            },
        )
    };
    map_on_resolved_type(cx, op_reason, l, f)
}

pub fn mk_instance<'a>(
    cx: &mut Context<'a>,
    instance_reason: Reason,
    l: Type,
    type_t_kind: Option<TypeTKind>,
    use_desc: Option<bool>,
) -> Type {
    let type_t_kind = type_t_kind.unwrap_or(TypeTKind::InstanceKind);
    let use_desc = use_desc.unwrap_or(false);
    let instance_reason_clone = instance_reason.dupe();
    let f = move |cx: &mut Context<'_>, t: Type| -> Result<Type, JobError> {
        let concrete = flow_js_utils::flow_js_result_to_job_error(
            FlowJs::singleton_concrete_type_for_inspection(cx, &instance_reason_clone, &t),
        )?;
        match concrete.deref() {
            TypeInner::DefT(_, def_t)
                if let DefTInner::PolyT(deref!(PolyTData { tparams: ids, .. })) = def_t.deref()
                    && !ids.is_empty()
                    && ids.iter().all(|tp| tp.default.is_some()) =>
            {
                // Treat missing type args the same as empty type args (Foo = Foo<>) when all
                // parameters have defaults. Route through the flow engine so the PolyT +
                // ValueToTypeReferenceT handler applies. Otherwise, fall through to
                // EMissingTypeArgs.
                tvar_resolver::mk_tvar_and_fully_resolve_where(
                    cx,
                    instance_reason_clone.dupe(),
                    |cx, tout| {
                        let tvar = match tout.deref() {
                            TypeInner::OpenT(tvar) => tvar.dupe(),
                            _ => unreachable!("mk_where always creates OpenT"),
                        };
                        let use_t = UseT::new(UseTInner::ValueToTypeReferenceT(Box::new(
                            ValueToTypeReferenceTData {
                                use_op: unknown_use(),
                                reason: instance_reason_clone.dupe(),
                                kind: type_t_kind,
                                from_typeapp: false,
                                tout: Box::new(tvar),
                            },
                        )));
                        flow_js::flow_non_speculating(cx, (&concrete, &use_t))
                    },
                )
            }
            _ => {
                let t = flow_js_utils::flow_js_result_to_job_error(
                    value_to_type_reference_transform::run_on_concrete_type(
                        cx,
                        unknown_use(),
                        &instance_reason_clone,
                        type_t_kind,
                        concrete,
                    ),
                )?;
                Ok(tvar_resolver::resolved_t(
                    tvar_resolver::default_no_lowers,
                    true,
                    cx,
                    t,
                ))
            }
        }
    };
    Type::new(TypeInner::AnnotT(
        instance_reason.dupe(),
        map_on_resolved_type(cx, instance_reason, l, f),
        use_desc,
    ))
}
