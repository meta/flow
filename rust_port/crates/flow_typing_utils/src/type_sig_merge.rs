/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::cell::LazyCell;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::rc::Rc;
use std::sync::Arc;

use dupe::Dupe;
use dupe::IterDupedExt;
use dupe::OptionDupedExt;
use flow_aloc::ALoc;
use flow_aloc::ALocTable;
use flow_aloc::aloc_representation_do_not_use;
use flow_common::alpha_rename;
use flow_common::error_ref::FunctionReferenceKind;
use flow_common::flow_import_specifier::FlowImportSpecifier;
use flow_common::flow_import_specifier::Userland;
use flow_common::flow_symbol::Symbol;
use flow_common::polarity::Polarity;
use flow_common::reason;
use flow_common::reason::Name;
use flow_common::reason::Reason;
use flow_common::subst_name::SubstName;
use flow_data_structure_wrapper::ord_map::FlowOrdMap;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_parser::file_key::FileKey;
use flow_parser::loc::LOC_NONE;
use flow_parser::loc::Loc;
use flow_parser::loc_sig::LocSig;
use flow_type_sig::compact_table::Index;
use flow_type_sig::compact_table::Table;
use flow_type_sig::packed_type_sig;
use flow_type_sig::type_sig;
use flow_type_sig::type_sig::*;
use flow_type_sig::type_sig_pack as Pack;
use flow_typing_context::Context;
use flow_typing_context::ContextArena;
use flow_typing_context::Cx;
use flow_typing_context::LazyType;
use flow_typing_context::LocalDefBindingKind;
use flow_typing_context::LocalDefEntry;
use flow_typing_context::ModuleTypeThunk;
use flow_typing_context::RemoteRefEntry;
use flow_typing_context::ResolvedRequire;
use flow_typing_context::SigFile;
use flow_typing_context::SigFileId;
use flow_typing_flow_common::flow_js_utils;
use flow_typing_flow_common::obj_type;
use flow_typing_flow_js::intersection_kit;
use flow_typing_type::type_;
use flow_typing_type::type_::ArrayATData;
use flow_typing_type::type_::DestructorConditionalTypeData;
use flow_typing_type::type_::DestructorMappedTypeData;
use flow_typing_type::type_::DestructorSpreadTypeData;
use flow_typing_type::type_::GenericTData;
use flow_typing_type::type_::ModuleType;
use flow_typing_type::type_::ModuleTypeInner;
use flow_typing_type::type_::NamedSymbol;
use flow_typing_type::type_::PolyTKind;
use flow_typing_type::type_::ROArrayATData;
use flow_typing_type::type_::ThisInstanceTData;
use flow_typing_type::type_::TupleATData;
use flow_typing_type::type_::Type;
use flow_typing_type::type_::TypeStrictnessKind;
use flow_typing_type::type_::aconstraint::AnnotationInferenceOperation;
use flow_typing_type::type_util;
use vec1::Vec1;

use crate::annotation_inference;

type LazyModuleType = Rc<flow_lazy::Lazy<Cx, ModuleType>>;

pub type LazyExport = Rc<flow_lazy::Lazy<Cx, NamedSymbol>>;
pub type LazyCjsExport = Rc<flow_lazy::Lazy<Cx, (Option<ALoc>, Type)>>;

pub enum TsPendingClassified {
    TsPendingType(NamedSymbol),
    TsPendingValue(NamedSymbol),
}

pub type LazyTsPendingClassified = Rc<flow_lazy::Lazy<Cx, TsPendingClassified>>;

pub enum Exports {
    CJSExports {
        type_exports: BTreeMap<FlowSmolStr, LazyExport>,
        exports: Option<LazyCjsExport>,
        type_stars: Vec<(ALoc, Index<FlowImportSpecifier>)>,
        strict: bool,
        platform_availability_set: Option<flow_common::platform_set::PlatformSet>,
    },
    ESExports {
        type_exports: BTreeMap<FlowSmolStr, LazyExport>,
        exports: BTreeMap<FlowSmolStr, LazyExport>,
        ts_pending: Vec<(FlowSmolStr, LazyTsPendingClassified)>,
        type_stars: Vec<(ALoc, Index<FlowImportSpecifier>)>,
        stars: Vec<(ALoc, Index<FlowImportSpecifier>)>,
        strict: bool,
        platform_availability_set: Option<flow_common::platform_set::PlatformSet>,
    },
}

pub struct TargetedMergeResult<L: Ord> {
    roots: BTreeMap<L, Type>,
}

impl<L: Ord> TargetedMergeResult<L> {
    pub fn roots(&self) -> &BTreeMap<L, Type> {
        &self.roots
    }
}

/// Merges the requested roots of a local-only targeted signature into semantic types.
pub fn merge_targeted_module<'cx>(
    cx: &mut Context<'cx>,
    locs: Table<Loc>,
    targeted: packed_type_sig::TargetedModule<Loc>,
) -> TargetedMergeResult<Loc> {
    let aloc = Rc::new(move |index: &Index<Loc>| ALoc::of_loc(locs.get(*index).dupe()));
    merge_targeted_module_impl(cx, aloc, &targeted.module, targeted.roots)
}

/// Merges roots already present in the ordinary packed signature.
pub fn merge_packed_target_roots<'cx>(
    cx: &mut Context<'cx>,
    source: Option<FileKey>,
    module: &packed_type_sig::Module<Loc>,
    roots: BTreeMap<ALoc, packed_type_sig::TargetedRoot<Loc>>,
) -> TargetedMergeResult<ALoc> {
    let aloc = Rc::new(move |index: &Index<Loc>| {
        flow_aloc::aloc_representation_do_not_use::make_keyed(
            source.dupe(),
            index.as_usize() as u32,
        )
    });
    merge_targeted_module_impl(cx, aloc, module, roots)
}

fn mk_targeted_sig_tvar<'cx>(
    cx: &mut Context<'cx>,
    file_id: SigFileId,
    reason: Reason,
    resolve: impl for<'a> FnOnce(&mut Context<'a>, &SigFile) -> Type + 'static,
) -> Type {
    let resolved: LazyType = Rc::new(flow_lazy::Lazy::new(move |cx: &mut Context<'_>| {
        let file = cx.sig_file(file_id);
        resolve(cx, &file)
    }));
    annotation_inference::mk_sig_tvar(cx, reason, resolved)
}

fn merge_targeted_module_impl<'cx, L: Ord>(
    cx: &mut Context<'cx>,
    aloc: Rc<dyn Fn(&Index<Loc>) -> ALoc>,
    module: &packed_type_sig::Module<Loc>,
    roots: BTreeMap<L, packed_type_sig::TargetedRoot<Loc>>,
) -> TargetedMergeResult<L> {
    use flow_common::reason::VirtualReasonDesc::RExports;
    use packed_type_sig::TargetedRoot;

    let file_id = cx.add_sig_file(|file_id| {
        let dependencies = module.module_refs.map(|mref| {
            let specifier = FlowImportSpecifier::Userland(mref.dupe());
            let resolved: Rc<flow_lazy::Lazy<Cx, ResolvedRequire>> =
                Rc::new(flow_lazy::Lazy::new(move |cx: &mut Context<'_>| {
                    cx.find_require(&specifier)
                }));
            (mref.dupe(), resolved)
        });

        let local_defs = module.local_defs.map(|def| {
            let aloc = aloc.dupe();
            let def = def.clone();
            let entry: Rc<flow_lazy::Lazy<Cx, LocalDefEntry>> =
                Rc::new(flow_lazy::Lazy::new(move |_cx: &mut Context<'_>| {
                    let def = Rc::new(def.map(
                        &mut (),
                        |_, loc: &Index<Loc>| (*aloc)(loc),
                        |_, t: &Pack::Packed<Index<Loc>>| t.map(&|i| (*aloc)(i)),
                    ));
                    let loc = def.id_loc();
                    let name = def.name().dupe();
                    let binding_kind = def_binding_kind(&def);
                    let reason = def_reason(&def);
                    let make_type = |const_decl: bool| -> LazyType {
                        let reason_for_tvar = reason.dupe();
                        let reason = reason.dupe();
                        let def = def.dupe();
                        Rc::new(flow_lazy::Lazy::new(move |cx: &mut Context<'_>| {
                            mk_targeted_sig_tvar(
                                cx,
                                file_id,
                                reason_for_tvar,
                                move |cx: &mut Context<'_>, file: &SigFile| {
                                    merge_def(cx, file, reason, &def, const_decl)
                                },
                            )
                        }))
                    };
                    (loc, name, binding_kind, make_type(false), make_type(true))
                }));
            entry
        });

        let remote_refs = module.remote_refs.map(|remote_ref| {
            let aloc = aloc.dupe();
            let remote_ref = remote_ref.clone();
            let entry: Rc<flow_lazy::Lazy<Cx, RemoteRefEntry>> =
                Rc::new(flow_lazy::Lazy::new(move |cx: &mut Context<'_>| {
                    let remote_ref = remote_ref.map(&|i| (*aloc)(i));
                    let loc = remote_ref.loc().dupe();
                    let name = remote_ref.name().dupe();
                    let reason = remote_ref_reason(&remote_ref);
                    let reason_for_type = reason.dupe();
                    let remote_ref_for_type = remote_ref.clone();
                    let t = mk_targeted_sig_tvar(
                        cx,
                        file_id,
                        reason.dupe(),
                        move |cx: &mut Context<'_>, file: &SigFile| {
                            merge_remote_ref(cx, file, reason_for_type, &remote_ref_for_type)
                        },
                    );
                    let reason_for_extends = reason.dupe();
                    let t_for_extends: LazyType =
                        Rc::new(flow_lazy::Lazy::new(move |cx: &mut Context<'_>| {
                            mk_targeted_sig_tvar(
                                cx,
                                file_id,
                                reason,
                                move |cx: &mut Context<'_>, file: &SigFile| {
                                    merge_remote_ref_for_extends(
                                        cx,
                                        file,
                                        reason_for_extends,
                                        &remote_ref,
                                    )
                                },
                            )
                        }));
                    (loc, name, t, t_for_extends)
                }));
            entry
        });

        let pattern_defs = module.pattern_defs.map(|def| {
            let aloc = aloc.dupe();
            let def = def.clone();
            let t: LazyType = Rc::new(flow_lazy::Lazy::new(move |cx: &mut Context<'_>| {
                let def = def.map(&|i| (*aloc)(i));
                let file = cx.sig_file(file_id);
                merge(FlowOrdMap::new(), cx, &file, &def)
            }));
            t
        });

        let patterns = module.patterns.map(|pattern| {
            let aloc = aloc.dupe();
            let pattern = pattern.clone();
            let t: LazyType = Rc::new(flow_lazy::Lazy::new(move |cx: &mut Context<'_>| {
                let pattern = pattern.map(&|i| (*aloc)(i));
                let file = cx.sig_file(file_id);
                merge_pattern(cx, &file, &pattern)
            }));
            t
        });

        SigFile {
            dependencies,
            exports: Rc::new(|_cx: &mut Context<'_>| {
                Err(type_::any_t::annot(reason::locationless_reason(RExports)))
            }),
            local_defs,
            remote_refs,
            pattern_defs,
            patterns,
        }
    });
    let file = cx.sig_file(file_id);

    let roots = roots
        .into_iter()
        .map(|(loc, root)| {
            let type_ = match root {
                TargetedRoot::LocalDef(index) => {
                    let entry = file.local_defs.get(index).dupe();
                    let (_, _, _, general, _) = entry.get_forced(cx);
                    general.get_forced(cx).dupe()
                }
                TargetedRoot::RemoteRef(index) => {
                    let entry = file.remote_refs.get(index).dupe();
                    let (_, _, type_, _) = entry.get_forced(cx);
                    type_.dupe()
                }
            };
            (loc, type_)
        })
        .collect();

    TargetedMergeResult { roots }
}

pub type TparamsMap = FlowOrdMap<FlowSmolStr, Type>;

pub fn def_reason<T>(def: &Def<ALoc, T>) -> Reason {
    use flow_common::reason::VirtualReasonDesc::*;
    match def {
        Def::TypeAlias(_) | Def::OpaqueType(_) => {
            type_::desc_format::type_reason(Name::new(def.name().dupe()), def.id_loc())
        }
        Def::Interface(_)
        | Def::ClassBinding(_)
        | Def::DeclareClassBinding(_)
        | Def::RecordBinding(_)
        | Def::DisabledRecordBinding(_) => {
            type_::desc_format::instance_reason(Name::new(def.name().dupe()), def.id_loc())
        }
        Def::FunBinding(inner) => {
            reason::func_reason(inner.async_, inner.generator, inner.fn_loc.dupe())
        }
        Def::DeclareFun(_) => reason::mk_reason(RFunctionType, def.id_loc()),
        Def::ComponentBinding(inner) => {
            reason::mk_reason(RComponent(inner.name.dupe()), inner.fn_loc.dupe())
        }
        Def::DisabledComponentBinding(inner) => {
            reason::mk_reason(RIdentifier(inner.name.dupe()), inner.id_loc.dupe())
        }
        Def::Variable(_) | Def::Parameter(_) => {
            reason::mk_reason(RIdentifier(def.name().dupe()), def.id_loc())
        }
        Def::DisabledEnumBinding(_) | Def::EnumBinding(_) => reason::mk_reason(
            REnum {
                name: Some(def.name().dupe()),
            },
            def.id_loc(),
        ),
        Def::NamespaceBinding(_) => reason::mk_reason(None, def.id_loc()),
    }
}

pub fn def_binding_kind<T>(def: &Def<ALoc, T>) -> LocalDefBindingKind {
    let tparams_kind = |tparams: &TParams<ALoc, T>| match tparams {
        TParams::Mono => LocalDefBindingKind::DefBindingClassLikeMono,
        TParams::Poly(_) => LocalDefBindingKind::DefBindingClassLikePoly,
    };
    match def {
        Def::Interface(inner) => tparams_kind(&inner.tparams),
        Def::ClassBinding(inner) => tparams_kind(&inner.def.tparams),
        Def::DeclareClassBinding(inner) => tparams_kind(&inner.def.tparams),
        Def::RecordBinding(inner) => tparams_kind(&inner.def.tparams),
        Def::TypeAlias(_)
        | Def::OpaqueType(_)
        | Def::DisabledRecordBinding(_)
        | Def::FunBinding(_)
        | Def::DeclareFun(_)
        | Def::ComponentBinding(_)
        | Def::DisabledComponentBinding(_)
        | Def::Variable(_)
        | Def::Parameter(_)
        | Def::EnumBinding(_)
        | Def::DisabledEnumBinding(_)
        | Def::NamespaceBinding(_) => LocalDefBindingKind::DefBindingOther,
    }
}

pub fn remote_ref_reason(remote_ref: &Pack::RemoteRef<ALoc>) -> Reason {
    use flow_common::reason::VirtualReasonDesc::*;
    match remote_ref {
        Pack::RemoteRef::Import { id_loc, name, .. }
        | Pack::RemoteRef::ImportNs { id_loc, name, .. } => {
            reason::mk_reason(RIdentifier(name.dupe()), id_loc.dupe())
        }
        Pack::RemoteRef::ImportType { id_loc, name, .. }
        | Pack::RemoteRef::ImportTypeof { id_loc, name, .. }
        | Pack::RemoteRef::ImportTypeofNs { id_loc, name, .. }
        | Pack::RemoteRef::ImportTypeNs { id_loc, name, .. } => {
            type_::desc_format::type_reason(Name::new(name.dupe()), id_loc.dupe())
        }
    }
}

fn eval_id_of_aloc<'cx>(cx: &mut Context<'cx>, loc: ALoc) -> type_::eval::Id {
    type_::eval::Id::of_aloc_id(true, cx.make_aloc_id(&loc))
}

fn specialize<'cx>(
    cx: &mut Context<'cx>,
    reason_op: Reason,
    t: Type,
    operation: AnnotationInferenceOperation,
) -> Type {
    let reason = type_util::reason_of_t(&t).dupe();
    annotation_inference::specialize(
        cx,
        t,
        type_::unknown_use(),
        reason_op,
        reason,
        None,
        operation,
    )
}

/// Repositioning the underlying type does not seem to have any perceptible impact
/// when dealing with annotations. Instead of invoking the convoluted Flow_js.reposition
/// implementation here, we just return the type intact. What does have an effect is the
/// lazy tvar indirection, which updates the reason on the new OpenT.
fn reposition_sig_tvar<'cx>(cx: &mut Context<'cx>, loc: ALoc, t: Type) -> Type {
    let reason = type_util::reason_of_t(&t).dupe().reposition(loc);
    annotation_inference::mk_sig_tvar(cx, reason, Rc::new(flow_lazy::Lazy::new_forced(t)))
}

fn eval_arith<'cx>(
    cx: &mut Context<'cx>,
    loc: ALoc,
    lhs_t: Type,
    rhs_t: Type,
    op: flow_parser::ast::expression::BinaryOperator,
) -> Type {
    let reason = reason::mk_reason(None, loc);
    let kind = type_::arith_kind::ArithKind::of_binary_operator(op);
    annotation_inference::arith(cx, reason, lhs_t, rhs_t, kind)
}

fn eval_unary<'cx>(
    cx: &mut Context<'cx>,
    loc: ALoc,
    t: Type,
    op: flow_parser::ast::expression::UnaryOperator,
) -> Type {
    use flow_parser::ast::expression::UnaryOperator;
    match op {
        UnaryOperator::Minus => {
            let reason = reason::mk_reason(type_util::desc_of_t(&t).cloned(), loc);
            annotation_inference::unary_arith(cx, reason, t, type_::UnaryArithKind::Minus)
        }
        UnaryOperator::Plus => {
            let reason = reason::mk_reason(type_util::desc_of_t(&t).cloned(), loc);
            annotation_inference::unary_arith(cx, reason, t, type_::UnaryArithKind::Plus)
        }
        UnaryOperator::BitNot => {
            let reason = reason::mk_reason(type_util::desc_of_t(&t).cloned(), loc);
            annotation_inference::unary_arith(cx, reason, t, type_::UnaryArithKind::BitNot)
        }
        UnaryOperator::Not => {
            let reason = reason::mk_reason(None, loc);
            annotation_inference::unary_not(cx, reason, t)
        }
        UnaryOperator::Typeof => type_::str_module_t::at(loc),
        UnaryOperator::Void => type_::void::at(loc),
        UnaryOperator::Delete => type_::bool_module_t::at(loc),
        // This is a parse error
        UnaryOperator::Nonnull | UnaryOperator::Await => {
            type_::any_t::at(type_::AnySource::AnyError(None), loc)
        }
    }
}

fn eval_update<'cx>(cx: &mut Context<'cx>, loc: ALoc, t: Type) -> Type {
    let reason = reason::mk_reason(type_util::desc_of_t(&t).cloned(), loc);
    annotation_inference::unary_arith(cx, reason, t, type_::UnaryArithKind::Update)
}

fn eval<'cx>(cx: &mut Context<'cx>, loc: ALoc, t: Type, op: Op<Type>) -> Type {
    use flow_common::reason::VirtualReasonDesc::*;
    match op {
        Op::Arith(box (bin_op, rhs_t)) => eval_arith(cx, loc, t, rhs_t, bin_op),
        Op::Unary(unary_op) => eval_unary(cx, loc, t, unary_op),
        Op::Update => eval_update(cx, loc, t),
        Op::GetProp(box name) => {
            let name = Name::new(name);
            let reason = reason::mk_reason(RProperty(Some(name.dupe())), loc);
            // TODO: use_op
            let use_op = type_::unknown_use();
            annotation_inference::get_prop(cx, use_op, reason, None, name, t)
        }
        Op::GetElem(box index) => {
            let reason = reason::mk_reason(RProperty(None), loc);
            // TODO: use_op
            let use_op = type_::unknown_use();
            annotation_inference::get_elem(cx, use_op, reason, index, t)
        }
    }
}

fn async_void_return<'cx>(cx: &mut Context<'cx>, loc: ALoc) -> Type {
    let reason = reason::mk_reason(None, loc.dupe());
    let targs = vec![flow_typing_type::type_::void::at(loc)];
    flow_js_utils::lookup_builtin_typeapp(cx, reason, "Promise", targs)
}

fn add_default_constructor<T>(
    reason: Reason,
    extends: &ClassExtends<ALoc, T>,
    props: &mut BTreeMap<Name, type_::Property>,
) {
    match extends {
        ClassExtends::ClassExplicitExtends { .. }
        | ClassExtends::ClassExplicitExtendsApp { .. } => {}
        ClassExtends::ClassImplicitExtends | ClassExtends::ObjectPrototypeExtendsNull => {
            let key = Name::new(FlowSmolStr::new_inline("constructor"));
            if props.contains_key(&key) {
                return;
            }
            let reason = reason.replace_desc(None);
            let return_t = type_::void::why(reason.dupe());
            let statics = type_::dummy_static(reason.dupe());
            let funtype = type_::mk_boundfunctiontype(
                type_::mixed_t::make(reason.dupe()),
                None,
                vec![],
                None,
                reason.dupe(),
                FunctionReferenceKind::DefaultConstructor,
                None,
                None,
                return_t,
            );
            let type_ = Type::new(type_::TypeInner::DefT(
                reason,
                type_::DefT::new(type_::DefTInner::FunT(statics, Rc::new(funtype))),
            ));
            props.insert(
                key,
                type_::Property::new(type_::PropertyInner::Method {
                    key_loc: None,
                    type_,
                }),
            );
        }
    }
}

fn add_record_constructor<'cx>(
    cx: &mut Context<'cx>,
    reason: Reason,
    name: &FlowSmolStr,
    own_props: &BTreeMap<Name, type_::Property>,
    defaulted_props: &flow_data_structure_wrapper::ord_set::FlowOrdSet<FlowSmolStr>,
    class_props: &mut BTreeMap<Name, type_::Property>,
) {
    use flow_common::reason::VirtualReasonDesc::*;

    let record_reason = reason.dupe().replace_desc(RRecordType(name.dupe()));
    let return_t = type_::void::why(reason.dupe());
    let statics = type_::dummy_static(reason.dupe());
    let props: type_::properties::PropertiesMap = own_props
        .iter()
        .map(|(prop_name, prop)| {
            let new_prop = match &**prop {
                type_::PropertyInner::Field(fd) => {
                    if prop_name
                        .as_smol_str_opt()
                        .is_some_and(|s| defaulted_props.contains(s))
                    {
                        let r = type_util::reason_of_t(&fd.type_).dupe();
                        type_::Property::new(type_::PropertyInner::Field(Box::new(
                            type_::FieldData {
                                preferred_def_locs: fd.preferred_def_locs.clone(),
                                key_loc: fd.key_loc.dupe(),
                                type_: Type::new(type_::TypeInner::OptionalT {
                                    reason: r,
                                    type_: fd.type_.dupe(),
                                    use_desc: false,
                                }),
                                polarity: fd.polarity.clone(),
                            },
                        )))
                    } else {
                        prop.dupe()
                    }
                }
                _ => prop.dupe(),
            };
            (prop_name.dupe(), new_prop)
        })
        .collect();
    let param = obj_type::mk_with_proto(
        cx,
        record_reason.dupe(),
        type_::ObjKind::Exact,
        None,
        None,
        Some(props),
        None,
        Type::new(type_::TypeInner::NullProtoT(record_reason)),
        false,
    );
    let funtype = type_::mk_boundfunctiontype(
        type_::mixed_t::make(reason.dupe()),
        None,
        vec![param],
        None,
        reason.dupe(),
        FunctionReferenceKind::FunctionType,
        None,
        None,
        return_t,
    );
    let constructor = type_::Property::new(type_::PropertyInner::Method {
        key_loc: None,
        type_: Type::new(type_::TypeInner::DefT(
            reason,
            type_::DefT::new(type_::DefTInner::FunT(statics, Rc::new(funtype))),
        )),
    });
    class_props.insert(
        Name::new(FlowSmolStr::new_inline("constructor")),
        constructor,
    );
}

fn add_name_field(reason: Reason, props: &mut BTreeMap<Name, type_::Property>) {
    let name_key = Name::new(FlowSmolStr::new("name"));
    props.entry(name_key).or_insert_with(|| {
        type_::Property::new(type_::PropertyInner::Field(Box::new(type_::FieldData {
            preferred_def_locs: None,
            key_loc: None,
            type_: type_::str_module_t::why(reason),
            polarity: Polarity::Neutral,
        })))
    });
}

fn require<'cx>(
    cx: &mut Context<'cx>,
    file: &SigFile,
    loc: ALoc,
    index: Index<FlowImportSpecifier>,
    standard_cjs_esm_interop: bool,
) -> Type {
    let (mref, lazy_resolved) = {
        let deps = &file.dependencies;
        let (m, l) = deps.get(index);
        (m.dupe(), l.dupe())
    };
    let resolved_require = lazy_resolved.get_forced(cx).dupe();
    let reason = reason::mk_reason(reason::VirtualReasonDesc::RModule(mref.dupe()), loc.dupe());
    let symbol = Symbol::mk_module_symbol(mref.dupe().into_inner(), loc.dupe());
    annotation_inference::cjs_require(
        cx,
        reason,
        symbol,
        false,
        standard_cjs_esm_interop,
        resolved_require,
    )
}

fn import<'cx>(
    cx: &mut Context<'cx>,
    file: &SigFile,
    reason: Reason,
    index: Index<FlowImportSpecifier>,
    kind: type_::ImportKind,
    remote: &FlowSmolStr,
    local: &FlowSmolStr,
) -> Type {
    let (mref, lazy_resolved) = {
        let deps = &file.dependencies;
        let (m, l) = deps.get(index);
        (m.dupe(), l.dupe())
    };
    let resolved_require = lazy_resolved.get_forced(cx).dupe();
    if remote.as_str() == "default" {
        annotation_inference::import_default(
            cx,
            reason,
            kind,
            local,
            mref.dupe(),
            false,
            resolved_require,
        )
    } else {
        annotation_inference::import_named(
            cx,
            reason,
            kind,
            remote,
            mref.dupe(),
            false,
            resolved_require,
        )
    }
}

fn import_for_extends<'cx>(
    cx: &mut Context<'cx>,
    file: &SigFile,
    reason: Reason,
    index: Index<FlowImportSpecifier>,
    kind: type_::ImportKind,
    remote: &FlowSmolStr,
    local: &FlowSmolStr,
) -> Type {
    let (mref, lazy_resolved) = {
        let deps = &file.dependencies;
        let (m, l) = deps.get(index);
        (m.dupe(), l.dupe())
    };
    let resolved_require = lazy_resolved.get_forced(cx).dupe();
    if remote.as_str() == "default" {
        annotation_inference::import_default_for_extends(
            cx,
            reason,
            kind,
            local,
            mref.dupe(),
            false,
            resolved_require,
        )
    } else {
        annotation_inference::import_named_for_extends(
            cx,
            reason,
            kind,
            remote,
            mref.dupe(),
            false,
            resolved_require,
        )
    }
}

fn import_ns<'cx>(
    cx: &mut Context<'cx>,
    file: &SigFile,
    reason: Reason,
    name: &FlowSmolStr,
    id_loc: ALoc,
    index: Index<FlowImportSpecifier>,
) -> Type {
    let lazy_resolved = file.dependencies.get(index).1.dupe();
    let resolved_require = lazy_resolved.get_forced(cx).dupe();
    let namespace_symbol = Symbol::mk_module_symbol(
        Userland::from_smol_str(name.dupe()).into_inner(),
        id_loc.dupe(),
    );
    annotation_inference::import_ns(cx, reason, namespace_symbol, false, resolved_require)
}

fn import_typeof_ns<'cx>(
    cx: &mut Context<'cx>,
    file: &SigFile,
    reason: Reason,
    name: &FlowSmolStr,
    id_loc: ALoc,
    index: Index<FlowImportSpecifier>,
) -> Type {
    let lazy_resolved = file.dependencies.get(index).1.dupe();
    let resolved_require = lazy_resolved.get_forced(cx).dupe();
    let namespace_symbol = Symbol::mk_namespace_symbol(name.dupe(), id_loc.dupe());
    let ns_t = annotation_inference::import_ns(
        cx,
        reason.dupe(),
        namespace_symbol,
        false,
        resolved_require,
    );
    annotation_inference::import_typeof(cx, reason, "*", ns_t)
}

// A TS enum (from a .ts/.d.ts signature) is modeled as a NamespaceT tagged with
// SymbolEnum. The representation and its consequences live in the shared
// `ts_enum.rs`'s `mk_ts_enum_namespace_type`, which also backs the local checking
// path `ts_enum.rs`'s `mk_ts_enum_namespace`. Here we just recover each member's
// loc (the signature stores values without locs; the merge step recovers them from
// [members]).
fn merge_ts_enum_namespace<'cx>(
    cx: &mut Context<'cx>,
    reason: Reason,
    id_loc: ALoc,
    enum_name: &FlowSmolStr,
    members: &BTreeMap<FlowSmolStr, ALoc>,
    member_values: &BTreeMap<FlowSmolStr, TsEnumMemberValue>,
) -> Type {
    let resolved_members: Vec<(FlowSmolStr, ALoc, TsEnumMemberValue)> = member_values
        .iter()
        .map(|(name, value)| {
            let loc = match members.get(name) {
                Some(loc) => loc.dupe(),
                None => id_loc.dupe(),
            };
            (name.dupe(), loc, value.clone())
        })
        .collect();
    let namespace_symbol = Symbol::mk_enum_symbol(enum_name.dupe(), id_loc.dupe());
    flow_typing_flow_common::ts_enum::mk_ts_enum_namespace_type(
        cx,
        reason,
        namespace_symbol,
        &resolved_members,
    )
}

