/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::sync::Arc;

use dupe::Dupe;
use flow_aloc::ALoc;
use flow_analysis::bindings;
use flow_analysis::scope_api;
use flow_common::reason::Name;
use flow_common_ty::ty::ALocElt;
use flow_common_ty::ty::Alias;
use flow_common_ty::ty::AliasKind;
use flow_common_ty::ty::Binder;
use flow_common_ty::ty::BinderKind;
use flow_common_ty::ty::Decl;
use flow_common_ty::ty::DeclModuleDeclData;
use flow_common_ty::ty::Elt;
use flow_common_ty::ty::ImportProvenance;
use flow_common_ty::ty::ImportSpecifier;
use flow_common_ty::ty::NamedProp;
use flow_common_ty::ty::ObjKind;
use flow_common_ty::ty::Prop;
use flow_common_ty::ty::Ty;
use flow_common_ty::ty::TypeAtPosResult;
use flow_common_ty::ty::TypeParameterContext;
use flow_common_ty::ty::head_symbol_of_decl;
use flow_common_ty::ty::symbols_of_elt;
use flow_common_ty::ty::symbols_of_elt_without_decl_head;
use flow_common_ty::ty_symbol::ImportMode;
use flow_common_ty::ty_symbol::Provenance;
use flow_common_ty::ty_symbol::Symbol;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_parser::ast;
use flow_parser::loc::Loc;
use flow_parser_utils::file_sig::FileSig;
use flow_parser_utils::file_sig::Require;
use flow_typing_context::Context;
use flow_typing_debug;
use flow_typing_flow_js::flow_js::FlowJs;
use flow_typing_ty_normalizer::env::EvaluateTypeDestructorsMode;
use flow_typing_ty_normalizer::env::Options;
use flow_typing_ty_normalizer::normalizer::Error;
use flow_typing_type::type_::Type;
use flow_typing_type::type_::TypeInner;
use flow_typing_type::type_util;
use flow_typing_utils::convert_types;
use flow_typing_utils::type_env;
use flow_typing_utils::typed_ast_utils;
use serde_json::Value as Json;

use crate::ty_normalizer_flow;
use crate::typed_ast_finder;
use crate::typed_ast_finder::type_at_pos::Framing;
use crate::typed_ast_finder::type_at_pos::TypeAtPosResult as FinderResult;

pub enum QueryResult<A> {
    FailureNoMatch,
    FailureUnparseable(Loc, Type, String),
    Success(Loc, A),
}
fn concretize_loc_pairs<T>(pair_list: Vec<(ALoc, T)>) -> Vec<(Loc, T)> {
    pair_list
        .into_iter()
        .map(|(loc, x)| (loc.to_loc_exn().dupe(), x))
        .collect()
}

fn sort_loc_pairs<T>(mut pair_list: Vec<(Loc, T)>) -> Vec<(Loc, T)> {
    pair_list.sort_by(|(a, _), (b, _)| a.cmp(b));
    pair_list
}

fn result_of_normalizer_error<A>(loc: Loc, t: Type, err: Error) -> QueryResult<A> {
    let msg = err.to_string();
    QueryResult::FailureUnparseable(loc, t, msg)
}

/// Matches the depth member extraction uses elsewhere (autocomplete).
const MAX_DEPTH_OF_MEMBER_LOOKUP: u32 = 40;

