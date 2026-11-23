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
use std::collections::HashMap;
use std::collections::HashSet;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::OnceLock;

use dupe::Dupe;
use flow_aloc::ALoc;
use flow_aloc::ALocTable;
use flow_aloc::LazyALocTable;
use flow_aloc::aloc_representation_do_not_use;
use flow_analysis::scope_api;
use flow_analysis::scope_builder;
use flow_codemods::utils::codemod_report;
use flow_codemods::utils::codemod_runner;
use flow_codemods::utils::codemod_utils::MakeMain;
use flow_common::flow_import_specifier::FlowImportSpecifier;
use flow_common::flow_import_specifier::Userland;
use flow_common::options::Options;
use flow_common_ty::ty_printer;
use flow_data_structure_wrapper::ord_set::FlowOrdSet;
use flow_heap::parsing_heaps::Transaction;
use flow_heap::resolved_requires::Dependency;
use flow_heap::resolved_requires::ResolvedModule;
use flow_lsp::lsp::lsp_range_to_flow_loc;
use flow_lsp_server::document_symbol_provider;
use flow_parser::ast;
use flow_parser::ast_utils;
use flow_parser::ast_visitor;
use flow_parser::ast_visitor::AstVisitor;
use flow_parser::file_key::FileKey;
use flow_parser::loc::Loc;
use flow_parser::offset_utils::OffsetTable;
use flow_parser_utils::file_sig::FileSig;
use flow_parser_utils::file_sig::Require;
use flow_parser_utils::file_sig::RequireBindings;
use flow_profiling::profiling_js;
use flow_server::server::string_of_init_trigger;
use flow_server::server::string_of_saved_state_fetcher;
use flow_server_env::server_status::StartCause;
use flow_services_autocomplete::find_documentation;
use flow_type_sig::compact_table::Index;
use flow_type_sig::compact_table::Table;
use flow_type_sig::packed_type_sig;
use flow_type_sig::type_sig;
use flow_type_sig::type_sig::Accessor;
use flow_type_sig::type_sig::ObjAnnotProp;
use flow_type_sig::type_sig::ObjValueProp;
use flow_type_sig::type_sig_pack;
use flow_type_sig::type_sig_pack::PackedDef;
use flow_typing::ty_normalizer_flow;
use flow_typing_context::Context;
use flow_typing_flow_common::flow_js_utils;
use flow_typing_ty_normalizer::env::Options as TyNormalizerOptions;
use flow_typing_type::type_::Type;
use flow_typing_type::type_util;
use serde_json::Value;

use crate::codemod_command::PreparedCodemod;
use crate::glean_schema::declaration;
use crate::glean_schema::declaration_info;
use crate::glean_schema::export;
use crate::glean_schema::file_of_string_module;
use crate::glean_schema::import_declaration;
use crate::glean_schema::local_declaration_reference;
use crate::glean_schema::member_declaration;
use crate::glean_schema::member_declaration_info;
use crate::glean_schema::member_declaration_reference;
use crate::glean_schema::module_;
use crate::glean_schema::module_docblock_range;
use crate::glean_schema::module_export;
use crate::glean_schema::module_type_export;
use crate::glean_schema::source_of_export;
use crate::glean_schema::source_of_type_export;
use crate::glean_schema::src;
use crate::glean_schema::type_declaration;
use crate::glean_schema::type_declaration_info;
use crate::glean_schema::type_declaration_reference;
use crate::glean_schema::type_export;
use crate::glean_schema::type_import_declaration;
use crate::offset_cache::OffsetCache;

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub(crate) struct GleanRuntimeConfig {
    pub(crate) output_dir: Option<PathBuf>,
    pub(crate) write_root: String,
    pub(crate) include_direct_deps: bool,
    pub(crate) project_root_input: bool,
    pub(crate) glean_log: bool,
    pub(crate) glean_timeout: i32,
}

pub(crate) static GLEAN_RUNTIME_CONFIG: OnceLock<GleanRuntimeConfig> = OnceLock::new();

type OffsetTableOfFileKey<'a> = dyn Fn(&FileKey) -> Option<Arc<OffsetTable>> + 'a;
type LocOfALoc = dyn Fn(&ALoc) -> Loc;

fn make_loc_of_aloc(reader: Arc<Transaction>) -> impl Fn(&ALoc) -> Loc {
    let tables: RefCell<HashMap<FileKey, LazyALocTable>> = RefCell::new(HashMap::new());
    move |aloc| {
        let source = match aloc.source() {
            Some(source) => source.dupe(),
            None => return aloc.to_loc_exn().dupe(),
        };
        {
            let tables = tables.borrow();
            if let Some(table) = tables.get(&source) {
                return aloc.to_loc(table);
            }
        }
        let Some(packed) = reader.get_aloc_table(&source) else {
            return aloc.to_loc_exn().dupe();
        };
        let source_for_table = source.dupe();
        let table: LazyALocTable = Rc::new(LazyCell::new(Box::new(move || {
            Rc::new(ALocTable::unpack(source_for_table.dupe(), &packed))
        })
            as Box<dyn FnOnce() -> Rc<ALocTable>>));
        tables.borrow_mut().insert(source.dupe(), table);
        let tables = tables.borrow();
        aloc.to_loc(tables.get(&source).expect("ALoc table should be cached"))
    }
}

#[allow(dead_code)]
fn implementation_file(
    transaction: &flow_heap::parsing_heaps::Transaction,
    resolved_module: &ResolvedModule,
) -> Option<FileKey> {
    if let Some(dependency) = resolved_module.as_dependency() {
        if let Some(file) = transaction.get_provider(&dependency) {
            if transaction.is_typed_file(&file) {
                return Some(file);
            }
        }
    }
    None
}

pub(crate) mod documentation_fullspan_map {
    use std::collections::BTreeMap;

    use flow_parser::loc::Loc;

    #[derive(Clone, Debug)]
    pub(crate) struct DocSpan {
        pub(crate) documentation: Option<Loc>,
        pub(crate) span: Option<Loc>,
    }

    // A map from symbol location to the documentation loc
    // and span of the entity defined by the symbol
    pub(crate) type T = BTreeMap<Loc, DocSpan>;

    pub(crate) fn combine(ds: &DocSpan, ds_prime: &DocSpan) -> DocSpan {
        fn or_(a: &Option<Loc>, b: &Option<Loc>) -> Option<Loc> {
            match a {
                None => b.clone(),
                _ => a.clone(),
            }
        }
        DocSpan {
            documentation: or_(&ds.documentation, &ds_prime.documentation),
            span: or_(&ds.span, &ds_prime.span),
        }
    }

    pub(crate) fn create(
        ast: &flow_parser::ast::Program<Loc, Loc>,
        source: Option<&flow_parser::file_key::FileKey>,
    ) -> T {
        fn add_symbol_spans(
            map: &mut T,
            source: Option<&flow_parser::file_key::FileKey>,
            symbol_spans: &[lsp_types::DocumentSymbol],
        ) {
            for symbol in symbol_spans {
                let documentation = None;
                let loc = super::lsp_range_to_flow_loc(source.cloned(), &symbol.selection_range);
                let span = Some(super::lsp_range_to_flow_loc(source.cloned(), &symbol.range));
                let ds_prime = DocSpan {
                    documentation,
                    span,
                };
                match map.get(&loc) {
                    None => {
                        map.insert(loc, ds_prime);
                    }
                    Some(ds) => {
                        map.insert(loc, combine(ds, &ds_prime));
                    }
                }
                if let Some(children) = &symbol.children {
                    add_symbol_spans(map, source, children);
                }
            }
        }

        let comment_map = super::find_documentation::def_loc_to_comment_loc_map(ast);
        let mut comment_loc_map: T = comment_map
            .into_iter()
            .map(|(loc, documentation)| {
                (
                    loc,
                    DocSpan {
                        documentation: Some(documentation),
                        span: None,
                    },
                )
            })
            .collect();
        let symbol_spans = super::document_symbol_provider::provide_document_symbols(ast);
        add_symbol_spans(&mut comment_loc_map, source, &symbol_spans);
        comment_loc_map
    }
}

pub(crate) struct MemberSearcher<F> {
    pub(crate) add_member: F,
}

pub(crate) struct TypeReferenceSearcher<F> {
    pub(crate) add_reference: F,
}

