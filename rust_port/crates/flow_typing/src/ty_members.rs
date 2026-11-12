/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::collections::BTreeMap;
use std::sync::Arc;

use dupe::Dupe;
use dupe::IterDupedExt;
use flow_aloc::ALoc;
use flow_common::reason::Name;
use flow_common_ty::ty::ALocTy;
use flow_common_ty::ty::BotKind;
use flow_common_ty::ty::NamedProp;
use flow_common_ty::ty::Prop;
use flow_common_ty::ty::PropSource;
use flow_common_ty::ty::Ty;
use flow_common_ty::ty_debug::dump_t_EXPOSES_ABSTRACT_LOCS;
use flow_common_ty::ty_utils::simplify_type;
use flow_parser_utils::file_sig::FileSig;
use flow_typing_context::Context;
use flow_typing_ty_normalizer::env::EvaluateTypeDestructorsMode;
use flow_typing_ty_normalizer::env::Options;
use flow_typing_type::type_::Type;

use crate::ty_normalizer_flow;

#[derive(Debug, Clone)]
pub struct MemberInfo<T> {
    pub ty: T,
    pub optional: bool,
    /// The location where the member is defined. There may be multiple in cases
    /// involving unions (e.g., `{ foo: string } | { foo: number }`).
    pub def_locs: Vec<ALoc>,
    /// [inherited] indicates whether the member was inherited either from a super
    /// class, higher in the prototype chain, or from an interface.
    pub inherited: bool,
    /// [source] indicates whether the member was defined in a "primitive prototype",
    /// an interface, or some other object/class.
    pub source: PropSource,
    /// If a member came from a possibly-null/undefined object, autocomplete may suggest
    /// that the user use optional chaining to access it.
    /// [from_nullable] indicates that the member is from a possibly-null/undefined object.
    pub from_nullable: bool,
}

fn map_member_info<A, B>(f: impl FnOnce(A) -> B, info: MemberInfo<A>) -> MemberInfo<B> {
    MemberInfo {
        ty: f(info.ty),
        optional: info.optional,
        def_locs: info.def_locs,
        inherited: info.inherited,
        source: info.source,
        from_nullable: info.from_nullable,
    }
}

/// Special cases we'll consider when recursively getting the members of a constituent type
/// in a union/intersection
enum MembershipBehavior {
    /// the given type's member set is the universal set, where each member's type is the given type.
    EmptyOrAny,
    /// the given type is Void or Null
    Nullish,
    /// a type which falls into neither of these special cases
    Normal,
}

fn membership_behavior(ty: &Ty<ALoc>) -> MembershipBehavior {
    match ty {
        Ty::Bot(_) | Ty::Any(_) => MembershipBehavior::EmptyOrAny,
        Ty::Void | Ty::Undefined | Ty::TsVoid | Ty::Null => MembershipBehavior::Nullish,
        _ => MembershipBehavior::Normal,
    }
}

fn merge_sources(src1: PropSource, src2: PropSource) -> PropSource {
    match (&src1, &src2) {
        // Object.prototype has the lowest priority. If a prop comes from anywhere
        // else, take that instead.
        (_, PropSource::PrimitiveProto(s)) if s.as_str() == "Object" => src1,
        (PropSource::PrimitiveProto(s), _) if s.as_str() == "Object" => src2,
        // All other primitive protos have the next lowest priority.
        (_, PropSource::PrimitiveProto(_)) => src1,
        (PropSource::PrimitiveProto(_), _) => src2,
        // "Other" has the highest priority."
        (PropSource::Other, _) | (_, PropSource::Other) => PropSource::Other,
        // A prop is treated as from an interface only if both constituent props are.
        (PropSource::Interface, PropSource::Interface) => PropSource::Interface,
    }
}