/// The declaration form of a binding, for the kinds whose hover reads better as a
/// declaration than as a bare type. Kinds that already print a head of their own
/// (`class A`, `type X = …`) and kinds that need more than a name and a type
/// (imports, which name a declaration in another module) are left unframed.
pub fn binder_kind_of_binding_kind(kind: bindings::Kind) -> Option<BinderKind> {
    match kind {
        bindings::Kind::Var | bindings::Kind::DeclaredVar => Some(BinderKind::Var),
        bindings::Kind::Let | bindings::Kind::DeclaredLet => Some(BinderKind::Let),
        bindings::Kind::Const | bindings::Kind::DeclaredConst => Some(BinderKind::Const),
        bindings::Kind::Function | bindings::Kind::DeclaredFunction => Some(BinderKind::Function),
        bindings::Kind::Parameter
        | bindings::Kind::CatchParameter
        | bindings::Kind::ComponentParameter => Some(BinderKind::Parameter),
        bindings::Kind::TypeParam => Some(BinderKind::TypeParameter),
        bindings::Kind::ThisAnnot
        | bindings::Kind::Type { .. }
        | bindings::Kind::Interface { .. }
        | bindings::Kind::Enum
        | bindings::Kind::Class
        | bindings::Kind::DeclaredClass
        | bindings::Kind::DeclaredNamespace
        | bindings::Kind::Import { .. }
        | bindings::Kind::TsImport
        | bindings::Kind::Internal
        | bindings::Kind::Component
        | bindings::Kind::Record => None,
    }
}

/// True for the binding kinds that name something declared in another module, so
/// that a use of an imported name is framed as an alias for it.
fn is_imported_binding_kind(kind: bindings::Kind) -> bool {
    match kind {
        bindings::Kind::Import { .. } | bindings::Kind::TsImport => true,
        bindings::Kind::Type { imported, .. } | bindings::Kind::Interface { imported, .. } => {
            imported
        }
        bindings::Kind::Var
        | bindings::Kind::DeclaredVar
        | bindings::Kind::Let
        | bindings::Kind::DeclaredLet
        | bindings::Kind::Const
        | bindings::Kind::DeclaredConst
        | bindings::Kind::Function
        | bindings::Kind::DeclaredFunction
        | bindings::Kind::Parameter
        | bindings::Kind::CatchParameter
        | bindings::Kind::ComponentParameter
        | bindings::Kind::ThisAnnot
        | bindings::Kind::TypeParam
        | bindings::Kind::Enum
        | bindings::Kind::Class
        | bindings::Kind::DeclaredClass
        | bindings::Kind::DeclaredNamespace
        | bindings::Kind::Internal
        | bindings::Kind::Component
        | bindings::Kind::Record => false,
    }
}

/// What a hover prints ahead of the type: a declaration head, and the `(alias)`
/// marker with the statement that introduced the alias. The two are independent —
/// an imported class has an alias but no binder of its own, an exported local
/// `const` has both.
#[derive(Default)]
struct Framed {
    binder: Option<Binder>,
    alias: Option<Alias>,
}

/// How many signatures a binding has beyond the one hover is about to print.
///
/// Flow spells an overload as a name declared more than once, so the extra
/// declaration sites the binding records are the extra signatures. Only
/// functions overload; any other kind with repeated declarations is a redeclared
/// name, which is a different thing and gets no count.
fn overloads_of_def<L>(kind: BinderKind, def: &scope_api::Def<L>) -> u32 {
    match kind {
        BinderKind::Function => (def.locs.len() - 1) as u32,
        _ => 0,
    }
}

fn import_provenance(file_sig: &FileSig, local_name: &FlowSmolStr) -> Option<ImportProvenance> {
    file_sig.requires().iter().find_map(|require| {
        let Require::Import {
            source,
            named,
            ns,
            types,
            typesof,
            typesof_ns,
            type_ns,
            ..
        } = require
        else {
            return None;
        };
        let named = [
            (ImportMode::ValueMode, named),
            (ImportMode::TypeMode, types),
            (ImportMode::TypeofMode, typesof),
        ]
        .into_iter()
        .find_map(|(mode, imports)| {
            imports.iter().find_map(|(remote_name, locals)| {
                locals.contains_key(local_name).then(|| {
                    let specifier = if remote_name.as_str() == "default" {
                        ImportSpecifier::Default
                    } else {
                        ImportSpecifier::Named {
                            remote_name: remote_name.dupe(),
                        }
                    };
                    (mode, specifier)
                })
            })
        });
        let namespace = [
            (ImportMode::ValueMode, ns),
            (ImportMode::TypeMode, type_ns),
            (ImportMode::TypeofMode, typesof_ns),
        ]
        .into_iter()
        .find_map(|(mode, namespace)| {
            namespace
                .as_ref()
                .is_some_and(|namespace| &namespace.1 == local_name)
                .then_some((mode, ImportSpecifier::Namespace))
        });
        named
            .or(namespace)
            .map(|(mode, specifier)| ImportProvenance {
                mode,
                specifier,
                source: source.name().dupe(),
            })
    })
}