impl<'ast, F> AstVisitor<'ast, ALoc, (ALoc, Type), &'ast ALoc, !> for MemberSearcher<F>
where
    F: FnMut(&Type, &ALoc, &str),
{
    fn normalize_loc(loc: &'ast ALoc) -> &'ast ALoc {
        loc
    }

    fn normalize_type(type_: &'ast (ALoc, Type)) -> &'ast ALoc {
        &type_.0
    }

    fn member(
        &mut self,
        _loc: &'ast (ALoc, Type),
        member: &'ast ast::expression::Member<ALoc, (ALoc, Type)>,
    ) -> Result<(), !> {
        let (_, type_) = member.object.loc();
        if let ast::expression::member::Property::PropertyIdentifier(id) = &member.property {
            (self.add_member)(type_, &id.loc.0, id.name.as_str());
        }
        ast_visitor::member_default(self, _loc, member)
    }
}

impl<'ast, F> AstVisitor<'ast, ALoc, (ALoc, Type), &'ast ALoc, !> for TypeReferenceSearcher<F>
where
    F: FnMut(&ast::Identifier<ALoc, (ALoc, Type)>),
{
    fn normalize_loc(loc: &'ast ALoc) -> &'ast ALoc {
        loc
    }

    fn normalize_type(type_: &'ast (ALoc, Type)) -> &'ast ALoc {
        &type_.0
    }

    fn generic_identifier_type(
        &mut self,
        git: &'ast ast::types::generic::Identifier<ALoc, (ALoc, Type)>,
    ) -> Result<(), !> {
        match git {
            ast::types::generic::Identifier::Unqualified(id) => {
                (self.add_reference)(id);
            }
            ast::types::generic::Identifier::Qualified(qualified) => {
                (self.add_reference)(&qualified.id);
            }
            ast::types::generic::Identifier::ImportTypeAnnot(_) => {}
        }
        ast_visitor::generic_identifier_type_default(self, git)
    }
}

pub(crate) fn remove_dot_flow_suffix(module: module_::T) -> module_::T {
    match module {
        module_::T::File(file) => {
            module_::T::File(file.strip_suffix(".flow").unwrap_or(&file).to_string())
        }
        module_ => module_,
    }
}

pub(crate) fn module_of_module_ref(
    resolved_modules: &BTreeMap<
        FlowImportSpecifier,
        Result<Dependency, Option<FlowImportSpecifier>>,
    >,
    root: &str,
    write_root: &str,
    module_ref: &Userland,
) -> module_::T {
    let key = FlowImportSpecifier::Userland(module_ref.dupe());
    match resolved_modules.get(&key) {
        Some(Ok(dep)) => {
            let m = dep.to_modulename();
            module_::of_modulename(root, write_root, &m)
        }
        Some(Err(mapped_name)) => {
            // TODO: We reach this codepath for requires that might resolve to builtin
            // modules. During check we check the master context, which we can also do
            // here.
            let name = match mapped_name {
                None => module_ref.dupe(),
                Some(FlowImportSpecifier::Userland(name)) => name.dupe(),
            };
            module_::T::String(name.display().to_string())
        }
        None => module_::T::String(module_ref.display().to_string()),
    }
}

pub(crate) fn loc_of_index(
    loc_source: Option<FileKey>,
    loc_of_aloc: &LocOfALoc,
    i: Index<Loc>,
) -> Loc {
    let key = i.as_usize() as u32;
    let aloc = aloc_representation_do_not_use::make_keyed(loc_source, key);
    loc_of_aloc(&aloc)
}

pub(crate) fn loc_of_def(
    loc_source: Option<FileKey>,
    loc_of_aloc: &LocOfALoc,
    packed_def: &PackedDef<Index<Loc>>,
) -> Loc {
    let idx = packed_def.id_loc();
    loc_of_index(loc_source, loc_of_aloc, idx)
}

fn source_of_type_exports(
    root: &str,
    write_root: &str,
    file: &FileKey,
    loc_of_aloc: &LocOfALoc,
    loc_source: Option<FileKey>,
    type_sig: &packed_type_sig::Module<Loc>,
    resolved_modules: &BTreeMap<
        FlowImportSpecifier,
        Result<Dependency, Option<FlowImportSpecifier>>,
    >,
    offset_table_of_file_key: &OffsetTableOfFileKey<'_>,
) -> Vec<Value> {
    let packed_type_sig::Module {
        module_kind,
        module_refs,
        local_defs,
        remote_refs,
        ..
    } = type_sig;
    let source_of_remote_ref =
        |remote_ref: &type_sig_pack::RemoteRef<Index<Loc>>| -> Vec<source_of_type_export::Source> {
            match remote_ref {
                type_sig_pack::RemoteRef::ImportType { index, remote, .. } => {
                    let module_ = {
                        let module_ref = module_refs.get(*index);
                        module_of_module_ref(resolved_modules, root, write_root, module_ref)
                    };
                    let type_export = type_export::T::Named(remote.as_str().to_string());
                    vec![source_of_type_export::Source::ModuleTypeExport(
                        module_type_export::T {
                            module_,
                            type_export,
                        },
                    )]
                }
                type_sig_pack::RemoteRef::ImportTypeof { id_loc, name, .. } => {
                    let loc = loc_of_index(loc_source.clone(), loc_of_aloc, *id_loc);
                    vec![source_of_type_export::Source::TypeDeclaration(
                        type_declaration::T {
                            name: name.as_str().to_string(),
                            loc,
                        },
                    )]
                }
                type_sig_pack::RemoteRef::ImportTypeofNs { index, .. }
                | type_sig_pack::RemoteRef::ImportTypeNs { index, .. } => {
                    let module_ = {
                        let module_ref = module_refs.get(*index);
                        module_of_module_ref(resolved_modules, root, write_root, module_ref)
                    };
                    vec![source_of_type_export::Source::ModuleNamespace(module_)]
                }
                type_sig_pack::RemoteRef::Import { .. }
                | type_sig_pack::RemoteRef::ImportNs { .. } => {
                    vec![]
                }
            }
        };
    let source_of_packed_ref =
        |packed_ref: &type_sig_pack::PackedRef<Index<Loc>>| -> Vec<source_of_type_export::Source> {
            match packed_ref {
                type_sig_pack::PackedRef::LocalRef(inner) => {
                    let packed_def = local_defs.get(inner.index);
                    let name = packed_def.name();
                    let loc = loc_of_def(loc_source.clone(), loc_of_aloc, packed_def);
                    vec![source_of_type_export::Source::TypeDeclaration(
                        type_declaration::T {
                            name: name.as_str().to_string(),
                            loc,
                        },
                    )]
                }
                type_sig_pack::PackedRef::RemoteRef(inner) => {
                    let remote_ref = remote_refs.get(inner.index);
                    source_of_remote_ref(remote_ref)
                }
                type_sig_pack::PackedRef::BuiltinRef(inner) => {
                    let loc = loc_of_index(loc_source.clone(), loc_of_aloc, inner.ref_loc);
                    vec![source_of_type_export::Source::TypeDeclaration(
                        type_declaration::T {
                            name: inner.name.as_str().to_string(),
                            loc,
                        },
                    )]
                }
            }
        };
    let module_ = remove_dot_flow_suffix(module_::of_file_key(root, write_root, file));
    match module_kind {
        type_sig_pack::ModuleKind::CJSModule {
            type_exports,
            info:
                type_sig_pack::CJSModuleInfo {
                    type_export_keys,
                    type_stars,
                    ..
                },
            ..
        }
        | type_sig_pack::ModuleKind::ESModule {
            type_exports,
            info:
                type_sig_pack::ESModuleInfo {
                    type_export_keys,
                    type_stars,
                    ..
                },
            ..
        } => {
            let star_type_exports: Vec<source_of_type_export::T> = type_stars
                .iter()
                .flat_map(|(_, index)| {
                    let module_ref = module_refs.get(*index);
                    let remote_module =
                        module_of_module_ref(resolved_modules, root, write_root, module_ref);
                    let te = type_export::T::Star(remote_module.clone());
                    let module_type_export = module_type_export::T {
                        module_: module_.clone(),
                        type_export: te,
                    };
                    let source = source_of_type_export::Source::ModuleNamespace(remote_module);
                    vec![source_of_type_export::T {
                        source,
                        module_type_export,
                    }]
                })
                .collect();
            let non_star_type_exports: Vec<source_of_type_export::T> = type_exports
                .iter()
                .zip(type_export_keys.iter())
                .flat_map(|(type_export, type_export_key)| {
                    let te = type_export::T::Named(type_export_key.as_str().to_string());
                    let mte = module_type_export::T {
                        module_: module_.clone(),
                        type_export: te,
                    };
                    let sources: Vec<source_of_type_export::Source> = match type_export {
                        type_sig_pack::TypeExport::ExportTypeRef(packed_ref) => {
                            source_of_packed_ref(packed_ref)
                        }
                        type_sig_pack::TypeExport::ExportTypeBinding(index) => {
                            let packed_def = local_defs.get(*index);
                            let name = packed_def.name();
                            let loc =
                                loc_of_index(loc_source.clone(), loc_of_aloc, packed_def.id_loc());
                            vec![source_of_type_export::Source::TypeDeclaration(
                                type_declaration::T {
                                    name: name.as_str().to_string(),
                                    loc,
                                },
                            )]
                        }
                        type_sig_pack::TypeExport::ExportTypeFrom(index) => {
                            let remote_ref = remote_refs.get(*index);
                            source_of_remote_ref(remote_ref)
                        }
                    };
                    sources
                        .into_iter()
                        .map(|source| source_of_type_export::T {
                            source,
                            module_type_export: mte.clone(),
                        })
                        .collect::<Vec<_>>()
                })
                .collect();
            let mut all = star_type_exports;
            all.extend(non_star_type_exports);
            all.iter()
                .map(|sote| {
                    source_of_type_export::to_json(root, write_root, offset_table_of_file_key, sote)
                })
                .collect()
        }
    }
}

pub(crate) fn export_of_export_name(name: &str) -> export::T {
    match name {
        "default" => export::T::Default,
        name => export::T::Named(name.to_string()),
    }
}

fn type_import_declarations(
    root: &str,
    write_root: &str,
    resolved_modules: &BTreeMap<
        FlowImportSpecifier,
        Result<Dependency, Option<FlowImportSpecifier>>,
    >,
    file_sig: &FileSig,
    offset_table_of_file_key: &OffsetTableOfFileKey<'_>,
) -> Vec<Value> {
    let results: Vec<type_import_declaration::T> = file_sig
        .requires()
        .iter()
        .flat_map(|require| -> Vec<type_import_declaration::T> {
            match require {
                Require::Import {
                    source,
                    types,
                    typesof,
                    typesof_ns,
                    type_ns,
                    ..
                } => {
                    let module_ = module_of_module_ref(
                        resolved_modules,
                        root,
                        write_root,
                        &Userland::from_smol_str(source.name().dupe()),
                    );
                    let types_info: Vec<type_import_declaration::T> = types
                        .iter()
                        .flat_map(|(export_name, local)| {
                            let te = type_export::T::Named(export_name.as_str().to_string());
                            let import =
                                type_import_declaration::Import::Type(module_type_export::T {
                                    module_: module_.clone(),
                                    type_export: te,
                                });
                            local
                                .iter()
                                .flat_map(move |(name, locs)| {
                                    let import = import.clone();
                                    locs.iter()
                                        .map(move |imported_locs| {
                                            let td = type_declaration::T {
                                                name: name.as_str().to_string(),
                                                loc: imported_locs.local_loc.clone(),
                                            };
                                            type_import_declaration::T {
                                                import: import.clone(),
                                                type_declaration: td,
                                            }
                                        })
                                        .collect::<Vec<_>>()
                                })
                                .collect::<Vec<_>>()
                        })
                        .collect();
                    let typesof_info: Vec<type_import_declaration::T> = typesof
                        .iter()
                        .flat_map(|(export_name, local)| {
                            let export = export_of_export_name(export_name.as_str());
                            let import =
                                type_import_declaration::Import::Typeof(module_export::T {
                                    module_: module_.clone(),
                                    export,
                                });
                            local
                                .iter()
                                .flat_map(move |(name, locs)| {
                                    let import = import.clone();
                                    locs.iter()
                                        .map(move |imported_locs| {
                                            let td = type_declaration::T {
                                                name: name.as_str().to_string(),
                                                loc: imported_locs.local_loc.clone(),
                                            };
                                            type_import_declaration::T {
                                                import: import.clone(),
                                                type_declaration: td,
                                            }
                                        })
                                        .collect::<Vec<_>>()
                                })
                                .collect::<Vec<_>>()
                        })
                        .collect();
                    let typesof_ns_info: Vec<type_import_declaration::T> = typesof_ns
                        .iter()
                        .map(|id| {
                            let import =
                                type_import_declaration::Import::ModuleTypeof(module_.clone());
                            let td = type_declaration::T {
                                loc: id.loc().clone(),
                                name: id.name().as_str().to_string(),
                            };
                            type_import_declaration::T {
                                import,
                                type_declaration: td,
                            }
                        })
                        .collect();
                    let type_ns_info: Vec<type_import_declaration::T> = type_ns
                        .iter()
                        .map(|id| {
                            let import =
                                type_import_declaration::Import::ModuleTypeof(module_.clone());
                            let td = type_declaration::T {
                                loc: id.loc().clone(),
                                name: id.name().as_str().to_string(),
                            };
                            type_import_declaration::T {
                                import,
                                type_declaration: td,
                            }
                        })
                        .collect();
                    let mut result = types_info;
                    result.extend(typesof_info);
                    result.extend(typesof_ns_info);
                    result.extend(type_ns_info);
                    result
                }
                Require::Require { .. }
                | Require::Import0 { .. }
                | Require::ImportDynamic { .. }
                | Require::ImportSyntheticUserland { .. }
                | Require::ExportFrom { .. } => vec![],
            }
        })
        .collect();
    results
        .iter()
        .map(|tid| {
            type_import_declaration::to_json(root, write_root, offset_table_of_file_key, tid)
        })
        .collect()
}

fn type_declaration_references(
    root: &str,
    write_root: &str,
    loc_of_aloc: &LocOfALoc,
    cx: &mut Context<'_>,
    typed_ast: &ast::Program<ALoc, (ALoc, Type)>,
    offset_table_of_file_key: &OffsetTableOfFileKey<'_>,
) -> Vec<Value> {
    let mut results: Vec<type_declaration_reference::T> = Vec::new();
    let add_reference = |id: &ast::Identifier<ALoc, (ALoc, Type)>| {
        let loc = loc_of_aloc(&id.loc.0);
        let mut def_loc_of_t = |t: &Type| -> Option<Loc> {
            let mut t = t.dupe();
            loop {
                let def_loc = loc_of_aloc(type_util::def_loc_of_t(&t));
                if Loc::contains(&def_loc, &loc) {
                    let possible = flow_js_utils::possible_types_of_type(cx, &t);
                    match possible.as_slice() {
                        [t_prime] => t = t_prime.dupe(),
                        _ => {
                            return None;
                        }
                    }
                } else {
                    return Some(def_loc);
                }
            }
        };
        if let Some(def_loc) = def_loc_of_t(&id.loc.1) {
            let type_declaration = type_declaration::T {
                loc: def_loc,
                name: id.name.as_str().to_string(),
            };
            results.push(type_declaration_reference::T {
                type_declaration,
                loc,
            });
        }
    };
    let mut searcher = TypeReferenceSearcher { add_reference };
    let Ok(()) = searcher.program(typed_ast);
    results
        .iter()
        .map(|result| {
            type_declaration_reference::to_json(root, write_root, offset_table_of_file_key, result)
        })
        .collect()
}

fn extract_member_def<'a, 'cx>(
    imported_names: &ty_normalizer_flow::ImportedNames<'a, 'cx>,
    cx: &mut Context<'cx>,
    typed_ast: &'a ast::Program<ALoc, (ALoc, Type)>,
    file_sig: &Arc<FileSig>,
    scheme: &Type,
    name: &str,
) -> Vec<ALoc> {
    match flow_typing::ty_members::extract(
        false,
        Some(vec![flow_common::reason::Name::new(name)]),
        Some(imported_names.dupe()),
        cx,
        Some(typed_ast),
        file_sig.clone(),
        scheme,
    ) {
        Err(_) => vec![],
        Ok(ty_members) => ty_members
            .members
            .get(&flow_common::reason::Name::new(name))
            .map(|member_info| member_info.def_locs.clone())
            .unwrap_or_default(),
    }
}

fn member_declaration_references<'a, 'cx>(
    imported_names: &ty_normalizer_flow::ImportedNames<'a, 'cx>,
    root: &str,
    write_root: &str,
    loc_of_aloc: &LocOfALoc,
    cx: &mut Context<'cx>,
    typed_ast: &'a ast::Program<ALoc, (ALoc, Type)>,
    file_sig: &Arc<FileSig>,
    offset_table_of_file_key: &OffsetTableOfFileKey<'_>,
) -> Vec<Value> {
    let mut results: Vec<member_declaration_reference::T> = Vec::new();
    let add_member = |type_: &Type, aloc: &ALoc, name: &str| {
        for def_aloc in extract_member_def(imported_names, cx, typed_ast, file_sig, type_, name) {
            let member_declaration = member_declaration::T {
                name: name.to_string(),
                loc: loc_of_aloc(&def_aloc),
            };
            results.push(member_declaration_reference::T {
                member_declaration,
                loc: loc_of_aloc(aloc),
            });
        }
    };
    let mut searcher = MemberSearcher { add_member };
    let Ok(()) = searcher.program(typed_ast);
    results
        .iter()
        .map(|result| {
            member_declaration_reference::to_json(
                root,
                write_root,
                offset_table_of_file_key,
                result,
            )
        })
        .collect()
}

fn import_declarations(
    root: &str,
    write_root: &str,
    resolved_modules: &BTreeMap<
        FlowImportSpecifier,
        Result<Dependency, Option<FlowImportSpecifier>>,
    >,
    file_sig: &FileSig,
    offset_table_of_file_key: &OffsetTableOfFileKey<'_>,
) -> Vec<Value> {
    let results: Vec<import_declaration::T> = file_sig
        .requires()
        .iter()
        .flat_map(|require| -> Vec<import_declaration::T> {
            match require {
                Require::Require {
                    source, bindings, ..
                } => {
                    let module_ = module_of_module_ref(
                        resolved_modules,
                        root,
                        write_root,
                        &Userland::from_smol_str(source.name().dupe()),
                    );
                    let make_import_decl = |export_val: export::T,
                                            loc: Loc,
                                            name: &str|
                     -> Vec<import_declaration::T> {
                        let import = import_declaration::Import::ModuleExport(module_export::T {
                            module_: module_.clone(),
                            export: export_val,
                        });
                        let decl = declaration::T {
                            loc,
                            name: name.to_string(),
                        };
                        vec![import_declaration::T {
                            import,
                            declaration: decl,
                        }]
                    };
                    match bindings {
                        None => make_import_decl(
                            export::T::CommonJS,
                            source.loc().clone(),
                            source.name().as_str(),
                        ),
                        Some(RequireBindings::BindIdent(id)) => make_import_decl(
                            export::T::CommonJS,
                            id.loc().clone(),
                            id.name().as_str(),
                        ),
                        Some(RequireBindings::BindNamed(named_bindings)) => named_bindings
                            .iter()
                            .flat_map(|(remote_id, inner_binding)| -> Vec<import_declaration::T> {
                                match inner_binding {
                                    // currently we only track the top-level members of commonJS imports/exports
                                    RequireBindings::BindNamed(_) => vec![],
                                    RequireBindings::BindIdent(local_id) => make_import_decl(
                                        export::T::CommonJSMember(
                                            remote_id.name().as_str().to_string(),
                                        ),
                                        local_id.loc().clone(),
                                        local_id.name().as_str(),
                                    ),
                                }
                            })
                            .collect(),
                    }
                }
                Require::ImportDynamic { .. }
                | Require::Import0 { .. }
                | Require::ImportSyntheticUserland { .. }
                | Require::ExportFrom { .. } => vec![],
                Require::Import {
                    source, named, ns, ..
                } => {
                    let module_ = module_of_module_ref(
                        resolved_modules,
                        root,
                        write_root,
                        &Userland::from_smol_str(source.name().dupe()),
                    );
                    let named_import_declarations: Vec<import_declaration::T> = named
                        .iter()
                        .flat_map(|(export_name, local)| {
                            let export = export_of_export_name(export_name.as_str());
                            let import =
                                import_declaration::Import::ModuleExport(module_export::T {
                                    module_: module_.clone(),
                                    export,
                                });
                            local
                                .iter()
                                .flat_map(move |(name, locs)| {
                                    let import = import.clone();
                                    locs.iter()
                                        .map(move |imported_locs| {
                                            let decl = declaration::T {
                                                loc: imported_locs.local_loc.clone(),
                                                name: name.as_str().to_string(),
                                            };
                                            import_declaration::T {
                                                import: import.clone(),
                                                declaration: decl,
                                            }
                                        })
                                        .collect::<Vec<_>>()
                                })
                                .collect::<Vec<_>>()
                        })
                        .collect();
                    let namespace_import_declarations: Vec<import_declaration::T> = ns
                        .iter()
                        .map(|id| {
                            let import =
                                import_declaration::Import::ModuleNamespace(module_.clone());
                            let decl = declaration::T {
                                loc: id.loc().clone(),
                                name: id.name().as_str().to_string(),
                            };
                            import_declaration::T {
                                import,
                                declaration: decl,
                            }
                        })
                        .collect();
                    let mut result = namespace_import_declarations;
                    result.extend(named_import_declarations);
                    result
                }
            }
        })
        .collect();
    results
        .iter()
        .map(|id| import_declaration::to_json(root, write_root, offset_table_of_file_key, id))
        .collect()
}

pub(crate) fn loc_of_obj_value_prop<T>(
    loc_source: Option<FileKey>,
    loc_of_aloc: &LocOfALoc,
    prop: &ObjValueProp<Index<Loc>, T>,
) -> Loc {
    match prop {
        ObjValueProp::ObjValueField(deref!((index, _, _))) => {
            loc_of_index(loc_source, loc_of_aloc, *index)
        }
        ObjValueProp::ObjValueAccess(deref!(accessor)) => match accessor {
            Accessor::Get(deref!((index, _))) => loc_of_index(loc_source, loc_of_aloc, *index),
            Accessor::Set(deref!((index, _))) => loc_of_index(loc_source, loc_of_aloc, *index),
            Accessor::GetSet(deref!((_, _, index, _))) => loc_of_index(loc_source, loc_of_aloc, *index),
        },
        ObjValueProp::ObjValueMethod(deref!(ms)) => {
            let type_sig::ObjValueMethodData { id_loc: index, .. } = &ms[0];
            loc_of_index(loc_source, loc_of_aloc, *index)
        }
    }
}

pub(crate) fn loc_of_obj_annot_prop<T>(
    loc_source: Option<FileKey>,
    loc_of_aloc: &LocOfALoc,
    prop: &ObjAnnotProp<Index<Loc>, T>,
) -> Loc {
    match prop {
        ObjAnnotProp::ObjAnnotField(deref!((index, _, _))) => {
            loc_of_index(loc_source, loc_of_aloc, *index)
        }
        ObjAnnotProp::ObjAnnotAccess(deref!(accessor)) => match accessor {
            Accessor::Get(deref!((index, _))) => loc_of_index(loc_source, loc_of_aloc, *index),
            Accessor::Set(deref!((index, _))) => loc_of_index(loc_source, loc_of_aloc, *index),
            Accessor::GetSet(deref!((_, _, index, _))) => loc_of_index(loc_source, loc_of_aloc, *index),
        },
        ObjAnnotProp::ObjAnnotMethod(deref!(type_sig::ObjAnnotMethodData {
            id_loc: index, ..
        })) => loc_of_index(loc_source, loc_of_aloc, *index),
    }
}

fn source_of_exports(
    root: &str,
    write_root: &str,
    loc_source: Option<FileKey>,
    type_sig: &packed_type_sig::Module<Loc>,
    resolved_modules: &BTreeMap<
        FlowImportSpecifier,
        Result<Dependency, Option<FlowImportSpecifier>>,
    >,
    loc_of_aloc: &LocOfALoc,
    offset_table_of_file_key: &OffsetTableOfFileKey<'_>,
) -> Vec<Value> {
    let packed_type_sig::Module {
        module_kind,
        local_defs,
        remote_refs,
        module_refs,
        ..
    } = type_sig;
    let module_ = remove_dot_flow_suffix(module_::of_loc_source(
        root,
        write_root,
        loc_source.as_ref(),
    ));
    let source_of_remote_ref =
        |remote_ref: &type_sig_pack::RemoteRef<Index<Loc>>| -> Vec<source_of_export::Source> {
            match remote_ref {
                type_sig_pack::RemoteRef::Import { index, remote, .. } => {
                    let m = {
                        let module_ref = module_refs.get(*index);
                        module_of_module_ref(resolved_modules, root, write_root, module_ref)
                    };
                    let export = export::T::Named(remote.as_str().to_string());
                    vec![source_of_export::Source::ModuleExport(module_export::T {
                        module_: m,
                        export,
                    })]
                }
                type_sig_pack::RemoteRef::ImportNs { index, .. } => {
                    let m = {
                        let module_ref = module_refs.get(*index);
                        module_of_module_ref(resolved_modules, root, write_root, module_ref)
                    };
                    vec![source_of_export::Source::ModuleNamespace(m)]
                }
                type_sig_pack::RemoteRef::ImportType { .. }
                | type_sig_pack::RemoteRef::ImportTypeof { .. }
                | type_sig_pack::RemoteRef::ImportTypeofNs { .. }
                | type_sig_pack::RemoteRef::ImportTypeNs { .. } => vec![],
            }
        };
    let source_of_packed_ref =
        |packed_ref: &type_sig_pack::PackedRef<Index<Loc>>| -> Vec<source_of_export::Source> {
            match packed_ref {
                type_sig_pack::PackedRef::LocalRef(inner) => {
                    let source = {
                        let packed_def = local_defs.get(inner.index);
                        let name = packed_def.name();
                        let loc = loc_of_def(loc_source.clone(), loc_of_aloc, packed_def);
                        source_of_export::Source::Declaration(declaration::T {
                            name: name.as_str().to_string(),
                            loc,
                        })
                    };
                    vec![source]
                }
                type_sig_pack::PackedRef::RemoteRef(inner) => {
                    let remote_ref = remote_refs.get(inner.index);
                    source_of_remote_ref(remote_ref)
                }
                type_sig_pack::PackedRef::BuiltinRef(inner) => {
                    let loc = loc_of_index(loc_source.clone(), loc_of_aloc, inner.ref_loc);
                    vec![source_of_export::Source::Declaration(declaration::T {
                        name: inner.name.as_str().to_string(),
                        loc,
                    })]
                }
            }
        };
    let sources_of_all_exports: Vec<source_of_export::T> = match module_kind {
        type_sig_pack::ModuleKind::CJSModule { exports, .. } => {
            fn soes_of_packed_value<'a>(
                module_: &'a module_::T,
                loc_source: Option<FileKey>,
                loc_of_aloc: &'a LocOfALoc,
            ) -> Box<
                dyn Fn(
                        &type_sig::Value<Index<Loc>, type_sig_pack::Packed<Index<Loc>>>,
                    ) -> Vec<source_of_export::T>
                    + 'a,
            > {
                Box::new(move |value| match value {
                    type_sig::Value::ObjLit(obj_lit) => obj_lit
                        .props
                        .iter()
                        .flat_map(|(name, prop)| {
                            let me = module_export::T {
                                module_: module_.clone(),
                                export: export::T::CommonJSMember(name.as_str().to_string()),
                            };
                            let source = {
                                let loc =
                                    loc_of_obj_value_prop(loc_source.clone(), loc_of_aloc, prop);
                                source_of_export::Source::MemberDeclaration(member_declaration::T {
                                    name: name.as_str().to_string(),
                                    loc,
                                })
                            };
                            vec![source_of_export::T {
                                module_export: me,
                                source,
                            }]
                        })
                        .collect::<Vec<_>>(),
                    _ => vec![],
                })
            }
            fn soes_of_packed_def<'a>(
                module_: &'a module_::T,
                loc_source: Option<FileKey>,
                loc_of_aloc: &'a LocOfALoc,
                local_defs: &'a Table<PackedDef<Index<Loc>>>,
                module_refs: &'a Table<Userland>,
                remote_refs: &'a Table<type_sig_pack::RemoteRef<Index<Loc>>>,
                resolved_modules: &'a BTreeMap<
                    FlowImportSpecifier,
                    Result<Dependency, Option<FlowImportSpecifier>>,
                >,
                root: &'a str,
                write_root: &'a str,
                seen: &mut HashSet<usize>,
                packed_def: &PackedDef<Index<Loc>>,
            ) -> Vec<source_of_export::T> {
                match packed_def {
                    type_sig::Def::Variable(inner) => soes_of_packed(
                        module_,
                        loc_source,
                        loc_of_aloc,
                        local_defs,
                        module_refs,
                        remote_refs,
                        resolved_modules,
                        root,
                        write_root,
                        seen,
                        &inner.def,
                    ),
                    _ => vec![],
                }
            }
            fn soes_of_packed_annot(
                module_: &module_::T,
                loc_source: Option<FileKey>,
                loc_of_aloc: &LocOfALoc,
                annot: &type_sig::Annot<Index<Loc>, type_sig_pack::Packed<Index<Loc>>>,
            ) -> Vec<source_of_export::T> {
                match annot {
                    type_sig::Annot::ObjAnnot(obj_annot) => obj_annot
                        .props
                        .iter()
                        .flat_map(|(name, prop)| {
                            let me = module_export::T {
                                module_: module_.clone(),
                                export: export::T::CommonJSMember(name.as_str().to_string()),
                            };
                            let source = {
                                let loc =
                                    loc_of_obj_annot_prop(loc_source.clone(), loc_of_aloc, prop);
                                source_of_export::Source::MemberDeclaration(member_declaration::T {
                                    name: name.as_str().to_string(),
                                    loc,
                                })
                            };
                            vec![source_of_export::T {
                                module_export: me,
                                source,
                            }]
                        })
                        .collect::<Vec<_>>(),
                    _ => vec![],
                }
            }
            fn soes_of_packed<'a>(
                module_: &'a module_::T,
                loc_source: Option<FileKey>,
                loc_of_aloc: &'a LocOfALoc,
                local_defs: &'a Table<PackedDef<Index<Loc>>>,
                module_refs: &'a Table<Userland>,
                remote_refs: &'a Table<type_sig_pack::RemoteRef<Index<Loc>>>,
                resolved_modules: &'a BTreeMap<
                    FlowImportSpecifier,
                    Result<Dependency, Option<FlowImportSpecifier>>,
                >,
                root: &'a str,
                write_root: &'a str,
                seen: &mut HashSet<usize>,
                packed: &type_sig_pack::Packed<Index<Loc>>,
            ) -> Vec<source_of_export::T> {
                match packed {
                    type_sig_pack::Packed::Value(packed_value) => {
                        soes_of_packed_value(module_, loc_source, loc_of_aloc)(packed_value)
                    }
                    type_sig_pack::Packed::Ref(packed_ref) => {
                        let source_of_packed_ref_fn = |pr: &type_sig_pack::PackedRef<Index<Loc>>| -> Vec<source_of_export::Source> {
                            match pr {
                                type_sig_pack::PackedRef::LocalRef(inner) => {
                                    let pd = local_defs.get(inner.index);
                                    let name = pd.name();
                                    let loc = loc_of_def(loc_source.clone(), loc_of_aloc, pd);
                                    vec![source_of_export::Source::Declaration(declaration::T {
                                        name: name.as_str().to_string(),
                                        loc,
                                    })]
                                }
                                type_sig_pack::PackedRef::RemoteRef(inner) => {
                                    let rr = remote_refs.get(inner.index);
                                    match rr {
                                        type_sig_pack::RemoteRef::Import { index, remote, .. } => {
                                            let m = {
                                                let module_ref = module_refs.get(*index);
                                                module_of_module_ref(resolved_modules, root, write_root, module_ref)
                                            };
                                            let export = export::T::Named(remote.as_str().to_string());
                                            vec![source_of_export::Source::ModuleExport(module_export::T {
                                                module_: m,
                                                export,
                                            })]
                                        }
                                        type_sig_pack::RemoteRef::ImportNs { index, .. } => {
                                            let m = {
                                                let module_ref = module_refs.get(*index);
                                                module_of_module_ref(resolved_modules, root, write_root, module_ref)
                                            };
                                            vec![source_of_export::Source::ModuleNamespace(m)]
                                        }
                                        _ => vec![],
                                    }
                                }
                                type_sig_pack::PackedRef::BuiltinRef(inner) => {
                                    let loc = loc_of_index(loc_source.clone(), loc_of_aloc, inner.ref_loc);
                                    vec![source_of_export::Source::Declaration(declaration::T {
                                        name: inner.name.as_str().to_string(),
                                        loc,
                                    })]
                                }
                            }
                        };
                        let overall_source_of_exports: Vec<source_of_export::T> =
                            source_of_packed_ref_fn(packed_ref)
                                .into_iter()
                                .map(|source| {
                                    let me = module_export::T {
                                        module_: module_.clone(),
                                        export: export::T::CommonJS,
                                    };
                                    source_of_export::T {
                                        module_export: me,
                                        source,
                                    }
                                })
                                .collect();
                        let member_source_of_exports: Vec<source_of_export::T> = match packed_ref {
                            type_sig_pack::PackedRef::LocalRef(inner)
                                if !seen.contains(&inner.index.as_usize()) =>
                            {
                                let packed_def = local_defs.get(inner.index);
                                seen.insert(inner.index.as_usize());
                                soes_of_packed_def(
                                    module_,
                                    loc_source,
                                    loc_of_aloc,
                                    local_defs,
                                    module_refs,
                                    remote_refs,
                                    resolved_modules,
                                    root,
                                    write_root,
                                    seen,
                                    packed_def,
                                )
                            }
                            _ => vec![],
                        };
                        let mut result = overall_source_of_exports;
                        result.extend(member_source_of_exports);
                        result
                    }
                    type_sig_pack::Packed::Annot(packed_annot) => {
                        soes_of_packed_annot(module_, loc_source, loc_of_aloc, packed_annot)
                    }
                    // if we want to, we could follow TyRefs through to Annots
                    type_sig_pack::Packed::TyRef(_) => vec![],
                    _ => vec![],
                }
            }
            let mut seen = HashSet::new();
            exports
                .iter()
                .flat_map(|exp| {
                    soes_of_packed(
                        &module_,
                        loc_source.clone(),
                        loc_of_aloc,
                        local_defs,
                        module_refs,
                        remote_refs,
                        resolved_modules,
                        root,
                        write_root,
                        &mut seen,
                        exp,
                    )
                })
                .collect()
        }
        type_sig_pack::ModuleKind::ESModule {
            exports,
            info: type_sig_pack::ESModuleInfo {
                export_keys, stars, ..
            },
            ..
        } => {
            let sources_of_non_star_exports: Vec<source_of_export::T> = exports
                .iter()
                .enumerate()
                .flat_map(|(i, es_export)| {
                    let export_key = &export_keys[i];
                    match es_export {
                        type_sig_pack::Export::ExportRef(packed_ref) => {
                            let export = export::T::Named(export_key.as_str().to_string());
                            let me = module_export::T {
                                module_: module_.clone(),
                                export,
                            };
                            source_of_packed_ref(packed_ref)
                                .into_iter()
                                .map(|source| source_of_export::T {
                                    module_export: me.clone(),
                                    source,
                                })
                                .collect::<Vec<_>>()
                        }
                        type_sig_pack::Export::ExportBinding(index) => {
                            let packed_def = local_defs.get(*index);
                            let name = packed_def.name();
                            let me = module_export::T {
                                module_: module_.clone(),
                                export: export::T::Named(name.as_str().to_string()),
                            };
                            let source = {
                                let loc = loc_of_def(loc_source.clone(), loc_of_aloc, packed_def);
                                source_of_export::Source::Declaration(declaration::T {
                                    name: name.as_str().to_string(),
                                    loc,
                                })
                            };
                            vec![source_of_export::T {
                                module_export: me,
                                source,
                            }]
                        }
                        type_sig_pack::Export::ExportDefault(inner) => {
                            let me = module_export::T {
                                module_: module_.clone(),
                                export: export::T::Default,
                            };
                            let sources: Vec<source_of_export::Source> = match &inner.def {
                                type_sig_pack::Packed::Ref(packed_ref) => {
                                    source_of_packed_ref(packed_ref)
                                }
                                _ => {
                                    let name = "default";
                                    let loc = loc_of_index(
                                        loc_source.clone(),
                                        loc_of_aloc,
                                        inner.default_loc,
                                    );
                                    vec![source_of_export::Source::Declaration(declaration::T {
                                        name: name.to_string(),
                                        loc,
                                    })]
                                }
                            };
                            sources
                                .into_iter()
                                .map(|source| source_of_export::T {
                                    module_export: me.clone(),
                                    source,
                                })
                                .collect::<Vec<_>>()
                        }
                        type_sig_pack::Export::ExportDefaultBinding(inner) => {
                            let me = module_export::T {
                                module_: module_.clone(),
                                export: export::T::Default,
                            };
                            let source = {
                                let packed_def = local_defs.get(inner.index);
                                let name = packed_def.name();
                                let loc = loc_of_def(loc_source.clone(), loc_of_aloc, packed_def);
                                source_of_export::Source::Declaration(declaration::T {
                                    name: name.as_str().to_string(),
                                    loc,
                                })
                            };
                            vec![source_of_export::T {
                                module_export: me,
                                source,
                            }]
                        }
                        type_sig_pack::Export::ExportFrom(index) => {
                            let me = module_export::T {
                                module_: module_.clone(),
                                export: export::T::Named(export_key.as_str().to_string()),
                            };
                            let remote_ref = remote_refs.get(*index);
                            source_of_remote_ref(remote_ref)
                                .into_iter()
                                .map(|source| source_of_export::T {
                                    module_export: me.clone(),
                                    source,
                                })
                                .collect::<Vec<_>>()
                        }
                    }
                })
                .collect();
            let sources_of_star_exports: Vec<source_of_export::T> = stars
                .iter()
                .flat_map(|(_, index)| {
                    let star_module = {
                        let module_ref = module_refs.get(*index);
                        module_of_module_ref(resolved_modules, root, write_root, module_ref)
                    };
                    let me = module_export::T {
                        module_: module_.clone(),
                        export: export::T::Star(star_module.clone()),
                    };
                    let source = source_of_export::Source::ModuleNamespace(star_module);
                    vec![source_of_export::T {
                        module_export: me,
                        source,
                    }]
                })
                .collect();
            let mut result = sources_of_star_exports;
            result.extend(sources_of_non_star_exports);
            result
        }
    };
    sources_of_all_exports
        .iter()
        .map(|soe| source_of_export::to_json(root, write_root, offset_table_of_file_key, soe))
        .collect()
}

fn local_declaration_references(
    root: &str,
    write_root: &str,
    scope_info: &scope_api::ScopeInfo<Loc>,
    offset_table_of_file_key: &OffsetTableOfFileKey<'_>,
) -> Vec<Value> {
    let uses_of_all_defs = scope_info.uses_of_all_defs();
    let mut acc: Vec<Value> = Vec::new();
    for (def, uses) in uses_of_all_defs {
        let jsons: Vec<Value> = def
            .locs
            .iter()
            .flat_map(|loc| {
                let decl = declaration::T {
                    name: def.actual_name.as_str().to_string(),
                    loc: loc.clone(),
                };
                uses.iter()
                    .filter(|use_loc| *use_loc != loc)
                    .map(|use_loc| {
                        let ldr = local_declaration_reference::T {
                            declaration: declaration::T {
                                name: decl.name.clone(),
                                loc: decl.loc.clone(),
                            },
                            loc: use_loc.clone(),
                        };
                        local_declaration_reference::to_json(
                            root,
                            write_root,
                            offset_table_of_file_key,
                            &ldr,
                        )
                    })
                    .collect::<Vec<_>>()
            })
            .collect();
        acc.extend(jsons);
    }
    acc
}

fn module_documentations(
    root: &str,
    write_root: &str,
    ast: &ast::Program<Loc, Loc>,
    file: &FileKey,
    offset_table_of_file_key: &OffsetTableOfFileKey<'_>,
) -> Vec<Value> {
    match (
        module_::of_file_key(root, write_root, file),
        find_documentation::module_doc_loc(ast),
    ) {
        (module_::T::File(file), Some(documentation)) => vec![module_docblock_range::to_json(
            offset_table_of_file_key,
            &module_docblock_range::T {
                module_: module_::T::File(file.clone()),
                file,
                span: documentation,
            },
        )],
        _ => vec![],
    }
}

#[derive(Clone, Copy)]
enum DeclarationKind {
    Declaration,
    MemberDeclaration,
    TypeDeclaration,
}

struct DeclarationInfoCollector<'a, F, G, H> {
    scope_info: &'a scope_api::ScopeInfo<Loc>,
    loc_of_aloc: &'a LocOfALoc,
    add_var_info: F,
    add_member_info: G,
    add_type_info: H,
}