fn merge_enum<'cx>(
    cx: &mut Context<'cx>,
    reason: Reason,
    id_loc: ALoc,
    enum_name: &FlowSmolStr,
    rep: Option<&EnumRep>,
    members: &BTreeMap<FlowSmolStr, ALoc>,
    member_values: Option<&BTreeMap<FlowSmolStr, TsEnumMemberValue>>,
    has_unknown_members: bool,
) -> Type {
    use flow_common::reason::VirtualReasonDesc::*;

    match member_values {
        // TS enum (from a .ts/.d.ts signature): model as an object of literal members
        // + union type. This path does not depend on the Flow-Enum representation
        // (`rep`), which may be `None` for member shapes Flow Enums reject (e.g.
        // defaulted numeric members).
        Some(member_values) => {
            merge_ts_enum_namespace(cx, reason, id_loc, enum_name, members, member_values)
        }
        None => match rep {
            None => type_::any_t::error(reason),
            Some(rep) => {
                let rep_t = |desc, def_t_inner| {
                    let reason = reason::mk_reason(desc, id_loc.dupe());
                    Type::new(type_::TypeInner::DefT(
                        reason,
                        type_::DefT::new(def_t_inner),
                    ))
                };
                let representation_t = match rep {
                    EnumRep::BoolRep(None) => rep_t(RBoolean, type_::DefTInner::BoolGeneralT),
                    EnumRep::BoolRep(Some(lit)) => rep_t(
                        RBoolean,
                        type_::DefTInner::SingletonBoolT {
                            value: *lit,
                            from_annot: false,
                        },
                    ),
                    EnumRep::NumberRep { truthy } => {
                        let lit = if *truthy {
                            type_::Literal::Truthy
                        } else {
                            type_::Literal::AnyLiteral
                        };
                        rep_t(RNumber, type_::DefTInner::NumGeneralT(lit))
                    }
                    EnumRep::StringRep { truthy } => {
                        let lit = if *truthy {
                            type_::Literal::Truthy
                        } else {
                            type_::Literal::AnyLiteral
                        };
                        rep_t(RString, type_::DefTInner::StrGeneralT(lit))
                    }
                    EnumRep::SymbolRep => rep_t(RSymbol, type_::DefTInner::SymbolT),
                    EnumRep::BigIntRep { truthy } => {
                        let lit = if *truthy {
                            type_::Literal::Truthy
                        } else {
                            type_::Literal::AnyLiteral
                        };
                        rep_t(RBigInt, type_::DefTInner::BigIntGeneralT(lit))
                    }
                };
                let enum_id = cx.make_aloc_id(&id_loc);
                let enum_info = type_::EnumInfo::new(type_::EnumInfoInner::ConcreteEnum(
                    type_::EnumConcreteInfo::new(type_::EnumConcreteInfoInner {
                        enum_name: enum_name.dupe(),
                        enum_id,
                        members: members.iter().map(|(k, v)| (k.dupe(), v.dupe())).collect(),
                        representation_t,
                        has_unknown_members,
                    }),
                ));
                type_::mk_enum_object_type(reason, Rc::new(enum_info))
            }
        },
    }
}

pub fn merge_pattern<'cx>(
    cx: &mut Context<'cx>,
    file: &SigFile,
    pattern: &Pack::Pattern<ALoc>,
) -> Type {
    use flow_common::reason::VirtualReasonDesc::*;
    match pattern {
        Pack::Pattern::PDef(i) => {
            let __rc = file.pattern_defs.get(*i).dupe();
            __rc.get_forced(cx).dupe()
        }
        Pack::Pattern::PropP { id_loc, name, def } => {
            let t = {
                let __rc = file.patterns.get(*def).dupe();
                __rc.get_forced(cx).dupe()
            };
            let name = Name::new(name.dupe());
            let reason = reason::mk_reason(RProperty(Some(name.dupe())), id_loc.dupe());
            // TODO: use_op
            let use_op = type_::unknown_use();
            annotation_inference::get_prop(cx, use_op, reason, None, name, t)
        }
        Pack::Pattern::ComputedP { elem, def } => {
            let elem = {
                let __rc = file.pattern_defs.get(*elem).dupe();
                __rc.get_forced(cx).dupe()
            };
            let t = {
                let __rc = file.patterns.get(*def).dupe();
                __rc.get_forced(cx).dupe()
            };
            let loc = type_util::loc_of_t(&elem);
            let reason = reason::mk_reason(RProperty(None), loc.dupe());
            // TODO: use_op
            let use_op = type_::unknown_use();
            annotation_inference::get_elem(cx, use_op, reason, elem, t)
        }
        Pack::Pattern::UnsupportedLiteralP(loc) => {
            type_::any_t::at(type_::AnySource::AnyError(None), loc.dupe())
        }
        Pack::Pattern::ObjRestP { loc, xs, def } => {
            let t = {
                let __rc = file.patterns.get(*def).dupe();
                __rc.get_forced(cx).dupe()
            };
            let reason = reason::mk_reason(RObjectPatternRestProp, loc.dupe());
            annotation_inference::obj_rest(cx, reason, xs.iter().duped().collect(), t)
        }
        Pack::Pattern::IndexP { loc, i, def } => {
            let t = {
                let __rc = file.patterns.get(*def).dupe();
                __rc.get_forced(cx).dupe()
            };
            let reason = reason::mk_reason(None, loc.dupe());
            let i = {
                let reason = reason::mk_reason(RNumber, loc.dupe());
                Type::new(type_::TypeInner::DefT(
                    reason,
                    type_::DefT::new(type_::DefTInner::SingletonNumT {
                        value: type_::NumberLiteral(*i as f64, FlowSmolStr::new(i.to_string())),
                        from_annot: false,
                    }),
                ))
            };
            // TODO: use_op
            let use_op = type_::unknown_use();
            annotation_inference::get_elem(cx, use_op, reason, i, t.dupe())
        }
        Pack::Pattern::ArrRestP { loc, i, def } => {
            let t = {
                let __rc = file.patterns.get(*def).dupe();
                __rc.get_forced(cx).dupe()
            };
            let reason = reason::mk_reason(RArrayPatternRestProp, loc.dupe());
            // TODO: use_op
            let use_op = type_::unknown_use();
            annotation_inference::arr_rest(cx, use_op, reason, *i as i32, t)
        }
    }
}

pub fn merge_remote_ref<'cx>(
    cx: &mut Context<'cx>,
    file: &SigFile,
    reason: Reason,
    remote_ref: &Pack::RemoteRef<ALoc>,
) -> Type {
    match remote_ref {
        Pack::RemoteRef::Import {
            id_loc: _,
            name,
            index,
            remote,
        } => import(
            cx,
            file,
            reason,
            index.clone(),
            type_::ImportKind::ImportValue,
            remote,
            name,
        ),
        Pack::RemoteRef::ImportType {
            id_loc: _,
            name,
            index,
            remote,
        } => import(
            cx,
            file,
            reason,
            index.clone(),
            type_::ImportKind::ImportType,
            remote,
            name,
        ),
        Pack::RemoteRef::ImportTypeof {
            id_loc: _,
            name,
            index,
            remote,
        } => import(
            cx,
            file,
            reason,
            index.clone(),
            type_::ImportKind::ImportTypeof,
            remote,
            name,
        ),
        Pack::RemoteRef::ImportNs {
            id_loc,
            name,
            index,
        } => import_ns(cx, file, reason, name, id_loc.dupe(), *index),
        Pack::RemoteRef::ImportTypeofNs {
            id_loc,
            name,
            index,
        } => import_typeof_ns(cx, file, reason, name, id_loc.dupe(), *index),
        Pack::RemoteRef::ImportTypeNs {
            id_loc,
            name,
            index,
        } => import_ns(cx, file, reason, name, id_loc.dupe(), *index),
    }
}

pub fn merge_remote_ref_for_extends<'cx>(
    cx: &mut Context<'cx>,
    file: &SigFile,
    reason: Reason,
    remote_ref: &Pack::RemoteRef<ALoc>,
) -> Type {
    match remote_ref {
        Pack::RemoteRef::Import {
            id_loc: _,
            name,
            index,
            remote,
        } => import_for_extends(
            cx,
            file,
            reason,
            index.clone(),
            type_::ImportKind::ImportValue,
            remote,
            name,
        ),
        Pack::RemoteRef::ImportType {
            id_loc: _,
            name,
            index,
            remote,
        } => import_for_extends(
            cx,
            file,
            reason,
            index.clone(),
            type_::ImportKind::ImportType,
            remote,
            name,
        ),
        Pack::RemoteRef::ImportTypeof {
            id_loc: _,
            name,
            index,
            remote,
        } => import_for_extends(
            cx,
            file,
            reason,
            index.clone(),
            type_::ImportKind::ImportTypeof,
            remote,
            name,
        ),
        Pack::RemoteRef::ImportNs {
            id_loc,
            name,
            index,
        } => import_ns(cx, file, reason, name, id_loc.dupe(), *index),
        Pack::RemoteRef::ImportTypeofNs {
            id_loc,
            name,
            index,
        } => import_typeof_ns(cx, file, reason, name, id_loc.dupe(), *index),
        Pack::RemoteRef::ImportTypeNs {
            id_loc,
            name,
            index,
        } => import_ns(cx, file, reason, name, id_loc.dupe(), *index),
    }
}

fn merge_ref<'cx, R>(
    cx: &mut Context<'cx>,
    file: &SigFile,
    f: impl FnOnce(&mut Context<'cx>, Type, ALoc, ALoc, &FlowSmolStr) -> R,
    packed_ref: &Pack::PackedRef<ALoc>,
    const_decl: bool,
) -> R {
    use flow_common::reason::VirtualReasonDesc::*;

    match packed_ref {
        Pack::PackedRef::LocalRef(box Pack::PackedRefLocal { ref_loc, index }) => {
            let __rc = file.local_defs.get(*index).dupe();
            let entry = __rc.get_forced(cx);
            let (def_loc, name, _binding_kind, t_general, t_const) = entry;
            let t = if const_decl { t_const } else { t_general }
                .get_forced(cx)
                .dupe();
            let t = reposition_sig_tvar(cx, ref_loc.dupe(), t);
            f(cx, t, ref_loc.dupe(), def_loc.dupe(), name)
        }
        Pack::PackedRef::RemoteRef(box Pack::PackedRefRemote { ref_loc, index }) => {
            let __rc = file.remote_refs.get(*index).dupe();
            let entry = __rc.get_forced(cx);
            let (def_loc, name, t, _t_for_extends) = entry;
            let t = reposition_sig_tvar(cx, ref_loc.dupe(), t.dupe());
            f(cx, t, ref_loc.dupe(), def_loc.dupe(), name)
        }
        Pack::PackedRef::BuiltinRef(box Pack::PackedRefBuiltin {
            ref_loc,
            type_ref,
            name,
        }) => {
            let reason = reason::mk_reason(RIdentifier(name.dupe()), ref_loc.dupe());
            let t = if *type_ref {
                flow_js_utils::lookup_builtin_type(cx, name.as_str(), reason)
            } else {
                flow_js_utils::lookup_builtin_value(cx, name.as_str(), reason)
            };
            let def_loc = type_util::reason_of_t(&t).def_loc().dupe();
            f(cx, t, ref_loc.dupe(), def_loc, name)
        }
    }
}

fn merge_tyref<'cx, R>(
    cx: &mut Context<'cx>,
    file: &SigFile,
    f: impl FnOnce(&mut Context<'cx>, Type, ALoc, Vec<FlowSmolStr>) -> R,
    tyref: &Pack::TyRef<ALoc>,
) -> R {
    use flow_common::reason::VirtualReasonDesc::*;

    let mut qualifications = Vec::new();
    let mut current = tyref;
    loop {
        match current {
            Pack::TyRef::Unqualified(packed_ref) => {
                let (mut t, mut loc, mut names) = merge_ref(
                    cx,
                    file,
                    |_, t, ref_loc, _def_loc, name| (t, ref_loc, vec![name.dupe()]),
                    packed_ref,
                    false,
                );
                for (q_loc, q_id_loc, q_name) in qualifications.iter().rev() {
                    let q_loc: &ALoc = q_loc;
                    let q_id_loc: &ALoc = q_id_loc;
                    let q_name: &FlowSmolStr = q_name;
                    names.push(q_name.dupe());
                    let qname: String = names
                        .iter()
                        .map(|s| s.as_str())
                        .collect::<Vec<_>>()
                        .join(".");
                    let id_reason = reason::mk_reason(RType(q_name.dupe()), q_id_loc.dupe());
                    let name = Name::new(q_name.dupe());
                    let op_reason =
                        reason::mk_reason(RType(FlowSmolStr::new(&qname)), q_loc.dupe());
                    let use_op = type_::VirtualUseOp::Op(Arc::new(
                        type_::VirtualRootUseOp::GetProperty(op_reason.dupe()),
                    ));
                    t = annotation_inference::qualify_type(
                        cx, use_op, id_reason, op_reason, name, t,
                    );
                    loc = q_loc.dupe();
                }
                names.reverse();
                return f(cx, t, loc, names);
            }
            Pack::TyRef::Qualified(box Pack::TyRefQualified {
                loc,
                id_loc,
                name,
                qualification,
            }) => {
                qualifications.push((loc.dupe(), id_loc.dupe(), name.dupe()));
                current = qualification;
            }
        }
    }
}

pub fn merge_type_export<'cx>(
    cx: &mut Context<'cx>,
    file: &SigFile,
    reason: Reason,
    type_export: &Pack::TypeExport<ALoc>,
) -> NamedSymbol {
    match type_export {
        Pack::TypeExport::ExportTypeRef(ref_) => match ref_ {
            Pack::PackedRef::RemoteRef(box Pack::PackedRefRemote { ref_loc, index }) => {
                let __rc = file.remote_refs.get(*index).dupe();
                let entry = __rc.get_forced(cx);
                let (def_loc, name, t, t_for_extends) = entry;
                let t = reposition_sig_tvar(cx, ref_loc.dupe(), t.dupe());
                let type_ =
                    annotation_inference::assert_export_is_type(cx, reason, name.as_str(), t);
                NamedSymbol::new(
                    Some(def_loc.dupe()),
                    None,
                    type_,
                    Some(t_for_extends.get_forced(cx).dupe()),
                )
            }
            _ => {
                let reason = reason.dupe();
                merge_ref(
                    cx,
                    file,
                    |cx, t, _ref_loc, def_loc, name| {
                        let type_ = annotation_inference::assert_export_is_type(
                            cx,
                            reason,
                            name.as_str(),
                            t,
                        );
                        NamedSymbol::new(Some(def_loc), None, type_, None)
                    },
                    ref_,
                    false,
                )
            }
        },
        Pack::TypeExport::ExportTypeBinding(index) => {
            let __rc = file.local_defs.get(*index).dupe();
            let entry = __rc.get_forced(cx);
            let (loc, name, _binding_kind, t_general, _t_const) = entry;
            let t = t_general.get_forced(cx).dupe();
            let type_ = annotation_inference::assert_export_is_type(cx, reason, name.as_str(), t);
            NamedSymbol::new(Some(loc.dupe()), None, type_, None)
        }
        Pack::TypeExport::ExportTypeFrom(index) => {
            let __rc = file.remote_refs.get(*index).dupe();
            let entry = __rc.get_forced(cx);
            let (loc, _name, type_, t_for_extends) = entry;
            NamedSymbol::new(
                Some(loc.dupe()),
                None,
                type_.dupe(),
                Some(t_for_extends.get_forced(cx).dupe()),
            )
        }
    }
}

fn mk_commonjs_module_t<'cx>(
    cx: &mut Context<'cx>,
    module_reason: Reason,
    module_is_strict: bool,
    module_available_platforms: Option<flow_common::platform_set::PlatformSet>,
    def_loc: Option<ALoc>,
    t: Type,
) -> LazyModuleType {
    let module_export_types = type_::ExportTypes {
        value_exports_tmap: cx.make_export_map(type_::exports::T::new()),
        type_exports_tmap: cx.make_export_map(type_::exports::T::new()),
        cjs_export: Some((def_loc, t.dupe())),
        has_every_named_export: false,
    };
    let local_module = ModuleType::new(ModuleTypeInner {
        module_reason: module_reason.dupe(),
        module_export_types,
        module_is_strict,
        module_available_platforms,
    });
    annotation_inference::lazy_cjs_extract_named_exports(module_reason, local_module, t)
}

pub fn merge_exports<'cx>(
    cx: &mut Context<'cx>,
    file: &SigFile,
    reason: Reason,
    exports: Exports,
) -> LazyModuleType {
    type FromNs = Option<ModuleTypeThunk>;

    fn merge_star<'cx>(
        cx: &mut Context<'cx>,
        file: &SigFile,
        (loc, index): &(ALoc, Index<FlowImportSpecifier>),
    ) -> (ALoc, FromNs) {
        let lazy_resolved = file.dependencies.get(index.clone()).1.dupe();
        let resolved_require = lazy_resolved.get_forced(cx).dupe();
        let f: FromNs = match resolved_require {
            ResolvedRequire::TypedModule(f) => Some(f),
            ResolvedRequire::UncheckedModule(_) => None,
            ResolvedRequire::MissingModule | ResolvedRequire::GlobalLibdefModule(_) => None,
        };
        (loc.dupe(), f)
    }

    fn mk_es_module_t<'cx>(
        cx: &mut Context<'cx>,
        module_reason: Reason,
        module_is_strict: bool,
        module_available_platforms: Option<flow_common::platform_set::PlatformSet>,
    ) -> ModuleType {
        let module_export_types = type_::ExportTypes {
            value_exports_tmap: cx.make_export_map(type_::exports::T::new()),
            type_exports_tmap: cx.make_export_map(type_::exports::T::new()),
            cjs_export: None,
            has_every_named_export: false,
        };
        ModuleType::new(ModuleTypeInner {
            module_reason,
            module_export_types,
            module_is_strict,
            module_available_platforms,
        })
    }

    fn copy_named_exports_star<'cx>(
        cx: &mut Context<'cx>,
        target_module_type: &ModuleType,
        (_, from_ns): &(ALoc, FromNs),
    ) {
        match from_ns {
            None => (),
            Some(f) => {
                let source = f(cx);
                annotation_inference::copy_named_exports(cx, source, target_module_type);
            }
        }
    }

    fn copy_type_exports_star<'cx>(
        cx: &mut Context<'cx>,
        reason: &Reason,
        target_module_type: &ModuleType,
        (loc, from_ns): &(ALoc, FromNs),
    ) {
        match from_ns {
            None => (),
            Some(f) => {
                let reason = reason.dupe().reposition(loc.dupe());
                let source = f(cx);
                annotation_inference::copy_type_exports(cx, source, reason, target_module_type);
            }
        }
    }

    fn copy_star_exports<'cx>(
        cx: &mut Context<'cx>,
        reason: &Reason,
        xs: &[(ALoc, FromNs)],
        ys: &[(ALoc, FromNs)],
        target_module_type: &ModuleType,
    ) {
        let mut xs = xs;
        let mut ys = ys;
        loop {
            match (xs, ys) {
                ([], []) => break,
                (xs, []) => {
                    for x in xs {
                        copy_named_exports_star(cx, target_module_type, x);
                    }
                    break;
                }
                ([], ys) => {
                    for y in ys {
                        copy_type_exports_star(cx, reason, target_module_type, y);
                    }
                    break;
                }
                ([x, xs_ @ ..], [y, ys_ @ ..]) => {
                    if x.0.cmp(&y.0) == std::cmp::Ordering::Greater {
                        copy_named_exports_star(cx, target_module_type, x);
                        xs = xs_;
                    } else {
                        copy_type_exports_star(cx, reason, target_module_type, y);
                        ys = ys_;
                    }
                }
            }
        }
    }

    match exports {
        Exports::CJSExports {
            type_exports,
            exports,
            type_stars,
            strict,
            platform_availability_set,
        } => {
            let (def_loc_opt, exports_t) = match exports {
                Some(lazy_val) => {
                    let (def_loc_opt, t) = lazy_val.get_forced(cx).clone();
                    (def_loc_opt, t)
                }
                None => (
                    None,
                    obj_type::mk_with_proto(
                        cx,
                        reason.dupe(),
                        type_::ObjKind::Exact,
                        None,
                        None,
                        None,
                        None,
                        Type::new(type_::TypeInner::ObjProtoT(reason.dupe())),
                        false,
                    ),
                ),
            };
            let type_exports_map: type_::exports::T = type_exports
                .iter()
                .map(|(key, lazy_val)| (Name::new(key.dupe()), lazy_val.get_forced(cx).clone()))
                .collect();
            let type_stars: Vec<(ALoc, FromNs)> =
                type_stars.iter().map(|s| merge_star(cx, file, s)).collect();
            let reason2 = reason.dupe();
            Rc::new(flow_lazy::Lazy::new(move |cx: &mut Context<'_>| {
                let lazy_module = mk_commonjs_module_t(
                    cx,
                    reason2.dupe(),
                    strict,
                    platform_availability_set,
                    def_loc_opt,
                    exports_t,
                );
                let module_type = lazy_module.get_forced(cx).dupe();
                flow_js_utils::export_named_t_kit::mod_module_t(
                    cx,
                    type_::exports::T::new(),
                    type_exports_map,
                    type_::ExportKind::DirectExport,
                    &module_type,
                );
                copy_star_exports(cx, &reason2, &[], &type_stars, &module_type);
                module_type
            }))
        }
        Exports::ESExports {
            type_exports,
            exports,
            ts_pending,
            stars,
            type_stars,
            strict,
            platform_availability_set,
        } => {
            let mut exports_map: type_::exports::T = exports
                .iter()
                .map(|(key, lazy_val)| (Name::new(key.dupe()), lazy_val.get_forced(cx).clone()))
                .collect();
            let mut type_exports_map: type_::exports::T = type_exports
                .iter()
                .map(|(key, lazy_val)| (Name::new(key.dupe()), lazy_val.get_forced(cx).clone()))
                .collect();
            for (name, lazy_entry) in &ts_pending {
                match lazy_entry.get_forced(cx) {
                    TsPendingClassified::TsPendingType(sym) => {
                        type_exports_map.insert(Name::new(name.dupe()), sym.clone());
                    }
                    TsPendingClassified::TsPendingValue(sym) => {
                        exports_map.insert(Name::new(name.dupe()), sym.clone());
                    }
                }
            }
            let stars: Vec<(ALoc, FromNs)> =
                stars.iter().map(|s| merge_star(cx, file, s)).collect();
            let type_stars: Vec<(ALoc, FromNs)> =
                type_stars.iter().map(|s| merge_star(cx, file, s)).collect();
            let reason2 = reason.dupe();
            Rc::new(flow_lazy::Lazy::new(move |cx: &mut Context<'_>| {
                let module_type =
                    mk_es_module_t(cx, reason2.dupe(), strict, platform_availability_set);
                flow_js_utils::export_named_t_kit::mod_module_t(
                    cx,
                    exports_map,
                    type_exports_map,
                    type_::ExportKind::DirectExport,
                    &module_type,
                );
                copy_star_exports(cx, &reason2, &stars, &type_stars, &module_type);
                module_type
            }))
        }
    }
}

#[derive(Clone, Dupe)]
struct MergeEnv {
    tps: FlowOrdMap<FlowSmolStr, Type>,
    infer_tps: FlowOrdMap<FlowSmolStr, (ALoc, Type)>,
    in_no_infer: bool,
    in_renders_arg: bool,
}

fn mk_merge_env(tps: FlowOrdMap<FlowSmolStr, Type>) -> MergeEnv {
    MergeEnv {
        tps,
        infer_tps: FlowOrdMap::new(),
        in_no_infer: false,
        in_renders_arg: false,
    }
}

fn merge_overloaded_methods<M>(
    ms: &Vec1<M>,
    merge_one: impl FnMut(&M) -> Type,
    id_loc: impl Fn(&M) -> &ALoc,
) -> type_::Property {
    let key_loc = id_loc(&ms[0]).dupe();
    let mut iter = ms.iter().map(merge_one);
    let t0 = iter.next().expect("Vec1 always has at least one element");
    let type_ = match iter.next() {
        None => t0,
        Some(t1) => {
            let rest: Vec<Type> = iter.collect();
            let reason = type_util::reason_of_t(&t0).dupe();
            Type::new(type_::TypeInner::IntersectionT(
                reason,
                type_::inter_rep::make(t0, t1, rest.into()),
            ))
        }
    };
    type_::Property::new(type_::PropertyInner::Method {
        key_loc: Some(key_loc),
        type_,
    })
}

fn merge_impl<'cx>(
    env: &MergeEnv,
    cx: &mut Context<'cx>,
    file: &SigFile,
    packed: &Pack::Packed<ALoc>,
    as_const: bool,
    const_decl: bool,
) -> Type {
    use flow_common::reason::VirtualReasonDesc::*;

    match packed {
        Pack::Packed::Annot(t) => merge_annot(env, cx, file, t),
        Pack::Packed::Value(t) => merge_value(env, cx, file, t, as_const, const_decl),
        // Let's have the property retain the precise type in
        // export const FOO = "foo";
        // export const OBJ = { FOO } as const;
        Pack::Packed::Ref(ref_) => merge_ref(
            cx,
            file,
            |_, t, _ref_loc, _def_loc, _name| t,
            ref_,
            as_const || const_decl,
        ),
        Pack::Packed::TyRef(name) => {
            let in_renders_arg = env.in_renders_arg;
            merge_tyref(
                cx,
                file,
                |cx, t, ref_loc, names| {
                    let (name, _) = (names.first().duped().unwrap_or_default(), &names[1..]);
                    let reason = reason::mk_annot_reason(RType(name), ref_loc.dupe());
                    let type_t_kind = if in_renders_arg {
                        type_::TypeTKind::RenderTypeKind
                    } else {
                        type_::TypeTKind::TypeAliasKind
                    };
                    annotation_inference::mk_type_reference(cx, type_t_kind, reason, t.dupe())
                },
                name,
            )
        }
        Pack::Packed::TyRefApp(box Pack::PackedTyRefApp { loc, name, targs }) => {
            let targs: Vec<Type> = targs
                .iter()
                .map(|t| merge_impl(env, cx, file, t, false, false))
                .collect();
            let loc = loc.dupe();
            merge_tyref(
                cx,
                file,
                move |_, t, _ref_loc, _names| {
                    type_util::typeapp_annot(false, false, loc.dupe(), t.dupe(), targs)
                },
                name,
            )
        }
        Pack::Packed::AsyncVoidReturn(box loc) => async_void_return(cx, loc.dupe()),
        Pack::Packed::Pattern(i) => {
            let __rc = file.patterns.get(*i).dupe();
            __rc.get_forced(cx).dupe()
        }
        Pack::Packed::Err(box loc) => {
            type_::any_t::at(type_::AnySource::AnyError(None), loc.dupe())
        }
        Pack::Packed::Eval(box Pack::PackedEval { loc, packed: t, op }) => {
            let (eval_as_const, eval_const_decl) = match op {
                Op::Unary(flow_parser::ast::expression::UnaryOperator::Minus)
                | Op::Unary(flow_parser::ast::expression::UnaryOperator::Not) => {
                    (as_const, const_decl)
                }
                _ => (false, false),
            };
            let merged_t = merge_impl(env, cx, file, t, eval_as_const, eval_const_decl);
            let merged_op = merge_op(env, cx, file, op);
            eval(cx, loc.dupe(), merged_t, merged_op)
        }
        Pack::Packed::Require(box Pack::PackedLocIndex { loc, index }) => {
            require(cx, file, loc.dupe(), *index, false)
        }
        Pack::Packed::ImportDynamic(box Pack::PackedLocIndex { loc, index }) => {
            let mref = file.dependencies.get(*index).0.dupe();
            let ns_reason = reason::mk_reason(RModule(mref.dupe()), loc.dupe());
            let name = mref.dupe().into_inner();
            let ns_t = import_ns(cx, file, ns_reason, &name, loc.dupe(), index.clone());
            let reason = reason::mk_annot_reason(None, loc.dupe());
            flow_js_utils::lookup_builtin_typeapp(cx, reason, "Promise", vec![ns_t])
        }
        Pack::Packed::ModuleRef(box Pack::PackedLocIndex { loc, index }) => {
            let t = require(cx, file, loc.dupe(), *index, true);
            let reason = reason::mk_reason(None, loc.dupe());
            flow_js_utils::lookup_builtin_typeapp(cx, reason, "$Flow$ModuleRef", vec![t])
        }
        Pack::Packed::ImportTypeAnnot(box Pack::PackedLocIndex { loc, index }) => {
            let mref = file.dependencies.get(*index).0.dupe();
            let ns_reason = reason::mk_reason(RModule(mref.dupe()), loc.dupe());
            let name = mref.dupe().into_inner();
            import_ns(cx, file, ns_reason, &name, loc.dupe(), *index)
        }
    }
}

fn resolve_computed_key<'cx>(
    env: &MergeEnv,
    cx: &mut Context<'cx>,
    file: &SigFile,
    key: &Pack::Packed<ALoc>,
) -> Option<Name> {
    // Merge the key as a `const` declaration would be read, so `const k = 'a'`
    // gives the literal `'a'` and not the general `string`. The checking
    // pipeline reads a computed key the same way, and a key that widened to
    // `string` would name no property at all.
    let key_t = merge_impl(env, cx, file, key, false, true);
    match cx.find_resolved(&key_t) {
        Some(resolved_t) => match flow_js_utils::propref_for_elem_t(cx, &resolved_t) {
            type_::PropRef::Named { name, .. } => Some(name),
            type_::PropRef::Computed(_) => None,
        },
        None => None,
    }
}

