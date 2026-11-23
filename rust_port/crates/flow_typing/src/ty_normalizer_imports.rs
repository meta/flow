/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::cell::OnceCell;
use std::collections::BTreeMap;
use std::ops::Deref;

use dupe::Dupe;
use flow_aloc::ALoc;
use flow_common::flow_import_specifier::Userland;
use flow_common::flow_symbol::Symbol as FlowSymbol;
use flow_common::reason;
use flow_common::reason::Name;
use flow_common_ty::ty_symbol::ImportMode;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_parser::ast;
use flow_parser::ast::statement::ImportKind as AstImportKind;
use flow_parser::loc::Loc;
use flow_parser_utils::file_sig;
use flow_parser_utils::file_sig::FileSig;
use flow_parser_utils::file_sig::Require;
use flow_parser_utils::file_sig::RequireBindings;
use flow_typing_context::Context;
use flow_typing_flow_common::flow_js_utils;
use flow_typing_flow_js::flow_js;
use flow_typing_type::type_::DefTInner;
use flow_typing_type::type_::ModuleType;
use flow_typing_type::type_::PolyTData;
use flow_typing_type::type_::ThisInstanceTData;
use flow_typing_type::type_::Type;
use flow_typing_type::type_::TypeInner;
use flow_utils_concurrency::job_error::JobError;

fn import_mode_to_import_kind(mode: ImportMode) -> AstImportKind {
    match mode {
        ImportMode::ValueMode => AstImportKind::ImportValue,
        ImportMode::TypeMode => AstImportKind::ImportType,
        ImportMode::TypeofMode => AstImportKind::ImportTypeof,
    }
}

fn add_bind_ident_from_typed_ast(
    typed_ast: &ast::Program<ALoc, (ALoc, Type)>,
    name: FlowSmolStr,
    import_mode: ImportMode,
    loc: ALoc,
    mut acc: Vec<(FlowSmolStr, ALoc, ImportMode, Type)>,
) -> Vec<(FlowSmolStr, ALoc, ImportMode, Type)> {
    match crate::typed_ast_finder::find_exact_match_annotation(typed_ast, loc.dupe()) {
        Some(t) => {
            acc.push((name, loc, import_mode, t));
            acc
        }
        None => acc,
    }
}

fn add_bind_ident_from_imports<'a>(
    cx: &mut Context<'a>,
    local_name: &FlowSmolStr,
    import_mode: ImportMode,
    local_loc: ALoc,
    source: (&ALoc, &Userland, &Result<ModuleType, Type>),
    remote_name: &FlowSmolStr,
    mut acc: Vec<(FlowSmolStr, ALoc, ImportMode, Type)>,
) -> Result<Vec<(FlowSmolStr, ALoc, ImportMode, Type)>, JobError> {
    let (source_loc, module_name, source_module) = source;
    let import_reason = reason::mk_reason(
        reason::VirtualReasonDesc::RNamedImportedType(module_name.dupe(), local_name.dupe()),
        source_loc.dupe(),
    );
    let import_kind = import_mode_to_import_kind(import_mode);
    let (_loc, t) = flow_js_utils::flow_js_result_to_job_error(
        flow_js_utils::import_export_utils::import_named_specifier_type(
            cx,
            import_reason,
            &|cx, reason, t| {
                flow_js::FlowJs::singleton_concretize_type_for_imports_exports(cx, &reason, &t)
            },
            &import_kind,
            module_name.dupe(),
            source_module,
            remote_name,
            local_name,
        ),
    )?;
    acc.push((local_name.dupe(), local_loc, import_mode, t));
    Ok(acc)
}