impl<'ast, F, G, H> AstVisitor<'ast, ALoc, (ALoc, Type), &'ast ALoc, !>
    for DeclarationInfoCollector<'_, F, G, H>
where
    F: FnMut(&str, Loc, &Type),
    G: FnMut(&str, Loc, &Type),
    H: FnMut(&str, Loc, &Type),
{
    fn normalize_loc(loc: &'ast ALoc) -> &'ast ALoc {
        loc
    }

    fn normalize_type(type_: &'ast (ALoc, Type)) -> &'ast ALoc {
        &type_.0
    }

    fn identifier(&mut self, ident: &'ast ast::Identifier<ALoc, (ALoc, Type)>) -> Result<(), !> {
        let loc = (self.loc_of_aloc)(&ident.loc.0);
        if self.scope_info.is_local_use(&loc) && self.scope_info.use_is_def(&loc) {
            (self.add_var_info)(ident.name.as_str(), loc, &ident.loc.1);
        }
        Ok(())
    }

    fn object_key_identifier(
        &mut self,
        ident: &'ast ast::Identifier<ALoc, (ALoc, Type)>,
    ) -> Result<(), !> {
        let loc = (self.loc_of_aloc)(&ident.loc.0);
        (self.add_member_info)(ident.name.as_str(), loc, &ident.loc.1);
        Ok(())
    }

    fn type_alias(
        &mut self,
        _loc: &'ast ALoc,
        alias: &'ast ast::statement::TypeAlias<ALoc, (ALoc, Type)>,
    ) -> Result<(), !> {
        let loc = (self.loc_of_aloc)(&alias.id.loc.0);
        (self.add_type_info)(alias.id.name.as_str(), loc, &alias.id.loc.1);
        Ok(())
    }

    fn opaque_type(
        &mut self,
        _loc: &'ast ALoc,
        otype: &'ast ast::statement::OpaqueType<ALoc, (ALoc, Type)>,
    ) -> Result<(), !> {
        let loc = (self.loc_of_aloc)(&otype.id.loc.0);
        (self.add_type_info)(otype.id.name.as_str(), loc, &otype.id.loc.1);
        Ok(())
    }

    fn interface(
        &mut self,
        _loc: &'ast ALoc,
        iface: &'ast ast::statement::Interface<ALoc, (ALoc, Type)>,
    ) -> Result<(), !> {
        let loc = (self.loc_of_aloc)(&iface.id.loc.0);
        (self.add_type_info)(iface.id.name.as_str(), loc, &iface.id.loc.1);
        Ok(())
    }

    fn class_identifier(
        &mut self,
        ident: &'ast ast::Identifier<ALoc, (ALoc, Type)>,
    ) -> Result<(), !> {
        let loc = (self.loc_of_aloc)(&ident.loc.0);
        (self.add_type_info)(ident.name.as_str(), loc, &ident.loc.1);
        ast_visitor::class_identifier_default(self, ident)
    }

    fn enum_declaration(
        &mut self,
        _loc: &'ast ALoc,
        enum_: &'ast ast::statement::EnumDeclaration<ALoc, (ALoc, Type)>,
    ) -> Result<(), !> {
        let loc = (self.loc_of_aloc)(&enum_.id.loc.0);
        (self.add_type_info)(enum_.id.name.as_str(), loc, &enum_.id.loc.1);
        for member in enum_.body.members.iter() {
            let (aloc, name) = match member {
                ast::statement::enum_declaration::Member::BooleanMember(member) => (
                    &member.loc,
                    ast_utils::string_of_enum_member_name(&member.id),
                ),
                ast::statement::enum_declaration::Member::NumberMember(member) => (
                    &member.loc,
                    ast_utils::string_of_enum_member_name(&member.id),
                ),
                ast::statement::enum_declaration::Member::StringMember(member) => (
                    &member.loc,
                    ast_utils::string_of_enum_member_name(&member.id),
                ),
                ast::statement::enum_declaration::Member::BigIntMember(member) => (
                    &member.loc,
                    ast_utils::string_of_enum_member_name(&member.id),
                ),
                ast::statement::enum_declaration::Member::DefaultedMember(member) => (
                    &member.loc,
                    ast_utils::string_of_enum_member_name(&member.id),
                ),
            };
            let loc = (self.loc_of_aloc)(aloc);
            (self.add_member_info)(name, loc, &enum_.id.loc.1);
        }
        ast_visitor::enum_declaration_default(self, _loc, enum_)
    }
}

fn declaration_infos<'a, 'cx>(
    imported_names: &ty_normalizer_flow::ImportedNames<'a, 'cx>,
    root: &str,
    write_root: &str,
    glean_log: bool,
    scope_info: &scope_api::ScopeInfo<Loc>,
    file: &FileKey,
    file_sig: &Arc<FileSig>,
    cx: &mut Context<'cx>,
    loc_of_aloc: &LocOfALoc,
    typed_ast: &'a ast::Program<ALoc, (ALoc, Type)>,
    ast: &ast::Program<Loc, Loc>,
    offset_table_of_file_key: &OffsetTableOfFileKey<'_>,
) -> (Vec<Value>, Vec<Value>, Vec<Value>) {
    let infos = std::cell::RefCell::new(Vec::<((DeclarationKind, String, Loc), Type)>::new());
    let add_var_info = |name: &str, loc: Loc, type_: &Type| {
        infos.borrow_mut().push((
            (DeclarationKind::Declaration, name.to_string(), loc),
            type_.dupe(),
        ));
    };
    let add_member_info = |name: &str, loc: Loc, type_: &Type| {
        infos.borrow_mut().push((
            (DeclarationKind::MemberDeclaration, name.to_string(), loc),
            type_.dupe(),
        ));
    };
    let add_type_info = |name: &str, loc: Loc, type_: &Type| {
        infos.borrow_mut().push((
            (DeclarationKind::TypeDeclaration, name.to_string(), loc),
            type_.dupe(),
        ));
    };
    let mut collector = DeclarationInfoCollector {
        scope_info,
        loc_of_aloc,
        add_var_info,
        add_member_info,
        add_type_info,
    };
    let Ok(()) = collector.program(typed_ast);
    // OCaml builds this list with `::`; append and reverse once to keep the
    // same order without front-inserting every declaration.
    let mut infos = infos.into_inner();
    infos.reverse();

    let options = TyNormalizerOptions::default();
    let genv = ty_normalizer_flow::mk_genv_with_imported_names(
        options,
        Some(typed_ast),
        file_sig.clone(),
        imported_names.dupe(),
    );
    let docs_and_spans = documentation_fullspan_map::create(ast, Some(file));
    let printer_opts = ty_printer::PrinterOptions::default();
    let mut var_infos = Vec::new();
    let mut member_infos = Vec::new();
    let mut type_infos = Vec::new();
    let log_normalizing = |(_, _, loc): &(DeclarationKind, String, Loc)| {
        if glean_log {
            eprintln!("normalizing: {:?}", loc);
        }
    };
    for ((kind, name, loc), elt_result) in
        ty_normalizer_flow::from_types(Some(&log_normalizing), cx, &genv, infos)
    {
        let Ok(elt) = elt_result else {
            continue;
        };
        let type_ = ty_printer::string_of_elt(&elt, &printer_opts);
        let (documentation, span) = match docs_and_spans.get(&loc) {
            None => (None, None),
            Some(documentation_fullspan_map::DocSpan {
                documentation,
                span: None,
            }) => (documentation.clone(), Some(loc.clone())),
            Some(documentation_fullspan_map::DocSpan {
                documentation,
                span,
            }) => (documentation.clone(), span.clone()),
        };
        match kind {
            DeclarationKind::Declaration => {
                let declaration = declaration::T { name, loc };
                var_infos.push(declaration_info::to_json(
                    root,
                    write_root,
                    offset_table_of_file_key,
                    &declaration_info::T {
                        declaration,
                        type_,
                        documentation,
                        span,
                    },
                ));
            }
            DeclarationKind::MemberDeclaration => {
                let member_declaration = member_declaration::T { name, loc };
                member_infos.push(member_declaration_info::to_json(
                    root,
                    write_root,
                    offset_table_of_file_key,
                    &member_declaration_info::T {
                        member_declaration,
                        type_,
                        documentation,
                        span,
                    },
                ));
            }
            DeclarationKind::TypeDeclaration => {
                let type_declaration = type_declaration::T { name, loc };
                type_infos.push(type_declaration_info::to_json(
                    root,
                    write_root,
                    offset_table_of_file_key,
                    &type_declaration_info::T {
                        type_declaration,
                        type_,
                        documentation,
                        span,
                    },
                ));
            }
        }
    }
    var_infos.reverse();
    member_infos.reverse();
    type_infos.reverse();
    (var_infos, member_infos, type_infos)
}

fn file_of_string_modules(
    root: &str,
    write_root: &str,
    options: &Options,
    file_key: &FileKey,
) -> Vec<Value> {
    let file = match remove_dot_flow_suffix(module_::of_file_key(root, write_root, file_key)) {
        module_::T::File(file) => file,
        _ => return vec![],
    };
    let string = match flow_services_module::exported_module(
        options,
        file_key,
        &flow_services_module::PackageInfo::none(),
    ) {
        Some(haste_module_info) => haste_module_info.module_name().as_str().to_string(),
        None => return vec![],
    };
    vec![file_of_string_module::to_json(&file_of_string_module::T {
        file,
        string,
    })]
}

fn file_liness(
    root: &str,
    write_root: &str,
    file_key: &FileKey,
    offset_cache: &OffsetCache,
) -> Vec<Value> {
    let file = match module_::of_file_key(root, write_root, file_key) {
        module_::T::File(file) => file,
        _ => return vec![],
    };
    let info = match offset_cache.info_of_file_key(file_key) {
        Some(info) => info,
        None => return vec![],
    };
    let mut lengths: Vec<i32> = info
        .offsets
        .line_lengths()
        .iter()
        .map(|&l| l as i32)
        .collect();
    // Rust's offset table keeps an extra exclusive newline lookup slot for parser ranges.
    // OCaml FileLines lengths are based on newline-start offsets.
    if let Some(first) = lengths.first_mut() {
        *first -= 1;
    }
    let has_unicode_or_tabs = info.offsets.contains_multibyte_character();
    vec![src::file_lines::to_json(&src::file_lines::T {
        file,
        lengths,
        ends_in_newline: info.ends_in_newline,
        has_unicode_or_tabs,
    })]
}

// Latest version of the 'all' schema supported by this indexer
pub(crate) const ALL_SCHEMA_VERSION: i32 = 7;

pub(crate) const FLOW_SCHEMA_VERSION: i32 = 3;

struct GleanRunnerConfig;

impl codemod_runner::SimpleTypedRunnerConfig for GleanRunnerConfig {
    type Accumulator = GleanAccumulator;

    fn reporter() -> codemod_report::CodemodReport<Self::Accumulator> {
        codemod_report::CodemodReport {
            report: codemod_report::Reporter::StringReporter(Box::new(|_, value| {
                use std::io::Write;
                for output_file in &value.json_filenames {
                    let oc = std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(output_file)
                        .unwrap();
                    let mut oc = std::io::BufWriter::new(oc);
                    oc.write_all(b"]").unwrap();
                    oc.flush().unwrap();
                }
                format!(
                    "Wrote facts about {} JavaScript files.",
                    value.files_analyzed
                )
            })),
            combine: Box::new(|left, right| GleanAccumulator {
                files_analyzed: left.files_analyzed + right.files_analyzed,
                json_filenames: left
                    .json_filenames
                    .union(&right.json_filenames)
                    .cloned()
                    .collect(),
            }),
            empty: GleanAccumulator {
                files_analyzed: 0,
                json_filenames: BTreeSet::new(),
            },
        }
    }

    fn expand_roots(
        env: &flow_server_env::server_env::Env,
        options: &Options,
        roots: BTreeSet<FileKey>,
    ) -> BTreeSet<FileKey> {
        let config = GLEAN_RUNTIME_CONFIG.get().unwrap();
        let roots = if config.project_root_input {
            let roots = env
                .files
                .iter()
                .filter(|file| file.to_path_buf().starts_with(&*options.root))
                .cloned()
                .collect();
            codemod_runner::get_target_filename_set(&options.file_options, options.all, roots)
        } else {
            roots
        };
        if config.include_direct_deps {
            let roots: FlowOrdSet<FileKey> = roots.iter().cloned().collect();
            let dependency_info = env.dependency_info();
            flow_services_inference::pure_dep_graph_operations::calc_direct_dependencies(
                dependency_info.implementation_dependency_graph(),
                &roots,
            )
            .iter()
            .cloned()
            .collect()
        } else {
            roots
        }
    }

    fn check_options(options: Options) -> Options {
        options
    }

    fn visit(
        options: &Options,
        ast: &ast::Program<Loc, Loc>,
        cctx: &mut flow_codemods::utils::codemod_context::typed::TypedCodemodContext<'_>,
    ) -> Self::Accumulator {
        let config = GLEAN_RUNTIME_CONFIG.get().unwrap();
        let file = &cctx.file;
        let file_sig = &cctx.file_sig;
        let type_sig = &cctx.type_sig;
        if config.glean_log {
            eprintln!("visiting: {}", file.to_absolute());
        }
        let mut f = || {
            let root = options.root.to_string_lossy().to_string();
            let root = &root;
            let reader = &cctx.reader;
            let loc_of_aloc = make_loc_of_aloc(cctx.reader.dupe());
            let offset_cache = OffsetCache::default();
            let resolved_modules = reader.get_resolved_modules_unsafe(file);
            let log = |msg: &str| {
                if config.glean_log {
                    eprintln!("{}: {}", msg, file.to_absolute());
                }
            };
            let imported_names = {
                let options = TyNormalizerOptions::default();
                let genv =
                    ty_normalizer_flow::mk_genv(options, Some(&cctx.typed_ast), file_sig.clone());
                genv.imported_names.dupe()
            };
            log("scope info");
            let scope_info =
                scope_builder::program(options.enums, false, file_sig.requires().is_empty(), ast);
            log("module documentations");
            let offset_table_of_file_key = |file_key: &FileKey| -> Option<Arc<OffsetTable>> {
                offset_cache.offset_table_of_file_key(file_key)
            };
            let module_documentation = module_documentations(
                root,
                &config.write_root,
                ast,
                file,
                &offset_table_of_file_key,
            );
            log("declaration info");
            let (declaration_info, member_declaration_info, type_declaration_info) =
                declaration_infos(
                    &imported_names,
                    root,
                    &config.write_root,
                    config.glean_log,
                    &scope_info,
                    file,
                    file_sig,
                    &mut cctx.cx,
                    &loc_of_aloc,
                    &cctx.typed_ast,
                    ast,
                    &offset_table_of_file_key,
                );
            log("type import declaration");
            let type_import_declaration = type_import_declarations(
                root,
                &config.write_root,
                &resolved_modules,
                file_sig.as_ref(),
                &offset_table_of_file_key,
            );
            let loc_source = ast.loc.source.clone();
            log("source of exports");
            let source_of_export = source_of_exports(
                root,
                &config.write_root,
                loc_source.clone(),
                type_sig.as_ref(),
                &resolved_modules,
                &loc_of_aloc,
                &offset_table_of_file_key,
            );
            log("source of type exports");
            let source_of_type_export = source_of_type_exports(
                root,
                &config.write_root,
                file,
                &loc_of_aloc,
                loc_source.clone(),
                type_sig.as_ref(),
                &resolved_modules,
                &offset_table_of_file_key,
            );
            log("local declaration reference");
            let local_declaration_reference = local_declaration_references(
                root,
                &config.write_root,
                &scope_info,
                &offset_table_of_file_key,
            );
            log("import declaration");
            let import_declaration = import_declarations(
                root,
                &config.write_root,
                &resolved_modules,
                file_sig.as_ref(),
                &offset_table_of_file_key,
            );
            log("member declaration reference");
            let member_declaration_reference = member_declaration_references(
                &imported_names,
                root,
                &config.write_root,
                &loc_of_aloc,
                &mut cctx.cx,
                &cctx.typed_ast,
                file_sig,
                &offset_table_of_file_key,
            );
            log("type declaration reference");
            let type_declaration_reference = type_declaration_references(
                root,
                &config.write_root,
                &loc_of_aloc,
                &mut cctx.cx,
                &cctx.typed_ast,
                &offset_table_of_file_key,
            );
            log("file of string module");
            let file_of_string_module =
                file_of_string_modules(root, &config.write_root, options, file);
            let file_lines = file_liness(root, &config.write_root, file, &offset_cache);
            let json_filenames = match &config.output_dir {
                None => BTreeSet::new(),
                Some(output_dir) => {
                    log("outputting");
                    let output_file = {
                        let file_name = format!(
                            "{}-{:?}.json",
                            std::process::id(),
                            std::thread::current().id()
                        );
                        output_dir.join(file_name).to_string_lossy().to_string()
                    };
                    let is_first_write_to_file = !std::path::Path::new(&output_file).exists();
                    let out_channel = std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(&output_file)
                        .unwrap();
                    let mut out_channel = std::io::BufWriter::new(out_channel);
                    use std::io::Write;
                    fn output_facts<W: std::io::Write>(
                        out_channel: &mut W,
                        predicate: &str,
                        facts: &[Value],
                    ) {
                        out_channel.write_all(b"{\"predicate\":").unwrap();
                        serde_json::to_writer(&mut *out_channel, predicate).unwrap();
                        out_channel.write_all(b",\"facts\":").unwrap();
                        serde_json::to_writer(&mut *out_channel, facts).unwrap();
                        out_channel.write_all(b"}").unwrap();
                    }
                    if is_first_write_to_file {
                        out_channel.write_all(b"[").unwrap();
                    } else {
                        out_channel.write_all(b",").unwrap();
                    }
                    let flow_pred =
                        |pred: &str| -> String { format!("flow.{}.{}", pred, FLOW_SCHEMA_VERSION) };
                    output_facts(
                        &mut out_channel,
                        &flow_pred("LocalDeclarationReference"),
                        &local_declaration_reference,
                    );
                    out_channel.write_all(b",").unwrap();
                    output_facts(
                        &mut out_channel,
                        &flow_pred("DeclarationInfo"),
                        &declaration_info,
                    );
                    out_channel.write_all(b",").unwrap();
                    output_facts(
                        &mut out_channel,
                        &flow_pred("SourceOfExport"),
                        &source_of_export,
                    );
                    out_channel.write_all(b",").unwrap();
                    output_facts(
                        &mut out_channel,
                        &flow_pred("ImportDeclaration"),
                        &import_declaration,
                    );
                    out_channel.write_all(b",").unwrap();
                    output_facts(
                        &mut out_channel,
                        &flow_pred("MemberDeclarationReference"),
                        &member_declaration_reference,
                    );
                    out_channel.write_all(b",").unwrap();
                    output_facts(
                        &mut out_channel,
                        &flow_pred("MemberDeclarationInfo"),
                        &member_declaration_info,
                    );
                    out_channel.write_all(b",").unwrap();
                    output_facts(
                        &mut out_channel,
                        &flow_pred("TypeDeclarationReference"),
                        &type_declaration_reference,
                    );
                    out_channel.write_all(b",").unwrap();
                    output_facts(
                        &mut out_channel,
                        &flow_pred("TypeDeclarationInfo"),
                        &type_declaration_info,
                    );
                    out_channel.write_all(b",").unwrap();
                    output_facts(
                        &mut out_channel,
                        &flow_pred("TypeImportDeclaration"),
                        &type_import_declaration,
                    );
                    out_channel.write_all(b",").unwrap();
                    output_facts(
                        &mut out_channel,
                        &flow_pred("SourceOfTypeExport"),
                        &source_of_type_export,
                    );
                    out_channel.write_all(b",").unwrap();
                    output_facts(
                        &mut out_channel,
                        &flow_pred("FileOfStringModule"),
                        &file_of_string_module,
                    );
                    out_channel.write_all(b",").unwrap();
                    output_facts(
                        &mut out_channel,
                        &flow_pred("ModuleDocblockRange"),
                        &module_documentation,
                    );
                    out_channel.write_all(b",").unwrap();
                    output_facts(&mut out_channel, "src.FileLines.1", &file_lines);
                    out_channel.flush().unwrap();
                    let mut set = BTreeSet::new();
                    set.insert(output_file);
                    set
                }
            };
            log("done");
            GleanAccumulator {
                files_analyzed: 1,
                json_filenames,
            }
        };
        if config.glean_timeout > 0 {
            let (timeout_canceller, timeout_receiver) = std::sync::mpsc::channel::<()>();
            let timeout = config.glean_timeout;
            let file = file.to_absolute();
            let timeout_thread = std::thread::Builder::new()
                .name("glean_runner_visit_timeout".to_string())
                .spawn(move || {
                    match timeout_receiver
                        .recv_timeout(std::time::Duration::from_secs(timeout as u64))
                    {
                        Ok(()) | Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {}
                        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                            let msg = format!("Timed out visiting: {}", file);
                            flow_common_exit_status::exit_with_msg(
                                flow_common_exit_status::FlowExitStatus::UnknownError,
                                &msg,
                            );
                        }
                    }
                })
                .expect("failed to spawn glean_runner_visit_timeout thread");
            let ret = f();
            match timeout_canceller.send(()) {
                Ok(()) | Err(_) => {}
            }
            timeout_thread
                .join()
                .expect("glean_runner_visit_timeout thread panicked");
            ret
        } else {
            f()
        }
    }
}

#[derive(Clone, Debug)]
struct GleanAccumulator {
    files_analyzed: usize,
    json_filenames: BTreeSet<String>,
}

pub(crate) fn make(
    output_dir_opt: Option<PathBuf>,
    write_root: String,
    include_direct_deps: bool,
    include_reachable_deps: bool,
    glean_log: bool,
    glean_timeout: i32,
    prepared: &PreparedCodemod,
) {
    GLEAN_RUNTIME_CONFIG
        .set(GleanRuntimeConfig {
            output_dir: output_dir_opt,
            write_root,
            include_direct_deps,
            project_root_input: prepared.project_root_input,
            glean_log,
            glean_timeout,
        })
        .expect("glean config already initialized");
    flow_event_logger::set_root(Some(prepared.options.root.to_string_lossy().into_owned()));
    flow_event_logger::set_eden(Some(flow_common_vcs::eden::is_eden(&prepared.options.root)));
    flow_logging_utils::set_server_options(&prepared.options);

    let should_print_summary = prepared.options.profile && !prepared.options.quiet;
    let (profiling, ()) = profiling_js::with_profiling_sync("Glean", should_print_summary, |_| {
        if include_reachable_deps {
            MakeMain::<codemod_runner::MakeSimpleTypedTwoPassRunner<GleanRunnerConfig>>::main(
                &prepared.options,
                prepared.write,
                prepared.repeat,
                prepared.log_level,
                prepared.roots.clone(),
            );
        } else {
            MakeMain::<codemod_runner::MakeSimpleTypedRunner<GleanRunnerConfig>>::main(
                &prepared.options,
                prepared.write,
                prepared.repeat,
                prepared.log_level,
                prepared.roots.clone(),
            );
        }
    });
    let profiling_props = profiling.to_event_logger_json_properties();
    let init_trigger = string_of_init_trigger(StartCause::UserInitiated);
    let saved_state_fetcher = string_of_saved_state_fetcher(&prepared.options);
    flow_event_logger::init_done(None, init_trigger, saved_state_fetcher, &profiling_props);
}