fn framed_of_identifier_reference(cx: &mut Context<'_>, file_sig: &FileSig, loc: &ALoc) -> Framed {
    let env = cx.environment();
    let Some(def) = env.var_info.scopes.def_of_use_opt(loc) else {
        return Framed::default();
    };
    let binder = binder_kind_of_binding_kind(def.kind).map(|kind| Binder {
        kind,
        name: def.actual_name.dupe(),
        owner: None,
        type_parameter_context: None,
        overloads: overloads_of_def(kind, def),
        targs: None,
    });
    let alias = is_imported_binding_kind(def.kind).then(|| Alias {
        kind: AliasKind::Import,
        name: def.actual_name.dupe(),
        import: import_provenance(file_sig, &def.actual_name),
    });
    Framed { binder, alias }
}

fn is_local_definition_site<F>(
    cx: &mut Context<'_>,
    loc_of_aloc: &F,
    loc: &Loc,
    head_name: Option<&FlowSmolStr>,
) -> bool
where
    F: Fn(&ALoc) -> Loc + ?Sized,
{
    let Some(head_name) = head_name else {
        return false;
    };
    let aloc = ALoc::of_loc(loc.dupe());
    cx.environment()
        .var_info
        .scopes
        .def_of_use_opt(&aloc)
        .is_some_and(|def| {
            !is_imported_binding_kind(def.kind)
                && &def.actual_name == head_name
                && def.locs.iter().any(|def_loc| &loc_of_aloc(def_loc) == loc)
        })
}

/// The `(index)` head name for the first constituent carrying a string-keyed
/// index signature, traversing the same shapes as `named_prop_of_ty`. Only
/// string keys: every caller is a named access, which a number-keyed indexer
/// does not serve. The label is the user-written one (`[k: string]`), falling
/// back to `[string]` when the signature is anonymous.
fn index_head_name_of_ty(ty: &Ty<ALoc>) -> Option<FlowSmolStr> {
    match ty {
        Ty::Obj(obj) => {
            if let ObjKind::IndexedObj(dict) = &obj.obj_kind
                && matches!(&*dict.dict_key, Ty::Str)
            {
                let head = match &dict.dict_name {
                    Some(name) => FlowSmolStr::new(format!("[{name}: string]")),
                    None => FlowSmolStr::new("[string]"),
                };
                return Some(head);
            }
            obj.obj_props.iter().find_map(|p| match p {
                Prop::SpreadProp(t) => index_head_name_of_ty(t),
                _ => None,
            })
        }
        Ty::Fun(fun) => index_head_name_of_ty(&fun.fun_static),
        Ty::Union(_, t1, t2, ts) | Ty::Inter(t1, t2, ts) => [t1, t2]
            .into_iter()
            .chain(ts.iter())
            .find_map(|t| index_head_name_of_ty(t)),
        _ => None,
    }
}

/// The first constituent of a union or intersection that has the property is
/// enough, since only the property's kind is read off the result, not its type.
fn named_prop_of_ty<'a>(ty: &'a Ty<ALoc>, name: &Name) -> Option<&'a NamedProp<ALoc>> {
    match ty {
        Ty::Obj(obj) => obj.obj_props.iter().find_map(|p| match p {
            Prop::NamedProp {
                name: prop_name,
                prop,
                ..
            } if prop_name == name => Some(prop),
            Prop::SpreadProp(t) => named_prop_of_ty(t, name),
            _ => None,
        }),
        Ty::Fun(fun) => named_prop_of_ty(&fun.fun_static, name),
        Ty::Union(_, t1, t2, ts) | Ty::Inter(t1, t2, ts) => [t1, t2]
            .into_iter()
            .chain(ts.iter())
            .find_map(|t| named_prop_of_ty(t, name)),
        _ => None,
    }
}