fn members_of_ty(
    ts_void: bool,
    ty: &Ty<ALoc>,
) -> (BTreeMap<Name, MemberInfo<ALocTy>>, Vec<String>) {
    fn ty_of_named_prop(ts_void: bool, prop: &NamedProp<ALoc>) -> (ALocTy, bool) {
        let undefined = || Arc::new(if ts_void { Ty::Undefined } else { Ty::Void });
        match prop {
            NamedProp::Field {
                t, optional: false, ..
            }
            | NamedProp::Get(t)
            | NamedProp::Set(t) => (t.dupe(), false),
            NamedProp::Field {
                t, optional: true, ..
            } => (
                Arc::new(Ty::Union(false, t.dupe(), undefined(), Arc::from([]))),
                true,
            ),
            NamedProp::Method { fun, optional } => {
                let t = Arc::new(Ty::Fun(Box::new(fun.clone())));
                let t = if *optional {
                    Arc::new(Ty::Union(false, t, undefined(), Arc::from([])))
                } else {
                    t
                };
                (t, *optional)
            }
        }
    }

    fn members_of_obj(
        ts_void: bool,
        obj_props: &[Prop<ALoc>],
    ) -> (BTreeMap<Name, MemberInfo<ALocTy>>, Vec<String>) {
        let mut mems1: BTreeMap<Name, MemberInfo<ALocTy>> = BTreeMap::new();
        let mut errs1: Vec<String> = Vec::new();
        for prop in obj_props {
            let (mems2, errs2) = match prop {
                Prop::NamedProp {
                    name,
                    prop,
                    inherited,
                    source,
                    def_locs,
                } => {
                    let (ty, optional) = ty_of_named_prop(ts_void, prop);
                    let member = MemberInfo {
                        ty,
                        optional,
                        inherited: *inherited,
                        source: source.clone(),
                        from_nullable: false,
                        def_locs: def_locs.to_vec(),
                    };
                    let mut m = BTreeMap::new();
                    m.insert(name.dupe(), member);
                    (m, vec![])
                }
                Prop::SpreadProp(ty) => members_of_ty(ts_void, ty),
                // TODO(jmbrown): Mapped Type autocomplete
                Prop::MappedTypeProp { .. } => (BTreeMap::new(), vec![]),
                Prop::CallProp(_) => (BTreeMap::new(), vec![]),
            };
            for (k, v) in mems2 {
                mems1.insert(k, v);
            }
            errs1.extend(errs2);
        }
        (mems1, errs1)
    }

    // given a list of objects, collates the members that exist in all of them
    fn intersection_of_members(
        t1_members: BTreeMap<Name, MemberInfo<ALocTy>>,
        ts_members: Vec<BTreeMap<Name, MemberInfo<ALocTy>>>,
    ) -> BTreeMap<Name, MemberInfo<Vec<ALocTy>>> {
        let mut result: BTreeMap<Name, MemberInfo<Vec<ALocTy>>> = t1_members
            .into_iter()
            .map(|(k, v)| (k, map_member_info(|ty| vec![ty], v)))
            .collect();
        for ts_member in ts_members.into_iter().rev() {
            let old_result = std::mem::take(&mut result);
            for (key, tys_info) in old_result {
                if let Some(ty_info) = ts_member.get(&key) {
                    // We say that a member formed by unioning other members should be treated:
                    // - as inherited if all of its constituent members are.
                    // - as from a nullable object if any of its constituent members are.
                    // - as defined at the definition locations of any of its constituent members.
                    let mut new_tys = vec![ty_info.ty.clone()];
                    new_tys.extend(tys_info.ty);
                    let mut new_def_locs = ty_info.def_locs.clone();
                    new_def_locs.extend(tys_info.def_locs);
                    result.insert(
                        key,
                        MemberInfo {
                            ty: new_tys,
                            optional: ty_info.optional || tys_info.optional,
                            inherited: ty_info.inherited && tys_info.inherited,
                            source: merge_sources(ty_info.source.clone(), tys_info.source),
                            from_nullable: ty_info.from_nullable || tys_info.from_nullable,
                            def_locs: new_def_locs,
                        },
                    );
                }
            }
        }
        result
    }

    // given a list of objects, collates the members that exist in any of them
    fn union_of_members(
        t1_members: BTreeMap<Name, MemberInfo<ALocTy>>,
        ts_members: Vec<BTreeMap<Name, MemberInfo<ALocTy>>>,
    ) -> BTreeMap<Name, MemberInfo<Vec<ALocTy>>> {
        let mut result: BTreeMap<Name, MemberInfo<Vec<ALocTy>>> = t1_members
            .into_iter()
            .map(|(k, v)| (k, map_member_info(|ty| vec![ty], v)))
            .collect();
        for ts_member in ts_members.into_iter().rev() {
            let old_result = std::mem::take(&mut result);
            let mut all_keys: std::collections::BTreeSet<Name> =
                old_result.keys().duped().collect();
            all_keys.extend(ts_member.keys().duped());
            for key in all_keys {
                let ty_opt = ts_member.get(&key);
                let tys_opt = old_result.get(&key);
                match (ty_opt, tys_opt) {
                    (Some(ty_info), Some(tys_info)) => {
                        // We say that a member formed by intersecting other members should be treated:
                        // - as inherited if all of its constituent members are.
                        // - as from a nullable object only if all its constituent members are.
                        // - as defined at the definition locations of any of its constituent members.
                        let mut new_tys = vec![ty_info.ty.clone()];
                        new_tys.extend(tys_info.ty.clone());
                        let mut new_def_locs = ty_info.def_locs.clone();
                        new_def_locs.extend(tys_info.def_locs.clone());
                        result.insert(
                            key,
                            MemberInfo {
                                ty: new_tys,
                                optional: ty_info.optional || tys_info.optional,
                                inherited: ty_info.inherited && tys_info.inherited,
                                source: merge_sources(
                                    ty_info.source.clone(),
                                    tys_info.source.clone(),
                                ),
                                from_nullable: ty_info.from_nullable && tys_info.from_nullable,
                                def_locs: new_def_locs,
                            },
                        );
                    }
                    (Some(ty_info), None) => {
                        result.insert(key, map_member_info(|ty| vec![ty], ty_info.clone()));
                    }
                    (None, Some(tys_info)) => {
                        result.insert(key, tys_info.clone());
                    }
                    (None, None) => {}
                }
            }
        }
        result
    }

    fn intersection_of_member_types(
        members: BTreeMap<Name, MemberInfo<Vec<ALocTy>>>,
    ) -> BTreeMap<Name, MemberInfo<ALocTy>> {
        members
            .into_iter()
            .map(|(k, v)| {
                (
                    k,
                    map_member_info(
                        |tys: Vec<ALocTy>| {
                            let mut iter = tys.into_iter();
                            let t = iter.next().unwrap();
                            let rest: Vec<ALocTy> = iter.collect();
                            if rest.is_empty() {
                                t
                            } else {
                                let t2 = rest[0].dupe();
                                let ts: Vec<ALocTy> = rest[1..].to_vec();
                                simplify_type(true, None, Arc::new(Ty::Inter(t, t2, Arc::from(ts))))
                            }
                        },
                        v,
                    ),
                )
            })
            .collect()
    }

    fn union_of_member_types(
        from_bounds: bool,
        members: BTreeMap<Name, MemberInfo<Vec<ALocTy>>>,
    ) -> BTreeMap<Name, MemberInfo<ALocTy>> {
        members
            .into_iter()
            .map(|(k, v)| {
                (
                    k,
                    map_member_info(
                        |tys: Vec<ALocTy>| {
                            let mut iter = tys.into_iter();
                            let t = iter.next().unwrap();
                            let rest: Vec<ALocTy> = iter.collect();
                            if rest.is_empty() {
                                t
                            } else {
                                let t2 = rest[0].dupe();
                                let ts: Vec<ALocTy> = rest[1..].to_vec();
                                simplify_type(
                                    true,
                                    None,
                                    Arc::new(Ty::Union(from_bounds, t, t2, Arc::from(ts))),
                                )
                            }
                        },
                        v,
                    ),
                )
            })
            .collect()
    }

    fn add_special_cases(
        special_cases: &[ALocTy],
        members: BTreeMap<Name, MemberInfo<Vec<ALocTy>>>,
    ) -> BTreeMap<Name, MemberInfo<Vec<ALocTy>>> {
        members
            .into_iter()
            .map(|(k, v)| {
                (
                    k,
                    map_member_info(
                        |tys: Vec<ALocTy>| {
                            let mut new_tys = special_cases.to_vec();
                            new_tys.extend(tys);
                            new_tys
                        },
                        v,
                    ),
                )
            })
            .collect()
    }

    fn members_of_union(
        ts_void: bool,
        from_bounds: bool,
        t1: &Ty<ALoc>,
        t2: &Ty<ALoc>,
        ts: &[Arc<Ty<ALoc>>],
    ) -> (BTreeMap<Name, MemberInfo<ALocTy>>, Vec<String>) {
        let (t1_members, errs1) = members_of_ty(ts_void, t1);
        let (t2_members, errs2) = members_of_ty(ts_void, t2);
        let ts_results: Vec<(BTreeMap<Name, MemberInfo<ALocTy>>, Vec<String>)> =
            ts.iter().map(|t| members_of_ty(ts_void, t)).collect();
        let (ts_members, errss): (Vec<_>, Vec<_>) = ts_results.into_iter().unzip();
        let mut errs = errs1;
        errs.extend(errs2);
        for e in errss {
            errs.extend(e);
        }
        // set union of all child members
        let mut universe: BTreeMap<Name, ()> = BTreeMap::new();
        for k in t1_members.keys() {
            universe.insert(k.dupe(), ());
        }
        for k in t2_members.keys() {
            universe.insert(k.dupe(), ());
        }
        for ts_m in &ts_members {
            for k in ts_m.keys() {
                universe.insert(k.dupe(), ());
            }
        }
        // empty and any have all possible members
        let f = |ty: &Ty<ALoc>,
                 ty_members: BTreeMap<Name, MemberInfo<ALocTy>>|
         -> BTreeMap<Name, MemberInfo<ALocTy>> {
            match membership_behavior(ty) {
                MembershipBehavior::EmptyOrAny => universe
                    .keys()
                    .map(|k| {
                        (
                            k.dupe(),
                            MemberInfo {
                                ty: Arc::new(ty.clone()),
                                optional: false,
                                inherited: true,
                                source: PropSource::PrimitiveProto("Object".into()),
                                from_nullable: false,
                                def_locs: vec![],
                            },
                        )
                    })
                    .collect(),
                MembershipBehavior::Nullish => universe
                    .keys()
                    .map(|k| {
                        (
                            k.dupe(),
                            MemberInfo {
                                ty: Arc::new(Ty::Bot(BotKind::EmptyType)),
                                optional: false,
                                inherited: true,
                                source: PropSource::PrimitiveProto("Object".into()),
                                from_nullable: true,
                                def_locs: vec![],
                            },
                        )
                    })
                    .collect(),
                MembershipBehavior::Normal => ty_members,
            }
        };
        let t1_members = f(t1, t1_members);
        let t2_members = f(t2, t2_members);
        let ts_members: Vec<BTreeMap<Name, MemberInfo<ALocTy>>> = ts
            .iter()
            .zip(ts_members)
            .map(|(ty, ty_members)| f(ty, ty_members))
            .collect();
        let mut all_ts_members = vec![t2_members];
        all_ts_members.extend(ts_members);
        // set intersection of members; type union upon overlaps
        let mems = intersection_of_members(t1_members, all_ts_members);
        let mems = union_of_member_types(from_bounds, mems);
        (mems, errs)
    }

    fn members_of_intersection(
        ts_void: bool,
        t1: &Ty<ALoc>,
        t2: &Ty<ALoc>,
        ts: &[Arc<Ty<ALoc>>],
    ) -> (BTreeMap<Name, MemberInfo<ALocTy>>, Vec<String>) {
        let (t1_members, errs1) = members_of_ty(ts_void, t1);
        let (t2_members, errs2) = members_of_ty(ts_void, t2);
        let ts_results: Vec<(BTreeMap<Name, MemberInfo<ALocTy>>, Vec<String>)> =
            ts.iter().map(|t| members_of_ty(ts_void, t)).collect();
        let (ts_members, errss): (Vec<_>, Vec<_>) = ts_results.into_iter().unzip();
        let mut errs = errs1;
        errs.extend(errs2);
        for e in errss {
            errs.extend(e);
        }
        let all_tys: Vec<&Ty<ALoc>> = std::iter::once(t1)
            .chain(std::iter::once(t2))
            .chain(ts.iter().map(|t| t.as_ref()))
            .collect();
        let special_cases: Vec<ALocTy> = all_tys
            .iter()
            .filter_map(|ty| match membership_behavior(ty) {
                MembershipBehavior::EmptyOrAny => Some(Arc::new((*ty).clone())),
                MembershipBehavior::Nullish | MembershipBehavior::Normal => None,
            })
            .collect();
        let mut all_ts_members = vec![t2_members];
        all_ts_members.extend(ts_members);
        // set union of members; type intersection upon overlaps
        let mems = union_of_members(t1_members, all_ts_members);
        let mems = add_special_cases(&special_cases, mems);
        let mems = intersection_of_member_types(mems);
        (mems, errs)
    }

    match ty {
        Ty::Obj(obj) => members_of_obj(ts_void, &obj.obj_props),
        Ty::Fun(fun) => members_of_ty(ts_void, &fun.fun_static),
        Ty::Union(from_bounds, t1, t2, ts) => members_of_union(ts_void, *from_bounds, t1, t2, ts),
        Ty::Inter(t1, t2, ts) => members_of_intersection(ts_void, t1, t2, ts),
        Ty::Bound(..)
        | Ty::Generic(..)
        | Ty::Symbol
        | Ty::UniqueSymbol
        | Ty::Num
        | Ty::Str
        | Ty::Bool
        | Ty::BigInt
        | Ty::NumLit(_)
        | Ty::StrLit(_)
        | Ty::BoolLit(_)
        | Ty::BigIntLit(_)
        | Ty::Arr(_)
        | Ty::Tup { .. } => (
            BTreeMap::new(),
            vec![format!(
                "members_of_ty unexpectedly applied to ({})",
                dump_t_EXPOSES_ABSTRACT_LOCS(ty)
            )],
        ),
        Ty::Truncated
        | Ty::Any(_)
        | Ty::Top
        | Ty::Bot(_)
        | Ty::Void
        | Ty::Undefined
        | Ty::TsVoid
        | Ty::Null
        | Ty::InlineInterface(_)
        | Ty::TypeOf(_)
        | Ty::Utility(_)
        | Ty::IndexedAccess { .. }
        | Ty::Conditional { .. }
        | Ty::Component { .. }
        | Ty::Infer(_)
        | Ty::Renders(_, _)
        | Ty::TemplateLiteral { .. } => (BTreeMap::new(), vec![]),
    }
}