/// A source location that orders an object-type member against the others.
/// Members do not overlap in source, so any interior location orders them,
/// except a getter and setter pair, which is two members sharing a name. Its
/// last location is used, so a computed key only wins when it follows both
/// halves. A computed key written between them is still read as following
/// both, which is the one ordering this cannot reproduce.
fn obj_annot_prop_source_loc<T>(prop: &ObjAnnotProp<ALoc, T>) -> ALoc {
    match prop {
        ObjAnnotProp::ObjAnnotField(box (loc, _, _)) => loc.dupe(),
        ObjAnnotProp::ObjAnnotAccess(box accessor) => match accessor {
            Accessor::Get(box (loc, _)) | Accessor::Set(box (loc, _)) => loc.dupe(),
            Accessor::GetSet(box (get_loc, _, set_loc, _)) => {
                if get_loc.quick_compare(set_loc).is_gt() {
                    get_loc.dupe()
                } else {
                    set_loc.dupe()
                }
            }
        },
        ObjAnnotProp::ObjAnnotMethod(box data) => data.id_loc.dupe(),
    }
}

/// Fold a computed-key member into the property map. An ordinary property of
/// the same name wins when it comes later in source, matching the in-file
/// checker, which folds every member in one source-order pass. `prop_locs`
/// holds the source location of the current occupant of each name.
fn insert_computed_prop(
    props_smap: &mut BTreeMap<Name, type_::Property>,
    prop_locs: &mut BTreeMap<Name, ALoc>,
    name: Name,
    prop: type_::Property,
    loc: ALoc,
) {
    if let Some(existing) = prop_locs.get(&name)
        && existing.quick_compare(&loc).is_gt()
    {
        return;
    }
    match props_smap.entry(name.dupe()) {
        std::collections::btree_map::Entry::Vacant(e) => {
            e.insert(prop);
        }
        std::collections::btree_map::Entry::Occupied(mut e) => {
            let merged = merge_overloaded_property(e.get(), prop);
            e.insert(merged);
        }
    }
    prop_locs.insert(name, loc);
}

/// A source location that orders an interface or `declare class` member against
/// the others, the way `obj_annot_prop_source_loc` orders an object-type one. A
/// field may carry no location, in which case it does not order at all and a
/// computed key of the same name wins over it.
fn interface_prop_source_loc<T>(prop: &InterfaceProp<ALoc, T>) -> Option<ALoc> {
    match prop {
        InterfaceProp::InterfaceField(box (loc, _, _)) => loc.dupe(),
        InterfaceProp::InterfaceAccess(box accessor) => Some(match accessor {
            Accessor::Get(box (loc, _)) | Accessor::Set(box (loc, _)) => loc.dupe(),
            Accessor::GetSet(box (get_loc, _, set_loc, _)) => {
                if get_loc.quick_compare(set_loc).is_gt() {
                    get_loc.dupe()
                } else {
                    set_loc.dupe()
                }
            }
        }),
        // Overloads are written under one name, so the last of them is where
        // that name was last written.
        InterfaceProp::InterfaceMethod(box overloads) => Some(overloads.last().0.dupe()),
    }
}

/// Record where a plain member's name was written, keeping the later location
/// when the same name holds more than one member of the map.
fn record_prop_loc<K: Ord + Dupe, T>(
    prop_locs: &mut BTreeMap<K, ALoc>,
    name: &K,
    prop: &InterfaceProp<ALoc, T>,
) {
    let Some(loc) = interface_prop_source_loc(prop) else {
        return;
    };
    if prop_locs
        .get(name)
        .is_none_or(|existing| loc.quick_compare(existing).is_gt())
    {
        prop_locs.insert(name.dupe(), loc);
    }
}

/// Fold a computed-key member of an interface or a `declare class` into the
/// name-keyed map its plain members were folded into. The in-file checker folds
/// every member in one source-order pass, where a field, a getter or a setter
/// replaces whatever else held the name, so the member written last in source
/// wins here too. Two methods are the exception: they are overloads of one
/// member whichever order they come in. `prop_locs` holds the source location
/// of the current occupant of each name.
///
/// `sibling` is the other map a name can be taken from, and is given only where
/// the checker takes it: an interface is structural, so one name holds one
/// member across its own and proto sides. A `declare class` is not, and keeps
/// each side to itself.
fn insert_computed_class_prop<K: Ord + Dupe>(
    target: &mut BTreeMap<K, type_::Property>,
    sibling: Option<&mut BTreeMap<K, type_::Property>>,
    prop_locs: &mut BTreeMap<K, ALoc>,
    name: K,
    prop: type_::Property,
    loc: Option<ALoc>,
) {
    fn is_method(prop: &type_::Property) -> bool {
        matches!(&**prop, type_::PropertyInner::Method { .. })
    }

    if let std::collections::btree_map::Entry::Occupied(mut e) = target.entry(name.dupe())
        && is_method(e.get())
        && is_method(&prop)
    {
        let merged = merge_overloaded_property(e.get(), prop);
        e.insert(merged);
        return;
    }
    if let Some(loc) = &loc
        && let Some(existing) = prop_locs.get(&name)
        && existing.quick_compare(loc).is_gt()
    {
        return;
    }
    if let Some(sibling) = sibling {
        sibling.remove(&name);
    }
    target.insert(name.dupe(), prop);
    if let Some(loc) = loc {
        prop_locs.insert(name, loc);
    }
}

fn merge_overloaded_property(
    existing_prop: &type_::Property,
    new_prop: type_::Property,
) -> type_::Property {
    match (&**existing_prop, &*new_prop) {
        (
            type_::PropertyInner::Method {
                type_: existing_t, ..
            },
            type_::PropertyInner::Method {
                key_loc,
                type_: new_t,
            },
        ) => {
            let reason = type_util::reason_of_t(new_t).dupe();
            type_::Property::new(type_::PropertyInner::Method {
                key_loc: key_loc.dupe(),
                type_: Type::new(type_::TypeInner::IntersectionT(
                    reason,
                    type_::inter_rep::make(existing_t.dupe(), new_t.dupe(), vec![].into()),
                )),
            })
        }
        _ => new_prop,
    }
}

/// If `prop` is a field whose type is an unnamed `unique symbol`, attach `name` as
/// the symbol's display name so user-facing rendering shows `[name]` instead of a
/// generic `[symbol]`. The symbol's nominal identity (its `ALocId`) is unchanged,
/// so this only affects display, never property lookup.
fn name_symbol_field(prop: type_::Property, name: &FlowSmolStr) -> type_::Property {
    use std::ops::Deref;
    if let type_::PropertyInner::Field(fd) = prop.deref()
        && let type_::TypeInner::DefT(reason, def) = fd.type_.deref()
        && let type_::DefTInner::UniqueSymbolT(data) = def.deref()
        && data.symbol.name().is_none()
    {
        let named = flow_common::reason::SymbolID::with_name(
            data.symbol.aloc_id().dupe(),
            Some(name.dupe()),
        );
        let type_ = Type::new(type_::TypeInner::DefT(
            reason.dupe(),
            type_::DefT::new(type_::DefTInner::UniqueSymbolT(type_::UniqueSymbolTData {
                symbol: named,
                from_annot: data.from_annot,
            })),
        ));
        return type_::Property::new(type_::PropertyInner::Field(Box::new(type_::FieldData {
            preferred_def_locs: fd.preferred_def_locs.clone(),
            key_loc: fd.key_loc.dupe(),
            type_,
            polarity: fd.polarity,
        })));
    }
    prop
}