fn name_of_symbol(symbol: &Symbol<ALoc>) -> Option<FlowSmolStr> {
    if symbol.sym_anonymous {
        None
    } else {
        Some(FlowSmolStr::new(symbol.sym_name.as_str()))
    }
}

/// The name to qualify a member binder with: the `A` in `(property) A.p`.
fn owner_of_receiver_ty(ty: &Ty<ALoc>) -> Option<FlowSmolStr> {
    match ty {
        Ty::Generic(generic) => {
            let (symbol, _, _) = generic.as_ref();
            name_of_symbol(symbol)
        }
        Ty::Union(_, t1, t2, ts) => [t1, t2]
            .into_iter()
            .chain(ts.iter())
            .filter(|t| !matches!(t.as_ref(), Ty::Null | Ty::Void | Ty::Undefined | Ty::TsVoid))
            .map(|t| owner_of_receiver_ty(t))
            .reduce(|a, b| if a == b { a } else { None })
            .flatten(),
        _ => None,
    }
}

/// The receiver's qualifying name, and whether the receiver is an enum.
fn owner_of_receiver_elt(elt: &Elt<ALoc>) -> (Option<FlowSmolStr>, bool) {
    match elt {
        Elt::Type(ty) => (owner_of_receiver_ty(ty), false),
        Elt::Decl(decl) => match decl {
            Decl::ClassDecl(data) => (name_of_symbol(&data.0), false),
            Decl::InterfaceDecl(data) => (name_of_symbol(&data.0), false),
            Decl::RecordDecl(data) => (name_of_symbol(&data.0), false),
            Decl::EnumDecl(data) => (name_of_symbol(&data.name), true),
            Decl::NamespaceDecl(data) => (data.name.as_ref().and_then(name_of_symbol), false),
            Decl::VariableDecl(_)
            | Decl::TypeAliasDecl(_)
            | Decl::NominalComponentDecl(_)
            | Decl::ModuleDecl(_) => (None, false),
        },
    }
}

