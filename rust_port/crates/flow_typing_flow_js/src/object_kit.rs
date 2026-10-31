/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::borrow::Cow;
use std::ops::Deref;
use std::rc::Rc;
use std::sync::Arc;

use dupe::Dupe;
use dupe::IterDupedExt;
use flow_common::polarity::Polarity;
use flow_common::reason::Name;
use flow_common::reason::Reason;
use flow_common::reason::VirtualReasonDesc;
use flow_common::subst_name;
use flow_typing_context::Context;
use flow_typing_flow_common::flow_js_utils;
use flow_typing_flow_common::flow_js_utils::FlowJsException;
use flow_typing_flow_common::obj_type;
use flow_typing_flow_js_env::FlowJsEnv;
use flow_typing_type::type_::CondTData;
use flow_typing_type::type_::DefTInner;
use flow_typing_type::type_::DepthTrace;
use flow_typing_type::type_::DictType;
use flow_typing_type::type_::EvalMappedTypeData;
use flow_typing_type::type_::Flags;
use flow_typing_type::type_::MappedTypeFlags;
use flow_typing_type::type_::MappedTypeKeyCompatibilityData;
use flow_typing_type::type_::ObjKind;
use flow_typing_type::type_::Property;
use flow_typing_type::type_::Tvar;
use flow_typing_type::type_::Type;
use flow_typing_type::type_::TypeInner;
use flow_typing_type::type_::TypeStrictnessKind;
use flow_typing_type::type_::UnifyCause;
use flow_typing_type::type_::UseOp;
use flow_typing_type::type_::UseT;
use flow_typing_type::type_::UseTInner;
use flow_typing_type::type_::VirtualFrameUseOp;
use flow_typing_type::type_::VirtualRootUseOp;
use flow_typing_type::type_::VirtualUseOp;
use flow_typing_type::type_::mixed_t;
use flow_typing_type::type_::num_module_t;
use flow_typing_type::type_::object;
use flow_typing_type::type_::object::ObjectToolObjectMapData;
use flow_typing_type::type_::object::ObjectToolReactConfigData;
use flow_typing_type::type_::properties;
use flow_typing_type::type_::property;
use flow_typing_type::type_::root_of_use_op;
use flow_typing_type::type_::str_module_t;
use flow_typing_type::type_::symbol_t;
use flow_typing_type::type_::type_collector::TypeCollector;
use flow_typing_type::type_::type_or_type_desc::ErrorTypeDesc;
use flow_typing_type::type_::type_or_type_desc::TypeOrTypeDescT;
use flow_typing_type::type_::union_rep;
use flow_typing_type::type_::unknown_use;
use flow_typing_type::type_util;
use vec1::Vec1;

use crate::flow_js::FlowJs;
use crate::slice_utils;
use crate::tvar_resolver;

// We use this function to transform a resolved UnionT into a `(Vec<(Name, Reason)>, Vec<Type>)`,
// which represents a tuple of key names (with reasons for error messages) and an indexer to be
// used to create a new object. All SinlgetonStrTs/StrTs are turned into keys and all other
// non-empty types are turned into indexers. This is how we support non-homomorphic mapped types,
// like {[key in 'a' | 'b' | number]: string} and also how we support mapped types that use
// tparams with $Keys/keyof upper bounds:
// type Pick<O: {...}, Keys: $Keys<O>> = {[key in Keys]: O[key]}
//
// Note that we use the ConcretizeT constraint to flatten the union. A non-homomorphic mapped
// type *must not* operate over Unresolved tvar because additional keys received in the future
// will cause us to incorrectly output a union. While this approach of eagerly resolving the type
// may lead to edge cases where we miss keys, it seems like the best way forward. To get rid of
// these bugs in 100% of cases we'd need to make our EvalT machinery only operate over resolved
// types.
fn partition_keys_and_indexer<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    use_op: UseOp,
    reason: &Reason,
    keys: &Type,
) -> Result<(Vec<(Name, Reason)>, Vec<Type>), FlowJsException> {
    let key_upper_bound_reason = |desc| flow_common::reason::mk_reason(desc, reason.loc().dupe());
    let str_t = str_module_t::make(key_upper_bound_reason(VirtualReasonDesc::RString));
    let num_t = num_module_t::make(key_upper_bound_reason(VirtualReasonDesc::RNumber));
    let symbol_t = symbol_t::make(key_upper_bound_reason(VirtualReasonDesc::RSymbol));
    let union = Type::new(TypeInner::UnionT(
        reason.dupe(),
        union_rep::make(
            None,
            union_rep::UnionKind::UnknownKind,
            str_t,
            num_t,
            vec![symbol_t].into(),
        ),
    ));
    let mapped_type = match root_of_use_op(&use_op) {
        VirtualRootUseOp::EvalMappedType(box EvalMappedTypeData { mapped_type, .. }) => {
            mapped_type.clone()
        }
        _ => TypeOrTypeDescT::TypeDesc(ErrorTypeDesc::Unknown),
    };
    let compatibility_use_op = VirtualUseOp::Frame(
        Arc::new(VirtualFrameUseOp::MappedTypeKeyCompatibility(Box::new(
            MappedTypeKeyCompatibilityData {
                source_type_loc: type_util::loc_of_t(keys).dupe(),
                source_type: TypeOrTypeDescT::Type(keys.dupe()),
                mapped_type_loc: reason.loc.dupe(),
                mapped_type,
            },
        ))),
        Arc::new(use_op.dupe()),
    );
    // All keys must be a subtype of string | number | symbol
    FlowJs::rec_flow_t_with_env(cx, env, trace, compatibility_use_op, keys, &union)?;
    let possible_types =
        FlowJs::possible_concrete_types_for_inspection_with_env(cx, env, reason, keys)?;
    Ok(split_mapped_type_keys(possible_types))
}