fn merge_annot<'cx>(
    env: &MergeEnv,
    cx: &mut Context<'cx>,
    file: &SigFile,
    annot: &Pack::PackedAnnot<ALoc>,
) -> Type {
    use flow_common::reason::VirtualReasonDesc::*;
    use flow_type_sig::type_sig::Annot;
    match annot {
        Annot::Any(box loc) => type_::any_t::at(type_::AnySource::AnnotatedAny, loc.dupe()),
        Annot::Mixed(box loc) => type_::mixed_t::at(loc.dupe()),
        Annot::Empty(box loc) => type_::empty_t::at(loc.dupe()),
        Annot::Void(box loc) => type_::void::at(loc.dupe()),
        Annot::Null(box loc) => type_::null::at(loc.dupe()),
        Annot::Symbol(box loc) => type_::symbol_t::at(loc.dupe()),
        Annot::UniqueSymbol(box loc) => {
            type_::unique_symbol_t::at(cx.make_aloc_id(loc), loc.dupe(), None)
        }
        Annot::Number(box loc) => type_::num_module_t::at(loc.dupe()),
        Annot::BigInt(box loc) => type_::bigint_module_t::at(loc.dupe()),
        Annot::String(box loc) => type_::str_module_t::at(loc.dupe()),
        Annot::Boolean(box loc) => type_::bool_module_t::at(loc.dupe()),
        Annot::Exists(box loc) => type_::any_t::at(type_::AnySource::AnnotatedAny, loc.dupe()),
        Annot::ExactObjectType(box loc) => {
            let reason = reason::mk_annot_reason(
                RType(FlowSmolStr::new_inline("ExactObjectType")),
                loc.dupe(),
            );
            obj_type::mk_exact_object_type(cx, reason)
        }
        Annot::Optional(t) => {
            let t = merge_impl(env, cx, file, t, false, false);
            type_util::optional(t, None, false)
        }
        Annot::Maybe(box (loc, t)) => {
            let t = merge_impl(env, cx, file, t, false, false);
            let desc = type_util::desc_of_t(&t).cloned();
            let reason = reason::mk_annot_reason(desc, loc.dupe());
            Type::new(type_::TypeInner::MaybeT(reason, t))
        }
        Annot::Union(inner) => {
            let AnnotUnion { loc, t0, t1, ts } = inner.as_ref();
            let reason = reason::mk_annot_reason(None, loc.dupe());
            let t0 = merge_impl(env, cx, file, t0, false, false);
            let t1 = merge_impl(env, cx, file, t1, false, false);
            let ts: Vec<Type> = ts
                .iter()
                .map(|t| merge_impl(env, cx, file, t, false, false))
                .collect();
            let source_aloc = Some(cx.make_aloc_id(loc));
            Type::new(type_::TypeInner::UnionT(
                reason,
                type_::union_rep::make(
                    source_aloc,
                    type_::union_rep::UnionKind::UnknownKind,
                    t0,
                    t1,
                    ts.into(),
                ),
            ))
        }
        Annot::Intersection(inner) => {
            let AnnotIntersection { loc, t0, t1, ts } = inner.as_ref();
            let reason = reason::mk_annot_reason(None, loc.dupe());
            let t0 = merge_impl(env, cx, file, t0, false, false);
            let t1 = merge_impl(env, cx, file, t1, false, false);
            let ts: Vec<Type> = ts
                .iter()
                .map(|t| merge_impl(env, cx, file, t, false, false))
                .collect();
            let intersection = Type::new(type_::TypeInner::IntersectionT(
                reason.dupe(),
                type_::inter_rep::make(t0, t1, ts.into()),
            ));
            intersection_kit::defer_annotation(cx, reason, intersection, |cx| {
                eval_id_of_aloc(cx, loc.dupe())
            })
        }
        Annot::Tuple(inner) => {
            let AnnotTuple {
                loc,
                elems,
                inexact,
                strictness_kind,
            } = inner.as_ref();
            let reason = reason::mk_annot_reason(RTupleType, loc.dupe());
            let unresolved: Vec<type_::UnresolvedParam> = elems
                .iter()
                .map(|elem: &type_sig::TupleElement<_, _>| match elem {
                    type_sig::TupleElement::TupleElement(box type_sig::TupleElementData {
                        loc,
                        name,
                        t,
                        polarity,
                        optional,
                    }) => {
                        let reason = reason::mk_reason(None, loc.dupe());
                        let t = merge_impl(env, cx, file, t, false, false);
                        let elem = type_::TupleElement {
                            name: name.dupe(),
                            t,
                            polarity: *polarity,
                            optional: *optional,
                            reason,
                        };
                        type_::UnresolvedParam::UnresolvedArg(Box::new(type_::UnresolvedArgData(
                            elem, None,
                        )))
                    }
                    type_sig::TupleElement::TupleSpread(box type_sig::TupleSpreadData {
                        loc: _,
                        name: _,
                        t,
                    }) => {
                        let t = merge_impl(env, cx, file, t, false, false);
                        type_::UnresolvedParam::UnresolvedSpreadArg(t)
                    }
                })
                .collect();
            let mk_type_destructor = |_cx: &mut Context,
                                      use_op: type_::UseOp,
                                      reason: Reason,
                                      t: Type,
                                      destructor: type_::Destructor,
                                      id: type_::eval::Id|
             -> Result<Type, flow_js_utils::FlowJsException> {
                Ok(Type::new(type_::TypeInner::EvalT {
                    type_: t,
                    defer_use_t: type_::TypeDestructorT::new(type_::TypeDestructorTInner(
                        use_op,
                        reason,
                        Rc::new(destructor),
                    )),
                    id,
                }))
            };
            let id = eval_id_of_aloc(cx, loc.dupe());
            flow_js_utils::mk_tuple_type(
                cx,
                *strictness_kind,
                id,
                mk_type_destructor,
                *inexact,
                reason,
                unresolved,
            )
            .unwrap()
        }
        Annot::Array(box (loc, t)) => {
            let reason = reason::mk_annot_reason(RArrayType, loc.dupe());
            let elem_t = merge_impl(env, cx, file, t, false, false);
            Type::new(type_::TypeInner::DefT(
                reason,
                type_::DefT::new(type_::DefTInner::ArrT(Rc::new(type_::ArrType::ArrayAT(
                    Box::new(ArrayATData {
                        elem_t,
                        tuple_view: None,
                        react_dro: None,
                        strictness_kind: TypeStrictnessKind::from_is_typescript(
                            loc.source().is_some_and(flow_common::files::has_ts_ext),
                        ),
                        literal: false,
                        fresh: false,
                    }),
                )))),
            ))
        }
        Annot::ReadOnlyArray(box (loc, t)) => {
            let reason = reason::mk_annot_reason(RROArrayType, loc.dupe());
            let t = merge_impl(env, cx, file, t, false, false);
            Type::new(type_::TypeInner::DefT(
                reason,
                type_::DefT::new(type_::DefTInner::ArrT(Rc::new(type_::ArrType::ROArrayAT(
                    Box::new(ROArrayATData {
                        elem_t: t,
                        react_dro: None,
                        literal: false,
                    }),
                )))),
            ))
        }
        Annot::SingletonString(box (loc, str)) => {
            let reason = reason::mk_annot_reason(RStringLit(str.dupe()), loc.dupe());
            Type::new(type_::TypeInner::DefT(
                reason,
                type_::DefT::new(type_::DefTInner::SingletonStrT {
                    from_annot: true,
                    value: str.dupe(),
                }),
            ))
        }
        Annot::SingletonNumber(box (loc, num, raw)) => {
            let reason = reason::mk_annot_reason(RNumberLit(raw.dupe()), loc.dupe());
            Type::new(type_::TypeInner::DefT(
                reason,
                type_::DefT::new(type_::DefTInner::SingletonNumT {
                    from_annot: true,
                    value: type_::NumberLiteral(*num, raw.dupe()),
                }),
            ))
        }
        Annot::SingletonBigInt(box (loc, bigint, raw)) => {
            let reason = reason::mk_annot_reason(RBigIntLit(raw.dupe()), loc.dupe());
            Type::new(type_::TypeInner::DefT(
                reason,
                type_::DefT::new(type_::DefTInner::SingletonBigIntT {
                    from_annot: true,
                    value: type_::BigIntLiteral(*bigint, raw.dupe()),
                }),
            ))
        }
        Annot::SingletonBoolean(box (loc, b)) => {
            let reason = reason::mk_annot_reason(RBooleanLit(*b), loc.dupe());
            Type::new(type_::TypeInner::DefT(
                reason,
                type_::DefT::new(type_::DefTInner::SingletonBoolT {
                    from_annot: true,
                    value: *b,
                }),
            ))
        }
        Annot::Uppercase(inner) => {
            let (loc, t) = inner.as_ref();
            let t = merge_impl(env, cx, file, t, false, false);
            flow_typing_flow_common::string_case_transform::resolve(
                None,
                cx,
                type_::StringMappingKind::StringMappingUppercase,
                loc.dupe(),
                t,
            )
        }
        Annot::Lowercase(inner) => {
            let (loc, t) = inner.as_ref();
            let t = merge_impl(env, cx, file, t, false, false);
            flow_typing_flow_common::string_case_transform::resolve(
                None,
                cx,
                type_::StringMappingKind::StringMappingLowercase,
                loc.dupe(),
                t,
            )
        }
        Annot::Capitalize(inner) => {
            let (loc, t) = inner.as_ref();
            let t = merge_impl(env, cx, file, t, false, false);
            flow_typing_flow_common::string_case_transform::resolve(
                None,
                cx,
                type_::StringMappingKind::StringMappingCapitalize,
                loc.dupe(),
                t,
            )
        }
        Annot::Uncapitalize(inner) => {
            let (loc, t) = inner.as_ref();
            let t = merge_impl(env, cx, file, t, false, false);
            flow_typing_flow_common::string_case_transform::resolve(
                None,
                cx,
                type_::StringMappingKind::StringMappingUncapitalize,
                loc.dupe(),
                t,
            )
        }
        Annot::TemplateLiteral(inner) => {
            let AnnotTemplateLiteral { loc, quasis, types } = inner.as_ref();
            let ts = types
                .iter()
                .map(|t| merge_impl(env, cx, file, t, false, false))
                .collect();
            flow_typing_flow_js::template_literal_type::resolve(quasis.clone(), ts, loc.dupe(), cx)
        }
        Annot::Typeof(inner) => {
            let AnnotTypeof {
                loc,
                qname,
                t,
                targs,
            } = inner.as_ref();
            let qname_str: FlowSmolStr = qname.join(".").into();
            let reason = reason::mk_reason(RTypeof(qname_str.dupe()), loc.dupe());
            let t = merge_impl(env, cx, file, t, false, true);
            let targs = targs.as_ref().map(|ts| {
                (
                    qname_str,
                    ts.iter()
                        .map(|t| merge_impl(env, cx, file, t, false, false))
                        .collect(),
                )
            });
            type_util::typeof_annotation(reason, t, targs)
        }
        Annot::Bound(inner) => {
            let AnnotBound { ref_loc, name } = inner.as_ref();
            let t = match env.infer_tps.get(name) {
                Some((name_loc, t)) if name_loc == ref_loc => t.dupe(),
                _ => {
                    let t = env
                        .tps
                        .get(name)
                        .unwrap_or_else(|| {
                            panic!("merge_annot Bound: name '{}' not found in tps", name)
                        })
                        .dupe();
                    if env.in_no_infer {
                        match &*t {
                            type_::TypeInner::GenericT(box GenericTData {
                                reason,
                                name,
                                bound,
                                no_infer: _,
                                id,
                            }) => Type::new(type_::TypeInner::GenericT(Box::new(GenericTData {
                                reason: reason.dupe(),
                                name: name.dupe(),
                                bound: bound.dupe(),
                                no_infer: true,
                                id: id.clone(),
                            }))),
                            _ => t,
                        }
                    } else {
                        t
                    }
                }
            };
            type_util::mod_reason_of_t(&|r| r.dupe().reposition(ref_loc.dupe()), &t)
        }
        Annot::NoInfer(t) => {
            let mut new_env = env.dupe();
            new_env.in_no_infer = true;
            merge_impl(&new_env, cx, file, t, false, false)
        }
        Annot::PropertyType(inner) => {
            let AnnotPropertyType { loc, obj, prop } = inner.as_ref();
            let reason = reason::mk_reason(RType("$PropertyType".into()), loc.dupe());
            let use_op = type_::UseOp::Op(Arc::new(type_::RootUseOp::TypeApplication {
                type_: type_::TypeApplicationTarget::Reason(reason.dupe()),
            }));
            let obj = merge_impl(env, cx, file, obj, false, false);
            let id = eval_id_of_aloc(cx, loc.dupe());
            Type::new(type_::TypeInner::EvalT {
                type_: obj,
                defer_use_t: type_::TypeDestructorT::new(type_::TypeDestructorTInner(
                    use_op,
                    reason,
                    Rc::new(type_::Destructor::PropertyType {
                        name: Name::new(prop.dupe()),
                    }),
                )),
                id,
            })
        }
        Annot::ElementType(inner) => {
            let AnnotElementType { loc, obj, elem } = inner.as_ref();
            let reason = reason::mk_reason(RType("$ElementType".into()), loc.dupe());
            let use_op = type_::UseOp::Op(Arc::new(type_::RootUseOp::TypeApplication {
                type_: type_::TypeApplicationTarget::Reason(reason.dupe()),
            }));
            let obj = merge_impl(env, cx, file, obj, false, false);
            let index_type = merge_impl(env, cx, file, elem, false, false);
            let id = eval_id_of_aloc(cx, loc.dupe());
            Type::new(type_::TypeInner::EvalT {
                type_: obj,
                defer_use_t: type_::TypeDestructorT::new(type_::TypeDestructorTInner(
                    use_op,
                    reason,
                    Rc::new(type_::Destructor::ElementType { index_type }),
                )),
                id,
            })
        }
        Annot::EnumValue(box (loc, t)) => {
            let reason = reason::mk_annot_reason(REnum { name: None }, loc.dupe());
            let representation_t = merge_impl(env, cx, file, t, false, false);
            Type::new(type_::TypeInner::DefT(
                reason,
                type_::DefT::new(type_::DefTInner::EnumValueT(Rc::new(type_::EnumInfo::new(
                    type_::EnumInfoInner::AbstractEnum { representation_t },
                )))),
            ))
        }
        Annot::Enum(box (loc, t)) => {
            let reason = reason::mk_annot_reason(REnum { name: None }, loc.dupe());
            let use_op = type_::UseOp::Op(Arc::new(type_::RootUseOp::TypeApplication {
                type_: type_::TypeApplicationTarget::Reason(reason.dupe()),
            }));
            let t = merge_impl(env, cx, file, t, false, false);
            let id = eval_id_of_aloc(cx, loc.dupe());
            Type::new(type_::TypeInner::EvalT {
                type_: t,
                defer_use_t: type_::TypeDestructorT::new(type_::TypeDestructorTInner(
                    use_op,
                    reason,
                    Rc::new(type_::Destructor::EnumType),
                )),
                id,
            })
        }
        Annot::OptionalIndexedAccessNonMaybeType(inner) => {
            let AnnotOptionalIndexedAccessNonMaybeType { loc, obj, index } = inner.as_ref();
            let reason = reason::mk_reason(None, loc.dupe());
            let object_type = merge_impl(env, cx, file, obj, false, false);
            let index_type = merge_impl(env, cx, file, index, false, false);
            let use_op = type_::UseOp::Op(Arc::new(type_::RootUseOp::IndexedTypeAccess(Box::new(
                type_::IndexedTypeAccessData {
                    index_loc: type_util::loc_of_t(&index_type).dupe(),
                    object: flow_js_utils::type_or_type_desc_for_error(&object_type),
                    index: flow_js_utils::type_or_type_desc_for_error(&index_type),
                },
            ))));
            let id = eval_id_of_aloc(cx, loc.dupe());
            let oia_index = match index {
                Pack::Packed::Annot(annot) => match annot.as_ref() {
                    Annot::SingletonString(box (_, str)) => {
                        type_::OptionalIndexedAccessIndex::OptionalIndexedAccessStrLitIndex(
                            Name::new(str.dupe()),
                        )
                    }
                    _ => type_::OptionalIndexedAccessIndex::OptionalIndexedAccessTypeIndex(
                        index_type,
                    ),
                },
                _ => type_::OptionalIndexedAccessIndex::OptionalIndexedAccessTypeIndex(index_type),
            };
            Type::new(type_::TypeInner::EvalT {
                type_: object_type,
                defer_use_t: type_::TypeDestructorT::new(type_::TypeDestructorTInner(
                    use_op,
                    reason,
                    Rc::new(type_::Destructor::OptionalIndexedAccessNonMaybeType {
                        index: oia_index,
                    }),
                )),
                id,
            })
        }
        Annot::OptionalIndexedAccessResultType(inner) => {
            let AnnotOptionalIndexedAccessResultType {
                loc,
                non_maybe_result,
                void_loc,
            } = inner.as_ref();
            let reason = reason::mk_reason(None, loc.dupe());
            let void_reason = reason::mk_reason(RVoid, void_loc.dupe());
            let non_maybe_result_type = merge_impl(env, cx, file, non_maybe_result, false, false);
            Type::new(type_::TypeInner::EvalT {
                type_: non_maybe_result_type,
                defer_use_t: type_::TypeDestructorT::new(type_::TypeDestructorTInner(
                    type_::unknown_use(),
                    reason,
                    Rc::new(type_::Destructor::OptionalIndexedAccessResultType { void_reason }),
                )),
                id: type_::eval::Id::generate_id(),
            })
        }
        Annot::NonMaybeType(box (loc, t)) => {
            let reason = reason::mk_reason(RType("$NonMaybeType".into()), loc.dupe());
            let use_op = type_::UseOp::Op(Arc::new(type_::RootUseOp::TypeApplication {
                type_: type_::TypeApplicationTarget::Reason(reason.dupe()),
            }));
            let t = merge_impl(env, cx, file, t, false, false);
            let id = eval_id_of_aloc(cx, loc.dupe());
            Type::new(type_::TypeInner::EvalT {
                type_: t,
                defer_use_t: type_::TypeDestructorT::new(type_::TypeDestructorTInner(
                    use_op,
                    reason,
                    Rc::new(type_::Destructor::NonMaybeType),
                )),
                id,
            })
        }
        Annot::Omit(box (loc, t1, t2)) => {
            let reason = reason::mk_reason(RType("Omit".into()), loc.dupe());
            let use_op = type_::UseOp::Op(Arc::new(type_::RootUseOp::TypeApplication {
                type_: type_::TypeApplicationTarget::Reason(reason.dupe()),
            }));
            let t1 = merge_impl(env, cx, file, t1, false, false);
            let t2 = merge_impl(env, cx, file, t2, false, false);
            let id = eval_id_of_aloc(cx, loc.dupe());
            let t2 = Type::new(type_::TypeInner::EvalT {
                type_: t2,
                defer_use_t: type_::TypeDestructorT::new(type_::TypeDestructorTInner(
                    type_::unknown_use(),
                    reason.dupe(),
                    Rc::new(type_::Destructor::MappedType(Box::new(
                        DestructorMappedTypeData {
                            homomorphic: type_::MappedTypeHomomorphicFlag::Unspecialized,
                            property_type: type_::mixed_t::make(reason.dupe()),
                            name_type: None,
                            mapped_type_flags: type_::MappedTypeFlags {
                                optional: type_::MappedTypeOptionality::KeepOptionality,
                                variance: type_::MappedTypeVariance::KeepVariance,
                            },
                            distributive_tparam_name: None,
                        },
                    ))),
                )),
                id: type_::eval::Id::generate_id(),
            });
            Type::new(type_::TypeInner::EvalT {
                type_: t1,
                defer_use_t: type_::TypeDestructorT::new(type_::TypeDestructorTInner(
                    use_op,
                    reason,
                    Rc::new(type_::Destructor::RestType(
                        type_::object::rest::MergeMode::Omit,
                        t2,
                    )),
                )),
                id,
            })
        }
        Annot::ReadOnly(box (loc, t)) => {
            let reason = reason::mk_reason(None, loc.dupe());
            let use_op = type_::UseOp::Op(Arc::new(type_::RootUseOp::MakeReadOnly {
                loc: Box::new(loc.dupe()),
            }));
            let t = merge_impl(env, cx, file, t, false, false);
            let id = eval_id_of_aloc(cx, loc.dupe());
            Type::new(type_::TypeInner::EvalT {
                type_: t,
                defer_use_t: type_::TypeDestructorT::new(type_::TypeDestructorTInner(
                    use_op,
                    reason,
                    Rc::new(type_::Destructor::ReadOnlyType),
                )),
                id,
            })
        }
        Annot::Partial(box (loc, t)) => {
            let t = merge_impl(env, cx, file, t, false, false);
            let reason = reason::mk_reason(
                type_util::desc_of_t(&t)
                    .cloned()
                    .map(|desc| RPartialOf(Arc::new(desc))),
                loc.dupe(),
            );
            let use_op = type_::UseOp::Op(Arc::new(type_::RootUseOp::TypeApplication {
                type_: type_::TypeApplicationTarget::Reason(reason.dupe()),
            }));
            let id = eval_id_of_aloc(cx, loc.dupe());
            Type::new(type_::TypeInner::EvalT {
                type_: t,
                defer_use_t: type_::TypeDestructorT::new(type_::TypeDestructorTInner(
                    use_op,
                    reason,
                    Rc::new(type_::Destructor::PartialType),
                )),
                id,
            })
        }
        Annot::Required(box (loc, t)) => {
            let t = merge_impl(env, cx, file, t, false, false);
            let reason = reason::mk_reason(
                type_util::desc_of_t(&t)
                    .cloned()
                    .map(|desc| RRequiredOf(Arc::new(desc))),
                loc.dupe(),
            );
            let use_op = type_::UseOp::Op(Arc::new(type_::RootUseOp::TypeApplication {
                type_: type_::TypeApplicationTarget::Reason(reason.dupe()),
            }));
            let id = eval_id_of_aloc(cx, loc.dupe());
            Type::new(type_::TypeInner::EvalT {
                type_: t,
                defer_use_t: type_::TypeDestructorT::new(type_::TypeDestructorTInner(
                    use_op,
                    reason,
                    Rc::new(type_::Destructor::RequiredType),
                )),
                id,
            })
        }
        Annot::Keys(box (loc, t)) => {
            let reason = reason::mk_reason(None, loc.dupe());
            let t = merge_impl(env, cx, file, t, false, false);
            Type::new(type_::TypeInner::KeysT(reason, t))
        }
        Annot::Values(box (loc, t)) => {
            let reason = reason::mk_reason(RType("$Values".into()), loc.dupe());
            let use_op = type_::UseOp::Op(Arc::new(type_::RootUseOp::TypeApplication {
                type_: type_::TypeApplicationTarget::Reason(reason.dupe()),
            }));
            let t = merge_impl(env, cx, file, t, false, false);
            let id = eval_id_of_aloc(cx, loc.dupe());
            Type::new(type_::TypeInner::EvalT {
                type_: t,
                defer_use_t: type_::TypeDestructorT::new(type_::TypeDestructorTInner(
                    use_op,
                    reason,
                    Rc::new(type_::Destructor::ValuesType),
                )),
                id,
            })
        }
        Annot::Exact(box (loc, t)) => {
            let t = merge_impl(env, cx, file, t, false, false);
            let desc = type_util::desc_of_t(&t).cloned();
            let reason = reason::mk_annot_reason(desc, loc.dupe());
            let t = type_util::push_type_alias_reason(&reason, t);
            let use_op = type_::UseOp::Op(Arc::new(type_::RootUseOp::TypeApplication {
                type_: type_::TypeApplicationTarget::Reason(reason.dupe()),
            }));
            let id = eval_id_of_aloc(cx, loc.dupe());
            Type::new(type_::TypeInner::EvalT {
                type_: t,
                defer_use_t: type_::TypeDestructorT::new(type_::TypeDestructorTInner(
                    use_op,
                    reason,
                    Rc::new(type_::Destructor::ExactType),
                )),
                id,
            })
        }
        Annot::ExportsT(box (loc, mref)) => {
            let reason = reason::mk_annot_reason(
                reason::VirtualReasonDesc::RModule(mref.dupe()),
                loc.dupe(),
            );
            let symbol = Symbol::mk_module_symbol(mref.dupe().into_inner(), loc.dupe());
            let f: ResolvedRequire = match cx.builtin_module_opt(mref) {
                Some((reason, lazy_module)) => {
                    ResolvedRequire::TypedModule(annotation_inference::force_module_type_thunk(
                        cx.id(),
                        type_::constraint::forcing_state::ModuleTypeForcingState::of_lazy_module(
                            reason,
                            lazy_module,
                        ),
                    ))
                }
                None => {
                    let err_t = flow_js_utils::lookup_builtin_module_error(
                        cx,
                        &mref.dupe().into_inner(),
                        loc.dupe(),
                    )
                    .unwrap();
                    ResolvedRequire::TypedModule(Rc::new(
                        move |_cx: &mut Context| Err(err_t.dupe()),
                    ))
                }
            };
            annotation_inference::cjs_require(cx, reason, symbol, false, false, f)
        }
        Annot::Conditional(inner) => {
            let AnnotConditional {
                loc,
                distributive_tparam,
                infer_tparams,
                check_type,
                extends_type,
                true_type,
                false_type,
            } = inner.as_ref();
            let reason = reason::mk_reason(None, loc.dupe());
            let id = eval_id_of_aloc(cx, loc.dupe());
            let convert = |distributive_tparam_name: Option<SubstName>, env: &MergeEnv| {
                let check_t = merge_impl(env, cx, file, check_type, false, false);
                let (tps_for_true_type, infer_tps_for_extends_types, infer_tparams) = {
                    let tparam_list: Vec<&TParam<ALoc, Pack::Packed<ALoc>>> = match infer_tparams {
                        TParams::Mono => vec![],
                        TParams::Poly(poly) => poly.tparams.iter().collect(),
                    };
                    let (new_tps, new_infer_tps, rev_tparams) = tparam_list.iter().fold(
                        (
                            FlowOrdMap::<FlowSmolStr, Type>::new(),
                            FlowOrdMap::<FlowSmolStr, (ALoc, Type)>::new(),
                            Vec::<type_::TypeParam>::new(),
                        ),
                        |(mut new_tps, mut new_infer_tps, mut rev_tparams), tp| {
                            let name_loc = &tp.name_loc;
                            let (tp_out, (name, _, t, _)) =
                                merge_tparam(&mut env.dupe(), cx, file, tp, true);
                            let name_str = name.string_of_subst_name().dupe();
                            new_tps.insert(name_str.dupe(), t.dupe());
                            // It's possible that for a given conditional extends type scope,
                            // both a regular generic type and infer type exists. e.g.
                            // type Both<T> = string extends [T, infer T] ? ... : ...
                            // To distinguish, we attach additional def_loc information for infer type.
                            new_infer_tps.insert(name_str, (name_loc.dupe(), t));
                            rev_tparams.push(tp_out);
                            (new_tps, new_infer_tps, rev_tparams)
                        },
                    );
                    let mut tps_for_true = env.tps.dupe();
                    for (k, v) in new_tps {
                        tps_for_true.insert(k, v);
                    }
                    let infer_tparams_out = rev_tparams;
                    (tps_for_true, new_infer_tps, infer_tparams_out)
                };
                let extends_env = MergeEnv {
                    infer_tps: infer_tps_for_extends_types,
                    ..env.dupe()
                };
                let extends_t = merge_impl(&extends_env, cx, file, extends_type, false, false);
                let true_env = MergeEnv {
                    tps: tps_for_true_type,
                    ..env.dupe()
                };
                let true_t = merge_impl(&true_env, cx, file, true_type, false, false);
                let false_t = merge_impl(env, cx, file, false_type, false, false);
                Type::new(type_::TypeInner::EvalT {
                    type_: check_t,
                    defer_use_t: type_::TypeDestructorT::new(type_::TypeDestructorTInner(
                        type_::unknown_use(),
                        reason.dupe(),
                        Rc::new(type_::Destructor::ConditionalType(Box::new(
                            DestructorConditionalTypeData {
                                distributive_tparam_name,
                                infer_tparams: infer_tparams.into(),
                                extends_t,
                                true_t,
                                false_t,
                            },
                        ))),
                    )),
                    id,
                })
            };
            match distributive_tparam {
                None => convert(None, env),
                Some(tp) => convert(Some(SubstName::name(tp.name.dupe())), env),
            }
        }
        Annot::ObjKeyMirror(inner) => {
            let AnnotObjKeyMirror { loc, obj } = inner.as_ref();
            let reason = reason::mk_reason(None, loc.dupe());
            let use_op = type_::UseOp::Op(Arc::new(type_::RootUseOp::TypeApplication {
                type_: type_::TypeApplicationTarget::Reason(
                    reason
                        .dupe()
                        .replace_desc(RType(FlowSmolStr::new_inline("$KeyMirror"))),
                ),
            }));
            let obj = merge_impl(env, cx, file, obj, false, false);
            let id = eval_id_of_aloc(cx, loc.dupe());
            Type::new(type_::TypeInner::EvalT {
                type_: obj,
                defer_use_t: type_::TypeDestructorT::new(type_::TypeDestructorTInner(
                    use_op,
                    reason,
                    Rc::new(type_::Destructor::TypeMap(type_::TypeMap::ObjectKeyMirror)),
                )),
                id,
            })
        }
        Annot::ClassT(box (loc, t)) => {
            let t = merge_impl(env, cx, file, t, false, false);
            let reason = reason::mk_reason(None, loc.dupe());
            Type::new(type_::TypeInner::DefT(
                reason,
                type_::DefT::new(type_::DefTInner::ClassT(t)),
            ))
        }
        Annot::Renders(inner) => {
            let AnnotRenders { loc, arg, variant } = inner.as_ref();
            let mut renders_env = env.dupe();
            renders_env.in_renders_arg = true;
            let t = merge_impl(&renders_env, cx, file, arg, false, false);
            let reason =
                reason::mk_annot_reason(type_util::reason_of_t(&t).desc(true).cloned(), loc.dupe());
            match type_util::mk_possibly_generic_render_type(*variant, reason.dupe(), t.dupe()) {
                Some(t) => t,
                None => {
                    let renders_variant = match variant {
                        flow_parser::ast::types::RendersVariant::Normal => {
                            type_::RendersVariant::RendersNormal
                        }
                        flow_parser::ast::types::RendersVariant::Maybe => {
                            type_::RendersVariant::RendersMaybe
                        }
                        flow_parser::ast::types::RendersVariant::Star => {
                            type_::RendersVariant::RendersStar
                        }
                    };
                    annotation_inference::mk_non_generic_render_type(cx, reason, renders_variant, t)
                }
            }
        }
        Annot::ComponentMissingRenders(box loc) => {
            let reason = reason::mk_annot_reason(RType("React.Node".into()), loc.dupe());
            Type::new(type_::TypeInner::DefT(
                reason,
                type_::DefT::new(type_::DefTInner::RendersT(Rc::new(
                    type_::CanonicalRendersForm::DefaultRenders,
                ))),
            ))
        }
        Annot::FunAnnot(box (loc, def)) | Annot::MethodAnnot(box (loc, _, def)) => {
            let reason = reason::mk_annot_reason(RFunctionType, loc.dupe());
            let statics =
                merge_fun_statics(env, cx, file, reason.dupe(), std::iter::empty(), false);
            merge_fun(
                env,
                cx,
                file,
                reason,
                FunctionReferenceKind::FunctionType,
                def,
                statics,
                matches!(annot, Annot::MethodAnnot(_))
                    && (cx.is_global_lib_context() || def.strictness_kind.is_typescript_loose()),
                matches!(annot, Annot::MethodAnnot(box (_, true, _))),
                false,
                matches!(annot, Annot::MethodAnnot(_)),
                false,
            )
        }
        Annot::ComponentAnnot(box (loc, def)) => {
            let reason = reason::mk_annot_reason(None, loc.dupe());
            merge_component(env, cx, file, reason, def, None)
        }
        Annot::ObjAnnot(inner) => {
            let AnnotObjAnnot {
                loc,
                obj_kind,
                props,
                computed_props,
                proto,
                strictness_kind,
            } = inner.as_ref();
            let reason = reason::mk_annot_reason(RObjectType, loc.dupe());
            let obj_kind = match obj_kind {
                type_sig::ObjKind::ExactObj => type_::ObjKind::Exact,
                type_sig::ObjKind::InexactObj => type_::ObjKind::Inexact,
                type_sig::ObjKind::IndexedObj(dict) => {
                    type_::ObjKind::Indexed(merge_dict(env, cx, file, dict, false))
                }
            };
            let mut props_smap: BTreeMap<Name, type_::Property> = BTreeMap::new();
            let mut prop_locs: BTreeMap<Name, ALoc> = BTreeMap::new();
            for (key, prop) in props {
                let name = Name::new(key.dupe());
                let p = merge_obj_annot_prop(env, cx, file, prop);
                let p = name_symbol_field(p, key);
                prop_locs.insert(name.dupe(), obj_annot_prop_source_loc(prop));
                props_smap.insert(name, p);
            }
            for (key, prop) in computed_props {
                if let Some(name) = resolve_computed_key(env, cx, file, key) {
                    let loc = obj_annot_prop_source_loc(prop);
                    let t = merge_obj_annot_prop(env, cx, file, prop);
                    insert_computed_prop(&mut props_smap, &mut prop_locs, name, t, loc);
                }
            }
            let props_map = type_::properties::PropertiesMap::from_btree_map(props_smap);
            let mk_object = |cx: &mut Context<'cx>, call: Option<Type>, proto: Type| {
                let id = type_::properties::Id::of_aloc_id(true, cx.make_aloc_id(loc));
                let flags = type_::Flags {
                    obj_kind: obj_kind.clone(),
                    react_dro: None,
                };
                let call = call.map(|t| cx.make_call_prop(t));
                cx.add_property_map(id.dupe(), props_map.dupe());
                let obj_type = type_::mk_objecttype_with_strictness(
                    Some(flags),
                    None,
                    call,
                    id,
                    proto,
                    *strictness_kind,
                    false,
                );
                if obj_kind == type_::ObjKind::Exact {
                    let reason_op = reason::mk_annot_reason(RObjectType, loc.dupe());
                    type_util::make_exact_object(reason.dupe(), Rc::new(obj_type), &reason_op)
                } else {
                    Type::new(type_::TypeInner::DefT(
                        reason.dupe(),
                        type_::DefT::new(type_::DefTInner::ObjT(Rc::new(obj_type))),
                    ))
                }
            };
            match proto {
                ObjAnnotProto::ObjAnnotImplicitProto => mk_object(
                    cx,
                    None,
                    Type::new(type_::TypeInner::ObjProtoT(reason.dupe())),
                ),
                ObjAnnotProto::ObjAnnotExplicitProto(box (proto_loc, t)) => {
                    let proto_reason = reason::mk_reason(None, proto_loc.dupe());
                    let proto_t = merge_impl(env, cx, file, t, false, false);
                    let proto =
                        annotation_inference::obj_test_proto(cx, proto_reason.dupe(), proto_t);
                    let proto = type_util::typeof_annotation(proto_reason, proto, None);
                    mk_object(cx, None, proto)
                }
                ObjAnnotProto::ObjAnnotCallable(box ts) => {
                    let proto = Type::new(type_::TypeInner::FunProtoT(reason.dupe()));
                    let ts_vec: Vec<Type> = ts
                        .iter()
                        .map(|t| {
                            let t = merge_impl(env, cx, file, t, false, false);
                            mk_object(cx, Some(t), proto.dupe())
                        })
                        .collect();
                    match ts_vec.len() {
                        1 => ts_vec.into_iter().next().unwrap(),
                        _ => {
                            let mut iter = ts_vec.into_iter();
                            let t0 = iter.next().unwrap();
                            let t1 = iter.next().unwrap();
                            let rest: Vec<Type> = iter.collect();
                            let callable_reason = reason::mk_annot_reason(None, loc.dupe());
                            Type::new(type_::TypeInner::IntersectionT(
                                callable_reason,
                                type_::inter_rep::make(t0, t1, rest.into()),
                            ))
                        }
                    }
                }
            }
        }
        Annot::ObjSpreadAnnot(inner) => {
            let AnnotObjSpreadAnnot {
                loc,
                exact,
                elems,
                strictness_kind,
            } = inner.as_ref();
            let reason = reason::mk_annot_reason(RObjectType, loc.dupe());
            let target = type_::object::spread::Target::Annot {
                make_exact: *exact,
                strictness_kind: *strictness_kind,
            };
            let merge_slice = |cx: &mut Context<'cx>,
                               dict: &Option<ObjAnnotDict<Pack::Packed<ALoc>>>,
                               props: &BTreeMap<
                FlowSmolStr,
                ObjAnnotProp<ALoc, Pack::Packed<ALoc>>,
            >,
                               computed_props: &[(
                Pack::Packed<ALoc>,
                ObjAnnotProp<ALoc, Pack::Packed<ALoc>>,
            )]| {
                let dict = dict.as_ref().map(|d| merge_dict(env, cx, file, d, false));
                let mut props_smap: BTreeMap<Name, type_::Property> = BTreeMap::new();
                let mut prop_locs: BTreeMap<Name, ALoc> = BTreeMap::new();
                for (key, prop) in props {
                    let name = Name::new(key.dupe());
                    let p = merge_obj_annot_prop(env, cx, file, prop);
                    let p = name_symbol_field(p, key);
                    prop_locs.insert(name.dupe(), obj_annot_prop_source_loc(prop));
                    props_smap.insert(name, p);
                }
                for (key, prop) in computed_props {
                    if let Some(name) = resolve_computed_key(env, cx, file, key) {
                        let loc = obj_annot_prop_source_loc(prop);
                        let t = merge_obj_annot_prop(env, cx, file, prop);
                        insert_computed_prop(&mut props_smap, &mut prop_locs, name, t, loc);
                    }
                }
                let prop_map = type_::properties::PropertiesMap::from_btree_map(props_smap);
                type_::object::spread::OperandSlice::new(type_::object::spread::OperandSliceInner {
                    reason: reason.dupe(),
                    prop_map,
                    generics: flow_typing_generics::spread_empty(),
                    dict,
                    reachable_targs: vec![].into(),
                })
            };
            let merge_elem = |elem: &ObjSpreadAnnotElem<ALoc, Pack::Packed<ALoc>>| match elem {
                ObjSpreadAnnotElem::ObjSpreadAnnotElem(t) => {
                    type_::object::spread::Operand::Type(merge_impl(env, cx, file, t, false, false))
                }
                ObjSpreadAnnotElem::ObjSpreadAnnotSlice(box ObjSpreadAnnotSliceData {
                    dict,
                    props,
                    computed_props,
                }) => type_::object::spread::Operand::Slice(merge_slice(
                    cx,
                    dict,
                    props,
                    computed_props,
                )),
            };
            let mut merged_elems_rev: Vec<type_::object::spread::Operand> =
                elems.iter().rev().map(merge_elem).collect();
            let first = merged_elems_rev.remove(0);
            let (t, todo_rev, head_slice) = match first {
                type_::object::spread::Operand::Type(t) => (t, merged_elems_rev, None),
                type_::object::spread::Operand::Slice(slice) => match merged_elems_rev.first() {
                    Some(type_::object::spread::Operand::Type(_)) => {
                        let second = merged_elems_rev.remove(0);
                        match second {
                            type_::object::spread::Operand::Type(t) => {
                                (t, merged_elems_rev, Some(slice))
                            }
                            _ => unreachable!(),
                        }
                    }
                    Some(type_::object::spread::Operand::Slice(_)) => {
                        panic!("unexpected adjacent slices")
                    }
                    None => {
                        panic!("unexpected solo slice")
                    }
                },
            };
            let id = eval_id_of_aloc(cx, loc.dupe());
            Type::new(type_::TypeInner::EvalT {
                type_: t,
                defer_use_t: type_::TypeDestructorT::new(type_::TypeDestructorTInner(
                    type_::unknown_use(),
                    reason,
                    Rc::new(type_::Destructor::SpreadType(Box::new(
                        DestructorSpreadTypeData(
                            target,
                            todo_rev.into_iter().collect(),
                            head_slice,
                        ),
                    ))),
                )),
                id,
            })
        }
        Annot::InlineInterface(box (loc, def)) => {
            let reason = reason::mk_annot_reason(None, loc.dupe());
            let id = cx.make_aloc_id(loc);
            merge_interface(env, cx, file, reason, None, id, def, true, false, vec![])
        }
        Annot::ObjectBuiltin(box (loc, strictness_kind)) => {
            // TS-only `object`: an empty interface that rejects primitives. Built
            // here (in the signature path) so it behaves identically to the
            // checking-path builtin in type_annotation.rs.
            let reason = reason::mk_annot_reason(None, loc.dupe());
            let id = cx.make_aloc_id(loc);
            let def = InterfaceSig {
                extends: vec![],
                props: Default::default(),
                computed_props: vec![],
                calls: vec![],
                constructs: vec![],
                dict: None,
                abstract_: false,
                strictness_kind: *strictness_kind,
            };
            merge_interface(env, cx, file, reason, None, id, &def, true, true, vec![])
        }
        Annot::MappedTypeAnnot(inner) => {
            let AnnotMappedTypeAnnot {
                loc,
                source_type,
                property_type,
                name_type,
                key_tparam,
                variance,
                variance_op,
                optional,
                inline_keyof,
                strictness_kind,
            } = inner.as_ref();
            let source_type = merge_impl(env, cx, file, source_type, false, false);
            let mut env = env.dupe();
            let (tp, _) = merge_tparam(&mut env, cx, file, key_tparam, false);
            let property_type = {
                let prop_type = merge_impl(&env, cx, file, property_type, false, false);
                let prop_reason = type_util::reason_of_t(&prop_type).dupe();
                let id = cx.make_source_poly_id(true, loc);
                Type::new(type_::TypeInner::DefT(
                    prop_reason,
                    type_::DefT::new(type_::DefTInner::PolyT(Box::new(type_::PolyTData {
                        kind: PolyTKind::MappedType,
                        tparams_loc: loc.dupe(),
                        tparams: vec![tp.clone()].into(),
                        t_out: prop_type,
                        id,
                        strictness_kind: *strictness_kind,
                    }))),
                ))
            };
            let name_type = name_type.as_ref().map(|nt| {
                let name_t = merge_impl(&env, cx, file, nt, false, false);
                let name_reason = type_util::reason_of_t(&name_t).dupe();
                /* Use the name_type's own loc for the poly id so it doesn't collide with the
                 * property_type's poly id (which uses the MappedTypeAnnot loc). [make_source_poly_id]
                 * is deterministic in its loc argument. */
                let id = cx.make_source_poly_id(true, name_reason.loc());
                Type::new(type_::TypeInner::DefT(
                    name_reason.dupe(),
                    type_::DefT::new(type_::DefTInner::PolyT(Box::new(type_::PolyTData {
                        kind: PolyTKind::MappedType,
                        tparams_loc: loc.dupe(),
                        tparams: vec![tp.clone()].into(),
                        t_out: name_t,
                        id,
                        strictness_kind: *strictness_kind,
                    }))),
                ))
            });
            let optional = {
                use flow_parser::ast::types::object::MappedTypeOptionalFlag;
                match optional {
                    MappedTypeOptionalFlag::PlusOptional | MappedTypeOptionalFlag::Optional => {
                        type_::MappedTypeOptionality::MakeOptional
                    }
                    MappedTypeOptionalFlag::MinusOptional => {
                        type_::MappedTypeOptionality::RemoveOptional
                    }
                    MappedTypeOptionalFlag::NoOptionalFlag => {
                        type_::MappedTypeOptionality::KeepOptionality
                    }
                }
            };
            let mapped_type_variance = match variance_op {
                Some(flow_parser::ast::types::object::MappedTypeVarianceOp::Add) => {
                    type_::MappedTypeVariance::OverrideVariance(*variance)
                }
                Some(flow_parser::ast::types::object::MappedTypeVarianceOp::Remove) => {
                    type_::MappedTypeVariance::RemoveVariance(*variance)
                }
                None => {
                    if *variance == Polarity::Neutral {
                        type_::MappedTypeVariance::KeepVariance
                    } else {
                        type_::MappedTypeVariance::OverrideVariance(*variance)
                    }
                }
            };
            let mapped_type_flags = type_::MappedTypeFlags {
                variance: mapped_type_variance,
                optional,
            };
            let id = eval_id_of_aloc(cx, loc.dupe());
            let reason = reason::mk_reason(RObjectType, loc.dupe());
            let (source_type, homomorphic) = if *inline_keyof {
                (source_type, type_::MappedTypeHomomorphicFlag::Homomorphic)
            } else {
                match &*source_type {
                    type_::TypeInner::GenericT(box GenericTData { bound, .. }) => match &**bound {
                        type_::TypeInner::KeysT(_, obj_t) => (
                            obj_t.dupe(),
                            type_::MappedTypeHomomorphicFlag::SemiHomomorphic(source_type.dupe()),
                        ),
                        _ => (source_type, type_::MappedTypeHomomorphicFlag::Unspecialized),
                    },
                    type_::TypeInner::OpenT(tvar) => {
                        let (_, constraints) = cx.find_constraints(tvar.id() as i32);
                        match constraints {
                            type_::constraint::Constraints::FullyResolved(s) => {
                                let forced = cx.force_fully_resolved_tvar(&s);
                                match &*forced {
                                    type_::TypeInner::GenericT(box GenericTData {
                                        bound, ..
                                    }) => match &**bound {
                                        type_::TypeInner::KeysT(_, obj_t) => (
                                            obj_t.dupe(),
                                            type_::MappedTypeHomomorphicFlag::SemiHomomorphic(
                                                source_type.dupe(),
                                            ),
                                        ),
                                        _ => (
                                            source_type,
                                            type_::MappedTypeHomomorphicFlag::Unspecialized,
                                        ),
                                    },
                                    _ => (
                                        source_type,
                                        type_::MappedTypeHomomorphicFlag::Unspecialized,
                                    ),
                                }
                            }
                            _ => (source_type, type_::MappedTypeHomomorphicFlag::Unspecialized),
                        }
                    }
                    _ => (source_type, type_::MappedTypeHomomorphicFlag::Unspecialized),
                }
            };
            let distributive_tparam_name: Option<SubstName> = match &*source_type {
                type_::TypeInner::GenericT(box GenericTData { name, .. }) => Some(name.dupe()),
                type_::TypeInner::OpenT(tvar) => {
                    let (_, constraints) = cx.find_constraints(tvar.id() as i32);
                    match constraints {
                        type_::constraint::Constraints::FullyResolved(s) => {
                            let forced = cx.force_fully_resolved_tvar(&s);
                            match &*forced {
                                type_::TypeInner::GenericT(box GenericTData { name, .. }) => {
                                    Some(name.dupe())
                                }
                                _ => None,
                            }
                        }
                        _ => None,
                    }
                }
                _ => None,
            };
            Type::new(type_::TypeInner::EvalT {
                type_: source_type,
                defer_use_t: type_::TypeDestructorT::new(type_::TypeDestructorTInner(
                    type_::unknown_use(),
                    reason,
                    Rc::new(type_::Destructor::MappedType(Box::new(
                        DestructorMappedTypeData {
                            property_type,
                            name_type,
                            mapped_type_flags,
                            homomorphic,
                            distributive_tparam_name,
                        },
                    ))),
                )),
                id,
            })
        }
    }
}