/// The declaration a `o.p` access refers to, so that hovering it prints
/// `(property) A.p: T` or `(method) A.m(): T`. The object's type has to be expanded
/// to tell which, since the property's own type looks the same either way.
fn binder_of_member_reference(
    cx: &mut Context<'_>,
    file_sig: Arc<FileSig>,
    typed_ast_opt: Option<&ast::Program<ALoc, (ALoc, Type)>>,
    object_type: &Type,
    name: &FlowSmolStr,
) -> Option<Binder> {
    let prop_name = Name::new(name.dupe());
    // A receiver like `super` can still be an open type variable after
    // checking, which member expansion sees opaquely. Resolve settled
    // constraints first, falling back to the type as found.
    let resolved = cx
        .find_resolved(object_type)
        .unwrap_or_else(|| object_type.dupe());
    let object_type = &resolved;
    // A `super` (or `this`) receiver resolves to a type application over its
    // class, which member expansion otherwise reads as the class object.
    // Force the instance view so instance members resolve.
    let force_instance = matches!(&**object_type, TypeInner::ThisTypeAppT(_));
    // Expanding members flows types, which can raise errors. Server state persists
    // across IDE requests, so they are rolled back rather than left behind.
    let (expanded, object_ty) = cx.run_and_rolled_back_cache(|cx| {
        let errors = cx.errors();
        let genv = ty_normalizer_flow::mk_genv(
            Options {
                expand_internal_types: true,
                expand_enum_members: false,
                evaluate_type_destructors: EvaluateTypeDestructorsMode::EvaluateNone,
                optimize_types: false,
                omit_targ_defaults_option: false,
                merge_bot_and_any_kinds: true,
                verbose_normalizer: false,
                max_depth: Some(MAX_DEPTH_OF_MEMBER_LOOKUP),
                max_type_size: None,
                toplevel_is_type_identifier_reference: false,
            },
            typed_ast_opt,
            file_sig,
        );
        let expanded = ty_normalizer_flow::expand_members(
            force_instance,
            Some(vec![prop_name.dupe()]),
            cx,
            &genv,
            object_type,
        );
        let object_ty = ty_normalizer_flow::from_type(cx, &genv, object_type);
        cx.reset_errors(errors);
        (expanded, object_ty)
    });
    let expanded = expanded.ok()?;
    let named_prop = named_prop_of_ty(&expanded, &prop_name);
    if named_prop.is_none()
        && let Some(name) = index_head_name_of_ty(&expanded)
    {
        // A named access that hits an index signature has no declared
        // member; frame the index signature itself.
        return Some(Binder {
            kind: BinderKind::Index,
            name,
            owner: None,
            type_parameter_context: None,
            overloads: 0,
            targs: None,
        });
    }
    let named_prop = named_prop?;
    let (owner, receiver_is_enum) = match &object_ty {
        Ok(elt) => owner_of_receiver_elt(elt),
        Err(_) => (None, false),
    };
    let kind = match named_prop {
        // An enum's own methods (`cast`, `members`, …) share the object its members
        // are expanded into, so being a field is what separates the two.
        NamedProp::Field { .. } if receiver_is_enum => BinderKind::EnumMember,
        NamedProp::Field { .. } => BinderKind::Property,
        NamedProp::Method { .. } => BinderKind::Method,
        NamedProp::Get(_) => BinderKind::Getter,
        NamedProp::Set(_) => BinderKind::Setter,
    };
    Some(Binder {
        kind,
        name: name.dupe(),
        owner,
        type_parameter_context: None,
        // `NamedProp::Method` holds one signature, so by the time a member has
        // been looked up an overloaded method is indistinguishable from a plain
        // one and there is nothing left to count.
        overloads: 0,
        targs: None,
    })
}
pub fn dump_type_at_pos(
    cx: &mut Context<'_>,
    typed_ast: &ast::Program<ALoc, (ALoc, Type)>,
    loc: Loc,
) -> Result<Option<(Loc, String)>, flow_utils_concurrency::job_error::JobError> {
    Ok(
        match typed_ast_finder::find_type_at_pos_annotation(cx, typed_ast, loc)? {
            FinderResult::NoResult => None,
            FinderResult::HardcodedModuleResult(loc, _) => Some((loc, "ModuleT".to_string())),
            FinderResult::TypeResult { loc, type_, .. } => {
                Some((loc, flow_typing_debug::dump_t(Some(10), cx, &type_)))
            }
        },
    )
}