/// Splits concrete mapped type keys into property names and indexer keys.
pub fn split_mapped_type_keys(
    keys: impl IntoIterator<Item = Type>,
) -> (Vec<(Name, Reason)>, Vec<Type>) {
    let mut keys_result: Vec<(Name, Reason)> = Vec::new();
    let mut indexers: Vec<Type> = Vec::new();
    for t in keys {
        match t.deref() {
            TypeInner::DefT(r, def_t)
                if let DefTInner::SingletonStrT { value: name, .. } = def_t.deref() =>
            {
                keys_result.push((Name::new(name.dupe()), r.dupe()));
                continue;
            }
            // A `unique symbol` mapped-type key is a distinct named destination
            // keyed by its nominal identity, not an indexer. Treating it as an
            // indexer would make `{[K in typeof s]: V}` a symbol-keyed dictionary
            // that matches every symbol, which e.g. makes `Omit<O, typeof s>` drop
            // every property. Mirrors `dispatch_substituted_name`.
            TypeInner::DefT(r, def_t) if let DefTInner::UniqueSymbolT(data) = def_t.deref() => {
                keys_result.push((Name::symbol(data.symbol.dupe()), r.dupe()));
                continue;
            }
            TypeInner::DefT(_, def_t) if let DefTInner::EmptyT = def_t.deref() => continue,
            _ => {}
        }

        indexers.push(t.dupe());
    }
    (keys_result, indexers)
}

fn dispatch_substituted_name<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    reason: &Reason,
    name_t: &Type,
) -> Result<(Vec<(Name, Reason)>, Vec<Type>), FlowJsException> {
    /* Returns (dest_names, indexer_keys) — splits the substituted name_type into
     * singleton-string destinations vs. types that contribute to the output indexer. */
    let possible =
        FlowJs::possible_concrete_types_for_inspection_with_env(cx, env, reason, name_t)?;
    let mut dests: Vec<(Name, Reason)> = Vec::new();
    let mut idx_keys: Vec<Type> = Vec::new();
    for t in possible {
        match t.deref() {
            TypeInner::DefT(_, def_t) if let DefTInner::EmptyT = def_t.deref() => {
                continue;
            }
            TypeInner::DefT(r, def_t)
                if let DefTInner::SingletonStrT { value, .. } = def_t.deref() =>
            {
                dests.push((Name::new(value.dupe()), r.dupe()));
            }
            TypeInner::DefT(r, def_t)
                if let DefTInner::SingletonNumT {
                    value: flow_typing_type::type_::NumberLiteral(_, str),
                    ..
                } = def_t.deref() =>
            {
                dests.push((Name::new(str.clone()), r.dupe()));
            }
            // A `unique symbol` destination is a distinct named property keyed by
            // its nominal identity, not an indexer. Routing it here keeps distinct
            // symbols apart instead of collapsing them into a single indexer.
            TypeInner::DefT(r, def_t) if let DefTInner::UniqueSymbolT(data) = def_t.deref() => {
                dests.push((Name::symbol(data.symbol.dupe()), r.dupe()));
            }
            _ => idx_keys.push(t.dupe()),
        }
    }
    Ok((dests, idx_keys))
}

#[allow(clippy::too_many_arguments)]
fn compute_name_remap<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    use_op: &UseOp,
    reason: &Reason,
    name_type: &Type,
    selected_keys: Option<&(Vec<(Name, Reason)>, Vec<Type>)>,
    slice: &object::Slice,
) -> Result<slice_utils::NameRemap, FlowJsException> {
    let object::Slice { props, flags, .. } = slice;
    let source_keys: Vec<Name> = match selected_keys {
        Some((named_keys, _)) => named_keys.iter().map(|(k, _)| k.clone()).collect(),
        None => props.keys().duped().collect(),
    };
    let key_t_for = |k: &Name| -> Type {
        let key_loc = match props.get(k) {
            Some(p) => property::first_loc(p).unwrap_or_else(|| reason.loc().dupe()),
            _ => reason.loc().dupe(),
        };
        // A symbol-typed key remaps as a `unique symbol` type, not a string literal.
        let Name::Str(key_str) = k else {
            let r = flow_common::reason::mk_reason(VirtualReasonDesc::RUniqueSymbol, key_loc);
            return flow_js_utils::type_of_key_name_with_env(env, k.dupe(), &r);
        };
        let key_str = key_str.dupe();
        Type::new(TypeInner::DefT(
            flow_common::reason::mk_reason(VirtualReasonDesc::RStringLit(key_str.dupe()), key_loc),
            flow_typing_type::type_::DefT::new(DefTInner::SingletonStrT {
                from_annot: true,
                value: key_str,
            }),
        ))
    };
    let substitute_name_for_t = |cx: &mut Context<'cx>, key_t: Type| -> Type {
        match name_type.deref() {
            TypeInner::DefT(_, def_t)
                if let flow_typing_type::type_::DefTInner::PolyT(
                    box flow_typing_type::type_::PolyTData { tparams, t_out, .. },
                ) = def_t.deref()
                    && tparams.len() == 1 =>
            {
                let kt = &tparams[0];
                let map = flow_data_structure_wrapper::ord_map::FlowOrdMap::from_iter([(
                    kt.name.dupe(),
                    key_t,
                )]);
                flow_typing_flow_common::type_subst::subst(
                    cx,
                    Some(use_op.dupe()),
                    false,
                    false,
                    flow_typing_flow_common::type_subst::Purpose::Normal,
                    &map,
                    t_out.dupe(),
                )
            }
            _ => type_util::typeapp_with_use_op(
                false,
                false,
                type_util::reason_of_t(name_type).dupe(),
                use_op.dupe(),
                name_type.dupe(),
                vec![key_t],
            ),
        }
    };
    let mut per_source: std::collections::BTreeMap<Name, slice_utils::PerSourceDispatch> =
        std::collections::BTreeMap::new();
    let mut all_substituted: Vec<Type> = Vec::new();
    for k in &source_keys {
        let substituted = substitute_name_for_t(cx, key_t_for(k));
        let (dests, idx_keys) = dispatch_substituted_name(cx, env, reason, &substituted)?;
        per_source.insert(
            k.dupe(),
            slice_utils::PerSourceDispatch {
                dest_names: dests,
                indexer_keys: idx_keys,
            },
        );
        all_substituted.push(substituted);
    }
    let (source_indexer, indexer_substituted_opt) = match (selected_keys, &flags.obj_kind) {
        (None, ObjKind::Indexed(dict_t)) => {
            let substituted = substitute_name_for_t(cx, dict_t.key.dupe());
            let possible = FlowJs::possible_concrete_types_for_inspection_with_env(
                cx,
                env,
                reason,
                &substituted,
            )?;
            let all_empty = possible.iter().all(|t| match t.deref() {
                TypeInner::DefT(_, def_t) => matches!(def_t.deref(), DefTInner::EmptyT),
                _ => false,
            });
            if all_empty {
                (
                    Some(slice_utils::SourceIndexerRemap::DropSourceIndexer),
                    Some(substituted),
                )
            } else {
                (
                    Some(slice_utils::SourceIndexerRemap::KeepSourceIndexerKey(
                        substituted.dupe(),
                    )),
                    Some(substituted),
                )
            }
        }
        _ => (None, None),
    };
    /* For semi-homomorphic mapped types, the selected_keys.xs entries are non-literal indexer
     * sources (e.g. `string` from `K = 'a' | string`). Substitute name_type over each xs entry to
     * produce destination indexer keys. Filter out entries that substitute to empty. */
    let (selected_xs_keys, xs_substituted_for_check): (Vec<Type>, Vec<Type>) = match selected_keys {
        Some((_, xs)) => {
            let mut keys: Vec<Type> = Vec::new();
            let mut all: Vec<Type> = Vec::new();
            for xs_t in xs {
                let substituted = substitute_name_for_t(cx, xs_t.dupe());
                let possible = FlowJs::possible_concrete_types_for_inspection_with_env(
                    cx,
                    env,
                    reason,
                    &substituted,
                )?;
                let all_empty = possible.iter().all(|t| match t.deref() {
                    TypeInner::DefT(_, def_t) => matches!(def_t.deref(), DefTInner::EmptyT),
                    _ => false,
                });
                if all_empty {
                    all.push(substituted);
                } else {
                    keys.push(substituted.dupe());
                    all.push(substituted);
                }
            }
            (keys, all)
        }
        None => (vec![], vec![]),
    };
    let all_substituted = {
        let mut acc = all_substituted;
        if let Some(t) = indexer_substituted_opt {
            acc.push(t);
        }
        let mut combined = xs_substituted_for_check;
        combined.append(&mut acc);
        combined
    };
    match all_substituted.as_slice() {
        [] => {}
        [t] => {
            partition_keys_and_indexer(cx, env, trace, use_op.dupe(), reason, t)?;
        }
        [t0, t1, ts @ ..] => {
            let union = Type::new(TypeInner::UnionT(
                reason.dupe(),
                union_rep::make(
                    None,
                    union_rep::UnionKind::UnknownKind,
                    t0.dupe(),
                    t1.dupe(),
                    ts.iter().duped().collect::<Rc<[_]>>(),
                ),
            ));
            partition_keys_and_indexer(cx, env, trace, use_op.dupe(), reason, &union)?;
        }
    }
    Ok(slice_utils::NameRemap {
        per_source,
        source_indexer,
        selected_xs_keys,
    })
}