fn add_imported_loc_map_bindings<'a>(
    cx: &mut Context<'a>,
    typed_ast: &Option<&ast::Program<ALoc, (ALoc, Type)>>,
    import_mode: ImportMode,
    source: &(Loc, FlowSmolStr),
    map: &BTreeMap<FlowSmolStr, BTreeMap<FlowSmolStr, vec1::Vec1<file_sig::ImportedLocs>>>,
    mut acc: Vec<(FlowSmolStr, ALoc, ImportMode, Type)>,
) -> Result<Vec<(FlowSmolStr, ALoc, ImportMode, Type)>, JobError> {
    let (source_loc, module_name) = source;
    let source_loc = ALoc::of_loc(source_loc.dupe());
    let source_module: OnceCell<Result<ModuleType, Type>> = OnceCell::new();

    for (remote_name, remote_map) in map {
        for (local_name, imported_locs_nel) in remote_map {
            for imported_loc in imported_locs_nel.iter() {
                let local_loc = ALoc::of_loc(imported_loc.local_loc.dupe());
                match typed_ast {
                    None => {
                        let source_module = source_module.get_or_init(|| {
                            let mref = Userland::from_smol_str(module_name.dupe());
                            flow_js_utils::import_export_utils::get_module_type_or_any(
                                cx,
                                false,
                                None,
                                source_loc.dupe(),
                                mref,
                            )
                            .unwrap()
                        });
                        let module_name_userland = Userland::from_smol_str(module_name.dupe());
                        acc = add_bind_ident_from_imports(
                            cx,
                            local_name,
                            import_mode,
                            local_loc,
                            (&source_loc, &module_name_userland, source_module),
                            remote_name,
                            acc,
                        )?;
                    }
                    Some(typed_ast) => {
                        acc = add_bind_ident_from_typed_ast(
                            typed_ast,
                            local_name.dupe(),
                            import_mode,
                            local_loc,
                            acc,
                        );
                    }
                }
            }
        }
    }
    Ok(acc)
}

fn add_require_bindings_from_exports_map<'a>(
    cx: &mut Context<'a>,
    loc: ALoc,
    source_name: Userland,
    binding: &RequireBindings,
    mut acc: Vec<(FlowSmolStr, ALoc, ImportMode, Type)>,
) -> Vec<(FlowSmolStr, ALoc, ImportMode, Type)> {
    let reason = reason::mk_reason(
        reason::VirtualReasonDesc::RModule(source_name.dupe()),
        loc.dupe(),
    );
    let source_module = flow_js_utils::import_export_utils::get_module_type_or_any(
        cx,
        false,
        None,
        loc.dupe(),
        source_name.dupe(),
    )
    .unwrap();
    let (_loc, t) = flow_js_utils::import_export_utils::cjs_require_type(
        cx,
        reason.dupe(),
        flow_js::reposition_non_speculating,
        FlowSymbol::mk_module_symbol(source_name.into_inner(), loc.dupe()),
        false,
        &source_module,
    )
    .unwrap();
    match binding {
        RequireBindings::BindIdent(id) => {
            let loc = ALoc::of_loc(id.0.dupe());
            let name = id.1.dupe();
            acc.push((name, loc, ImportMode::ValueMode, t));
            acc
        }
        RequireBindings::BindNamed(map) => match &source_module {
            Ok(m) => {
                let value_exports_tmap = cx.find_exports(m.module_export_types.value_exports_tmap);
                for (id, binding) in map {
                    let name = &id.1;
                    match binding {
                        RequireBindings::BindIdent(bind_id) => {
                            let key = Name::new(name.as_str());
                            match value_exports_tmap.get(&key) {
                                Some(named_symbol) => {
                                    acc.push((
                                        bind_id.1.dupe(),
                                        ALoc::of_loc(bind_id.0.dupe()),
                                        ImportMode::ValueMode,
                                        named_symbol.value_t(),
                                    ));
                                }
                                None => {}
                            }
                        }
                        // This case should be rare. Not worth collecting imported names from here
                        RequireBindings::BindNamed(_) => {}
                    }
                }
                acc
            }
            Err(_) => acc,
        },
    }
}

fn add_require_bindings_from_typed_ast(
    typed_ast: &ast::Program<ALoc, (ALoc, Type)>,
    import_mode: ImportMode,
    binding: &RequireBindings,
    acc: Vec<(FlowSmolStr, ALoc, ImportMode, Type)>,
) -> Vec<(FlowSmolStr, ALoc, ImportMode, Type)> {
    fn loop_binding(
        typed_ast: &ast::Program<ALoc, (ALoc, Type)>,
        import_mode: ImportMode,
        binding: &RequireBindings,
        mut acc: Vec<(FlowSmolStr, ALoc, ImportMode, Type)>,
    ) -> Vec<(FlowSmolStr, ALoc, ImportMode, Type)> {
        match binding {
            RequireBindings::BindIdent(id) => {
                let loc = ALoc::of_loc(id.0.dupe());
                add_bind_ident_from_typed_ast(typed_ast, id.1.dupe(), import_mode, loc, acc)
            }
            RequireBindings::BindNamed(map) => {
                for (_, binding) in map {
                    acc = loop_binding(typed_ast, import_mode, binding, acc);
                }
                acc
            }
        }
    }
    loop_binding(typed_ast, import_mode, binding, acc)
}