pub fn type_at_pos_type<'a>(
    cx: &mut Context<'a>,
    file_sig: Arc<FileSig>,
    omit_targ_defaults: bool,
    verbose_normalizer: bool,
    max_depth: u32,
    typed_ast: &ast::Program<ALoc, (ALoc, Type)>,
    no_typed_ast_for_imports: bool,
    include_refs: Option<&dyn Fn(&ALoc) -> Loc>,
    remote_binding_kind: Option<&dyn Fn(&ALoc) -> Option<BinderKind>>,
    loc: Loc,
) -> Result<QueryResult<TypeAtPosResult>, flow_utils_concurrency::job_error::JobError> {
    Ok(
        match typed_ast_finder::find_type_at_pos_annotation(cx, typed_ast, loc)? {
            FinderResult::NoResult => QueryResult::FailureNoMatch,
            FinderResult::HardcodedModuleResult(loc, name) => {
                let module_symbol = Symbol {
                    sym_provenance: Provenance::Local,
                    sym_name: FlowSmolStr::new(name),
                    sym_anonymous: false,
                    sym_def_loc: ALoc::of_loc(loc.dupe()),
                };
                let ty = Elt::Decl(Decl::ModuleDecl(Box::new(DeclModuleDeclData {
                    name: Some(module_symbol),
                    exports: Arc::from([]),
                    default: None,
                })));
                QueryResult::Success(
                    loc,
                    TypeAtPosResult {
                        ty,
                        refs: None,
                        binder: None,
                        alias: None,
                    },
                )
            }
            FinderResult::TypeResult {
                loc,
                is_type_identifier_reference: toplevel_is_type_identifier_reference,
                type_: t,
                framing,
                alias: found_alias,
            } => {
                let typed_ast_opt = if no_typed_ast_for_imports {
                    None
                } else {
                    Some(typed_ast)
                };
                let type_parameter_context = match &framing {
                    Some(Framing::Binder {
                        type_parameter_context,
                        ..
                    })
                    | Some(Framing::TypeIdentifierRef {
                        type_parameter_context,
                    }) => type_parameter_context.dupe(),
                    _ => None,
                };
                let Framed { mut binder, alias } = match framing {
                    None => Framed::default(),
                    Some(Framing::Binder { binder, .. }) => Framed {
                        binder: Some(binder),
                        alias: None,
                    },
                    Some(Framing::IdentifierRef) => {
                        framed_of_identifier_reference(cx, &file_sig, &ALoc::of_loc(loc.dupe()))
                    }
                    Some(Framing::RemoteIdentifierRef { def_loc, name }) => Framed {
                        binder: def_loc
                            .as_ref()
                            .and_then(|loc| remote_binding_kind.and_then(|resolve| resolve(loc)))
                            .map(|kind| Binder {
                                kind,
                                name,
                                owner: None,
                                type_parameter_context: None,
                                overloads: 0,
                                targs: None,
                            }),
                        alias: None,
                    },
                    Some(Framing::RenamedExportRef { local, name }) => {
                        let mut framed = framed_of_identifier_reference(cx, &file_sig, &local);
                        if let Some(binder) = framed.binder.as_mut() {
                            binder.name = name;
                        }
                        framed
                    }
                    Some(Framing::TypeIdentifierRef { .. }) => {
                        let framed = framed_of_identifier_reference(
                            cx,
                            &file_sig,
                            &ALoc::of_loc(loc.dupe()),
                        );
                        Framed {
                            // Every other kind of type name prints a declaration of
                            // its own, so framing it again would double the head.
                            binder: framed
                                .binder
                                .filter(|b| b.kind == BinderKind::TypeParameter),
                            alias: framed.alias,
                        }
                    }
                    Some(Framing::MemberRef { name, object_type }) => Framed {
                        binder: binder_of_member_reference(
                            cx,
                            file_sig.dupe(),
                            typed_ast_opt,
                            &object_type,
                            &name,
                        ),
                        alias: None,
                    },
                };
                // The finder saw the statement itself, so it knows whether the
                // alias is an import or an export and what name it writes; the
                // binding lookup can only ever report an import.
                let found_alias = found_alias.map(|mut found| {
                    if found.kind == AliasKind::Import {
                        found.import = import_provenance(&file_sig, &found.name);
                    }
                    found
                });
                let alias = match (found_alias, alias) {
                    (Some(mut found), Some(resolved)) => {
                        if found.kind == AliasKind::Import {
                            found.import = resolved.import;
                        }
                        Some(found)
                    }
                    (Some(found), None) => Some(found),
                    (None, resolved) => resolved,
                };
                let options = |evaluate_type_destructors: EvaluateTypeDestructorsMode| Options {
                    expand_internal_types: false,
                    expand_enum_members: false,
                    evaluate_type_destructors,
                    optimize_types: true,
                    omit_targ_defaults_option: omit_targ_defaults,
                    merge_bot_and_any_kinds: true,
                    verbose_normalizer,
                    max_depth: Some(max_depth),
                    max_type_size: None,
                    toplevel_is_type_identifier_reference,
                };

                let from_type = |cx: &mut Context<'_>, t: &Type, evaluate_type_destructors| {
                    let options = options(evaluate_type_destructors);
                    let genv = ty_normalizer_flow::mk_genv(options, typed_ast_opt, file_sig.dupe());
                    ty_normalizer_flow::from_type_with_found_computed_type(cx, &genv, t)
                };
                let callee_targs = cx.get_callee_targs(&ALoc::of_loc(loc.dupe()));
                // This pass evaluates the destructors it judges worth evaluating, so
                // it has to leave the server's caches and error set as it found them:
                // that state outlives the request, and a hover must not change how the
                // next one behaves. Its result is kept whatever the errors say -- there
                // is no second form to fall back on.
                let (ty, type_parameter_context, targs) = cx.run_and_rolled_back_cache(|cx| {
                    let errors = cx.errors();
                    let (ty, _) = from_type(cx, &t, EvaluateTypeDestructorsMode::EvaluateForHover);
                    let type_parameter_context = match type_parameter_context.as_ref() {
                        None => Ok(None),
                        Some(context) => {
                            let (declaration, _) = from_type(
                                cx,
                                &context.declaration,
                                EvaluateTypeDestructorsMode::EvaluateForHover,
                            );
                            declaration.map(|declaration| {
                                Some(Arc::new(TypeParameterContext {
                                    name: context.name.dupe(),
                                    declaration,
                                }))
                            })
                        }
                    };
                    // Type arguments are shown all or none: one that does not
                    // normalize to a type drops them all.
                    let targs = callee_targs.and_then(|targs| {
                        targs
                            .iter()
                            .map(|t| {
                                let (ty, _) =
                                    from_type(cx, t, EvaluateTypeDestructorsMode::EvaluateForHover);
                                match ty {
                                    Ok(Elt::Type(t)) => Some(t),
                                    Ok(Elt::Decl(_)) | Err(_) => None,
                                }
                            })
                            .collect::<Option<Arc<[_]>>>()
                    });
                    cx.reset_errors(errors);
                    (ty, type_parameter_context, targs)
                });
                match ty.and_then(|ty| {
                    type_parameter_context
                        .map(|type_parameter_context| (ty, type_parameter_context))
                }) {
                    Ok((ty, type_parameter_context)) => {
                        if let Some(binder) = binder.as_mut()
                            && binder.kind == BinderKind::TypeParameter
                        {
                            binder.type_parameter_context = type_parameter_context;
                        }
                        let refs = include_refs.map(|loc_of_aloc| {
                            let head_name = match &ty {
                                Elt::Decl(d) => head_symbol_of_decl(d).and_then(name_of_symbol),
                                Elt::Type(_) => None,
                            };
                            if is_local_definition_site(cx, loc_of_aloc, &loc, head_name.as_ref()) {
                                symbols_of_elt_without_decl_head(loc_of_aloc, &ty)
                            } else {
                                symbols_of_elt(loc_of_aloc, &ty)
                            }
                        });
                        // An aliased callable is headed `function f(…)`. A
                        // non-callable value import is headed `const x: T`, since
                        // the local import binding cannot be reassigned. Declarations
                        // such as classes and type aliases already print their own
                        // heads.
                        let binder = binder.or_else(|| {
                            let alias = alias.as_ref()?;
                            let kind = match &ty {
                                Elt::Type(t) if matches!(&**t, Ty::Fun(_)) => BinderKind::Function,
                                Elt::Type(_) if alias.kind == AliasKind::Import => {
                                    BinderKind::Const
                                }
                                Elt::Type(_) | Elt::Decl(_) => return None,
                            };
                            Some(Binder {
                                kind,
                                name: alias.name.dupe(),
                                owner: None,
                                type_parameter_context: None,
                                // The declaration sites are in the exporting
                                // module, so there is nothing here to count. An
                                // overloaded import is an intersection anyway, and
                                // `Ty::Fun` only matches a lone signature.
                                overloads: 0,
                                targs: None,
                            })
                        });
                        let binder = binder.map(|binder| Binder { targs, ..binder });
                        QueryResult::Success(
                            loc,
                            TypeAtPosResult {
                                ty,
                                refs,
                                binder,
                                alias,
                            },
                        )
                    }
                    Err(err) => result_of_normalizer_error(loc, t, err),
                }
            }
        },
    )
}