/// The object that a mapped type over a union of keys maps: one property per
/// key and one indexer per non-literal key type.
pub fn slice_of_keys(
    reason: &Reason,
    keys_with_reasons: &[(Name, Reason)],
    indexers: &[Type],
) -> object::Slice {
    // To go from a union to a MappedType we first build an object with all the keys we
    // extract from the union and create an object with all mixed values. Then we push it
    // through the mapped type machinery. The specific type we choose for the properties
    // does not matter because the mapped type code does not inspect the value types
    let mixed = mixed_t::why(reason.dupe());
    let mixed_prop_t = |key_reason: &Reason| -> Property {
        slice_utils::mk_slice_prop(
            Some(key_reason.loc().dupe()),
            mixed.dupe(),
            Polarity::Neutral,
            false,
        )
    };
    let props: properties::PropertiesMap = keys_with_reasons
        .iter()
        .map(|(key, key_reason)| (key.dupe(), mixed_prop_t(key_reason)))
        .collect();
    let generics = flow_typing_generics::spread_empty();
    let obj_kind = match indexers {
        [] => ObjKind::Exact,
        [t1, ts @ ..] => {
            let key_t = match ts {
                [] => t1.dupe(),
                [t2, rest @ ..] => Type::new(TypeInner::UnionT(
                    reason.dupe(),
                    union_rep::make(
                        None,
                        union_rep::UnionKind::UnknownKind,
                        t1.dupe(),
                        t2.dupe(),
                        rest.iter().duped().collect::<Rc<[_]>>(),
                    ),
                )),
            };
            ObjKind::Indexed(DictType {
                dict_name: None,
                key: key_t,
                value: mixed.dupe(),
                dict_polarity: Polarity::Neutral,
            })
        }
    };
    let flags = Flags {
        obj_kind,
        react_dro: None,
    };
    let interface = None;
    let obj_reason = reason.dupe().replace_desc(VirtualReasonDesc::RObjectType);
    object::Slice {
        reason: obj_reason,
        strictness_kind: TypeStrictnessKind::Flow,
        props,
        flags,
        frozen: false,
        generics,
        interface,
        reachable_targs: Rc::from([]),
        literal: false,
        exact_object_type: false,
    }
}

pub fn mapped_type_of_keys<'cx>(
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    trace: DepthTrace,
    use_op: UseOp,
    reason: &Reason,
    keys: &Type,
    property_type: &Type,
    name_type: Option<&Type>,
    mapped_type_flags: &MappedTypeFlags,
) -> Result<Type, FlowJsException> {
    let (keys_with_reasons, indexers) =
        partition_keys_and_indexer(cx, env, trace, use_op.dupe(), reason, keys)?;
    let slice = slice_of_keys(reason, &keys_with_reasons, &indexers);
    let filter_optional = |cx: &mut Context<'cx>, t: Type| -> Result<Type, FlowJsException> {
        let filter_id = crate::flow_js::filter_optional_with_env(cx, env, reason, &t)?;
        Ok(Type::new(TypeInner::OpenT(Tvar::new(
            reason.dupe(),
            filter_id,
        ))))
    };
    let name_remap = match name_type {
        None => None,
        Some(nt) => Some(compute_name_remap(
            cx, env, trace, &use_op, reason, nt, None, &slice,
        )?),
    };
    slice_utils::map_object(
        &filter_optional,
        name_remap.as_ref(),
        property_type,
        mapped_type_flags,
        cx,
        env,
        reason,
        &use_op,
        None,
        &slice,
    )
}