fn add_require_bindings<'a>(
    cx: &mut Context<'a>,
    typed_ast: &Option<&ast::Program<ALoc, (ALoc, Type)>>,
    import_mode: ImportMode,
    source: &(Loc, FlowSmolStr),
    bindings_opt: &Option<RequireBindings>,
    acc: Vec<(FlowSmolStr, ALoc, ImportMode, Type)>,
) -> Vec<(FlowSmolStr, ALoc, ImportMode, Type)> {
    match bindings_opt {
        None => acc,
        Some(bindings) => match typed_ast {
            None => {
                let (loc, name) = source;
                let loc = ALoc::of_loc(loc.dupe());
                add_require_bindings_from_exports_map(
                    cx,
                    loc,
                    Userland::from_smol_str(name.dupe()),
                    bindings,
                    acc,
                )
            }
            Some(typed_ast) => {
                add_require_bindings_from_typed_ast(typed_ast, import_mode, bindings, acc)
            }
        },
    }
}

/// A member of a namespace import: the qualified local name an export is
/// reachable under (`React.Node` for `import * as React from 'react'`), the
/// location of the namespace binding, the import mode, and where the export is
/// defined.
pub type NamespaceMember = (FlowSmolStr, ALoc, ImportMode, ALoc);

/// Where the class a value export holds is declared, when it holds one: a class
/// is printed by the symbol of its declaration, which a re-export such as
/// `declare export const Component: typeof React.Component` does not share.
fn class_def_loc(cx: &mut Context<'_>, t: &Type) -> Option<ALoc> {
    let t = cx.find_resolved(t).unwrap_or_else(|| t.dupe());
    let TypeInner::DefT(_, def_t) = t.deref() else {
        return None;
    };
    match def_t.deref() {
        DefTInner::PolyT(deref!(PolyTData { t_out, .. })) => class_def_loc(cx, t_out),
        DefTInner::ClassT(instance) => {
            let instance = cx
                .find_resolved(instance)
                .unwrap_or_else(|| instance.dupe());
            let inst = match instance.deref() {
                TypeInner::DefT(_, def_t) => match def_t.deref() {
                    DefTInner::InstanceT(instance_t) => &instance_t.inst,
                    _ => return None,
                },
                TypeInner::ThisInstanceT(deref!(ThisInstanceTData { instance, .. })) => &instance.inst,
                _ => return None,
            };
            inst.class_name.as_ref().map(|_| inst.class_id.0.dupe())
        }
        _ => None,
    }
}

/// The exports of a namespace import, each under its qualified local name. A
/// `typeof` namespace import only reaches value exports, and a `type` namespace
/// import reaches both kinds, as a value namespace import does. Exports are keyed
/// by where they are defined, as the export table records it, so nothing is
/// normalized: a module can have hundreds of exports.
fn add_namespace_members<'a>(
    cx: &mut Context<'a>,
    import_mode: ImportMode,
    source: &(Loc, FlowSmolStr),
    ns: &file_sig::Identifier,
    acc: &mut Vec<NamespaceMember>,
) -> Result<(), JobError> {
    let (source_loc, module_name) = source;
    let file_sig::Identifier(local_loc, local_name) = ns;
    let local_loc = ALoc::of_loc(local_loc.dupe());
    let source_module = flow_js_utils::flow_js_result_to_job_error(
        flow_js_utils::import_export_utils::get_module_type_or_any(
            cx,
            false,
            None,
            ALoc::of_loc(source_loc.dupe()),
            Userland::from_smol_str(module_name.dupe()),
        ),
    )?;
    let Ok(module) = source_module else {
        return Ok(());
    };
    let value_exports = cx.find_exports(module.module_export_types.value_exports_tmap.dupe());
    let type_exports = match import_mode {
        ImportMode::TypeofMode => None,
        ImportMode::ValueMode | ImportMode::TypeMode => {
            Some(cx.find_exports(module.module_export_types.type_exports_tmap.dupe()))
        }
    };
    let value_mode = match import_mode {
        ImportMode::TypeofMode => ImportMode::TypeofMode,
        ImportMode::ValueMode | ImportMode::TypeMode => ImportMode::ValueMode,
    };
    let exports = value_exports
        .iter()
        .map(|export| (export, value_mode))
        .chain(
            type_exports
                .iter()
                .flat_map(|exports| exports.iter())
                .map(|export| (export, ImportMode::TypeMode)),
        );
    let exports: Vec<_> = exports.collect();
    for ((name, named_symbol), mode) in exports {
        let class_loc = match mode {
            ImportMode::ValueMode | ImportMode::TypeofMode => {
                class_def_loc(cx, &named_symbol.value_t())
            }
            ImportMode::TypeMode => None,
        };
        let def_loc = class_loc.or_else(|| match &named_symbol.preferred_def_locs {
            Some(def_locs) => Some(def_locs.first().dupe()),
            None => named_symbol.name_loc.dupe(),
        });
        if let Some(def_loc) = def_loc {
            acc.push((
                FlowSmolStr::from(format!("{local_name}.{name}")),
                local_loc.dupe(),
                mode,
                def_loc,
            ));
        }
    }
    Ok(())
}