pub fn dump_types<'a>(
    printer: &dyn Fn(&ALocElt) -> String,
    evaluate_type_destructors: EvaluateTypeDestructorsMode,
    cx: &mut Context<'a>,
    file_sig: Arc<FileSig>,
    typed_ast: &ast::Program<ALoc, (ALoc, Type)>,
) -> Vec<(Loc, String)> {
    let options = Options {
        evaluate_type_destructors,
        ..Options::default()
    };
    let genv = ty_normalizer_flow::mk_genv(options, Some(typed_ast), file_sig);
    let result = ty_normalizer_flow::from_types(
        None,
        cx,
        &genv,
        typed_ast_utils::typed_ast_to_list(typed_ast),
    );
    let print_ok = |(l, r): (ALoc, Result<ALocElt, Error>)| -> Option<(ALoc, String)> {
        match r {
            Ok(t) => Some((l, printer(&t))),
            _ => None,
        }
    };
    let filtered: Vec<_> = result.into_iter().filter_map(print_ok).collect();
    sort_loc_pairs(concretize_loc_pairs(filtered))
}

/// Returns the per-location types and, when `dedup_threshold` is set, the
/// shared `$defs` table those locations refer to via `{"$ref": id}`.
pub fn dump_types_for_tool(
    cx: &mut Context<'_>,
    typed_ast: &ast::Program<ALoc, (ALoc, Type)>,
    depth: i32,
    dedup_threshold: Option<usize>,
) -> (Vec<(Loc, String)>, Option<String>) {
    let types = typed_ast_utils::typed_ast_to_list(typed_ast);
    let var_info = cx.environment().var_info.dupe();
    let env_values = &var_info.env_values;
    // That is also why `$defs` has to be a file-level table rather than being
    // folded into each location's string — cross-location sharing is where
    // almost all of the size win is.
    let mut json_cx = convert_types::TypeJsonCx::with_dedup(cx, dedup_threshold);
    let type_to_json = |json_cx: &mut convert_types::TypeJsonCx<'_, '_>, t: &Type| -> Json {
        let concrete =
            FlowJs::singleton_concrete_type_for_inspection(json_cx, type_util::reason_of_t(t), t)
                .unwrap_or_else(|_| t.dupe());
        convert_types::type_to_json(json_cx, depth, &concrete)
    };
    let print_type_json = |(loc, t): (ALoc, Type)| -> (ALoc, String) {
        let expression_type = type_to_json(&mut json_cx, &t);
        let mut fields: Vec<(String, Json)> =
            vec![("expression_type".to_string(), expression_type)];
        match env_values.get_ordinary(&loc) {
            Some(read) if let Some(dl) = &read.def_loc => {
                let dl = dl.dupe();
                let provider_t = type_env::provider_type_for_def_loc(false, &mut json_cx, dl);
                let provider_json = type_to_json(&mut json_cx, &provider_t);
                fields.push(("provider_type".to_string(), provider_json));
            }
            _ => {}
        }
        let obj = Json::Object(fields.into_iter().collect());
        (loc, obj.to_string())
    };
    let mapped: Vec<_> = types.into_iter().map(print_type_json).collect();
    let sorted = sort_loc_pairs(concretize_loc_pairs(mapped));
    let defs = dedup_threshold.map(|_| json_cx.take_defs().to_string());
    (sorted, defs)
}

pub fn insert_type_normalize<'cx>(
    cx: &mut Context<'cx>,
    file_sig: Arc<FileSig>,
    omit_targ_defaults: bool,
    typed_ast: &ast::Program<ALoc, (ALoc, Type)>,
    loc: Loc,
    t: &Type,
) -> QueryResult<ALocElt> {
    let options = Options {
        expand_internal_types: false,
        expand_enum_members: false,
        evaluate_type_destructors: EvaluateTypeDestructorsMode::EvaluateNone,
        optimize_types: false,
        omit_targ_defaults_option: omit_targ_defaults,
        merge_bot_and_any_kinds: true,
        verbose_normalizer: false,
        max_depth: None,
        max_type_size: None,
        toplevel_is_type_identifier_reference: false,
    };
    let genv = ty_normalizer_flow::mk_genv(options, Some(typed_ast), file_sig);
    match ty_normalizer_flow::from_type(cx, &genv, t) {
        Ok(elt) => QueryResult::Success(loc, elt),
        Err(err) => result_of_normalizer_error(loc, t.dupe(), err),
    }
}