pub struct TyMembers {
    pub members: BTreeMap<Name, MemberInfo<ALocTy>>,
    pub errors: Vec<String>,
}

pub fn extract<'a, 'cx>(
    force_instance: bool,
    allowed_prop_names: Option<Vec<Name>>,
    imported_names: Option<ty_normalizer_flow::ImportedNames<'a, 'cx>>,
    cx: &mut Context<'cx>,
    typed_ast_opt: Option<&'a flow_parser::ast::Program<ALoc, (ALoc, Type)>>,
    file_sig: std::sync::Arc<FileSig>,
    scheme: &Type,
) -> Result<TyMembers, String> {
    let options = Options {
        expand_internal_types: true,
        expand_enum_members: false,
        evaluate_type_destructors: EvaluateTypeDestructorsMode::EvaluateNone,
        optimize_types: false,
        omit_targ_defaults_option: false,
        merge_bot_and_any_kinds: true,
        verbose_normalizer: false,
        max_depth: Some(40),
        max_type_size: None,
        toplevel_is_type_identifier_reference: false,
    };
    let genv = match imported_names {
        Some(imported_names) => ty_normalizer_flow::mk_genv_with_imported_names(
            options,
            typed_ast_opt,
            file_sig,
            imported_names,
        ),
        None => ty_normalizer_flow::mk_genv(options, typed_ast_opt, file_sig),
    };
    match ty_normalizer_flow::expand_members(force_instance, allowed_prop_names, cx, &genv, scheme)
    {
        Err(error) => Err(error.to_string()),
        Ok(ref this_ty) if matches!(this_ty.as_ref(), Ty::Any(_)) => {
            Err("not enough type information to extract members".to_string())
        }
        Ok(this_ty) => {
            let (members, errors) = members_of_ty(cx.ts_void(), &this_ty);
            Ok(TyMembers { members, errors })
        }
    }
}