/// The members of every namespace import in the file.
pub fn namespace_members<'a>(
    cx: &mut Context<'a>,
    file_sig: &FileSig,
) -> Result<Vec<NamespaceMember>, JobError> {
    let mut members = Vec::new();
    for require in file_sig.requires() {
        let Require::Import {
            source,
            ns,
            type_ns,
            typesof_ns,
            ..
        } = require
        else {
            continue;
        };
        let source = (source.0.dupe(), source.1.dupe());
        for (ns, import_mode) in [
            (ns, ImportMode::ValueMode),
            (type_ns, ImportMode::TypeMode),
            (typesof_ns, ImportMode::TypeofMode),
        ] {
            if let Some(ns) = ns {
                add_namespace_members(cx, import_mode, &source, ns, &mut members)?;
            }
        }
    }
    Ok(members)
}

fn add_import_bindings<'a>(
    cx: &mut Context<'a>,
    typed_ast: &Option<&ast::Program<ALoc, (ALoc, Type)>>,
    mut acc: Vec<(FlowSmolStr, ALoc, ImportMode, Type)>,
    require: &Require,
) -> Result<Vec<(FlowSmolStr, ALoc, ImportMode, Type)>, JobError> {
    match require {
        Require::Require {
            source,
            require_loc: _,
            bindings,
            prefix: _,
        } => {
            let source_tuple = (source.0.dupe(), source.1.dupe());
            Ok(add_require_bindings(
                cx,
                typed_ast,
                ImportMode::ValueMode,
                &source_tuple,
                bindings,
                acc,
            ))
        }
        Require::Import {
            import_loc: _,
            source,
            named,
            ns: _,
            types,
            typesof,
            typesof_ns: _,
            type_ns: _,
        } => {
            let source_tuple = (source.0.dupe(), source.1.dupe());
            acc = add_imported_loc_map_bindings(
                cx,
                typed_ast,
                ImportMode::ValueMode,
                &source_tuple,
                named,
                acc,
            )?;
            acc = add_imported_loc_map_bindings(
                cx,
                typed_ast,
                ImportMode::TypeMode,
                &source_tuple,
                types,
                acc,
            )?;
            acc = add_imported_loc_map_bindings(
                cx,
                typed_ast,
                ImportMode::TypeofMode,
                &source_tuple,
                typesof,
                acc,
            )?;
            Ok(acc)
        }
        Require::ImportDynamic { .. }
        | Require::Import0 { .. }
        | Require::ImportSyntheticUserland { .. }
        | Require::ExportFrom { .. } => Ok(acc),
    }
}

pub fn extract_types<'a>(
    cx: &mut Context<'a>,
    file_sig: &FileSig,
    typed_ast: Option<&ast::Program<ALoc, (ALoc, Type)>>,
) -> Result<Vec<(FlowSmolStr, ALoc, ImportMode, Type)>, JobError> {
    let requires = file_sig.requires();
    let mut imports: Vec<(FlowSmolStr, ALoc, ImportMode, Type)> = Vec::new();
    for require in requires {
        imports = add_import_bindings(cx, &typed_ast, imports, require)?;
    }
    Ok(imports.into_iter().rev().collect())
}