fn merge_value<'cx>(
    env: &MergeEnv,
    cx: &mut Context<'cx>,
    file: &SigFile,
    value: &Pack::PackedValue<ALoc>,
    as_const: bool,
    const_decl: bool,
) -> Type {
    use flow_common::reason::VirtualReasonDesc::*;
    use flow_type_sig::type_sig::Value;
    match value {
        Value::ClassExpr(box (loc, def)) => {
            let name: FlowSmolStr = "<<anonymous class>>".into();
            let reason = type_::desc_format::instance_reason(Name::new(name), loc.dupe());
            let id = cx.make_aloc_id(loc);
            merge_class(env, cx, file, reason, None, id, def)
        }
        Value::FunExpr(inner) => {
            let ValueFunExpr {
                loc,
                async_,
                generator,
                def,
                statics,
            } = inner.as_ref();
            let reason = reason::func_reason(*async_, *generator, loc.dupe());
            let statics_t = merge_fun_statics(
                env,
                cx,
                file,
                reason.dupe(),
                statics
                    .iter()
                    .map(|(key, (loc, t))| (key, loc, t, Polarity::Neutral)),
                true,
            );
            merge_fun(
                env,
                cx,
                file,
                reason,
                FunctionReferenceKind::Function,
                def,
                statics_t,
                false,
                false,
                false,
                false,
                true,
            )
        }
        Value::StringVal(box loc) => {
            let reason = reason::mk_reason(RString, loc.dupe());
            Type::new(type_::TypeInner::DefT(
                reason,
                type_::DefT::new(type_::DefTInner::StrGeneralT(type_::Literal::AnyLiteral)),
            ))
        }
        Value::StringLit(box (loc, lit)) => {
            if as_const || const_decl {
                let reason = reason::mk_annot_reason(RStringLit(lit.dupe()), loc.dupe());
                Type::new(type_::TypeInner::DefT(
                    reason,
                    type_::DefT::new(type_::DefTInner::SingletonStrT {
                        from_annot: as_const,
                        value: lit.dupe(),
                    }),
                ))
            } else {
                let reason = reason::mk_reason(RString, loc.dupe());
                Type::new(type_::TypeInner::DefT(
                    reason,
                    type_::DefT::new(type_::DefTInner::StrGeneralT(type_::Literal::AnyLiteral)),
                ))
            }
        }
        Value::TemplateLiteral(inner) => {
            let ValueTemplateLiteral { loc, quasis, types } = inner.as_ref();
            if as_const || const_decl {
                let ts = types
                    .iter()
                    .map(|t| merge_impl(env, cx, file, t, as_const, const_decl))
                    .collect();
                flow_typing_flow_js::template_literal_type::resolve_for_value(
                    quasis.clone(),
                    ts,
                    loc.dupe(),
                    cx,
                    // Mirror `Value::StringLit`: only `as const` exports are
                    // frozen; plain `const` exports still generalize.
                    as_const,
                )
            } else {
                let reason = reason::mk_reason(RString, loc.dupe());
                Type::new(type_::TypeInner::DefT(
                    reason,
                    type_::DefT::new(type_::DefTInner::StrGeneralT(type_::Literal::AnyLiteral)),
                ))
            }
        }
        Value::NumberVal(box loc) => {
            let reason = reason::mk_reason(RNumber, loc.dupe());
            Type::new(type_::TypeInner::DefT(
                reason,
                type_::DefT::new(type_::DefTInner::NumGeneralT(type_::Literal::AnyLiteral)),
            ))
        }
        Value::NumberLit(box (loc, num, raw)) => {
            if as_const || const_decl {
                let reason = reason::mk_annot_reason(RNumberLit(raw.dupe()), loc.dupe());
                Type::new(type_::TypeInner::DefT(
                    reason,
                    type_::DefT::new(type_::DefTInner::SingletonNumT {
                        from_annot: as_const,
                        value: type_::NumberLiteral(*num, raw.dupe()),
                    }),
                ))
            } else {
                let reason = reason::mk_reason(RNumber, loc.dupe());
                Type::new(type_::TypeInner::DefT(
                    reason,
                    type_::DefT::new(type_::DefTInner::NumGeneralT(type_::Literal::AnyLiteral)),
                ))
            }
        }
        Value::BigIntVal(box loc) => {
            let reason = reason::mk_reason(RBigInt, loc.dupe());
            Type::new(type_::TypeInner::DefT(
                reason,
                type_::DefT::new(type_::DefTInner::BigIntGeneralT(type_::Literal::AnyLiteral)),
            ))
        }
        Value::BigIntLit(box (loc, bigint, raw)) => {
            if as_const || const_decl {
                let reason = reason::mk_annot_reason(RBigIntLit(raw.dupe()), loc.dupe());
                Type::new(type_::TypeInner::DefT(
                    reason,
                    type_::DefT::new(type_::DefTInner::SingletonBigIntT {
                        from_annot: as_const,
                        value: type_::BigIntLiteral(*bigint, raw.dupe()),
                    }),
                ))
            } else {
                let reason = reason::mk_reason(RBigInt, loc.dupe());
                Type::new(type_::TypeInner::DefT(
                    reason,
                    type_::DefT::new(type_::DefTInner::BigIntGeneralT(type_::Literal::AnyLiteral)),
                ))
            }
        }
        Value::BooleanVal(box loc) => {
            let reason = reason::mk_reason(RBoolean, loc.dupe());
            Type::new(type_::TypeInner::DefT(
                reason,
                type_::DefT::new(type_::DefTInner::BoolGeneralT),
            ))
        }
        Value::BooleanLit(box (loc, lit)) => {
            if as_const || const_decl {
                let reason = reason::mk_annot_reason(RBooleanLit(*lit), loc.dupe());
                Type::new(type_::TypeInner::DefT(
                    reason,
                    type_::DefT::new(type_::DefTInner::SingletonBoolT {
                        from_annot: as_const,
                        value: *lit,
                    }),
                ))
            } else {
                let reason = reason::mk_reason(RBoolean, loc.dupe());
                Type::new(type_::TypeInner::DefT(
                    reason,
                    type_::DefT::new(type_::DefTInner::BoolGeneralT),
                ))
            }
        }
        Value::UniqueSymbol(box loc) => {
            if as_const {
                type_::unique_symbol_t::at(cx.make_aloc_id(loc), loc.dupe(), None)
            } else if const_decl {
                type_::unique_symbol_t::inferred_at(cx.make_aloc_id(loc), loc.dupe(), None)
            } else {
                type_::symbol_t::at(loc.dupe())
            }
        }
        Value::NullLit(box loc) => type_::null::at(loc.dupe()),
        Value::DeclareModuleImplicitlyExportedObject(inner) => {
            merge_declare_module_implicitly_exported_object(
                env,
                cx,
                file,
                inner.loc.dupe(),
                &inner.module_name,
                &inner.props,
                inner.strictness_kind,
            )
        }
        Value::ObjLit(inner) => merge_object_lit(
            env,
            cx,
            file,
            inner.loc.dupe(),
            inner.frozen,
            &inner.proto,
            &inner.props,
            false,
            as_const,
            inner.strictness_kind,
        ),
        Value::ObjSpreadLit(inner) => merge_obj_spread_lit(
            env,
            cx,
            file,
            inner.loc.dupe(),
            inner.frozen,
            &inner.proto,
            &inner.elems,
            false,
            as_const,
            inner.strictness_kind,
        ),
        Value::EmptyConstArrayLit(inner) => {
            let reason = reason::mk_reason(None, inner.loc.dupe());
            let elem_t = type_::empty_t::make(reason::mk_reason(None, inner.loc.dupe()));
            let arrtype = type_::ArrType::TupleAT(Box::new(TupleATData {
                elem_t,
                elements: vec![].into(),
                react_dro: None,
                arity: (0, 0),
                inexact: false,
                rest: None,
                strictness_kind: inner.strictness_kind,
                literal: false,
                fresh: false,
            }));
            Type::new(type_::TypeInner::DefT(
                reason,
                type_::DefT::new(type_::DefTInner::ArrT(Rc::new(arrtype))),
            ))
        }
        Value::ArrayLit(inner) => {
            let loc = &inner.loc;
            let reason = if as_const {
                reason::mk_reason(None, loc.dupe())
            } else {
                reason::mk_reason(RArrayLit, loc.dupe())
            };
            let t = merge_impl(env, cx, file, &inner.elem_t, as_const, false);
            let ts: Vec<Type> = inner
                .elements
                .iter()
                .map(|t| merge_impl(env, cx, file, t, as_const, false))
                .collect();
            let elem_t = if ts.is_empty() {
                t.dupe()
            } else {
                let t0 = t.dupe();
                let t1 = ts[0].dupe();
                let rest: Vec<Type> = ts[1..].to_vec();
                let source_aloc = Some(cx.make_aloc_id(loc));
                Type::new(type_::TypeInner::UnionT(
                    reason.dupe(),
                    type_::union_rep::make(
                        source_aloc,
                        type_::union_rep::UnionKind::UnknownKind,
                        t0,
                        t1,
                        rest.into(),
                    ),
                ))
            };
            let arrtype = if as_const {
                let mut all_ts = vec![t.dupe()];
                all_ts.extend(ts.iter().duped());
                let elements: Vec<type_::TupleElement> = all_ts
                    .iter()
                    .map(|t| {
                        let reason = reason::mk_reason(RArrayLit, type_util::loc_of_t(t).dupe());
                        type_util::mk_tuple_element(
                            reason,
                            t.dupe(),
                            None,
                            false,
                            Polarity::Positive,
                        )
                    })
                    .collect();
                let num_elts = elements.len() as i32;
                type_::ArrType::TupleAT(Box::new(TupleATData {
                    elem_t,
                    elements: elements.into(),
                    react_dro: None,
                    arity: (num_elts, num_elts),
                    inexact: false,
                    rest: None,
                    strictness_kind: inner.strictness_kind,
                    literal: false,
                    fresh: false,
                }))
            } else {
                type_::ArrType::ArrayAT(Box::new(ArrayATData {
                    elem_t,
                    tuple_view: None,
                    react_dro: None,
                    strictness_kind: inner.strictness_kind,
                    literal: false,
                    fresh: false,
                }))
            };
            Type::new(type_::TypeInner::DefT(
                reason,
                type_::DefT::new(type_::DefTInner::ArrT(Rc::new(arrtype))),
            ))
        }
        Value::AsConst(value) => merge_value(env, cx, file, value, true, false),
    }
}

fn merge_declare_module_implicitly_exported_object<'cx>(
    env: &MergeEnv,
    cx: &mut Context<'cx>,
    file: &SigFile,
    loc: ALoc,
    module_name: &flow_common::flow_import_specifier::Userland,
    props: &BTreeMap<FlowSmolStr, ObjValueProp<ALoc, Pack::Packed<ALoc>>>,
    strictness_kind: TypeStrictnessKind,
) -> Type {
    use flow_common::reason::VirtualReasonDesc::*;

    let reason = reason::mk_reason(RModule(module_name.dupe()), loc);
    let proto = Type::new(type_::TypeInner::ObjProtoT(reason.dupe()));
    let props_map: type_::properties::PropertiesMap = props
        .iter()
        .map(|(key, prop)| {
            let p = merge_obj_value_prop(env, cx, file, key, prop, true, false, false);
            (Name::new(key.dupe()), p)
        })
        .collect();
    obj_type::mk_with_proto_and_strictness(
        cx,
        reason,
        type_::ObjKind::Exact,
        None,
        None,
        Some(props_map),
        None,
        proto,
        strictness_kind,
        false,
    )
}

fn merge_object_lit<'cx>(
    env: &MergeEnv,
    cx: &mut Context<'cx>,
    file: &SigFile,
    loc: ALoc,
    frozen: bool,
    proto: &Option<(ALoc, Pack::Packed<ALoc>)>,
    props: &BTreeMap<FlowSmolStr, ObjValueProp<ALoc, Pack::Packed<ALoc>>>,
    for_export: bool,
    as_const: bool,
    strictness_kind: TypeStrictnessKind,
) -> Type {
    let reason = reason::mk_obj_lit_reason(as_const, frozen, loc.dupe());
    let proto_t = match proto {
        None => Type::new(type_::TypeInner::ObjProtoT(reason.dupe())),
        Some((proto_loc, t)) => {
            let proto_reason = reason::mk_reason(None, proto_loc.dupe());
            let proto_t = merge_impl(env, cx, file, t, false, false);
            let proto = annotation_inference::obj_test_proto(cx, proto_reason.dupe(), proto_t);
            type_util::typeof_annotation(proto_reason, proto, None)
        }
    };
    let props_map: type_::properties::PropertiesMap = props
        .iter()
        .map(|(key, prop)| {
            let p = merge_obj_value_prop(env, cx, file, key, prop, for_export, as_const, frozen);
            (Name::new(key.dupe()), p)
        })
        .collect();
    obj_type::mk_with_proto_and_strictness(
        cx,
        reason,
        type_::ObjKind::Exact,
        None,
        None,
        Some(props_map),
        None,
        proto_t,
        strictness_kind,
        false,
    )
}

fn merge_obj_spread_lit<'cx>(
    env: &MergeEnv,
    cx: &mut Context<'cx>,
    file: &SigFile,
    loc: ALoc,
    frozen: bool,
    _proto: &Option<(ALoc, Pack::Packed<ALoc>)>,
    elems: &Vec1<ObjValueSpreadElem<ALoc, Pack::Packed<ALoc>>>,
    for_export: bool,
    as_const: bool,
    strictness_kind: TypeStrictnessKind,
) -> Type {
    let reason = reason::mk_obj_lit_reason(as_const, frozen, loc.dupe());
    // TODO: fix spread to use provided __proto__ prop
    let merge_slice =
        |cx: &mut Context<'cx>,
         props: &BTreeMap<FlowSmolStr, ObjValueProp<ALoc, Pack::Packed<ALoc>>>| {
            let mut prop_map = BTreeMap::new();
            for (key, prop) in props {
                let p =
                    merge_obj_value_prop(env, cx, file, key, prop, for_export, as_const, frozen);
                prop_map.insert(Name::new(key.dupe()), p);
            }
            type_::object::spread::OperandSlice::new(type_::object::spread::OperandSliceInner {
                reason: reason.dupe(),
                prop_map: prop_map.into_iter().collect(),
                generics: flow_typing_generics::spread_empty(),
                dict: None,
                reachable_targs: vec![].into(),
            })
        };
    let merge_elem = |elem: &ObjValueSpreadElem<ALoc, Pack::Packed<ALoc>>| match elem {
        ObjValueSpreadElem::ObjValueSpreadElem(t) => {
            type_::object::spread::Operand::Type(merge_impl(env, cx, file, t, as_const, false))
        }
        ObjValueSpreadElem::ObjValueSpreadSlice(props) => {
            type_::object::spread::Operand::Slice(merge_slice(cx, props))
        }
    };
    let mut merged_elems_rev: Vec<type_::object::spread::Operand> =
        elems.iter().rev().map(merge_elem).collect();
    let first = merged_elems_rev.remove(0);
    let (t, todo_rev, head_slice) = match first {
        type_::object::spread::Operand::Type(t) => (t, merged_elems_rev, None),
        type_::object::spread::Operand::Slice(slice) => match merged_elems_rev.first() {
            Some(type_::object::spread::Operand::Type(_)) => {
                let second = merged_elems_rev.remove(0);
                match second {
                    type_::object::spread::Operand::Type(t) => (t, merged_elems_rev, Some(slice)),
                    _ => unreachable!(),
                }
            }
            _ => panic!("unexpected spread"),
        },
    };
    let make_seal = if frozen {
        type_::object::spread::SealType::Frozen
    } else if as_const {
        type_::object::spread::SealType::AsConst
    } else {
        type_::object::spread::SealType::Sealed
    };
    let target = type_::object::spread::Target::Value {
        make_seal,
        strictness_kind,
        literal: false,
    };
    let use_op = type_::UseOp::Op(Arc::new(type_::RootUseOp::ObjectSpread {
        op: reason.dupe(),
    }));
    let acc: flow_data_structure_wrapper::list::FlowOcamlList<
        flow_typing_type::type_::object::spread::AccElement,
    > = match &head_slice {
        Some(slice) => flow_data_structure_wrapper::list::FlowOcamlList::unit(
            flow_typing_type::type_::object::spread::AccElement::InlineSlice(slice.clone()),
        ),
        None => flow_data_structure_wrapper::list::FlowOcamlList::new(),
    };
    let state = type_::object::spread::State {
        todo_rev: todo_rev.into_iter().collect(),
        acc,
        spread_id: reason::mk_id() as i32,
        union_t: None,
        curr_resolve_idx: 0,
    };
    annotation_inference::object_spread(cx, use_op, reason, target, state, t)
}

fn merge_accessor<'cx>(
    env: &MergeEnv,
    cx: &mut Context<'cx>,
    file: &SigFile,
    accessor: &Accessor<ALoc, Pack::Packed<ALoc>>,
) -> type_::Property {
    match accessor {
        Accessor::Get(box (loc, t)) => {
            let type_ = merge_impl(env, cx, file, t, false, false);
            type_::Property::new(type_::PropertyInner::Get {
                key_loc: Some(loc.dupe()),
                type_,
            })
        }
        Accessor::Set(box (loc, t)) => {
            let type_ = merge_impl(env, cx, file, t, false, false);
            type_::Property::new(type_::PropertyInner::Set {
                key_loc: Some(loc.dupe()),
                type_,
            })
        }
        Accessor::GetSet(box (gloc, gt, sloc, st)) => {
            let get_type = merge_impl(env, cx, file, gt, false, false);
            let set_type = merge_impl(env, cx, file, st, false, false);
            type_::Property::new(type_::PropertyInner::GetSet(Box::new(type_::GetSetData {
                get_key_loc: Some(gloc.dupe()),
                get_type,
                set_key_loc: Some(sloc.dupe()),
                set_type,
            })))
        }
    }
}

fn merge_obj_value_prop<'cx>(
    env: &MergeEnv,
    cx: &mut Context<'cx>,
    file: &SigFile,
    key: &FlowSmolStr,
    prop: &ObjValueProp<ALoc, Pack::Packed<ALoc>>,
    for_export: bool,
    as_const: bool,
    frozen: bool,
) -> type_::Property {
    match prop {
        ObjValueProp::ObjValueField(box (id_loc, Pack::Packed::Ref(ref_), polarity))
            if for_export =>
        {
            let polarity = if as_const || frozen {
                Polarity::Positive
            } else {
                *polarity
            };
            let id_loc = id_loc.dupe();
            let key = key.dupe();
            merge_ref(
                cx,
                file,
                move |_, type_, ref_loc, def_loc, value_name| {
                    // If name matches and prop loc is the name as ref loc, it must be shorthand syntax like
                    // module.exports = { foo }.
                    if key == *value_name && id_loc == ref_loc {
                        type_::Property::new(type_::PropertyInner::Field(Box::new(
                            type_::FieldData {
                                preferred_def_locs: Some(Vec1::new(def_loc)),
                                key_loc: Some(id_loc),
                                type_,
                                polarity,
                            },
                        )))
                    } else {
                        type_::Property::new(type_::PropertyInner::Field(Box::new(
                            type_::FieldData {
                                preferred_def_locs: None,
                                key_loc: Some(id_loc),
                                type_,
                                polarity,
                            },
                        )))
                    }
                },
                ref_,
                false,
            )
        }
        ObjValueProp::ObjValueField(box (id_loc, t, polarity)) => {
            let type_ = merge_impl(env, cx, file, t, as_const, false);
            let polarity = if as_const || frozen {
                Polarity::Positive
            } else {
                *polarity
            };
            type_::Property::new(type_::PropertyInner::Field(Box::new(type_::FieldData {
                preferred_def_locs: None,
                key_loc: Some(id_loc.dupe()),
                type_,
                polarity,
            })))
        }
        ObjValueProp::ObjValueAccess(box x) => merge_accessor(env, cx, file, x),
        ObjValueProp::ObjValueMethod(box ms) => {
            let merge_one =
                |ObjValueMethodData {
                     id_loc: _,
                     fn_loc,
                     async_,
                     generator,
                     uses_this: _,
                     def,
                 }: &ObjValueMethodData<ALoc, Pack::Packed<ALoc>>| {
                    let reason = reason::func_reason(*async_, *generator, fn_loc.dupe());
                    let statics =
                        merge_fun_statics(env, cx, file, reason.dupe(), std::iter::empty(), true);
                    merge_fun(
                        env,
                        cx,
                        file,
                        reason,
                        FunctionReferenceKind::Function,
                        def,
                        statics,
                        false,
                        false,
                        false,
                        false,
                        true,
                    )
                };
            merge_overloaded_methods(ms, merge_one, |m| &m.id_loc)
        }
    }
}

fn merge_class_prop<'cx>(
    env: &MergeEnv,
    cx: &mut Context<'cx>,
    file: &SigFile,
    prop: &ObjValueProp<ALoc, Pack::Packed<ALoc>>,
    is_static: bool,
) -> type_::Property {
    match prop {
        ObjValueProp::ObjValueField(box (id_loc, t, polarity)) => {
            let type_ = merge_impl(env, cx, file, t, false, false);
            type_::Property::new(type_::PropertyInner::Field(Box::new(type_::FieldData {
                preferred_def_locs: None,
                key_loc: Some(id_loc.dupe()),
                type_,
                polarity: *polarity,
            })))
        }
        ObjValueProp::ObjValueAccess(box x) => merge_accessor(env, cx, file, x),
        ObjValueProp::ObjValueMethod(box ms) => {
            let uses_this = ms.iter().any(|m| m.uses_this);
            let merge_one =
                |ObjValueMethodData {
                     id_loc: _,
                     fn_loc,
                     async_,
                     generator,
                     uses_this: _,
                     def,
                 }: &ObjValueMethodData<ALoc, Pack::Packed<ALoc>>| {
                    let reason = reason::func_reason(*async_, *generator, fn_loc.dupe());
                    let statics = type_::dummy_static(reason.dupe());
                    merge_fun(
                        env,
                        cx,
                        file,
                        reason,
                        FunctionReferenceKind::Function,
                        def,
                        statics,
                        true,
                        is_static,
                        uses_this,
                        false,
                        true,
                    )
                };
            merge_overloaded_methods(ms, merge_one, |m| &m.id_loc)
        }
    }
}

fn merge_obj_annot_prop<'cx>(
    env: &MergeEnv,
    cx: &mut Context<'cx>,
    file: &SigFile,
    prop: &ObjAnnotProp<ALoc, Pack::Packed<ALoc>>,
) -> type_::Property {
    use flow_common::reason::VirtualReasonDesc::*;
    match prop {
        ObjAnnotProp::ObjAnnotField(box (id_loc, t, polarity)) => {
            let type_ = merge_impl(env, cx, file, t, false, false);
            type_::Property::new(type_::PropertyInner::Field(Box::new(type_::FieldData {
                preferred_def_locs: None,
                key_loc: Some(id_loc.dupe()),
                type_,
                polarity: *polarity,
            })))
        }
        ObjAnnotProp::ObjAnnotAccess(box x) => merge_accessor(env, cx, file, x),
        ObjAnnotProp::ObjAnnotMethod(box ObjAnnotMethodData {
            id_loc,
            fn_loc,
            def,
        }) => {
            let reason = reason::mk_annot_reason(RFunctionType, fn_loc.dupe());
            let statics =
                merge_fun_statics(env, cx, file, reason.dupe(), std::iter::empty(), false);
            let type_ = merge_fun(
                env,
                cx,
                file,
                reason,
                FunctionReferenceKind::FunctionType,
                def,
                statics,
                false,
                false,
                false,
                false,
                false,
            );
            type_::Property::new(type_::PropertyInner::Method {
                key_loc: Some(id_loc.dupe()),
                type_,
            })
        }
    }
}

fn merge_interface_prop<'cx>(
    env: &MergeEnv,
    cx: &mut Context<'cx>,
    file: &SigFile,
    prop: &InterfaceProp<ALoc, Pack::Packed<ALoc>>,
    is_static: bool,
) -> type_::Property {
    use flow_common::reason::VirtualReasonDesc::*;
    match prop {
        InterfaceProp::InterfaceField(box (id_loc, t, polarity)) => {
            let t = merge_impl(env, cx, file, t, false, false);
            type_::Property::new(type_::PropertyInner::Field(Box::new(type_::FieldData {
                preferred_def_locs: None,
                key_loc: id_loc.dupe(),
                type_: t,
                polarity: *polarity,
            })))
        }
        InterfaceProp::InterfaceAccess(box x) => merge_accessor(env, cx, file, x),
        InterfaceProp::InterfaceMethod(box ms) => {
            let merge_one = |(_, fn_loc, def): &(ALoc, ALoc, FunSig<ALoc, Pack::Packed<ALoc>>)| {
                let reason = reason::mk_reason(RFunctionType, fn_loc.dupe());
                let statics = type_::dummy_static(reason.dupe());
                merge_fun(
                    env,
                    cx,
                    file,
                    reason,
                    FunctionReferenceKind::FunctionType,
                    def,
                    statics,
                    true,
                    is_static,
                    false,
                    false,
                    false,
                )
            };
            merge_overloaded_methods(ms, merge_one, |(id_loc, _, _)| id_loc)
        }
    }
}

fn merge_dict<'cx>(
    env: &MergeEnv,
    cx: &mut Context<'cx>,
    file: &SigFile,
    dict: &ObjAnnotDict<Pack::Packed<ALoc>>,
    as_const: bool,
) -> type_::DictType {
    let key = merge_impl(env, cx, file, &dict.key, false, false);
    let value = merge_impl(env, cx, file, &dict.value, false, false);
    let polarity = Polarity::apply_const(as_const, dict.polarity);
    type_::DictType {
        dict_name: dict.name.dupe(),
        dict_polarity: polarity,
        key,
        value,
    }
}

fn merge_tparams_targs<'cx>(
    env: &MergeEnv,
    cx: &mut Context<'cx>,
    file: &SigFile,
    reason: Reason,
    t: impl FnOnce(&mut Context<'cx>, &MergeEnv, Vec<(SubstName, Reason, Type, Polarity)>) -> Type,
    tparams: &TParams<ALoc, Pack::Packed<ALoc>>,
    strictness_kind: type_::TypeStrictnessKind,
) -> Type {
    match tparams {
        TParams::Mono => t(cx, env, Vec::new()),
        TParams::Poly(poly) => {
            let mut current_env = env.dupe();
            for tp in poly.tparams.iter() {
                current_env.tps.remove(&tp.name);
            }
            let mut partials = vec![None; poly.tparams.len()];
            for index in &poly.bound_order {
                partials[*index] = Some(merge_tparam_bound(
                    &mut current_env,
                    cx,
                    file,
                    &poly.tparams[*index],
                    false,
                ));
            }

            let mut tparams_vec = Vec::with_capacity(poly.tparams.len());
            let mut tparam_tuples = Vec::with_capacity(poly.tparams.len());
            for (index, tp) in poly.tparams.iter().enumerate() {
                let partial = partials[index]
                    .take()
                    .expect("every packed type parameter should be initialized");
                let (type_param, tuple) = finish_merged_tparam(&current_env, cx, file, tp, partial);
                tparams_vec.push(type_param);
                tparam_tuples.push(tuple);
            }
            let t_out = t(cx, &current_env, tparam_tuples);
            let id = cx.make_source_poly_id(true, &poly.loc);
            Type::new(type_::TypeInner::DefT(
                reason,
                type_::DefT::new(type_::DefTInner::PolyT(Box::new(type_::PolyTData {
                    kind: PolyTKind::Regular,
                    tparams_loc: poly.loc.dupe(),
                    tparams: tparams_vec.into(),
                    t_out,
                    id,
                    strictness_kind,
                }))),
            ))
        }
    }
}

fn merge_tparam_bound<'cx>(
    env: &mut MergeEnv,
    cx: &mut Context<'cx>,
    file: &SigFile,
    tp: &TParam<ALoc, Pack::Packed<ALoc>>,
    from_infer: bool,
) -> (type_::TypeParam, (SubstName, Reason, Type, Polarity)) {
    use flow_common::reason::VirtualReasonDesc::*;
    use flow_common::subst_name::SubstName;

    let TParam {
        name_loc,
        name,
        polarity,
        bound,
        default: _,
        is_const,
    } = tp;
    let polarity = polarity.unwrap_or(Polarity::Neutral);
    let reason = reason::mk_reason(RType(name.dupe()), name_loc.dupe());
    let bound = match bound {
        None => {
            let bound_reason = reason.dupe().replace_desc(RMixed);
            Type::new(type_::TypeInner::DefT(
                bound_reason,
                type_::DefT::new(type_::DefTInner::MixedT(
                    type_::MixedFlavor::MixedEverything,
                )),
            ))
        }
        Some(t) => merge_impl(env, cx, file, t, false, false),
    };
    let subst_name = if from_infer && env.tps.contains_key(name) {
        let fvs: flow_data_structure_wrapper::ord_set::FlowOrdSet<SubstName> =
            env.tps.keys().map(|n| SubstName::name(n.dupe())).collect();
        alpha_rename::subst_name(&SubstName::name(name.dupe()), &fvs)
    } else {
        SubstName::name(name.dupe())
    };
    let tp = type_::TypeParam::new(type_::TypeParamInner {
        reason: reason.dupe(),
        name: subst_name,
        bound,
        polarity,
        default: None,
        is_this: false,
        is_const: *is_const,
    });
    let t = flow_js_utils::generic_of_tparam(cx, |_, x: &Type| x.dupe(), &tp);
    env.tps.insert(name.dupe(), t.dupe());
    (tp, (SubstName::name(name.dupe()), reason, t, polarity))
}

fn finish_merged_tparam<'cx>(
    env: &MergeEnv,
    cx: &mut Context<'cx>,
    file: &SigFile,
    packed: &TParam<ALoc, Pack::Packed<ALoc>>,
    partial: (type_::TypeParam, (SubstName, Reason, Type, Polarity)),
) -> (type_::TypeParam, (SubstName, Reason, Type, Polarity)) {
    let (type_param, tuple) = partial;
    let default = packed
        .default
        .as_ref()
        .map(|t| merge_impl(env, cx, file, t, false, false));
    let type_param = type_::TypeParam::new(type_::TypeParamInner {
        reason: type_param.reason.dupe(),
        name: type_param.name.dupe(),
        bound: type_param.bound.dupe(),
        polarity: type_param.polarity,
        default,
        is_this: type_param.is_this,
        is_const: type_param.is_const,
    });
    (type_param, tuple)
}

fn merge_tparam<'cx>(
    env: &mut MergeEnv,
    cx: &mut Context<'cx>,
    file: &SigFile,
    tp: &TParam<ALoc, Pack::Packed<ALoc>>,
    from_infer: bool,
) -> (type_::TypeParam, (SubstName, Reason, Type, Polarity)) {
    let partial = merge_tparam_bound(env, cx, file, tp, from_infer);
    finish_merged_tparam(env, cx, file, tp, partial)
}

fn merge_op<'cx>(
    env: &MergeEnv,
    cx: &mut Context<'cx>,
    file: &SigFile,
    op: &Op<Box<Pack::Packed<ALoc>>>,
) -> Op<Type> {
    op.map(cx, |cx, t| merge_impl(env, cx, file, t, false, false))
}