pub fn run<'cx>(
    trace: DepthTrace,
    cx: &mut Context<'cx>,
    use_op: UseOp,
    reason: &Reason,
    resolve_tool: &object::ResolveTool,
    tool: &object::Tool,
    l: &Type,
    tout: &Type,
) -> Result<(), FlowJsException> {
    run_with_env(
        trace,
        cx,
        &FlowJsEnv::entry(),
        use_op,
        reason,
        resolve_tool,
        tool,
        l,
        tout,
    )
}

pub(super) fn run_with_env<'cx>(
    trace: DepthTrace,
    cx: &mut Context<'cx>,
    env: &FlowJsEnv,
    use_op: UseOp,
    reason: &Reason,
    resolve_tool: &object::ResolveTool,
    tool: &object::Tool,
    l: &Type,
    tout: &Type,
) -> Result<(), FlowJsException> {
    let object_spread = |options: &object::spread::Target,
                         state: &object::spread::State,
                         cx: &mut Context<'cx>,
                         use_op: UseOp,
                         reason: &Reason,
                         x: Vec1<object::Slice>,
                         tout: &Type| {
        let dict_check = |cx: &mut Context<'cx>,
                          use_op: UseOp,
                          d1: &DictType,
                          d2: &DictType|
         -> Result<(), FlowJsException> {
            FlowJs::rec_flow_t_with_env(cx, env, trace, use_op.dupe(), &d2.key, &d1.key)?;
            FlowJs::rec_flow_t_with_env(cx, env, trace, use_op, &d2.value, &d1.value)?;
            Ok(())
        };
        let return_ = |cx: &mut Context<'cx>, use_op: UseOp, t: Type| {
            FlowJs::rec_flow_t_with_env(cx, env, trace, use_op, &t, tout)?;
            Ok(())
        };
        let recurse = |cx: &mut Context<'cx>,
                       env: &FlowJsEnv,
                       use_op: UseOp,
                       reason: &Reason,
                       resolve_tool: object::ResolveTool,
                       tool: object::Tool,
                       t: Type|
         -> Result<(), FlowJsException> {
            FlowJs::rec_flow_with_env(
                cx,
                env,
                trace,
                &t,
                &UseT::new(UseTInner::ObjKitT(
                    use_op,
                    reason.dupe(),
                    Box::new(resolve_tool),
                    Box::new(tool),
                    tout.dupe(),
                )),
            )?;
            Ok(())
        };
        slice_utils::object_spread_with_env(
            &dict_check,
            &flow_js_utils::add_output_with_env,
            &return_,
            &recurse,
            options,
            state.clone(),
            cx,
            env,
            use_op,
            reason,
            x,
        )
    };

    // Check component config
    let check_component_config = |pmap: &properties::PropertiesMap,
                                  cx: &mut Context<'cx>,
                                  use_op: UseOp,
                                  reason: &Reason,
                                  x: Vec1<object::Slice>,
                                  tout: &Type|
     -> Result<(), FlowJsException> {
        let return_ = |cx: &mut Context<'cx>, use_op: UseOp, t: Type| {
            FlowJs::rec_flow_t_with_env(cx, env, trace, use_op, &t, tout)?;
            Ok(())
        };
        slice_utils::check_component_config(
            &|cx, env: &FlowJsEnv, msg| flow_js_utils::add_output_with_env(cx, env, msg),
            &return_,
            pmap,
            cx,
            env,
            use_op,
            reason,
            x,
        )
    };

    // Object Rest
    let object_rest = |options: &object::rest::MergeMode,
                       state: &object::rest::State,
                       cx: &mut Context<'cx>,
                       use_op: UseOp,
                       reason: &Reason,
                       x: Vec1<object::Slice>,
                       tout: &Type| {
        let return_ = |cx: &mut Context<'cx>,
                       use_op: Box<dyn Fn(Polarity) -> UseOp>,
                       options: object::rest::MergeMode,
                       t: Type| {
            match options {
                object::rest::MergeMode::ReactConfigMerge(Polarity::Neutral) => {
                    FlowJs::rec_unify_with_env(
                        cx,
                        env,
                        trace,
                        use_op(Polarity::Neutral),
                        UnifyCause::Uncategorized,
                        None,
                        &t,
                        tout,
                    )?;
                }
                object::rest::MergeMode::ReactConfigMerge(Polarity::Negative) => {
                    FlowJs::rec_flow_t_with_env(
                        cx,
                        env,
                        trace,
                        use_op(Polarity::Negative),
                        tout,
                        &t,
                    )?;
                }
                object::rest::MergeMode::ReactConfigMerge(Polarity::Positive) => {
                    FlowJs::rec_flow_t_with_env(
                        cx,
                        env,
                        trace,
                        use_op(Polarity::Positive),
                        &t,
                        tout,
                    )?;
                }
                _ => {
                    // Intentional UnknownUse here.
                    FlowJs::rec_flow_t_with_env(cx, env, trace, unknown_use(), &t, tout)?;
                }
            }
            Ok(())
        };
        let recurse = |cx: &mut Context<'cx>,
                       env: &FlowJsEnv,
                       use_op: UseOp,
                       reason: &Reason,
                       resolve_tool: object::ResolveTool,
                       tool: object::Tool,
                       t: Type| {
            FlowJs::rec_flow_with_env(
                cx,
                env,
                trace,
                &t,
                &UseT::new(UseTInner::ObjKitT(
                    use_op,
                    reason.dupe(),
                    Box::new(resolve_tool),
                    Box::new(tool),
                    tout.dupe(),
                )),
            )?;
            Ok(())
        };
        let subt_check = |use_op: UseOp,
                          cx: &mut Context<'cx>,
                          (t1, t2): (&Type, &Type)|
         -> Result<(), FlowJsException> {
            FlowJs::rec_flow_t_with_env(cx, env, trace, use_op, t1, t2)?;
            Ok(())
        };
        slice_utils::object_rest_with_env(
            &|cx, env: &FlowJsEnv, msg| flow_js_utils::add_output_with_env(cx, env, msg),
            &return_,
            &recurse,
            &subt_check,
            options,
            state,
            cx,
            env,
            use_op,
            reason,
            x,
        )
    };

    // Object Make Exact
    let object_make_exact = |cx: &mut Context<'cx>,
                             _use_op: UseOp,
                             reason: &Reason,
                             x: Vec1<object::Slice>,
                             tout: &Type|
     -> Result<(), FlowJsException> {
        // We always use an unknown_use intentionally when flowing to the tout. The use_op associated
        // with the tvar is more relevant with the use of the $Exact type than the use_op associated
        // with the $Exact instantiation
        let t = slice_utils::object_make_exact(cx, env, reason, x)?;
        FlowJs::rec_flow_t_with_env(cx, env, trace, unknown_use(), &t, tout)?;
        Ok(())
    };

    let object_read_only = |cx: &mut Context<'cx>,
                            _use_op: UseOp,
                            reason: &Reason,
                            x: Vec1<object::Slice>,
                            tout: &Type|
     -> Result<(), FlowJsException> {
        // We always use an unknown_use intentionally when flowing to the tout. The use_op associated
        // with the tvar is more relevant with the use of the ReadOnly type than the use_op associated
        // with the ReadOnly instantiation
        let read_only = slice_utils::object_read_only(cx, reason, x);
        FlowJs::rec_flow_t_with_env(cx, env, trace, unknown_use(), &read_only, tout)?;
        Ok(())
    };

    // Object Partial
    let object_partial = |cx: &mut Context<'cx>,
                          _use_op: UseOp,
                          reason: &Reason,
                          x: Vec1<object::Slice>,
                          tout: &Type|
     -> Result<(), FlowJsException> {
        // We always use an unknown_use intentionally when flowing to the tout. The use_op associated
        // with the tvar is more relevant with the use of the Partial type than the use_op associated
        // with the Partial instantiation
        let partial = slice_utils::object_update_optionality(
            slice_utils::ObjectUpdateOptionalityKind::Partial,
            cx,
            reason,
            x,
        );
        FlowJs::rec_flow_t_with_env(cx, env, trace, unknown_use(), &partial, tout)?;
        Ok(())
    };

    // Object Required
    let object_required = |cx: &mut Context<'cx>,
                           _use_op: UseOp,
                           reason: &Reason,
                           x: Vec1<object::Slice>,
                           tout: &Type|
     -> Result<(), FlowJsException> {
        // We always use an unknown_use intentionally when flowing to the tout. The use_op associated
        // with the tvar is more relevant with the use of the Required type than the use_op associated
        // with the Required instantiation
        let required = slice_utils::object_update_optionality(
            slice_utils::ObjectUpdateOptionalityKind::Required,
            cx,
            reason,
            x,
        );
        FlowJs::rec_flow_t_with_env(cx, env, trace, unknown_use(), &required, tout)?;
        Ok(())
    };

    // Object Rep
    let object_rep = |cx: &mut Context<'cx>,
                      use_op: UseOp,
                      reason: &Reason,
                      x: Vec1<object::Slice>,
                      tout: &Type|
     -> Result<(), FlowJsException> {
        let mk_object = |slice: &object::Slice| -> Type {
            let object::Slice {
                reason: r,
                strictness_kind,
                props,
                flags,
                frozen: _,
                generics,
                interface: _,
                reachable_targs,
                literal,
                exact_object_type: _,
            } = slice;
            // TODO(jmbrown): Add polarity information to props
            let polarity = Polarity::Neutral;
            let pmap = props.ident_map(|prop| property::with_polarity(prop, polarity));
            let new_flags = Flags {
                obj_kind: obj_type::map_dict(
                    |mut dict| {
                        dict.dict_polarity = polarity;
                        dict
                    },
                    flags.obj_kind.clone(),
                ),
                react_dro: flags.react_dro.clone(),
            };
            let call = None;
            let id = cx.generate_property_map(pmap);
            let proto = Type::new(TypeInner::ObjProtoT(reason.dupe()));
            slice_utils::mk_object_type(
                cx,
                r,
                false,
                true,
                None,
                reachable_targs,
                subst_name::OpKind::CreateElement,
                &new_flags,
                call,
                id,
                proto,
                generics.clone(),
                *strictness_kind,
                *literal,
            )
        };
        let ts = x.mapped_ref(mk_object);
        let (t0, rest) = ts.split_off_first();
        let t = if rest.is_empty() {
            t0
        } else {
            let mut iter = rest.into_iter();
            let t1 = iter.next().expect("non-empty object representation tail");
            let remaining: Rc<[Type]> = iter.collect();
            Type::new(TypeInner::UnionT(
                reason.dupe(),
                union_rep::make(None, union_rep::UnionKind::UnknownKind, t0, t1, remaining),
            ))
        };
        FlowJs::rec_flow_t_with_env(cx, env, trace, use_op, &t, tout)?;
        Ok(())
    };

    let react_config = |ref_manipulation: &object::react_config::RefManipulation,
                        state: &object::react_config::State,
                        literal: bool,
                        cx: &mut Context<'cx>,
                        use_op: UseOp,
                        reason: &Reason,
                        x: Vec1<object::Slice>,
                        tout: &Type|
     -> Result<(), FlowJsException> {
        // All props currently have a neutral polarity. However, they should have a
        // positive polarity (or even better, constant) since React.createElement()
        // freezes the type of props. We use a neutral polarity today because the
        // props type we flow the config into is written by users who very rarely
        // add a positive variance annotation. We may consider marking that type as
        // constant in the future as well.
        let prop_polarity = Polarity::Neutral;

        let finish = |cx: &mut Context<'cx>,
                      config: &object::Slice,
                      defaults: Option<&object::Slice>|
         -> Result<Type, FlowJsException> {
            let object::Slice {
                reason: config_reason,
                strictness_kind: config_strictness_kind,
                props: config_props,
                flags: config_flags,
                frozen: _,
                generics: config_generics,
                interface: _,
                reachable_targs: config_targs,
                literal: _,
                exact_object_type: _,
            } = config;
            let config_dict = obj_type::get_dict_opt(&config_flags.obj_kind);
            let key_name = Name::new("key");
            let ref_name = Name::new("ref");
            let config_prop = |key: &Name| -> Option<Cow<'_, Property>> {
                if key == &key_name {
                    None
                } else {
                    match ref_manipulation {
                        object::react_config::RefManipulation::AddRef(prop_t)
                            if key == &ref_name =>
                        {
                            Some(Cow::Owned(slice_utils::mk_slice_prop(
                                None,
                                prop_t.dupe(),
                                prop_polarity,
                                false,
                            )))
                        }
                        _ => {
                            slice_utils::get_prop(config_reason, config_props.get(key), config_dict)
                        }
                    }
                }
            };
            // Create the final props map and dict.
            //
            // NOTE: React will copy any enumerable prop whether or not it
            // is own to the config.
            let (props_map, flags, generics, reachable_targs): (
                properties::PropertiesMap,
                Flags,
                object::GenericSpreadId,
                Rc<[(Type, Polarity)]>,
            ) = match defaults {
                // If we have some default props then we want to add the types for those
                // default props to our final props object.
                Some(defaults_slice) => {
                    let object::Slice {
                        reason: defaults_reason,
                        strictness_kind: _,
                        props: defaults_props,
                        flags: defaults_flags,
                        frozen: _,
                        generics: defaults_generics,
                        interface: _,
                        reachable_targs: defaults_targs,
                        literal: _,
                        exact_object_type: _,
                    } = defaults_slice;
                    let defaults_dict = obj_type::get_dict_opt(&defaults_flags.obj_kind);
                    //  Merge our props and default props.
                    let mut merged_props = defaults_props
                        .ident_map(|prop| property::with_polarity(prop, prop_polarity));
                    let mut merge_key = |key: &Name| -> Result<(), FlowJsException> {
                        let p1 = config_prop(key);
                        let p2 = slice_utils::get_prop(
                            defaults_reason,
                            defaults_props.get(key),
                            defaults_dict,
                        );
                        let result = match (p1.as_deref(), p2.as_deref()) {
                            (None, None) => None,
                            (Some(p), None) => Some(property::with_polarity(p, prop_polarity)),
                            (None, Some(p)) => Some(property::with_polarity(p, prop_polarity)),
                            // If a property is defined in both objects, and the first property's
                            // type includes void then we want to replace every occurrence of void
                            // with the second property's type. This is consistent with the behavior
                            // of function default arguments. If you call a function, `f`, like:
                            // `f(undefined)` and there is a default value for the first argument,
                            // then we will ignore the void type and use the type for the default
                            // parameter instead.
                            (Some(p1_prop), Some(p2_prop)) => {
                                let t1 = property::type_(p1_prop);
                                let t2 = property::type_(p2_prop);
                                let is_method =
                                    property::is_method(p1_prop) || property::is_method(p2_prop);
                                let l = property::first_loc(p1_prop);
                                let collector = TypeCollector::create();
                                let filter_id = FlowJs::filter_optional_with_env(
                                    cx,
                                    env,
                                    Some(trace),
                                    reason,
                                    t1,
                                )?;
                                let open_t = Type::new(TypeInner::OpenT(Tvar::new(
                                    reason.dupe(),
                                    filter_id,
                                )));
                                FlowJs::rec_flow_with_env(
                                    cx,
                                    env,
                                    trace,
                                    &open_t,
                                    &UseT::new(UseTInner::CondT(Box::new(CondTData {
                                        loc: reason.loc().dupe(),
                                        opt_type: None,
                                        true_t: t2.dupe(),
                                        collector: collector.dupe(),
                                    }))),
                                )?;
                                let t = collector
                                    .union_opt(reason.dupe())
                                    .unwrap_or_else(|| tvar_resolver::default_no_lowers(reason));
                                Some(slice_utils::mk_slice_prop(l, t, prop_polarity, is_method))
                            }
                        };
                        if let Some(prop) = result {
                            merged_props.insert(key.dupe(), prop);
                        }
                        Ok(())
                    };
                    for key in config_props.keys() {
                        if key != &key_name {
                            merge_key(key)?;
                        }
                    }
                    if matches!(
                        ref_manipulation,
                        object::react_config::RefManipulation::AddRef(_)
                    ) {
                        merge_key(&ref_name)?;
                    }
                    // Merge the dictionary from our config with the defaults dictionary.
                    let dict: Option<DictType> = match (config_dict, defaults_dict) {
                        (Some(d1), Some(d2)) => Some(DictType {
                            dict_name: None,
                            key: Type::new(TypeInner::UnionT(
                                reason.dupe(),
                                union_rep::make(
                                    None,
                                    union_rep::UnionKind::UnknownKind,
                                    d1.key.dupe(),
                                    d2.key.dupe(),
                                    Rc::from([]),
                                ),
                            )),
                            value: Type::new(TypeInner::UnionT(
                                reason.dupe(),
                                union_rep::make(
                                    None,
                                    union_rep::UnionKind::UnknownKind,
                                    slice_utils::read_dict(config_reason, d1),
                                    slice_utils::read_dict(defaults_reason, d2),
                                    Rc::from([]),
                                ),
                            )),
                            dict_polarity: prop_polarity,
                        }),
                        (Some(d), None) | (None, Some(d)) => Some(d.clone()),
                        (None, None) => None,
                    };
                    // React freezes the config so we set the frozen flag to true. The
                    // final object is only exact if both the config and defaults objects
                    // are exact.
                    let obj_kind = match dict {
                        Some(d) => ObjKind::Indexed(d),
                        None => {
                            if obj_type::is_exact(&config_flags.obj_kind)
                                && obj_type::is_exact(&defaults_flags.obj_kind)
                            {
                                ObjKind::Exact
                            } else {
                                ObjKind::Inexact
                            }
                        }
                    };
                    let flags = Flags {
                        obj_kind,
                        react_dro: None,
                    };
                    let generics =
                        flow_typing_generics::spread_append(config_generics, defaults_generics);
                    let mut targs: Vec<(Type, Polarity)> = config_targs.to_vec();
                    targs.extend(defaults_targs.iter().cloned());
                    (merged_props, flags, generics, targs.into())
                }
                // Otherwise turn our slice props map into an object props.
                None => {
                    let mut props_map: properties::PropertiesMap = config_props
                        .keys()
                        .filter_map(|name| {
                            config_prop(name).map(|prop| {
                                (
                                    name.dupe(),
                                    property::with_polarity(prop.as_ref(), prop_polarity),
                                )
                            })
                        })
                        .collect();
                    if let object::react_config::RefManipulation::AddRef(prop_t) = ref_manipulation
                    {
                        if !props_map.contains_key(&ref_name) {
                            props_map.insert(
                                ref_name.dupe(),
                                slice_utils::mk_slice_prop(
                                    None,
                                    prop_t.dupe(),
                                    prop_polarity,
                                    false,
                                ),
                            );
                        }
                    }
                    // Create a new dictionary from our config's dictionary with a
                    // positive polarity.
                    let dict = config_dict.map(|d| DictType {
                        dict_name: None,
                        key: d.key.dupe(),
                        value: d.value.dupe(),
                        dict_polarity: prop_polarity,
                    });
                    let obj_kind = obj_type::obj_kind_from_optional_dict(
                        dict,
                        if obj_type::is_exact(&config_flags.obj_kind) {
                            ObjKind::Exact
                        } else {
                            ObjKind::Inexact
                        },
                    );
                    let flags = Flags {
                        obj_kind,
                        react_dro: None,
                    };
                    (
                        props_map,
                        flags,
                        config_generics.clone(),
                        config_targs.dupe(),
                    )
                }
            };
            let call = None;
            let strictness_kind = defaults
                .map(|defaults| config_strictness_kind.join(defaults.strictness_kind))
                .unwrap_or(*config_strictness_kind);
            // Finish creating our props object.
            let id = cx.generate_property_map(props_map);
            let proto = Type::new(TypeInner::ObjProtoT(reason.dupe()));
            Ok(slice_utils::mk_object_type(
                cx,
                reason,
                false,
                false,
                None,
                &reachable_targs,
                subst_name::OpKind::ReactConfig,
                &flags,
                call,
                id,
                proto,
                generics,
                strictness_kind,
                literal,
            ))
        };

        match state {
            // If we have some type for default props then we need to wait for that
            // type to resolve before finishing our props type.
            object::react_config::State::Config {
                component_default_props: Some(t),
            } => {
                let resolve_tool = object::ResolveTool::Resolve(object::Resolve::Next);
                let new_state = object::react_config::State::Defaults {
                    config: object::Resolved(x),
                };
                FlowJs::rec_flow_with_env(
                    cx,
                    env,
                    trace,
                    t,
                    &UseT::new(UseTInner::ObjKitT(
                        use_op,
                        reason.dupe(),
                        Box::new(resolve_tool),
                        Box::new(object::Tool::ReactConfig(Box::new(
                            ObjectToolReactConfigData {
                                state: new_state,
                                ref_manipulation: ref_manipulation.clone(),
                                literal,
                            },
                        ))),
                        tout.dupe(),
                    )),
                )?;
                Ok(())
            }
            // If we have no default props then finish our object and flow it to our tout type.
            object::react_config::State::Config {
                component_default_props: None,
            } => {
                let mut ts = Vec::new();
                for c in x.iter() {
                    ts.push(finish(cx, c, None)?);
                }
                let mut iter = ts.into_iter();
                let t0 = iter.next().unwrap();
                let t = if let Some(t1) = iter.next() {
                    let remaining: Rc<[Type]> = iter.collect();
                    Type::new(TypeInner::UnionT(
                        reason.dupe(),
                        union_rep::make(None, union_rep::UnionKind::UnknownKind, t0, t1, remaining),
                    ))
                } else {
                    t0
                };
                FlowJs::rec_flow_with_env(
                    cx,
                    env,
                    trace,
                    &t,
                    &UseT::new(UseTInner::UseT(use_op, tout.dupe())),
                )?;
                Ok(())
            }
            // If we had default props and those defaults resolved then finish our
            // props object with those default props.
            object::react_config::State::Defaults { config } => {
                let mut all_results: Vec<Type> = Vec::new();
                for c in config.0.iter() {
                    for d in x.iter() {
                        all_results.push(finish(cx, c, Some(d))?);
                    }
                }
                let t = if all_results.len() == 1 {
                    all_results.into_iter().next().unwrap()
                } else {
                    let mut iter = all_results.into_iter();
                    let t0 = iter.next().unwrap();
                    let t1 = iter.next().unwrap();
                    let remaining: Rc<[Type]> = iter.collect();
                    Type::new(TypeInner::UnionT(
                        reason.dupe(),
                        union_rep::make(None, union_rep::UnionKind::UnknownKind, t0, t1, remaining),
                    ))
                };
                FlowJs::rec_flow_with_env(
                    cx,
                    env,
                    trace,
                    &t,
                    &UseT::new(UseTInner::UseT(use_op, tout.dupe())),
                )?;
                Ok(())
            }
        }
    };

    // Object Map
    let object_map = |prop_type: &Type,
                      name_type: &Option<Type>,
                      mapped_type_flags: &MappedTypeFlags,
                      selected_keys_opt: &Option<Type>,
                      cx: &mut Context<'cx>,
                      use_op: UseOp,
                      reason: &Reason,
                      x: Vec1<object::Slice>,
                      tout: &Type|
     -> Result<(), FlowJsException> {
        let selected_keys = selected_keys_opt
            .as_ref()
            .map(|keys| partition_keys_and_indexer(cx, env, trace, use_op.dupe(), reason, keys))
            .transpose()?;
        let filter_optional = |cx: &mut Context<'cx>, t: Type| -> Result<Type, FlowJsException> {
            let filter_id = crate::flow_js::filter_optional_with_env(cx, env, reason, &t)?;
            Ok(Type::new(TypeInner::OpenT(Tvar::new(
                reason.dupe(),
                filter_id,
            ))))
        };
        let map_one =
            |cx: &mut Context<'cx>, slice: &object::Slice| -> Result<Type, FlowJsException> {
                let name_remap = match name_type {
                    None => None,
                    Some(nt) => Some(compute_name_remap(
                        cx,
                        env,
                        trace,
                        &use_op,
                        reason,
                        nt,
                        selected_keys.as_ref(),
                        slice,
                    )?),
                };
                slice_utils::map_object(
                    &filter_optional,
                    name_remap.as_ref(),
                    prop_type,
                    mapped_type_flags,
                    cx,
                    env,
                    reason,
                    &use_op,
                    selected_keys.as_ref(),
                    slice,
                )
            };
        let mut slices = x.iter();
        let t0 = map_one(cx, slices.next().expect("Vec1 has at least one element"))?;
        let t = match slices.next() {
            None => t0,
            Some(slice) => {
                let t1 = map_one(cx, slice)?;
                let remaining: Rc<[Type]> = slices
                    .map(|slice| map_one(cx, slice))
                    .collect::<Result<Vec<_>, _>>()?
                    .into();
                Type::new(TypeInner::UnionT(
                    reason.dupe(),
                    union_rep::make(None, union_rep::UnionKind::UnknownKind, t0, t1, remaining),
                ))
            }
        };
        // Intentional UnknownUse here.
        FlowJs::rec_flow_t_with_env(cx, env, trace, unknown_use(), &t, tout)?;
        Ok(())
    };

    // Object Resolution
    let next = |cx: &mut Context<'cx>,
                use_op: UseOp,
                tool: &object::Tool,
                reason: &Reason,
                x: Vec1<object::Slice>|
     -> Result<(), FlowJsException> {
        match tool {
            object::Tool::MakeExact => object_make_exact(cx, use_op, reason, x, tout),
            object::Tool::Spread(box (options, state)) => {
                object_spread(options, state, cx, use_op, reason, x, tout)?;
                Ok(())
            }
            object::Tool::Rest(box (options, state)) => {
                object_rest(options, state, cx, use_op, reason, x, tout)?;
                Ok(())
            }
            object::Tool::ReactConfig(box ObjectToolReactConfigData {
                state,
                ref_manipulation,
                literal,
            }) => react_config(
                ref_manipulation,
                state,
                *literal,
                cx,
                use_op,
                reason,
                x,
                tout,
            ),
            object::Tool::ReadOnly => object_read_only(cx, use_op, reason, x, tout),
            object::Tool::Partial => object_partial(cx, use_op, reason, x, tout),
            object::Tool::Required => object_required(cx, use_op, reason, x, tout),
            object::Tool::ObjectRep => object_rep(cx, use_op, reason, x, tout),
            object::Tool::ReactCheckComponentConfig { props: pmap } => {
                check_component_config(pmap, cx, use_op, reason, x, tout)
            }
            object::Tool::ObjectMap(box ObjectToolObjectMapData {
                prop_type,
                name_type,
                mapped_type_flags,
                selected_keys_opt,
            }) => object_map(
                prop_type,
                name_type,
                mapped_type_flags,
                selected_keys_opt,
                cx,
                use_op,
                reason,
                x,
                tout,
            ),
        }
    };

    // Resolution returns directly (e.g. for an `any` input) without running the tool, so the
    // result must reach `tout` in the same direction the tool's own return would use.
    let return_ = |cx: &mut Context<'cx>, use_op: UseOp, t: Type| -> Result<(), FlowJsException> {
        match tool {
            object::Tool::Rest(box (
                object::rest::MergeMode::ReactConfigMerge(Polarity::Neutral),
                _,
            )) => {
                FlowJs::rec_unify_with_env(
                    cx,
                    env,
                    trace,
                    use_op,
                    UnifyCause::Uncategorized,
                    None,
                    &t,
                    tout,
                )?;
            }
            object::Tool::Rest(box (
                object::rest::MergeMode::ReactConfigMerge(Polarity::Negative),
                _,
            )) => {
                FlowJs::rec_flow_t_with_env(cx, env, trace, use_op, tout, &t)?;
            }
            _ => {
                FlowJs::rec_flow_t_with_env(cx, env, trace, use_op, &t, tout)?;
            }
        }
        Ok(())
    };
    let recurse = |cx: &mut Context<'cx>,
                   env: &FlowJsEnv,
                   use_op: UseOp,
                   reason: &Reason,
                   resolve_tool: object::ResolveTool,
                   tool: &object::Tool,
                   t: Type|
     -> Result<(), FlowJsException> {
        FlowJs::rec_flow_with_env(
            cx,
            env,
            trace,
            &t,
            &UseT::new(UseTInner::ObjKitT(
                use_op,
                reason.dupe(),
                Box::new(resolve_tool),
                Box::new(tool.clone()),
                tout.dupe(),
            )),
        )?;
        Ok(())
    };
    let statics = |cx: &mut Context<'cx>,
                   env: &FlowJsEnv,
                   r: &Reason,
                   i: &Type|
     -> Result<Type, FlowJsException> {
        let collector = TypeCollector::create();
        FlowJs::rec_flow_with_env(
            cx,
            env,
            trace,
            i,
            &UseT::new(UseTInner::GetStaticsT(r.dupe(), collector.dupe())),
        )?;
        Ok(collector
            .union_opt(r.dupe())
            .unwrap_or_else(|| tvar_resolver::default_no_lowers(r)))
    };
    slice_utils::run_with_env(
        &flow_js_utils::add_output_with_env,
        &return_,
        &next,
        &recurse,
        &statics,
        cx,
        env,
        use_op,
        reason,
        resolve_tool.clone(),
        tool,
        l,
    )
}