fn merge_interface_extends<'cx>(
    bind_this: bool,
    env: &MergeEnv,
    cx: &mut Context<'cx>,
    file: &SigFile,
    packed: &Pack::Packed<ALoc>,
) -> (Type, bool, Option<Vec<Type>>) {
    #[derive(Clone, Copy)]
    enum PolymorphicClassKindForExtends {
        Poly,
        NonPoly,
        Other,
    }

    let resolve_raw = |cx: &mut Context<'cx>, name: &Pack::TyRef<ALoc>| -> Type {
        fn aux<'cx>(cx: &mut Context<'cx>, file: &SigFile, name: &Pack::TyRef<ALoc>) -> Type {
            use flow_common::reason::VirtualReasonDesc::*;
            match name {
                Pack::TyRef::Unqualified(Pack::PackedRef::RemoteRef(
                    box Pack::PackedRefRemote { ref_loc, index },
                )) => {
                    let __rc = file.remote_refs.get(*index).dupe();
                    let entry = __rc.get_forced(cx);
                    let (_def_loc, _name, _t, t_for_extends) = entry;
                    let t = t_for_extends.get_forced(cx).dupe();
                    reposition_sig_tvar(cx, ref_loc.dupe(), t)
                }
                Pack::TyRef::Qualified(box Pack::TyRefQualified {
                    loc,
                    id_loc,
                    name,
                    qualification,
                }) => {
                    let qual_t = aux(cx, file, qualification);
                    let name_r = Name::new(name.dupe());
                    let id_reason = reason::mk_reason(RType(name.dupe()), id_loc.dupe());
                    let op_reason = reason::mk_reason(RType(name.dupe()), loc.dupe());
                    let use_op = type_::VirtualUseOp::Op(Arc::new(
                        type_::VirtualRootUseOp::GetProperty(op_reason.dupe()),
                    ));
                    annotation_inference::qualify_type(
                        cx, use_op, id_reason, op_reason, name_r, qual_t,
                    )
                }
                Pack::TyRef::Unqualified(_) => {
                    merge_tyref(cx, file, |_, t, _ref_loc, _names| t, name)
                }
            }
        }
        aux(cx, file, name)
    };
    let leaf_binding_kind = |cx: &mut Context<'cx>, name: &Pack::TyRef<ALoc>| match name {
        Pack::TyRef::Unqualified(Pack::PackedRef::LocalRef(box Pack::PackedRefLocal {
            index,
            ..
        })) => {
            let __rc = file.local_defs.get(*index).dupe();
            let entry = __rc.get_forced(cx);
            let (_, _, kind, _, _) = entry;
            Some(*kind)
        }
        _ => None,
    };
    let polymorphic_class_kind =
        |cx: &mut Context<'cx>, t: &Type| match flow_js_utils::polymorphic_class_kind(cx, t) {
            flow_js_utils::PolymorphicClassKind::Poly => PolymorphicClassKindForExtends::Poly,
            flow_js_utils::PolymorphicClassKind::Mono => PolymorphicClassKindForExtends::NonPoly,
            flow_js_utils::PolymorphicClassKind::Other => PolymorphicClassKindForExtends::Other,
        };

    if !bind_this {
        (merge_impl(env, cx, file, packed, false, false), false, None)
    } else {
        match packed {
            Pack::Packed::TyRef(name) => {
                let kind = leaf_binding_kind(cx, name);
                match kind {
                    Some(LocalDefBindingKind::DefBindingOther) => {
                        (merge_impl(env, cx, file, packed, false, false), false, None)
                    }
                    Some(LocalDefBindingKind::DefBindingClassLikeMono) => {
                        let raw = resolve_raw(cx, name);
                        (raw, true, None)
                    }
                    Some(LocalDefBindingKind::DefBindingClassLikePoly) => {
                        (merge_impl(env, cx, file, packed, false, false), false, None)
                    }
                    None => {
                        let raw = resolve_raw(cx, name);
                        match polymorphic_class_kind(cx, &raw) {
                            PolymorphicClassKindForExtends::NonPoly => (raw, true, None),
                            PolymorphicClassKindForExtends::Poly
                            | PolymorphicClassKindForExtends::Other => {
                                (merge_impl(env, cx, file, packed, false, false), false, None)
                            }
                        }
                    }
                }
            }
            Pack::Packed::TyRefApp(box Pack::PackedTyRefApp { loc, name, targs }) => {
                let targs_t: Vec<Type> = targs
                    .iter()
                    .map(|t| merge_impl(env, cx, file, t, false, false))
                    .collect();
                let kind = leaf_binding_kind(cx, name);
                match kind {
                    Some(LocalDefBindingKind::DefBindingOther) => {
                        let raw = merge_impl(
                            env,
                            cx,
                            file,
                            &Pack::Packed::TyRef(Box::new(name.clone())),
                            false,
                            false,
                        );
                        let t = type_util::typeapp_annot(false, false, loc.dupe(), raw, targs_t);
                        (t, false, None)
                    }
                    Some(LocalDefBindingKind::DefBindingClassLikePoly) => {
                        let raw = resolve_raw(cx, name);
                        (raw, true, Some(targs_t))
                    }
                    Some(LocalDefBindingKind::DefBindingClassLikeMono) => {
                        let raw = resolve_raw(cx, name);
                        let t = type_util::typeapp_annot(false, false, loc.dupe(), raw, targs_t);
                        (t, false, None)
                    }
                    None => {
                        let raw = resolve_raw(cx, name);
                        match polymorphic_class_kind(cx, &raw) {
                            PolymorphicClassKindForExtends::Poly => (raw, true, Some(targs_t)),
                            PolymorphicClassKindForExtends::NonPoly
                            | PolymorphicClassKindForExtends::Other => {
                                let t = type_util::typeapp_annot(
                                    false,
                                    false,
                                    loc.dupe(),
                                    raw,
                                    targs_t,
                                );
                                (t, false, None)
                            }
                        }
                    }
                }
            }
            _ => (merge_impl(env, cx, file, packed, false, false), false, None),
        }
    }
}

fn merge_interface<'cx>(
    env: &MergeEnv,
    cx: &mut Context<'cx>,
    file: &SigFile,
    reason: Reason,
    class_name: Option<FlowSmolStr>,
    id: flow_aloc::ALocId,
    def: &InterfaceSig<ALoc, Pack::Packed<ALoc>>,
    inline: bool,
    reject_primitives: bool,
    targs: Vec<(SubstName, Reason, Type, Polarity)>,
) -> Type {
    use flow_common::reason::VirtualReasonDesc::*;

    let InterfaceSig {
        extends,
        props,
        computed_props,
        calls,
        constructs,
        dict,
        abstract_,
        strictness_kind: _,
    } = def;
    let static_ = {
        let static_reason = reason.dupe().replace_desc(None);
        // interfaces don't have a name field, or even statics
        let mut props_map = BTreeMap::new();
        add_name_field(reason.dupe(), &mut props_map);
        let props = type_::properties::PropertiesMap::from_btree_map(props_map);
        let proto = Type::new(type_::TypeInner::NullProtoT(static_reason.dupe()));
        obj_type::mk_with_proto(
            cx,
            static_reason,
            type_::ObjKind::Inexact,
            None,
            None,
            Some(props),
            None,
            proto,
            false,
        )
    };
    // Inline interfaces use the same polymorphic receiver as named interfaces.
    // Their outward representation is fixed below so they remain ordinary
    // inline instance types.
    let build = |cx: &mut Context<'cx>,
                 env: &MergeEnv,
                 targs: Vec<(SubstName, Reason, Type, Polarity)>|
     -> (type_::InstType, Box<dyn FnOnce(Option<Type>) -> Type + 'cx>) {
        let extends_resolved: Vec<(Type, bool, Option<Vec<Type>>)> = extends
            .iter()
            .map(|t| merge_interface_extends(true, env, cx, file, t))
            .collect();
        let function_like = !calls.is_empty() || !constructs.is_empty();
        let reason_for_super = reason.dupe();
        let make_super: Box<dyn FnOnce(Option<Type>) -> Type + 'cx> =
            Box::new(move |this_opt: Option<Type>| {
                let super_reason = reason_for_super.replace_desc(None);
                let mut ts: Vec<Type> = extends_resolved
                    .into_iter()
                    .map(
                        |(t, polymorphic_class, targs_opt)| match (&this_opt, polymorphic_class) {
                            (Some(this), true) => {
                                let annot_loc = type_util::reason_of_t(&t).loc().dupe();
                                type_util::this_typeapp(t, this.dupe(), targs_opt, Some(annot_loc))
                            }
                            _ => t,
                        },
                    )
                    .collect();
                if function_like {
                    ts.insert(
                        0,
                        Type::new(type_::TypeInner::FunProtoT(super_reason.dupe())),
                    );
                }
                match ts.len() {
                    0 => Type::new(type_::TypeInner::ObjProtoT(super_reason)),
                    1 => ts.into_iter().next().unwrap(),
                    _ => {
                        let mut iter = ts.into_iter();
                        let t0 = iter.next().unwrap();
                        let t1 = iter.next().unwrap();
                        let rest: Vec<Type> = iter.collect();
                        Type::new(type_::TypeInner::IntersectionT(
                            super_reason,
                            type_::inter_rep::make(t0, t1, rest.into()),
                        ))
                    }
                }
            });
        let (own_props, proto_props): (
            type_::properties::PropertiesMap,
            type_::properties::PropertiesMap,
        ) = {
            let mut own: BTreeMap<Name, type_::Property> = BTreeMap::new();
            let mut proto: BTreeMap<Name, type_::Property> = BTreeMap::new();
            let mut prop_locs: BTreeMap<Name, ALoc> = BTreeMap::new();
            for (k, prop) in props {
                let t = merge_interface_prop(env, cx, file, prop, false);
                let name = Name::new(k.dupe());
                record_prop_loc(&mut prop_locs, &name, prop);
                match prop {
                    InterfaceProp::InterfaceField(..) => {
                        let t = name_symbol_field(t, k);
                        own.insert(name, t);
                    }
                    InterfaceProp::InterfaceAccess(..) | InterfaceProp::InterfaceMethod(..) => {
                        proto.insert(name, t);
                    }
                }
            }
            for (key, prop) in computed_props {
                if let Some(name) = resolve_computed_key(env, cx, file, key) {
                    let t = merge_interface_prop(env, cx, file, prop, false);
                    let loc = interface_prop_source_loc(prop);
                    match prop {
                        InterfaceProp::InterfaceField(..) => insert_computed_class_prop(
                            &mut own,
                            Some(&mut proto),
                            &mut prop_locs,
                            name,
                            t,
                            loc,
                        ),
                        InterfaceProp::InterfaceAccess(..) | InterfaceProp::InterfaceMethod(..) => {
                            insert_computed_class_prop(
                                &mut proto,
                                Some(&mut own),
                                &mut prop_locs,
                                name,
                                t,
                                loc,
                            )
                        }
                    }
                }
            }
            (own.into(), proto.into())
        };
        let inst_call_t = {
            let ts: Vec<Type> = calls
                .iter()
                .map(|t| merge_impl(env, cx, file, t, false, false))
                .collect();
            match ts.len() {
                0 => None,
                1 => Some(cx.make_call_prop(ts.into_iter().next().unwrap())),
                _ => {
                    let mut iter = ts.into_iter();
                    let t0 = iter.next().unwrap();
                    let t1 = iter.next().unwrap();
                    let rest: Vec<Type> = iter.collect();
                    let call_reason = type_util::reason_of_t(&t0).dupe();
                    let t = Type::new(type_::TypeInner::IntersectionT(
                        call_reason,
                        type_::inter_rep::make(t0, t1, rest.into()),
                    ));
                    Some(cx.make_call_prop(t))
                }
            }
        };
        let inst_construct_t = {
            let ts: Vec<Type> = constructs
                .iter()
                .map(|t| merge_impl(env, cx, file, t, false, false))
                .collect();
            match ts.len() {
                0 => None,
                1 => Some(cx.make_call_prop(ts.into_iter().next().unwrap())),
                _ => {
                    let mut iter = ts.into_iter();
                    let t0 = iter.next().unwrap();
                    let t1 = iter.next().unwrap();
                    let rest: Vec<Type> = iter.collect();
                    let call_reason = type_util::reason_of_t(&t0).dupe();
                    let t = Type::new(type_::TypeInner::IntersectionT(
                        call_reason,
                        type_::inter_rep::make(t0, t1, rest.into()),
                    ));
                    Some(cx.make_call_prop(t))
                }
            }
        };
        let inst_dict = dict.as_ref().map(|d| merge_dict(env, cx, file, d, false));
        let inst = type_::InstType::new(type_::InstTypeInner {
            class_id: id,
            inst_react_dro: None,
            class_name: class_name.dupe(),
            type_args: targs.into(),
            own_props: cx.generate_property_map(own_props),
            proto_props: cx.generate_property_map(proto_props),
            inst_call_t,
            inst_construct_t,
            initialized_fields: flow_data_structure_wrapper::ord_set::FlowOrdSet::new(),
            initialized_static_fields: flow_data_structure_wrapper::ord_set::FlowOrdSet::new(),
            inst_kind: type_::InstanceKind::InterfaceKind {
                inline,
                reject_primitives,
            },
            inst_dict,
            class_private_fields: cx.generate_property_map(type_::properties::PropertiesMap::new()),
            class_private_methods: cx
                .generate_property_map(type_::properties::PropertiesMap::new()),
            class_private_static_fields: cx
                .generate_property_map(type_::properties::PropertiesMap::new()),
            class_private_static_methods: cx
                .generate_property_map(type_::properties::PropertiesMap::new()),
            // Preserve the [abstract] bit from the InterfaceSig — set only for
            // the inline interface lowered from [abstract new (...) => T].
            // Other interfaces stay non-abstract.
            inst_abstract: *abstract_,
            inst_abstract_props: flow_data_structure_wrapper::ord_set::FlowOrdSet::new(),
            strictness_kind: def.strictness_kind,
        });
        (inst, make_super)
    };
    let this_reason = reason.dupe().replace_desc(RThisType);
    let this_name = SubstName::name(FlowSmolStr::new("this"));
    let result_cell: Rc<RefCell<Option<Type>>> = Rc::new(RefCell::new(None));
    let result_cell_c = result_cell.dupe();
    let this_reason_c = this_reason.dupe();
    let rec_type = flow_typing_tvar::mk_fully_resolved_lazy(
        cx,
        this_reason.dupe(),
        false,
        move |_cx: &mut Context<'_>| Ok(result_cell_c.borrow().as_ref().unwrap().dupe()),
    );
    let this_tp = type_::TypeParam::new(type_::TypeParamInner {
        name: this_name.dupe(),
        reason: this_reason_c,
        bound: rec_type,
        polarity: Polarity::Positive,
        default: None,
        is_this: true,
        is_const: false,
    });
    let this = flow_js_utils::generic_of_tparam(cx, |_, x: &Type| x.dupe(), &this_tp);
    let mut env = env.dupe();
    env.tps.insert(FlowSmolStr::new("this"), this.dupe());
    let (inst, make_super) = build(cx, &env, targs);
    let super_ = make_super(Some(this));
    let instance = type_::InstanceT::new(type_::InstanceTInner {
        inst,
        static_,
        super_,
        implements: vec![].into(),
    });
    let result = Type::new(type_::TypeInner::ThisInstanceT(Box::new(
        ThisInstanceTData {
            reason: reason.dupe(),
            instance: instance.dupe(),
            is_this: false,
            subst_name: this_name.dupe(),
        },
    )));
    *result_cell.borrow_mut() = Some(result.dupe());
    if inline {
        flow_js_utils::fix_this_instance(cx, reason.dupe(), reason, &instance, false, this_name)
    } else {
        result
    }
}

fn merge_class_extends<'cx>(
    env: &MergeEnv,
    cx: &mut Context<'cx>,
    file: &SigFile,
    this: Type,
    reason: Reason,
    extends: &ClassExtends<ALoc, Pack::Packed<ALoc>>,
    mixins: &[ClassMixins<ALoc, Pack::Packed<ALoc>>],
) -> (Type, Type) {
    let super_reason = reason.dupe().replace_desc(None);
    let (super_, static_proto) = match extends {
        ClassExtends::ObjectPrototypeExtendsNull => (
            Type::new(type_::TypeInner::NullProtoT(super_reason.dupe())),
            Type::new(type_::TypeInner::FunProtoT(super_reason.dupe())),
        ),
        ClassExtends::ClassImplicitExtends => (
            Type::new(type_::TypeInner::ObjProtoT(super_reason.dupe())),
            Type::new(type_::TypeInner::FunProtoT(super_reason.dupe())),
        ),
        ClassExtends::ClassExplicitExtends(box (loc, t)) => {
            let reason_op = reason::mk_reason(None, loc.dupe());
            let t = merge_impl(env, cx, file, t, false, false);
            let t = specialize(cx, reason_op, t, AnnotationInferenceOperation::ClassExtends);
            let t = type_util::this_typeapp(t, this.dupe(), None, Some(loc.dupe()));
            let static_proto = type_util::class_type(t.dupe(), None);
            (t, static_proto)
        }
        ClassExtends::ClassExplicitExtendsApp(box (loc, t, targs)) => {
            let t = merge_impl(env, cx, file, t, false, false);
            let targs: Vec<Type> = targs
                .iter()
                .map(|targ| merge_impl(env, cx, file, targ, false, false))
                .collect();
            let t = type_util::this_typeapp(t, this.dupe(), Some(targs), Some(loc.dupe()));
            let static_proto = type_util::class_type(t.dupe(), None);
            (t, static_proto)
        }
    };
    let mut all: Vec<Type> = mixins
        .iter()
        .map(|m| merge_class_mixin(env, cx, file, this.dupe(), m))
        .collect();
    all.push(super_);
    let super_ = match all.len() {
        0 => unreachable!("impossible"),
        1 => all.pop().unwrap(),
        _ => {
            let mut iter = all.into_iter();
            let t0 = iter.next().unwrap();
            let t1 = iter.next().unwrap();
            Type::new(type_::TypeInner::IntersectionT(
                super_reason,
                type_::inter_rep::make(t0, t1, iter.collect::<Vec<_>>().into()),
            ))
        }
    };
    (super_, static_proto)
}

fn merge_class_mixin<'cx>(
    env: &MergeEnv,
    cx: &mut Context<'cx>,
    file: &SigFile,
    this: Type,
    mixin: &ClassMixins<ALoc, Pack::Packed<ALoc>>,
) -> Type {
    use flow_common::reason::VirtualReasonDesc::*;

    fn loop_mixin<'cx>(
        cx: &mut Context<'cx>,
        file: &SigFile,
        packed: &Pack::Packed<ALoc>,
    ) -> (Type, Vec<FlowSmolStr>) {
        match packed {
            Pack::Packed::Eval(box Pack::PackedEval {
                loc,
                packed: t,
                op: Op::GetProp(box name),
            }) => {
                let (t, mut names_rev) = loop_mixin(cx, file, t);
                let t = eval(cx, loc.dupe(), t, Op::GetProp(Box::new(name.dupe())));
                names_rev.push(name.dupe());
                (t, names_rev)
            }
            Pack::Packed::Ref(r) => {
                let f = |_: &mut Context<'_>,
                         t: Type,
                         _ref_loc: ALoc,
                         _def_loc: ALoc,
                         name: &FlowSmolStr| { (t, vec![name.dupe()]) };
                merge_ref(cx, file, f, r, false)
            }
            _ => panic!("unexpected class mixin"),
        }
    }

    let merge_mixin_ref =
        |cx: &mut Context<'cx>, file: &SigFile, loc: ALoc, packed: &Pack::Packed<ALoc>| -> Type {
            let (t, names_rev) = loop_mixin(cx, file, packed);
            let names: Vec<&str> = names_rev.iter().map(|s| s.as_str()).collect();
            let name = names.join(".");
            let reason = reason::mk_annot_reason(RType(FlowSmolStr::new(&name)), loc);
            annotation_inference::mixin(cx, reason, t)
        };

    match mixin {
        ClassMixins::ClassMixin(box (loc, t)) => {
            let reason_op = reason::mk_reason(None, loc.dupe());
            let t = merge_mixin_ref(cx, file, loc.dupe(), t);
            let t = specialize(cx, reason_op, t, AnnotationInferenceOperation::ClassMixins);
            type_util::this_typeapp(t, this, None, Some(loc.dupe()))
        }
        ClassMixins::ClassMixinApp(box (loc, t, targs)) => {
            let t = merge_mixin_ref(cx, file, loc.dupe(), t);
            let targs: Vec<Type> = targs
                .iter()
                .map(|targ| merge_impl(env, cx, file, targ, false, false))
                .collect();
            type_util::this_typeapp(t, this, Some(targs), Some(loc.dupe()))
        }
    }
}

fn merge_this_class_t<'cx>(
    file: &SigFile,
    reason: Reason,
    class_name: Option<FlowSmolStr>,
    id: flow_aloc::ALocId,
    def: ClassSig<ALoc, Pack::Packed<ALoc>>,
    this_reason: Reason,
    inst_kind: type_::InstanceKind,
) -> impl FnOnce(&mut Context<'cx>, &MergeEnv, Vec<(SubstName, Reason, Type, Polarity)>, Type) -> Type
{
    use flow_common::subst_name::SubstName;

    let ClassSig {
        extends,
        implements,
        static_props,
        own_props,
        proto_props,
        computed_own_props,
        computed_proto_props,
        computed_static_props,
        dict,
        tparams: _,
        abstract_,
        abstract_props,
        strictness_kind: _,
    } = def;
    let class_name_owned = class_name;
    move |cx: &mut Context<'cx>,
          env: &MergeEnv,
          targs: Vec<(SubstName, Reason, Type, Polarity)>,
          rec_type: Type| {
        let this = {
            let this_tp = type_::TypeParam::new(type_::TypeParamInner {
                name: SubstName::name(FlowSmolStr::new_inline("this")),
                reason: this_reason.dupe(),
                bound: rec_type,
                polarity: Polarity::Positive,
                default: None,
                is_this: true,
                is_const: false,
            });
            flow_js_utils::generic_of_tparam(cx, |_, x| x.dupe(), &this_tp)
        };
        let (super_, static_proto) =
            merge_class_extends(env, cx, file, this.dupe(), reason.dupe(), &extends, &[]);
        let implements: Vec<Type> = implements
            .iter()
            .map(|t| merge_impl(env, cx, file, t, false, false))
            .collect();
        let mut env = env.dupe();
        env.tps.insert(FlowSmolStr::new_inline("this"), this.dupe());
        let static_ = {
            let static_reason = reason.dupe().replace_desc(None);
            let mut props_map: BTreeMap<Name, type_::Property> = static_props
                .iter()
                .map(|(k, prop)| {
                    let p = merge_class_prop(&env, cx, file, prop, true);
                    (Name::new(k.dupe()), p)
                })
                .collect();
            for (key, prop) in &computed_static_props {
                if let Some(name) = resolve_computed_key(&env, cx, file, key) {
                    let p = merge_class_prop(&env, cx, file, prop, true);
                    match props_map.entry(name) {
                        std::collections::btree_map::Entry::Vacant(e) => {
                            e.insert(p);
                        }
                        std::collections::btree_map::Entry::Occupied(mut e) => {
                            let merged = merge_overloaded_property(e.get(), p);
                            e.insert(merged);
                        }
                    }
                }
            }
            add_name_field(reason.dupe(), &mut props_map);
            let props = type_::properties::PropertiesMap::from_btree_map(props_map);
            obj_type::mk_with_proto(
                cx,
                static_reason,
                type_::ObjKind::Inexact,
                None,
                None,
                Some(props),
                None,
                static_proto,
                false,
            )
        };
        let mut own_props_map: BTreeMap<Name, type_::Property> = BTreeMap::new();
        for (k, prop) in own_props {
            let p = merge_class_prop(&env, cx, file, &prop, false);
            own_props_map.insert(Name::new(k.dupe()), p);
        }
        for (key, prop) in &computed_own_props {
            if let Some(name) = resolve_computed_key(&env, cx, file, key) {
                let p = merge_class_prop(&env, cx, file, prop, false);
                match own_props_map.entry(name) {
                    std::collections::btree_map::Entry::Vacant(e) => {
                        e.insert(p);
                    }
                    std::collections::btree_map::Entry::Occupied(mut e) => {
                        let merged = merge_overloaded_property(e.get(), p);
                        e.insert(merged);
                    }
                }
            }
        }
        let mut proto_props_map: BTreeMap<Name, type_::Property> = BTreeMap::new();
        for (k, prop) in proto_props {
            let p = merge_class_prop(&env, cx, file, &prop, false);
            proto_props_map.insert(Name::new(k.dupe()), p);
        }
        for (key, prop) in &computed_proto_props {
            if let Some(name) = resolve_computed_key(&env, cx, file, key) {
                let p = merge_class_prop(&env, cx, file, prop, false);
                match proto_props_map.entry(name) {
                    std::collections::btree_map::Entry::Vacant(e) => {
                        e.insert(p);
                    }
                    std::collections::btree_map::Entry::Occupied(mut e) => {
                        let merged = merge_overloaded_property(e.get(), p);
                        e.insert(merged);
                    }
                }
            }
        }
        match &inst_kind {
            type_::InstanceKind::RecordKind { defaulted_props } => add_record_constructor(
                cx,
                reason.dupe(),
                class_name_owned.as_ref().unwrap(),
                &own_props_map,
                defaulted_props,
                &mut proto_props_map,
            ),
            _ => {}
        };
        let own_props_pmap: type_::properties::PropertiesMap = own_props_map.into_iter().collect();
        let own_props = cx.generate_property_map(own_props_pmap);
        add_default_constructor(reason.dupe(), &extends, &mut proto_props_map);
        let proto_props_pmap: type_::properties::PropertiesMap =
            proto_props_map.into_iter().collect();
        let proto_props = cx.generate_property_map(proto_props_pmap);
        let inst = type_::InstType::new(type_::InstTypeInner {
            class_id: id.dupe(),
            inst_react_dro: None,
            class_name: class_name_owned.dupe(),
            type_args: targs.into(),
            own_props,
            proto_props,
            inst_call_t: None,
            inst_construct_t: None,
            initialized_fields: flow_data_structure_wrapper::ord_set::FlowOrdSet::new(),
            initialized_static_fields: flow_data_structure_wrapper::ord_set::FlowOrdSet::new(),
            inst_kind,
            inst_dict: dict.as_ref().map(|d| merge_dict(&env, cx, file, d, false)),
            class_private_fields: cx.generate_property_map(type_::properties::PropertiesMap::new()),
            class_private_methods: cx
                .generate_property_map(type_::properties::PropertiesMap::new()),
            class_private_static_fields: cx
                .generate_property_map(type_::properties::PropertiesMap::new()),
            class_private_static_methods: cx
                .generate_property_map(type_::properties::PropertiesMap::new()),
            inst_abstract: abstract_,
            inst_abstract_props: abstract_props.iter().map(|n| Name::new(n.dupe())).collect(),
            strictness_kind: def.strictness_kind,
        });
        type_util::class_type(
            Type::new(type_::TypeInner::ThisInstanceT(Box::new(
                ThisInstanceTData {
                    reason: reason.dupe(),
                    instance: type_::InstanceT::new(type_::InstanceTInner {
                        inst,
                        static_,
                        super_,
                        implements: implements.into(),
                    }),
                    is_this: false,
                    subst_name: SubstName::name(FlowSmolStr::new_inline("this")),
                },
            ))),
            None,
        )
    }
}

fn merge_class<'cx>(
    env: &MergeEnv,
    cx: &mut Context<'cx>,
    file: &SigFile,
    reason: Reason,
    class_name: Option<&FlowSmolStr>,
    id: flow_aloc::ALocId,
    def: &ClassSig<ALoc, Pack::Packed<ALoc>>,
) -> Type {
    use flow_common::reason::VirtualReasonDesc::*;

    let tparams = &def.tparams;
    let this_reason = reason.dupe().replace_desc(RThisType);
    let this_class_t = merge_this_class_t(
        file,
        reason.dupe(),
        class_name.map(|n| n.dupe()),
        id,
        def.clone(),
        this_reason.dupe(),
        type_::InstanceKind::ClassKind,
    );
    let this_reason_c = this_reason.dupe();
    let t = move |cx: &mut Context<'cx>,
                  env: &MergeEnv,
                  targs: Vec<(SubstName, Reason, Type, Polarity)>| {
        let result_cell: Rc<RefCell<Option<Type>>> = Rc::new(RefCell::new(None));
        let result_cell_c = result_cell.dupe();
        let this_reason_for_lazy = this_reason_c.dupe();
        let env_clone = env.dupe();

        let rec_type = flow_typing_tvar::mk_fully_resolved_lazy(
            cx,
            this_reason_for_lazy,
            false,
            move |_cx: &mut Context<'_>| Ok(result_cell_c.borrow().as_ref().unwrap().dupe()),
        );
        let class_t = this_class_t(cx, &env_clone, targs, rec_type);
        *result_cell.borrow_mut() = Some(class_t.dupe());
        class_t
    };
    merge_tparams_targs(env, cx, file, reason, t, tparams, def.strictness_kind)
}

fn merge_record<'cx>(
    env: &MergeEnv,
    cx: &mut Context<'cx>,
    file: &SigFile,
    reason: Reason,
    record_name: Option<&FlowSmolStr>,
    id: flow_aloc::ALocId,
    def: &ClassSig<ALoc, Pack::Packed<ALoc>>,
    defaulted_props: &BTreeSet<FlowSmolStr>,
) -> Type {
    use flow_common::reason::VirtualReasonDesc::*;

    let tparams = &def.tparams;

    let this_reason = reason.dupe().replace_desc(RThisType);
    let this_class_t = merge_this_class_t(
        file,
        reason.dupe(),
        record_name.map(|n| n.dupe()),
        id,
        def.clone(),
        this_reason.dupe(),
        type_::InstanceKind::RecordKind {
            defaulted_props: defaulted_props.iter().duped().collect(),
        },
    );
    let this_reason_c = this_reason.dupe();
    let t = move |cx: &mut Context<'cx>,
                  env: &MergeEnv,
                  targs: Vec<(SubstName, Reason, Type, Polarity)>| {
        let result_cell: Rc<RefCell<Option<Type>>> = Rc::new(RefCell::new(None));
        let result_cell_c = result_cell.dupe();
        let this_reason_for_lazy = this_reason_c.dupe();
        let env_clone = env.dupe();

        let rec_type = flow_typing_tvar::mk_fully_resolved_lazy(
            cx,
            this_reason_for_lazy,
            false,
            move |_cx: &mut Context<'_>| Ok(result_cell_c.borrow().as_ref().unwrap().dupe()),
        );
        let class_t = this_class_t(cx, &env_clone, targs, rec_type);
        *result_cell.borrow_mut() = Some(class_t.dupe());
        class_t
    };
    merge_tparams_targs(env, cx, file, reason, t, tparams, def.strictness_kind)
}

fn merge_fun_statics<'cx, 'sig>(
    env: &MergeEnv,
    cx: &mut Context<'cx>,
    file: &SigFile,
    reason: Reason,
    statics: impl IntoIterator<
        Item = (
            &'sig FlowSmolStr,
            &'sig ALoc,
            &'sig Pack::Packed<ALoc>,
            Polarity,
        ),
    >,
    literal: bool,
) -> Type {
    let props: type_::properties::PropertiesMap = statics
        .into_iter()
        .map(|(key, id_loc, t, polarity)| {
            let t = merge_impl(env, cx, file, t, false, false);
            let prop =
                type_::Property::new(type_::PropertyInner::Field(Box::new(type_::FieldData {
                    preferred_def_locs: None,
                    key_loc: Some(id_loc.dupe()),
                    type_: t,
                    polarity,
                })));
            (Name::new(key.dupe()), prop)
        })
        .collect();
    let reason = reason.replace_desc(None);
    obj_type::mk_with_proto(
        cx,
        reason.dupe(),
        type_::ObjKind::Inexact,
        None,
        None,
        Some(props),
        None,
        Type::new(type_::TypeInner::FunProtoT(reason)),
        literal,
    )
}

fn merge_namespace_symbols<'cx>(
    env: &MergeEnv,
    cx: &mut Context<'cx>,
    file: &SigFile,
    members: &BTreeMap<FlowSmolStr, (ALoc, Pack::Packed<ALoc>)>,
) -> BTreeMap<Name, NamedSymbol> {
    let mut result: BTreeMap<Name, NamedSymbol> = BTreeMap::new();
    for (name, (loc, packed)) in members {
        let t = merge_impl(env, cx, file, packed, false, false);
        result.insert(
            Name::new(name.dupe()),
            NamedSymbol::new(Some(loc.dupe()), None, t, None),
        );
    }
    result
}

fn wrap_with_namespace_types<'cx>(
    cx: &mut Context<'cx>,
    namespace_name: &FlowSmolStr,
    namespace_loc: ALoc,
    values_type: Type,
    namespace_types: &BTreeMap<FlowSmolStr, (ALoc, Pack::Packed<ALoc>)>,
    env: &MergeEnv,
    file: &SigFile,
) -> Type {
    if namespace_types.is_empty() {
        values_type
    } else {
        let namespace_symbol =
            Symbol::mk_namespace_symbol(namespace_name.dupe(), namespace_loc.dupe());
        let namespace_types = merge_namespace_symbols(env, cx, file, namespace_types);
        flow_js_utils::namespace_type_with_values_type(
            cx,
            namespace_symbol,
            values_type,
            &namespace_types,
        )
    }
}

fn merge_fun<'cx>(
    env: &MergeEnv,
    cx: &mut Context<'cx>,
    file: &SigFile,
    reason: Reason,
    function_reference_kind: FunctionReferenceKind,
    def: &FunSig<ALoc, Pack::Packed<ALoc>>,
    statics: Type,
    is_method: bool,
    is_static: bool,
    uses_this: bool,
    is_optional_method: bool,
    literal: bool,
) -> Type {
    use flow_type_sig::type_sig;

    let statics = statics.dupe();
    let reason2 = reason.dupe();
    let def_ref = def;
    let t = |cx: &mut Context<'cx>,
             env: &MergeEnv,
             _tparam_tuples: Vec<(SubstName, Reason, Type, Polarity)>| {
        let params: Vec<type_::FunParam> = def_ref
            .params
            .iter()
            .map(|param| {
                let t = merge_impl(env, cx, file, &param.t, false, false);
                type_::FunParam(param.name.dupe(), t)
            })
            .collect();
        let rest_param = def_ref.rest_param.as_ref().map(|rp| {
            let t = merge_impl(env, cx, file, &rp.t, false, false);
            type_::FunRestParam(rp.name.dupe(), rp.loc.dupe(), t)
        });
        let this_t = match &def_ref.this_param {
            None => {
                if is_method
                    && !is_optional_method
                    && (!is_static || uses_this)
                    && let Some(this) = env.tps.get(&FlowSmolStr::new_inline("this"))
                {
                    if is_static {
                        type_util::class_type(this.dupe(), None)
                    } else {
                        this.dupe()
                    }
                } else {
                    type_::mixed_t::make(reason2.dupe())
                }
            }
            Some(t) => merge_impl(env, cx, file, t, false, false),
        };
        let return_t = merge_impl(env, cx, file, &def_ref.return_, false, false);
        let type_guard = match &def_ref.type_guard {
            None => None,
            Some(type_sig::TypeGuard {
                loc,
                param_name,
                type_guard: t,
                kind,
            }) => {
                let name = &param_name.1;
                let skip_for_rest = match &rest_param {
                    Some(type_::FunRestParam(Some(rest_name), _, _)) if *rest_name == *name => true,
                    _ => false,
                };
                if skip_for_rest
                    || name.as_str() != "this"
                        && params
                            .iter()
                            .all(|p| p.0.as_ref().is_none_or(|pname| *pname != *name))
                    || name.as_str() == "this" && (!is_method || is_static)
                {
                    None
                } else {
                    let tg_reason = reason::mk_reason(None, loc.dupe());
                    Some(type_::TypeGuard::new(type_::TypeGuardInner {
                        reason: tg_reason,
                        kind: *kind,
                        inferred: false,
                        param_name: param_name.dupe(),
                        type_guard: t
                            .as_ref()
                            .map(|t| merge_impl(env, cx, file, t, false, false)),
                    }))
                }
            }
        };
        let this_status = if is_method && def_ref.this_param.is_some() {
            type_::ThisStatus::ThisMethodWithExplicitThis
        } else if is_method {
            type_::ThisStatus::ThisMethod
        } else {
            type_::ThisStatus::ThisFunction
        };
        let effect_ = match &def_ref.effect_ {
            type_sig::ReactEffect::HookDecl(l) => {
                type_::ReactEffectType::HookDecl(cx.make_aloc_id(l))
            }
            type_sig::ReactEffect::HookAnnot => type_::ReactEffectType::HookAnnot,
            type_sig::ReactEffect::ArbitraryEffect => type_::ReactEffectType::ArbitraryEffect,
            type_sig::ReactEffect::AnyEffect => type_::ReactEffectType::AnyEffect,
        };
        let funtype = type_::FunType {
            this_t: (this_t, this_status),
            params: params.into(),
            rest_param,
            return_t,
            type_guard,
            def_reason: reason2.dupe(),
            function_reference_kind,
            effect_,
            strictness_kind: def_ref.strictness_kind,
            literal,
        };
        Type::new(type_::TypeInner::DefT(
            reason2.dupe(),
            type_::DefT::new(type_::DefTInner::FunT(statics.dupe(), Rc::new(funtype))),
        ))
    };
    merge_tparams_targs(env, cx, file, reason, t, &def.tparams, def.strictness_kind)
}

fn merge_component<'cx>(
    env: &MergeEnv,
    cx: &mut Context<'cx>,
    file: &SigFile,
    reason: Reason,
    def: &ComponentSig<ALoc, Pack::Packed<ALoc>>,
    id_opt: Option<(ALoc, &FlowSmolStr)>,
) -> Type {
    let reason2 = reason.dupe();
    let t = |cx: &mut Context<'cx>,
             env: &MergeEnv,
             _: Vec<(SubstName, Reason, Type, Polarity)>|
     -> Type {
        let pmap: type_::properties::PropertiesMap = def
            .params
            .iter()
            .map(|param| {
                let ComponentParam { name, name_loc, t } = param;
                let t = merge_impl(env, cx, file, t, false, false);
                (
                    Name::new(name.dupe()),
                    type_::Property::new(type_::PropertyInner::Field(Box::new(type_::FieldData {
                        preferred_def_locs: None,
                        key_loc: Some(name_loc.dupe()),
                        type_: t,
                        polarity: Polarity::Positive,
                    }))),
                )
            })
            .collect();
        let config_reason = reason::mk_reason(None, def.params_loc.dupe());
        let rest_t = match &def.rest_param {
            None => obj_type::mk_with_proto(
                cx,
                config_reason.dupe(),
                type_::ObjKind::Exact,
                None,
                None,
                None,
                None,
                Type::new(type_::TypeInner::ObjProtoT(config_reason.dupe())),
                false,
            ),
            Some(ComponentRestParam { t }) => merge_impl(env, cx, file, t, false, false),
        };
        let param = Type::new(type_::TypeInner::EvalT {
            type_: rest_t,
            defer_use_t: type_::TypeDestructorT::new(type_::TypeDestructorTInner(
                type_::unknown_use(),
                config_reason,
                Rc::new(type_::Destructor::ReactCheckComponentConfig { props: pmap }),
            )),
            id: type_::eval::Id::generate_id(),
        });
        let renders = merge_impl(env, cx, file, &def.renders, false, false);
        let component_kind = match id_opt {
            None => type_::ComponentKind::Structural,
            Some((loc, name)) => {
                let id = cx.make_aloc_id(&loc);
                type_::ComponentKind::Nominal(id, name.dupe(), None)
            }
        };
        Type::new(type_::TypeInner::DefT(
            reason2.dupe(),
            type_::DefT::new(type_::DefTInner::ReactAbstractComponentT(Box::new(
                type_::ReactAbstractComponentTData {
                    config: param,
                    renders,
                    component_kind,
                },
            ))),
        ))
    };
    merge_tparams_targs(env, cx, file, reason, t, &def.tparams, def.strictness_kind)
}

fn merge_type_alias<'cx>(
    cx: &mut Context<'cx>,
    file: &SigFile,
    reason: Reason,
    custom_error_loc_opt: Option<ALoc>,
    name: &FlowSmolStr,
    tparams: &TParams<ALoc, Pack::Packed<ALoc>>,
    body: &Pack::Packed<ALoc>,
    strictness_kind: type_::TypeStrictnessKind,
) -> Type {
    use flow_common::reason::VirtualReasonDesc::*;

    let name_owned = name.dupe();
    let reason_clone = reason.dupe();
    let t = move |cx: &mut Context<'cx>,
                  env: &MergeEnv,
                  targs: Vec<(SubstName, Reason, Type, Polarity)>| {
        let t = merge_impl(env, cx, file, body, false, false);
        match custom_error_loc_opt {
            Some(custom_error_loc) => {
                let id_loc = reason_clone.loc().dupe();
                let name_for_alias = name_owned.dupe();
                let id_loc_for_alias = id_loc.dupe();
                let nominal_reason = reason_clone.dupe().update_desc(move |desc| {
                    Some(RTypeAlias(Box::new((
                        name_for_alias.dupe(),
                        Some(id_loc_for_alias.dupe()),
                        desc.map(Arc::new),
                    ))))
                });
                let id = type_::nominal::Id::UserDefinedOpaqueTypeId(Box::new(
                    type_::nominal::UserDefinedOpaqueTypeIdData(
                        cx.make_aloc_id(&id_loc),
                        name_owned.dupe(),
                    ),
                ));
                let nominal_type = type_::NominalType::new(type_::NominalTypeInner {
                    underlying_t: type_::nominal::UnderlyingT::CustomError(Box::new(
                        type_::nominal::CustomErrorData {
                            custom_error_loc,
                            t,
                        },
                    )),
                    lower_t: None,
                    upper_t: None,
                    nominal_id: id,
                    nominal_type_args: targs.into(),
                });
                Type::new(type_::TypeInner::DefT(
                    reason_clone.dupe(),
                    type_::DefT::new(type_::DefTInner::TypeT(
                        type_::TypeTKind::TypeAliasKind,
                        Type::new(type_::TypeInner::NominalT {
                            reason: nominal_reason,
                            nominal_type: Rc::new(nominal_type),
                        }),
                    )),
                ))
            }
            None => {
                let id_loc = reason_clone.loc().dupe();
                let name_for_alias = name_owned.dupe();
                let t = type_util::mod_reason_of_t(
                    &move |reason: Reason| {
                        reason.update_desc(|desc| {
                            Some(RTypeAlias(Box::new((
                                name_for_alias.dupe(),
                                Some(id_loc.dupe()),
                                desc.map(Arc::new),
                            ))))
                        })
                    },
                    &t,
                );
                Type::new(type_::TypeInner::DefT(
                    reason_clone.dupe(),
                    type_::DefT::new(type_::DefTInner::TypeT(type_::TypeTKind::TypeAliasKind, t)),
                ))
            }
        }
    };
    merge_tparams_targs(
        &mk_merge_env(FlowOrdMap::new()),
        cx,
        file,
        reason,
        t,
        tparams,
        strictness_kind,
    )
}

fn merge_opaque_type<'cx>(
    cx: &mut Context<'cx>,
    file: &SigFile,
    reason: Reason,
    id: type_::nominal::Id,
    name: &FlowSmolStr,
    tparams: &TParams<ALoc, Pack::Packed<ALoc>>,
    lower_bound: Option<&Pack::Packed<ALoc>>,
    upper_bound: Option<&Pack::Packed<ALoc>>,
    body: Option<&Pack::Packed<ALoc>>,
    strictness_kind: type_::TypeStrictnessKind,
) -> Type {
    use flow_common::reason::VirtualReasonDesc::*;
    let name_owned = name.dupe();
    let reason_clone = reason.dupe();
    let t = move |cx: &mut Context<'cx>,
                  env: &MergeEnv,
                  targs: Vec<(SubstName, Reason, Type, Polarity)>| {
        let opaque_reason = reason_clone
            .dupe()
            .replace_desc(ROpaqueType(name_owned.dupe()));
        let lower_t = lower_bound.map(|t| merge_impl(env, cx, file, t, false, false));
        let upper_t = upper_bound.map(|t| merge_impl(env, cx, file, t, false, false));
        let underlying_t = match body {
            None => type_::nominal::UnderlyingT::FullyOpaque,
            Some(t) => type_::nominal::UnderlyingT::OpaqueWithLocal {
                t: merge_impl(env, cx, file, t, false, false),
            },
        };
        let nominal_type = type_::NominalType::new(type_::NominalTypeInner {
            underlying_t,
            lower_t,
            upper_t,
            nominal_id: id.clone(),
            nominal_type_args: targs.into(),
        });
        Type::new(type_::TypeInner::DefT(
            reason_clone.dupe(),
            type_::DefT::new(type_::DefTInner::TypeT(
                type_::TypeTKind::OpaqueKind,
                Type::new(type_::TypeInner::NominalT {
                    reason: opaque_reason,
                    nominal_type: Rc::new(nominal_type),
                }),
            )),
        ))
    };
    merge_tparams_targs(
        &mk_merge_env(FlowOrdMap::new()),
        cx,
        file,
        reason,
        t,
        tparams,
        strictness_kind,
    )
}

fn merge_declare_class<'cx>(
    cx: &mut Context<'cx>,
    file: &SigFile,
    reason: Reason,
    class_name: &FlowSmolStr,
    id: flow_aloc::ALocId,
    def: &DeclareClassSig<ALoc, Pack::Packed<ALoc>>,
) -> Type {
    use flow_common::reason::VirtualReasonDesc::*;
    use flow_common::subst_name::SubstName;

    let DeclareClassSig {
        tparams,
        extends,
        mixins,
        implements,
        static_props,
        own_props,
        proto_props,
        computed_own_props,
        computed_proto_props,
        computed_static_props,
        static_calls,
        calls,
        constructs,
        dict,
        // We don't actually support static indexers yet.
        static_dict: _,
        abstract_,
        abstract_props,
        strictness_kind: _,
    } = def;
    let this_reason = reason.dupe().replace_desc(RThisType);
    let class_name_owned = class_name.dupe();
    let id_owned = id.dupe();
    let reason_c = reason.dupe();
    let this_reason_c = this_reason.dupe();
    let this_class_t = move |cx: &mut Context<'cx>,
                             env: &MergeEnv,
                             targs: Vec<(SubstName, Reason, Type, Polarity)>,
                             rec_type: Type| {
        let this = {
            let this_tp = type_::TypeParam::new(type_::TypeParamInner {
                name: SubstName::name(FlowSmolStr::new_inline("this")),
                reason: this_reason_c.dupe(),
                bound: rec_type,
                polarity: Polarity::Positive,
                default: None,
                is_this: true,
                is_const: false,
            });
            flow_js_utils::generic_of_tparam(cx, |_, x| x.dupe(), &this_tp)
        };
        let (super_, static_proto) =
            merge_class_extends(env, cx, file, this.dupe(), reason_c.dupe(), extends, mixins);
        let implements: Vec<Type> = implements
            .iter()
            .map(|t| merge_impl(env, cx, file, t, false, false))
            .collect();
        let mut env = env.dupe();
        env.tps.insert(FlowSmolStr::new_inline("this"), this.dupe());
        let static_ = {
            let static_reason = reason_c.dupe().replace_desc(None);
            let mut props_map: BTreeMap<Name, type_::Property> = BTreeMap::new();
            let mut prop_locs: BTreeMap<Name, ALoc> = BTreeMap::new();
            for (k, prop) in static_props {
                let p = merge_interface_prop(&env, cx, file, prop, true);
                let p = name_symbol_field(p, k);
                let name = Name::new(k.dupe());
                record_prop_loc(&mut prop_locs, &name, prop);
                props_map.insert(name, p);
            }
            for (key, prop) in computed_static_props {
                if let Some(name) = resolve_computed_key(&env, cx, file, key) {
                    let t = merge_interface_prop(&env, cx, file, prop, true);
                    // A static member holds its name on the static side alone,
                    // so there is no other map to take it from.
                    insert_computed_class_prop(
                        &mut props_map,
                        None,
                        &mut prop_locs,
                        name,
                        t,
                        interface_prop_source_loc(prop),
                    );
                }
            }
            add_name_field(reason_c.dupe(), &mut props_map);
            let props = type_::properties::PropertiesMap::from_btree_map(props_map);
            let call = {
                let ts: Vec<Type> = static_calls
                    .iter()
                    .map(|t| merge_impl(&env, cx, file, t, false, false))
                    .collect();
                match ts.len() {
                    0 => None,
                    1 => Some(ts.into_iter().next().unwrap()),
                    _ => {
                        let mut iter = ts.into_iter();
                        let t0 = iter.next().unwrap();
                        let t1 = iter.next().unwrap();
                        let rest: Vec<Type> = iter.collect();
                        let r = type_util::reason_of_t(&t0).dupe();
                        Some(Type::new(type_::TypeInner::IntersectionT(
                            r,
                            type_::inter_rep::make(t0, t1, rest.into()),
                        )))
                    }
                }
            };
            obj_type::mk_with_proto(
                cx,
                static_reason,
                type_::ObjKind::Inexact,
                None,
                call,
                Some(props),
                None,
                static_proto,
                false,
            )
        };
        // The two sides are folded apart, since a `declare class` is not
        // structural: the in-file checker keeps a field in one map and a
        // method, a getter or a setter in another, and lets the field shadow
        // whatever the proto side holds under the same name. So only a member
        // on the side the computed key lands on can take the name from it, and
        // each side orders its members on its own.
        let (own_props, proto_props) = {
            let mut own: BTreeMap<Name, type_::Property> = BTreeMap::new();
            let mut own_locs: BTreeMap<Name, ALoc> = BTreeMap::new();
            let mut proto: BTreeMap<Name, type_::Property> = BTreeMap::new();
            let mut proto_locs: BTreeMap<Name, ALoc> = BTreeMap::new();
            for (k, prop) in own_props {
                let p = merge_interface_prop(&env, cx, file, prop, false);
                let p = name_symbol_field(p, k);
                let name = Name::new(k.dupe());
                record_prop_loc(&mut own_locs, &name, prop);
                own.insert(name, p);
            }
            for (k, prop) in proto_props {
                let p = merge_interface_prop(&env, cx, file, prop, false);
                let name = Name::new(k.dupe());
                record_prop_loc(&mut proto_locs, &name, prop);
                proto.insert(name, p);
            }
            add_default_constructor(reason_c.dupe(), extends, &mut proto);
            let mut fold_computed =
                |computed_props: &[(
                    Pack::Packed<ALoc>,
                    InterfaceProp<ALoc, Pack::Packed<ALoc>>,
                )],
                 target: &mut BTreeMap<Name, type_::Property>,
                 prop_locs: &mut BTreeMap<Name, ALoc>| {
                    for (key, prop) in computed_props {
                        if let Some(name) = resolve_computed_key(&env, cx, file, key) {
                            let t = merge_interface_prop(&env, cx, file, prop, false);
                            insert_computed_class_prop(
                                target,
                                None,
                                prop_locs,
                                name,
                                t,
                                interface_prop_source_loc(prop),
                            );
                        }
                    }
                };
            fold_computed(computed_own_props, &mut own, &mut own_locs);
            fold_computed(computed_proto_props, &mut proto, &mut proto_locs);
            let mut to_pmap = |props: BTreeMap<Name, type_::Property>| {
                let pmap: type_::properties::PropertiesMap = props.into_iter().collect();
                cx.generate_property_map(pmap)
            };
            (to_pmap(own), to_pmap(proto))
        };
        let inst_call_t = {
            let ts: Vec<Type> = calls
                .iter()
                .map(|t| merge_impl(&env, cx, file, t, false, false))
                .collect();
            match ts.len() {
                0 => None,
                1 => Some(cx.make_call_prop(ts.into_iter().next().unwrap())),
                _ => {
                    let mut iter = ts.into_iter();
                    let t0 = iter.next().unwrap();
                    let t1 = iter.next().unwrap();
                    let rest: Vec<Type> = iter.collect();
                    let r = type_util::reason_of_t(&t0).dupe();
                    let t = Type::new(type_::TypeInner::IntersectionT(
                        r,
                        type_::inter_rep::make(t0, t1, rest.into()),
                    ));
                    Some(cx.make_call_prop(t))
                }
            }
        };
        let inst_construct_t = {
            let ts: Vec<Type> = constructs
                .iter()
                .map(|t| merge_impl(&env, cx, file, t, false, false))
                .collect();
            match ts.len() {
                0 => None,
                1 => Some(cx.make_call_prop(ts.into_iter().next().unwrap())),
                _ => {
                    let mut iter = ts.into_iter();
                    let t0 = iter.next().unwrap();
                    let t1 = iter.next().unwrap();
                    let rest: Vec<Type> = iter.collect();
                    let r = type_util::reason_of_t(&t0).dupe();
                    let t = Type::new(type_::TypeInner::IntersectionT(
                        r,
                        type_::inter_rep::make(t0, t1, rest.into()),
                    ));
                    Some(cx.make_call_prop(t))
                }
            }
        };
        let inst_dict = dict.as_ref().map(|d| merge_dict(&env, cx, file, d, false));
        let inst = type_::InstType::new(type_::InstTypeInner {
            class_id: id_owned.dupe(),
            inst_react_dro: None,
            class_name: Some(class_name_owned.dupe()),
            type_args: targs.into(),
            own_props,
            proto_props,
            inst_call_t,
            inst_construct_t,
            initialized_fields: flow_data_structure_wrapper::ord_set::FlowOrdSet::new(),
            initialized_static_fields: flow_data_structure_wrapper::ord_set::FlowOrdSet::new(),
            inst_kind: type_::InstanceKind::ClassKind,
            inst_dict,
            class_private_fields: cx.generate_property_map(type_::properties::PropertiesMap::new()),
            class_private_methods: cx
                .generate_property_map(type_::properties::PropertiesMap::new()),
            class_private_static_fields: cx
                .generate_property_map(type_::properties::PropertiesMap::new()),
            class_private_static_methods: cx
                .generate_property_map(type_::properties::PropertiesMap::new()),
            inst_abstract: *abstract_,
            inst_abstract_props: abstract_props.iter().map(|n| Name::new(n.dupe())).collect(),
            strictness_kind: def.strictness_kind,
        });
        type_util::class_type(
            Type::new(type_::TypeInner::ThisInstanceT(Box::new(
                ThisInstanceTData {
                    reason: reason_c.dupe(),
                    instance: type_::InstanceT::new(type_::InstanceTInner {
                        inst,
                        static_,
                        super_,
                        implements: implements.into(),
                    }),
                    is_this: false,
                    subst_name: SubstName::name(FlowSmolStr::new_inline("this")),
                },
            ))),
            None,
        )
    };
    let this_reason_t = this_reason.dupe();
    let t = move |cx: &mut Context<'cx>,
                  env: &MergeEnv,
                  targs: Vec<(SubstName, Reason, Type, Polarity)>| {
        let result_cell: Rc<RefCell<Option<Type>>> = Rc::new(RefCell::new(None));
        let result_cell_c = result_cell.dupe();
        let this_reason_for_lazy = this_reason_t.dupe();
        let env_clone = env.dupe();

        let rec_type = flow_typing_tvar::mk_fully_resolved_lazy(
            cx,
            this_reason_for_lazy,
            false,
            move |_cx: &mut Context<'_>| Ok(result_cell_c.borrow().as_ref().unwrap().dupe()),
        );
        let class_t = this_class_t(cx, &env_clone, targs, rec_type);
        *result_cell.borrow_mut() = Some(class_t.dupe());
        class_t
    };
    merge_tparams_targs(
        &mk_merge_env(FlowOrdMap::new()),
        cx,
        file,
        reason,
        t,
        tparams,
        def.strictness_kind,
    )
}

fn merge_declare_fun<'cx>(
    cx: &mut Context<'cx>,
    file: &SigFile,
    reason: Reason,
    name: &FlowSmolStr,
    id_loc: ALoc,
    defs: &Vec1<(ALoc, ALoc, FunSig<ALoc, Pack::Packed<ALoc>>)>,
    statics: &BTreeMap<FlowSmolStr, (ALoc, Pack::Packed<ALoc>, Polarity)>,
    namespace_types: &BTreeMap<FlowSmolStr, (ALoc, Pack::Packed<ALoc>)>,
) -> Type {
    use flow_common::reason::VirtualReasonDesc::*;
    let env = mk_merge_env(FlowOrdMap::new());
    let statics_t = merge_fun_statics(
        &env,
        cx,
        file,
        reason.dupe(),
        statics
            .iter()
            .map(|(key, (loc, t, polarity))| (key, loc, t, *polarity)),
        false,
    );
    let ts: Vec<Type> = defs
        .iter()
        .map(|(_, fn_loc, def)| {
            let reason = reason::mk_reason(RFunctionType, fn_loc.dupe());
            merge_fun(
                &env,
                cx,
                file,
                reason,
                FunctionReferenceKind::FunctionType,
                def,
                statics_t.dupe(),
                false,
                false,
                false,
                false,
                false,
            )
        })
        .collect();
    let function_t = match ts.len() {
        1 => ts.into_iter().next().unwrap(),
        _ => {
            let mut iter = ts.into_iter();
            let t0 = iter.next().unwrap();
            let t1 = iter.next().unwrap();
            let rest: Vec<Type> = iter.collect();
            let reason = type_util::reason_of_t(&t0).dupe().replace_desc(None);
            Type::new(type_::TypeInner::IntersectionT(
                reason,
                type_::inter_rep::make(t0, t1, rest.into()),
            ))
        }
    };
    wrap_with_namespace_types(cx, name, id_loc, function_t, namespace_types, &env, file)
}

pub fn merge_def<'cx>(
    cx: &mut Context<'cx>,
    file: &SigFile,
    reason: Reason,
    def: &Def<ALoc, Pack::Packed<ALoc>>,
    const_decl: bool,
) -> Type {
    match def {
        Def::TypeAlias(inner) => merge_type_alias(
            cx,
            file,
            reason,
            inner.custom_error_loc_opt.dupe(),
            &inner.name,
            &inner.tparams,
            &inner.body,
            inner.strictness_kind,
        ),
        Def::OpaqueType(inner) => {
            let id = type_::nominal::Id::UserDefinedOpaqueTypeId(Box::new(
                type_::nominal::UserDefinedOpaqueTypeIdData(
                    cx.make_aloc_id(&inner.id_loc),
                    inner.name.dupe(),
                ),
            ));
            merge_opaque_type(
                cx,
                file,
                reason,
                id,
                &inner.name,
                &inner.tparams,
                inner.lower_bound.as_ref(),
                inner.upper_bound.as_ref(),
                inner.body.as_ref(),
                inner.strictness_kind,
            )
        }
        Def::Interface(inner) => {
            let id = cx.make_aloc_id(&inner.id_loc);
            let name = &inner.name;
            let def = &inner.def;
            let tparams = &inner.tparams;
            let t = |cx: &mut Context<'cx>,
                     env: &MergeEnv,
                     targs: Vec<(SubstName, Reason, Type, Polarity)>| {
                let t = merge_interface(
                    env,
                    cx,
                    file,
                    reason.dupe(),
                    Some(name.dupe()),
                    id.dupe(),
                    def,
                    false,
                    false,
                    targs,
                );
                type_util::class_type(t, None)
            };
            merge_tparams_targs(
                &mk_merge_env(FlowOrdMap::new()),
                cx,
                file,
                reason.dupe(),
                t,
                tparams,
                def.strictness_kind,
            )
        }
        Def::ClassBinding(inner) => {
            let id = cx.make_aloc_id(&inner.id_loc);
            let env = mk_merge_env(FlowOrdMap::new());
            let class_t = merge_class(
                &env,
                cx,
                file,
                reason.dupe(),
                Some(&inner.name),
                id,
                &inner.def,
            );
            wrap_with_namespace_types(
                cx,
                &inner.name,
                inner.id_loc.dupe(),
                class_t,
                &inner.namespace_types,
                &env,
                file,
            )
        }
        Def::DeclareClassBinding(inner) => {
            let nominal_id = cx.make_aloc_id(&inner.id_loc);
            let env = mk_merge_env(FlowOrdMap::new());
            let class_t =
                merge_declare_class(cx, file, reason, &inner.name, nominal_id, &inner.def);
            wrap_with_namespace_types(
                cx,
                &inner.name,
                inner.id_loc.dupe(),
                class_t,
                &inner.namespace_types,
                &env,
                file,
            )
        }
        Def::RecordBinding(inner) => {
            let id = cx.make_aloc_id(&inner.id_loc);
            merge_record(
                &mk_merge_env(FlowOrdMap::new()),
                cx,
                file,
                reason,
                Some(&inner.name),
                id,
                &inner.def,
                &inner.defaulted_props,
            )
        }
        Def::FunBinding(inner) => {
            let env = mk_merge_env(FlowOrdMap::new());
            let statics_t = merge_fun_statics(
                &env,
                cx,
                file,
                reason.dupe(),
                inner
                    .statics
                    .iter()
                    .map(|(key, (loc, t, polarity))| (key, loc, t, *polarity)),
                true,
            );
            let function_t = merge_fun(
                &env,
                cx,
                file,
                reason.dupe(),
                FunctionReferenceKind::Function,
                &inner.def,
                statics_t,
                false,
                false,
                false,
                false,
                true,
            );
            wrap_with_namespace_types(
                cx,
                &inner.name,
                inner.id_loc.dupe(),
                function_t,
                &inner.namespace_types,
                &env,
                file,
            )
        }
        Def::DeclareFun(inner) => {
            let mut all_defs = vec![(inner.id_loc.dupe(), inner.fn_loc.dupe(), inner.def.clone())];
            all_defs.extend(inner.tail.iter().cloned());
            let defs = Vec1::try_from_vec(all_defs).unwrap();
            merge_declare_fun(
                cx,
                file,
                reason,
                &inner.name,
                inner.id_loc.dupe(),
                &defs,
                &inner.statics,
                &inner.namespace_types,
            )
        }
        Def::ComponentBinding(inner) => {
            let env = mk_merge_env(FlowOrdMap::new());
            merge_component(
                &env,
                cx,
                file,
                reason,
                &inner.def,
                Some((inner.id_loc.dupe(), &inner.name)),
            )
        }
        Def::Variable(inner) => {
            let env = mk_merge_env(FlowOrdMap::new());
            merge_impl(&env, cx, file, &inner.def, false, const_decl)
        }
        Def::Parameter(inner) => {
            let def = &inner.def;
            let tparams = &inner.tparams;
            let t = |cx: &mut Context<'cx>,
                     env: &MergeEnv,
                     _targs: Vec<(SubstName, Reason, Type, Polarity)>| {
                merge_impl(env, cx, file, def, false, const_decl)
            };
            merge_tparams_targs(
                &mk_merge_env(FlowOrdMap::new()),
                cx,
                file,
                reason,
                t,
                tparams,
                inner.strictness_kind,
            )
        }
        Def::DisabledComponentBinding(_)
        | Def::DisabledEnumBinding(_)
        | Def::DisabledRecordBinding(_) => type_::any_t::error(reason),
        // `rep` may be None (invalid Flow Enum). merge_enum still produces a TS-enum
        // NamespaceT when member_values is present, falling back to AnyT.error only
        // for an invalid Flow Enum with no member_values.
        Def::EnumBinding(inner) => {
            let DefEnumBinding {
                id_loc,
                rep,
                members,
                member_values,
                has_unknown_members,
                name,
            } = inner.as_ref();
            merge_enum(
                cx,
                reason,
                id_loc.dupe(),
                name,
                rep.as_ref(),
                members,
                member_values.as_ref(),
                *has_unknown_members,
            )
        }
        Def::NamespaceBinding(inner) => {
            let (id_loc, name, values, types) =
                (&inner.id_loc, &inner.name, &inner.values, &inner.types);
            let env = mk_merge_env(FlowOrdMap::new());
            let namespace_symbol = Symbol::mk_namespace_symbol(name.dupe(), id_loc.dupe());
            let values = merge_namespace_symbols(&env, cx, file, values);
            let types = merge_namespace_symbols(&env, cx, file, types);
            flow_js_utils::namespace_type(cx, reason, namespace_symbol, &values, &types)
        }
    }
}

pub fn merge_export<'cx>(
    cx: &mut Context<'cx>,
    file: &SigFile,
    export: &Pack::Export<ALoc>,
) -> NamedSymbol {
    match export {
        Pack::Export::ExportRef(ref_) => match ref_ {
            Pack::PackedRef::RemoteRef(box Pack::PackedRefRemote { ref_loc, index }) => {
                let __rc = file.remote_refs.get(*index).dupe();
                let entry = __rc.get_forced(cx);
                let (def_loc, _name, type_, t_for_extends) = entry;
                let type_ = reposition_sig_tvar(cx, ref_loc.dupe(), type_.dupe());
                NamedSymbol::new(
                    Some(def_loc.dupe()),
                    None,
                    type_,
                    Some(t_for_extends.get_forced(cx).dupe()),
                )
            }
            _ => merge_ref(
                cx,
                file,
                |_, type_, _ref_loc, def_loc, _name| {
                    NamedSymbol::new(Some(def_loc), None, type_, None)
                },
                ref_,
                false,
            ),
        },
        Pack::Export::ExportDefault(box Pack::ExportDefaultData { default_loc, def }) => {
            if let Pack::Packed::Ref(ref_) = def {
                match ref_ {
                    Pack::PackedRef::RemoteRef(box Pack::PackedRefRemote { ref_loc, index }) => {
                        let __rc = file.remote_refs.get(*index).dupe();
                        let entry = __rc.get_forced(cx);
                        let (def_loc, _name, type_, t_for_extends) = entry;
                        let type_ = reposition_sig_tvar(cx, ref_loc.dupe(), type_.dupe());
                        NamedSymbol::new(
                            Some(def_loc.dupe()),
                            None,
                            type_,
                            Some(t_for_extends.get_forced(cx).dupe()),
                        )
                    }
                    _ => merge_ref(
                        cx,
                        file,
                        |_, type_, _ref_loc, def_loc, _name| {
                            NamedSymbol::new(Some(def_loc), None, type_, None)
                        },
                        ref_,
                        false,
                    ),
                }
            } else {
                let type_ = merge_impl(
                    &mk_merge_env(FlowOrdMap::new()),
                    cx,
                    file,
                    def,
                    false,
                    false,
                );
                NamedSymbol::new(Some(default_loc.dupe()), None, type_, None)
            }
        }
        Pack::Export::ExportBinding(index) => {
            let __rc = file.local_defs.get(*index).dupe();
            let entry = __rc.get_forced(cx);
            let (loc, _name, _binding_kind, _t_general, t_const) = entry;
            let type_ = t_const.get_forced(cx).dupe();
            NamedSymbol::new(Some(loc.dupe()), None, type_, None)
        }
        Pack::Export::ExportDefaultBinding(box Pack::ExportDefaultBindingData {
            default_loc: _,
            index,
        }) => {
            let __rc = file.local_defs.get(*index).dupe();
            let entry = __rc.get_forced(cx);
            let (loc, _name, _binding_kind, _t_general, t_const) = entry;
            let type_ = t_const.get_forced(cx).dupe();
            NamedSymbol::new(Some(loc.dupe()), None, type_, None)
        }
        Pack::Export::ExportFrom(index) => {
            let __rc = file.remote_refs.get(*index).dupe();
            let entry = __rc.get_forced(cx);
            let (loc, _name, type_, t_for_extends) = entry;
            NamedSymbol::new(
                Some(loc.dupe()),
                None,
                type_.dupe(),
                Some(t_for_extends.get_forced(cx).dupe()),
            )
        }
    }
}

pub fn classify_ts_pending_export<'cx>(
    cx: &mut Context<'cx>,
    file: &SigFile,
    pending: &Pack::TsPendingExport<ALoc>,
) -> TsPendingClassified {
    let mut classify_from_dep = |mref_index: &Index<FlowImportSpecifier>,
                                 remote_name: &FlowSmolStr|
     -> flow_js_utils::ExportClassification {
        let lazy_resolved = file.dependencies.get(mref_index.clone()).1.dupe();
        let resolved = lazy_resolved.get_forced(cx).dupe();
        match resolved {
            ResolvedRequire::TypedModule(f) => match f(cx) {
                Ok(m) => {
                    flow_js_utils::import_export_utils::classify_named_export(cx, &m, remote_name)
                }
                Err(_) => flow_js_utils::ExportClassification::ExportMissing,
            },
            _ => flow_js_utils::ExportClassification::ExportMissing,
        }
    };
    let is_type_only = match pending {
        Pack::TsPendingExport::TsExportRef {
            import_provenance: Some((mref_index, remote_name)),
            ..
        } => matches!(
            classify_from_dep(mref_index, remote_name),
            flow_js_utils::ExportClassification::FoundTypeOnly(_)
        ),
        Pack::TsPendingExport::TsExportRef {
            import_provenance: None,
            ..
        } => false,
        Pack::TsPendingExport::TsExportFrom {
            mref, remote_name, ..
        } => matches!(
            classify_from_dep(mref, remote_name),
            flow_js_utils::ExportClassification::FoundTypeOnly(_)
        ),
    };
    let sym = match pending {
        Pack::TsPendingExport::TsExportRef {
            export_loc, ref_, ..
        } => {
            let merged = merge_export(cx, file, &Pack::Export::ExportRef(ref_.clone()));
            NamedSymbol::new(
                Some(export_loc.dupe()),
                merged.preferred_def_locs.clone(),
                merged.type_.dupe(),
                merged.type_for_extends.dupe(),
            )
        }
        Pack::TsPendingExport::TsExportFrom {
            export_loc,
            mref,
            remote_name,
        } => {
            let remote_ref_node = Pack::RemoteRef::Import {
                id_loc: export_loc.dupe(),
                name: FlowSmolStr::new(""),
                index: *mref,
                remote: remote_name.dupe(),
            };
            let reason = reason::mk_reason(
                reason::VirtualReasonDesc::RIdentifier(remote_name.dupe()),
                export_loc.dupe(),
            );
            let type_ = merge_remote_ref(cx, file, reason.dupe(), &remote_ref_node);
            let type_for_extends = merge_remote_ref_for_extends(cx, file, reason, &remote_ref_node);
            NamedSymbol::new(Some(export_loc.dupe()), None, type_, Some(type_for_extends))
        }
    };
    if is_type_only {
        TsPendingClassified::TsPendingType(sym)
    } else {
        TsPendingClassified::TsPendingValue(sym)
    }
}

pub fn merge_resource_module_t<'cx>(
    cx: &mut Context<'cx>,
    file_key: FileKey,
    filename: &str,
) -> (Reason, LazyModuleType) {
    use flow_common::reason::VirtualReasonDesc::*;

    let exports_t = match std::path::Path::new(filename)
        .extension()
        .and_then(|s| s.to_str())
    {
        Some("css") => {
            let reason = reason::mk_reason(RObjectType, ALoc::none());
            type_::any_t::make(type_::AnySource::Untyped, reason)
        }
        Some(_) => {
            let reason = reason::mk_reason(RString, ALoc::none());
            type_::str_module_t::why(reason)
        }
        None => panic!("How did we find a resource file without an extension?!"),
    };
    let file_loc = ALoc::of_loc(Loc {
        source: Some(file_key),
        ..LOC_NONE
    });
    let reason = reason::mk_reason(RExports, file_loc);
    (
        reason.dupe(),
        mk_commonjs_module_t(cx, reason, cx.is_strict(), None, None, exports_t),
    )
}

pub fn merge<'cx>(
    tps: TparamsMap,
    cx: &mut Context<'cx>,
    file: &SigFile,
    packed: &Pack::Packed<ALoc>,
) -> Type {
    let env = mk_merge_env(tps);
    merge_impl(&env, cx, file, packed, false, false)
}

pub fn merge_cjs_export_t<'cx>(
    cx: &mut Context<'cx>,
    file: &SigFile,
    packed: &Pack::Packed<ALoc>,
) -> (Option<ALoc>, Type) {
    use flow_type_sig::type_sig::Value;
    // We run a special code path for objects in cjs exports,
    // in order to retain the original definition location of exported names
    match packed {
        Pack::Packed::Value(value) => match value.as_ref() {
            Value::ObjLit(inner) => {
                let env = mk_merge_env(FlowOrdMap::new());
                (
                    Some(inner.loc.dupe()),
                    merge_object_lit(
                        &env,
                        cx,
                        file,
                        inner.loc.dupe(),
                        inner.frozen,
                        &inner.proto,
                        &inner.props,
                        true,
                        false,
                        inner.strictness_kind,
                    ),
                )
            }
            Value::ObjSpreadLit(inner) => {
                let env = mk_merge_env(FlowOrdMap::new());
                (
                    Some(inner.loc.dupe()),
                    merge_obj_spread_lit(
                        &env,
                        cx,
                        file,
                        inner.loc.dupe(),
                        inner.frozen,
                        &inner.proto,
                        &inner.elems,
                        true,
                        false,
                        inner.strictness_kind,
                    ),
                )
            }
            _ => (None, merge(FlowOrdMap::new(), cx, file, packed)),
        },
        Pack::Packed::Ref(ref_) => merge_ref(
            cx,
            file,
            |_, type_, _ref_loc, def_loc, _name| (Some(def_loc), type_),
            ref_,
            false,
        ),
        _ => (None, merge(FlowOrdMap::new(), cx, file, packed)),
    }
}

pub fn merge_builtins(
    arena: &mut ContextArena,
    file_key: FileKey,
    builtin_locs: Arc<Table<Loc>>,
    builtins: Arc<packed_type_sig::Builtins<Loc>>,
) -> (
    BTreeMap<FlowSmolStr, flow_typing_builtins::LazyVal<Cx>>,
    BTreeMap<FlowSmolStr, flow_typing_builtins::LazyVal<Cx>>,
    BTreeMap<FlowSmolStr, flow_typing_builtins::LazyModule<Cx>>,
    BTreeMap<FlowSmolStr, flow_typing_builtins::LazyVal<Cx>>,
) {
    let source = Some(file_key.dupe());
    let aloc_table: flow_aloc::LazyALocTable = Rc::new(LazyCell::new(Box::new({
        let file_key = file_key.dupe();
        let builtin_locs = builtin_locs.dupe();
        move || {
            Rc::new(aloc_representation_do_not_use::make_table(
                file_key,
                builtin_locs.iter().cloned().collect(),
            ))
        }
    })
        as Box<dyn FnOnce() -> Rc<ALocTable>>));
    let aloc = {
        let source = source.dupe();
        let aloc_table = aloc_table.dupe();
        move |i: &Index<Loc>| -> ALoc {
            ALoc::of_loc(
                aloc_representation_do_not_use::make_keyed(source.dupe(), i.as_usize() as u32)
                    .to_loc(&aloc_table),
            )
        }
    };
    let aloc = Rc::new(aloc);

    fn local_def_impl(
        aloc: &Rc<impl Fn(&Index<Loc>) -> ALoc + 'static>,
        file_id: SigFileId,
        def: &Pack::PackedDef<Index<Loc>>,
    ) -> Rc<flow_lazy::Lazy<Cx, LocalDefEntry>> {
        let aloc = aloc.dupe();
        let def = def.clone();
        Rc::new(flow_lazy::Lazy::new(move |_cx: &mut Context<'_>| {
            let def = Rc::new(def.map(
                &mut (),
                |_, loc: &Index<Loc>| (*aloc)(loc),
                |_, t: &Pack::Packed<Index<Loc>>| t.map(&|i| (*aloc)(i)),
            ));
            let loc = def.id_loc();
            let name = def.name().dupe();
            let binding_kind = def_binding_kind(&def);
            let reason = def_reason(&def);
            let make_type = |const_decl: bool| -> LazyType {
                let reason = reason.dupe();
                let reason_for_tvar = reason.dupe();
                let def = def.dupe();
                Rc::new(flow_lazy::Lazy::new(move |cx: &mut Context<'_>| {
                    let once_resolved: LazyType =
                        Rc::new(flow_lazy::Lazy::new(move |cx: &mut Context<'_>| {
                            let file = cx.sig_file(file_id);
                            merge_def(cx, &file, reason, &def, const_decl)
                        }));
                    let resolved: LazyType =
                        Rc::new(flow_lazy::Lazy::new(move |cx: &mut Context<'_>| {
                            once_resolved.get_forced(cx).dupe()
                        }));
                    annotation_inference::mk_sig_tvar(cx, reason_for_tvar, resolved)
                }))
            };
            (loc, name, binding_kind, make_type(false), make_type(true))
        }))
    }

    let remote_ref_fn = {
        let aloc = aloc.dupe();
        move |file_id: SigFileId,
              rref: &Pack::RemoteRef<Index<Loc>>|
              -> Rc<flow_lazy::Lazy<Cx, RemoteRefEntry>> {
            let aloc = aloc.dupe();
            let rref = rref.clone();
            Rc::new(flow_lazy::Lazy::new(move |cx: &mut Context<'_>| {
                let rref = rref.map(&|i| (*aloc)(i));
                let loc = rref.loc().dupe();
                let name = rref.name().dupe();
                let reason = remote_ref_reason(&rref);
                let reason2 = reason.dupe();
                let rref2 = rref.clone();
                let resolved: LazyType =
                    Rc::new(flow_lazy::Lazy::new(move |cx: &mut Context<'_>| {
                        let file = cx.sig_file(file_id);
                        merge_remote_ref(cx, &file, reason2, &rref2)
                    }));
                let t = annotation_inference::mk_sig_tvar(cx, reason.dupe(), resolved);
                let reason2 = reason.dupe();
                let rref2 = rref.clone();
                let resolved_for_extends: LazyType =
                    Rc::new(flow_lazy::Lazy::new(move |cx: &mut Context<'_>| {
                        let file = cx.sig_file(file_id);
                        merge_remote_ref_for_extends(cx, &file, reason2, &rref2)
                    }));
                let t_for_extends: LazyType =
                    Rc::new(flow_lazy::Lazy::new(move |cx: &mut Context<'_>| {
                        annotation_inference::mk_sig_tvar(cx, reason, resolved_for_extends)
                    }));
                (loc, name, t, t_for_extends)
            }))
        }
    };

    let pattern_def_fn = {
        let aloc = aloc.dupe();
        move |file_id: SigFileId, def: &Pack::Packed<Index<Loc>>| -> LazyType {
            let aloc = aloc.dupe();
            let def = def.clone();
            Rc::new(flow_lazy::Lazy::new(move |cx: &mut Context<'_>| {
                let def = def.map(&|i| (*aloc)(i));
                let file = cx.sig_file(file_id);
                merge(FlowOrdMap::new(), cx, &file, &def)
            }))
        }
    };

    let pattern_fn = {
        let aloc = aloc.dupe();
        move |file_id: SigFileId, p: &Pack::Pattern<Index<Loc>>| -> LazyType {
            let aloc = aloc.dupe();
            let p = p.clone();
            Rc::new(flow_lazy::Lazy::new(move |cx: &mut Context<'_>| {
                let p = p.map(&|i| (*aloc)(i));
                let file = cx.sig_file(file_id);
                merge_pattern(cx, &file, &p)
            }))
        }
    };

    let map_module = {
        let aloc = aloc.dupe();
        move |file_id: SigFileId,
              module_loc: ALoc,
              module_kind: &Pack::ModuleKind<Index<Loc>>|
              -> flow_typing_builtins::LazyModule<Cx> {
            use flow_common::reason::VirtualReasonDesc::*;
            let reason = reason::mk_reason(RExports, module_loc);

            let aloc_fn = aloc.dupe();
            let module_kind = module_kind.clone();
            let reason2 = reason.dupe();
            Rc::new(flow_lazy::Lazy::new(move |cx: &mut Context<'_>| {
                let type_export = |export: &Pack::TypeExport<Index<Loc>>| -> LazyExport {
                    let export = export.clone();
                    let aloc_fn = aloc_fn.dupe();
                    let reason = reason2.dupe();
                    Rc::new(flow_lazy::Lazy::new(move |cx: &mut Context<'_>| {
                        let export = export.map(&*aloc_fn);
                        let file = cx.sig_file(file_id);
                        merge_type_export(cx, &file, reason, &export)
                    }))
                };
                let cjs_exports_fn = |export: Pack::Packed<Index<Loc>>| -> LazyCjsExport {
                    let aloc_fn = aloc_fn.dupe();
                    Rc::new(flow_lazy::Lazy::new(move |cx: &mut Context<'_>| {
                        let export = export.map(&*aloc_fn);
                        let file = cx.sig_file(file_id);
                        merge_cjs_export_t(cx, &file, &export)
                    }))
                };
                let es_export_fn = |export: &Pack::Export<Index<Loc>>| -> LazyExport {
                    let export = export.clone();
                    let aloc_fn = aloc_fn.dupe();
                    Rc::new(flow_lazy::Lazy::new(move |cx: &mut Context<'_>| {
                        let export = export.map(&*aloc_fn);
                        let file = cx.sig_file(file_id);
                        merge_export(cx, &file, &export)
                    }))
                };
                let cjs_module = |type_exports: &[Pack::TypeExport<Index<Loc>>],
                                  exports: Option<Pack::Packed<Index<Loc>>>,
                                  info: Pack::CJSModuleInfo<Index<Loc>>|
                 -> Exports {
                    let info = info.map(&*aloc_fn);
                    let Pack::CJSModuleInfo {
                        type_export_keys,
                        type_stars,
                        strict,
                        platform_availability_set,
                    } = info;
                    let type_exports = type_exports.iter().map(&type_export).collect::<Vec<_>>();
                    let exports = exports.map(cjs_exports_fn);
                    let type_exports: BTreeMap<FlowSmolStr, LazyExport> =
                        type_export_keys.into_iter().zip(type_exports).collect();
                    Exports::CJSExports {
                        type_exports,
                        exports,
                        type_stars,
                        strict,
                        platform_availability_set,
                    }
                };
                let es_module = |type_exports: &[Pack::TypeExport<Index<Loc>>],
                                 exports: &[Pack::Export<Index<Loc>>],
                                 ts_pending_arr: &[Pack::TsPendingExport<Index<Loc>>],
                                 info: Pack::ESModuleInfo<Index<Loc>>|
                 -> Exports {
                    let info = info.map(&*aloc_fn);
                    let Pack::ESModuleInfo {
                        type_export_keys,
                        export_keys,
                        ts_pending_keys,
                        type_stars,
                        stars,
                        strict,
                        platform_availability_set,
                    } = info;
                    let type_exports = type_exports.iter().map(&type_export).collect::<Vec<_>>();
                    let type_exports: BTreeMap<FlowSmolStr, LazyExport> =
                        type_export_keys.into_iter().zip(type_exports).collect();
                    let exports = exports.iter().map(es_export_fn).collect::<Vec<_>>();
                    let exports: BTreeMap<FlowSmolStr, LazyExport> =
                        export_keys.into_iter().zip(exports).collect();
                    let ts_pending: Vec<(FlowSmolStr, LazyTsPendingClassified)> = ts_pending_keys
                        .into_iter()
                        .zip(ts_pending_arr.iter())
                        .map(|(name, pending)| {
                            let pending = pending.clone();
                            let aloc_fn = aloc_fn.dupe();
                            let classified: LazyTsPendingClassified =
                                Rc::new(flow_lazy::Lazy::new(move |cx: &mut Context<'_>| {
                                    let pending = pending.map(&*aloc_fn);
                                    let file = cx.sig_file(file_id);
                                    classify_ts_pending_export(cx, &file, &pending)
                                }));
                            (name, classified)
                        })
                        .collect();
                    Exports::ESExports {
                        type_exports,
                        exports,
                        ts_pending,
                        type_stars,
                        stars,
                        strict,
                        platform_availability_set,
                    }
                };
                let info = match module_kind {
                    Pack::ModuleKind::CJSModule {
                        type_exports,
                        exports,
                        info,
                    } => cjs_module(&type_exports, exports, info),
                    Pack::ModuleKind::ESModule {
                        type_exports,
                        exports,
                        ts_pending,
                        info,
                    } => es_module(&type_exports, &exports, &ts_pending, info),
                };
                let reason2_ret = reason2.dupe();
                let file = cx.sig_file(file_id);
                let lazy_module = merge_exports(cx, &file, reason2, info);
                (reason2_ret, lazy_module)
            }))
        }
    };

    let mut builtin_modules: BTreeMap<FlowSmolStr, flow_typing_builtins::LazyModule<Cx>> =
        BTreeMap::new();
    let mut builtin_umd_modules = Vec::new();
    let file_id = arena.add_sig_file(|file_id| {
        builtin_modules = builtins
            .global_modules
            .iter()
            .map(|(module_name, module_def)| {
                let module_loc = (*aloc)(&module_def.loc);
                let lazy_module = map_module(file_id, module_loc, &module_def.module_kind);
                (module_name.dupe(), lazy_module)
            })
            .collect();
        builtin_umd_modules = builtins
            .global_umd_modules
            .iter()
            .map(|(name, module_def)| {
                let id_loc = (*aloc)(&module_def.loc);
                let lazy_module = map_module(file_id, id_loc.dupe(), &module_def.module_kind);
                let export_assignment = matches!(
                    module_def.module_kind,
                    Pack::ModuleKind::CJSModule {
                        exports: Some(_),
                        ..
                    }
                );
                (name.dupe(), id_loc, export_assignment, lazy_module)
            })
            .collect();

        let map_module_ref = |specifier: &Userland| -> Rc<flow_lazy::Lazy<Cx, ResolvedRequire>> {
            match builtin_modules.get(specifier.as_str()) {
                None => Rc::new(flow_lazy::Lazy::new(|_cx: &mut Context<'_>| {
                    ResolvedRequire::MissingModule
                })),
                Some(lazy_module) => {
                    let lazy_module = lazy_module.dupe();
                    Rc::new(flow_lazy::Lazy::new(move |cx: &mut Context<'_>| {
                        let (r, lazy_module) = lazy_module.get_forced(cx);
                        let s =
                            type_::constraint::forcing_state::ModuleTypeForcingState::of_lazy_module(
                                r.dupe(),
                                lazy_module.dupe(),
                            );
                        ResolvedRequire::TypedModule(annotation_inference::force_module_type_thunk(
                            cx.id(),
                            s,
                        ))
                    }))
                }
            }
        };

        use flow_common::reason::VirtualReasonDesc::*;
        let exports: ModuleTypeThunk = Rc::new(|_cx: &mut Context<'_>| {
            Err(type_::any_t::annot(reason::locationless_reason(RExports)))
        });

        SigFile {
            dependencies: builtins.module_refs.map(|mref| {
                let resolved = map_module_ref(mref);
                (mref.dupe(), resolved)
            }),
            exports,
            local_defs: builtins
                .local_defs
                .map(|def| local_def_impl(&aloc, file_id, def)),
            remote_refs: builtins
                .remote_refs
                .map(|rref| remote_ref_fn(file_id, rref)),
            pattern_defs: builtins
                .pattern_defs
                .map(|def| pattern_def_fn(file_id, def)),
            patterns: builtins.patterns.map(|p| pattern_fn(file_id, p)),
        }
    });

    let builtin_values: BTreeMap<FlowSmolStr, flow_typing_builtins::LazyVal<Cx>> = builtins
        .global_values
        .iter()
        .map(|(name, i)| {
            let inner_lazy = local_def_impl(&aloc, file_id, builtins.local_defs.get(*i));
            let lazy_val: flow_typing_builtins::LazyVal<Cx> =
                Rc::new(flow_lazy::Lazy::new(move |cx: &mut Context<'_>| {
                    let (loc, _, _, general, _) = inner_lazy.get_forced(cx);
                    (loc.dupe(), general.get_forced(cx).dupe())
                }));
            (name.dupe(), lazy_val)
        })
        .collect();

    let builtin_types: BTreeMap<FlowSmolStr, flow_typing_builtins::LazyVal<Cx>> = builtins
        .global_types
        .iter()
        .map(|(name, i)| {
            let inner_lazy = local_def_impl(&aloc, file_id, builtins.local_defs.get(*i));
            let lazy_val: flow_typing_builtins::LazyVal<Cx> =
                Rc::new(flow_lazy::Lazy::new(move |cx: &mut Context<'_>| {
                    let (loc, _, _, general, _) = inner_lazy.get_forced(cx);
                    (loc.dupe(), general.get_forced(cx).dupe())
                }));
            (name.dupe(), lazy_val)
        })
        .collect();

    // Like TypeScript's `resolveExternalModuleSymbol`, a UMD global is the `export =` value of
    // the module when there is one, and otherwise its namespace object.
    let builtin_umd_values: BTreeMap<FlowSmolStr, flow_typing_builtins::LazyVal<Cx>> =
        builtin_umd_modules
            .into_iter()
            .map(|(name, id_loc, export_assignment, lazy_module)| {
                let lazy_val: flow_typing_builtins::LazyVal<Cx> = Rc::new(flow_lazy::Lazy::new({
                    let name = name.dupe();
                    move |cx: &mut Context<'_>| {
                        let (r, lazy_module) = lazy_module.get_forced(cx);
                        let s =
                            type_::constraint::forcing_state::ModuleTypeForcingState::of_lazy_module(
                                r.dupe(),
                                lazy_module.dupe(),
                            );
                        let resolved_require = ResolvedRequire::TypedModule(
                            annotation_inference::force_module_type_thunk(cx.id(), s),
                        );
                        let reason = reason::mk_reason(
                            reason::VirtualReasonDesc::RIdentifier(name.dupe()),
                            id_loc.dupe(),
                        );
                        let module_name = Userland::from_smol_str(name.dupe());
                        let t = if export_assignment {
                            annotation_inference::import_default(
                                cx,
                                reason,
                                type_::ImportKind::ImportValue,
                                &name,
                                module_name,
                                false,
                                resolved_require,
                            )
                        } else {
                            annotation_inference::import_ns(
                                cx,
                                reason,
                                Symbol::mk_module_symbol(module_name.into_inner(), id_loc.dupe()),
                                false,
                                resolved_require,
                            )
                        };
                        (id_loc.dupe(), t)
                    }
                }));
                (name, lazy_val)
            })
            .collect();

    (
        builtin_values,
        builtin_types,
        builtin_modules,
        builtin_umd_values,
    )
}
