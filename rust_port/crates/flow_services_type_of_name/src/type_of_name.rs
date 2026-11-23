/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::Arc;

use dupe::Dupe;
use dupe::OptionDupedExt;
use flow_aloc::ALoc;
use flow_check_cache::CheckContentsCache;
use flow_common::options::Options;
use flow_common::reason;
use flow_common::reason::VirtualReasonDesc;
use flow_common_ty::ty;
use flow_common_ty::ty::ALocElt;
use flow_common_ty::ty::Decl;
use flow_common_ty::ty::DeclNominalComponentDeclData;
use flow_common_ty::ty::DeclTypeAliasDeclData;
use flow_common_ty::ty::Elt;
use flow_common_ty::ty::Ty;
use flow_common_ty::ty_printer;
use flow_common_ty::ty_printer::PrinterOptions;
use flow_heap::parsing_heaps::Transaction;
use flow_parser::ast;
use flow_parser::ast_visitor;
use flow_parser::ast_visitor::AstVisitor;
use flow_parser::file_key::FileKey;
use flow_parser::loc::Loc;
use flow_parser::loc_sig::LocSig;
use flow_server_env::server_orchestrator::ServerOrchestratorHandle;
use flow_server_env::server_prot::response;
use flow_server_env::server_prot::type_of_name_options;
use flow_services_autocomplete::find_documentation;
use flow_services_export::export_index;
use flow_services_export::export_search;
use flow_services_export::export_search_types;
use flow_services_inference::type_contents;
use flow_services_inference_types::FileArtifacts;
use flow_services_inference_types::ParseArtifacts;
use flow_services_inference_types::TypecheckArtifacts;
use flow_typing::ty_members;
use flow_typing::ty_normalizer_flow;
use flow_typing_context::Context;
use flow_typing_ty_normalizer::env::EvaluateTypeDestructorsMode;
use flow_typing_ty_normalizer::env::Genv;
use flow_typing_type::type_::Destructor;
use flow_typing_type::type_::Type;
use flow_typing_type::type_util;

enum Found {
    Found(ALoc, Type),
}

struct FindIdentifierVisitor<'a> {
    target_name: &'a str,
}

impl<'ast, 'a> AstVisitor<'ast, ALoc, (ALoc, Type), &'ast ALoc, Found>
    for FindIdentifierVisitor<'a>
{
    fn normalize_loc(loc: &'ast ALoc) -> &'ast ALoc {
        loc
    }

    fn normalize_type(type_: &'ast (ALoc, Type)) -> &'ast ALoc {
        &type_.0
    }

    fn identifier(&mut self, id: &'ast ast::Identifier<ALoc, (ALoc, Type)>) -> Result<(), Found> {
        let (aloc, t) = &id.loc;
        let id_name = &id.name;
        if id_name.as_str() == self.target_name {
            return Err(Found::Found(aloc.dupe(), t.dupe()));
        }
        ast_visitor::identifier_default(self, id)
    }
}

/// Find the first occurrence of the identifier by name and its type from typed_ast
fn find_identifier_and_type(
    target_name: &str,
    typed_ast: &ast::Program<ALoc, (ALoc, Type)>,
) -> Option<(ALoc, Type)> {
    let mut visitor = FindIdentifierVisitor { target_name };
    match visitor.program(typed_ast) {
        Err(Found::Found(aloc, t)) => Some((aloc, t)),
        Ok(()) => None,
    }
}

//  Extract per-prop documentation from flattened component props
fn extract_prop_docs(
    reader: &Transaction,
    ast: &ast::Program<Loc, Loc>,
    ty: &ALocElt,
) -> Option<Vec<response::infer_type_of_name::PropDoc>> {
    let get_ast_from_heap = |file_key: &FileKey| -> Option<ast::Program<Loc, Loc>> {
        reader.get_ast(file_key).map(|arc| (*arc).clone())
    };
    let props = match ty {
        Elt::Type(t)
            if let Ty::Component {
                regular_props: ty::ComponentProps::FlattenedComponentProps { props, .. },
                ..
            } = t.as_ref() =>
        {
            &props[..]
        }
        Elt::Decl(Decl::NominalComponentDecl(deref!(DeclNominalComponentDeclData {
            props: ty::ComponentProps::FlattenedComponentProps { props, .. },
            ..
        }))) => &props[..],
        _ => return None,
    };
    // Collect all def_locs, then batch-lookup JSDoc in a single AST traversal per file
    let prop_locs: Vec<Loc> = props
        .iter()
        .filter_map(|prop| {
            let ty::FlattenedComponentProp::FlattenedComponentProp { def_locs, .. } = prop;
            def_locs.first().map(|aloc| reader.loc_of_aloc(aloc))
        })
        .collect();
    let jsdoc_map = find_documentation::jsdocs_of_getdef_locs(ast, &get_ast_from_heap, &prop_locs);
    let docs: Vec<response::infer_type_of_name::PropDoc> = props
        .iter()
        .filter_map(|prop| {
            let ty::FlattenedComponentProp::FlattenedComponentProp { name, def_locs, .. } = prop;
            let prop_name = name.display_smol_str().to_string();
            let description = match def_locs.first() {
                Some(aloc) => {
                    let loc = reader.loc_of_aloc(aloc);
                    jsdoc_map
                        .get(&loc)
                        .and_then(find_documentation::documentation_of_jsdoc)
                }
                None => None,
            };
            description.map(|description| response::infer_type_of_name::PropDoc {
                prop_name,
                description,
            })
        })
        .collect();
    if docs.is_empty() { None } else { Some(docs) }
}

// Looks up a member's type and definition locations from an element.
fn lookup_member_in_component_props<L: Dupe>(
    member_name: &str,
    props: &[ty::FlattenedComponentProp<L>],
) -> Option<(Arc<Ty<L>>, Vec<L>)> {
    let target = reason::Name::new(member_name);
    props.iter().find_map(|prop| {
        let ty::FlattenedComponentProp::FlattenedComponentProp {
            name, t, def_locs, ..
        } = prop;
        if *name == target {
            Some((t.dupe(), def_locs.to_vec()))
        } else {
            None
        }
    })
}

fn lookup_member_in_obj_props<L: Dupe>(
    member_name: &str,
    obj_props: &[ty::Prop<L>],
) -> Option<(Arc<Ty<L>>, Vec<L>)> {
    let target = reason::Name::new(member_name);
    obj_props.iter().find_map(|prop| match prop {
        ty::Prop::NamedProp {
            name,
            prop,
            def_locs,
            ..
        } if *name == target => {
            let t = match prop {
                ty::NamedProp::Field { t, .. } => t.dupe(),
                ty::NamedProp::Method { fun: ft, .. } => Arc::new(Ty::Fun(Box::new(ft.clone()))),
                ty::NamedProp::Get(t) => t.dupe(),
                ty::NamedProp::Set(t) => t.dupe(),
            };
            Some((t, def_locs.to_vec()))
        }
        _ => None,
    })
}

fn lookup_member_in_elt<L: Dupe>(
    member_name: &str,
    ty_elt: &Elt<L>,
) -> Option<(Arc<Ty<L>>, Vec<L>)> {
    match ty_elt {
        // Component value types
        Elt::Type(t)
            if let Ty::Component {
                regular_props: ty::ComponentProps::FlattenedComponentProps { props, .. },
                ..
            } = t.as_ref() =>
        {
            lookup_member_in_component_props(member_name, props)
        }
        // Component declaration types
        Elt::Decl(Decl::NominalComponentDecl(deref!(DeclNominalComponentDeclData {
            props: ty::ComponentProps::FlattenedComponentProps { props, .. },
            ..
        }))) => lookup_member_in_component_props(member_name, props),
        // Object types
        Elt::Type(t) if let Ty::Obj(obj_t) = t.as_ref() => {
            lookup_member_in_obj_props(member_name, &obj_t.obj_props)
        }
        // Inline interfaces
        Elt::Type(t) if let Ty::InlineInterface(iface) = t.as_ref() => {
            lookup_member_in_obj_props(member_name, &iface.if_props)
        }
        // Type alias with body -- unwrap and recurse
        Elt::Decl(Decl::TypeAliasDecl(deref!(DeclTypeAliasDeclData {
            type_: Some(body), ..
        }))) => lookup_member_in_elt(member_name, &Elt::Type(body.dupe())),
        _ => None,
    }
}

// Generates a type summary string for ref expansion.
fn summarize_ty<L: Dupe>(t: &Ty<L>) -> Option<String> {
    let opts = PrinterOptions::default();
    let type_str = ty_printer::string_of_t_single_line(t, &opts);
    match t {
        Ty::Obj(obj_t) => {
            let n = obj_t.obj_props.len();
            if n > 5 {
                Some(format!("({} fields)", n))
            } else {
                Some(format!("= {}", type_str))
            }
        }
        Ty::Union(..) => {
            if type_str.len() > 60 {
                // Find a good truncation point near a '|' boundary
                let mut idx: usize = 57;
                while idx > 0 && type_str.as_bytes()[idx] != b'|' {
                    idx -= 1;
                }
                let truncation_point = if idx > 0 { idx - 1 } else { 57 };
                Some(format!("= {} | ...", &type_str[..truncation_point]))
            } else {
                Some(format!("= {}", type_str))
            }
        }
        Ty::Fun(_) => {
            if type_str.len() > 80 {
                None
            } else {
                Some(format!("= {}", type_str))
            }
        }
        Ty::Generic(deref!((_, ty::GenKind::ClassKind, _))) => Some("(class)".to_string()),
        Ty::Generic(deref!((_, ty::GenKind::InterfaceKind, _))) => Some("(interface)".to_string()),
        Ty::Generic(deref!((_, ty::GenKind::EnumKind, _))) => Some("(enum)".to_string()),
        Ty::Generic(deref!((_, ty::GenKind::ComponentKind, _))) => Some("(component)".to_string()),
        _ => {
            if type_str.len() > 60 {
                None
            } else {
                Some(format!("= {}", type_str))
            }
        }
    }
}

// Generate a type summary from a Ty.elt
fn summarize_ty_elt<L: Dupe>(ty_elt: &Elt<L>) -> Option<String> {
    match ty_elt {
        Elt::Decl(Decl::ClassDecl(..)) => Some("(class)".to_string()),
        Elt::Decl(Decl::InterfaceDecl(..)) => Some("(interface)".to_string()),
        Elt::Decl(Decl::NominalComponentDecl(..)) => Some("(component)".to_string()),
        Elt::Decl(Decl::EnumDecl(..)) => Some("(enum)".to_string()),
        Elt::Decl(Decl::TypeAliasDecl(deref!(DeclTypeAliasDeclData {
            tparams: Some(_), ..
        }))) => Some("(generic type)".to_string()),
        Elt::Decl(Decl::TypeAliasDecl(deref!(DeclTypeAliasDeclData {
            type_: Some(body),
            tparams: None,
            ..
        }))) => summarize_ty(body),
        Elt::Type(t) => summarize_ty(t),
        _ => None,
    }
}

// Compute a type summary for a ref's body Type.t
fn compute_ref_summary<'cx>(
    cx: &mut Context<'cx>,
    genv: &Genv<'_, 'cx>,
    body_type: &Type,
) -> Option<String> {
    // Strip the RTypeAlias reason to force the normalizer to expand the body
    // instead of creating another Generic node
    let stripped = type_util::mod_reason_of_t(
        &|r| {
            if matches!(r.desc(false), Some(VirtualReasonDesc::RTypeAlias(_))) {
                r.replace_desc(None)
            } else {
                r
            }
        },
        body_type,
    );
    // Use compact options: disable ref body collection to avoid recursion,
    // use shallow depth
    let mut compact_options = genv.options.dupe();
    compact_options.max_depth = Some(3);
    let compact_genv = Genv {
        typed_ast_opt: genv.typed_ast_opt,
        file_sig: genv.file_sig.dupe(),
        imported_names: genv.imported_names.dupe(),
        options: compact_options,
        ref_type_bodies: None,
        instantiation: None,
    };
    match ty_normalizer_flow::from_type(cx, &compact_genv, &stripped) {
        Err(_) => None,
        Ok(ty_elt) => summarize_ty_elt(&ty_elt),
    }
}

// Augment refs with type summaries from collected ref type bodies
fn augment_refs_with_summaries<'cx>(
    cx: &mut Context<'cx>,
    genv: &Genv<'_, 'cx>,
    ref_type_bodies_tbl: &Rc<RefCell<BTreeMap<String, Type>>>,
    refs: &Option<Vec<(String, Loc)>>,
) -> Option<Vec<(String, Loc, Option<String>)>> {
    refs.as_ref().map(|refs| {
        refs.iter()
            .map(|(name, loc)| {
                let body_type = ref_type_bodies_tbl.borrow().get(name).duped();
                let summary = match body_type {
                    Some(body_type) => compute_ref_summary(cx, genv, &body_type),
                    None => None,
                };
                (name.clone(), loc.dupe(), summary)
            })
            .collect()
    })
}

fn mk_normalizer_genv<'a, 'cx: 'a>(
    expand_component_props: bool,
    check_result: &'a FileArtifacts,
) -> (Genv<'a, 'cx>, Rc<RefCell<BTreeMap<String, Type>>>) {
    let (parse_artifacts, typecheck_artifacts) = check_result;
    let ParseArtifacts { file_sig, .. } = parse_artifacts.as_ref();
    let TypecheckArtifacts { typed_ast, .. } = typecheck_artifacts.as_ref();
    let options = flow_typing_ty_normalizer::env::Options {
        expand_internal_types: false,
        expand_enum_members: true,
        evaluate_type_destructors: EvaluateTypeDestructorsMode::EvaluateCustom(Rc::new(
            move |d: &Destructor| match d {
                Destructor::ReactCheckComponentConfig { .. } => expand_component_props,
                _ => false,
            },
        )),
        optimize_types: true,
        omit_targ_defaults_option: false,
        merge_bot_and_any_kinds: true,
        verbose_normalizer: false,
        max_depth: Some(10),
        max_type_size: None,
        toplevel_is_type_identifier_reference: false,
    };
    let ref_type_bodies_tbl = Rc::new(RefCell::new(BTreeMap::new()));
    let base_genv = ty_normalizer_flow::mk_genv(options, Some(typed_ast), file_sig.dupe());
    let genv = Genv {
        ref_type_bodies: Some(ref_type_bodies_tbl.dupe()),
        instantiation: None,
        ..base_genv
    };
    (genv, ref_type_bodies_tbl)
}

fn format_ty_elt_response<'cx>(
    cx: &mut Context<'cx>,
    reader: &Transaction,
    genv: &Genv<'_, 'cx>,
    ref_type_bodies_tbl: &Rc<RefCell<BTreeMap<String, Type>>>,
    loc: Loc,
    documentation: Option<String>,
    ast: &ast::Program<Loc, Loc>,
    actual_name: &str,
    source: export_index::Source,
    ty_elt: ALocElt,
) -> response::infer_type_of_name::T {
    let refs = ty::symbols_of_elt_without_decl_head(|aloc| reader.loc_of_aloc(aloc), &ty_elt);
    let opts = PrinterOptions::default();
    let (type_str, _instantiation, refs) = ty_printer::string_of_type_at_pos_result(
        ty_printer::TypeAtPosPrint {
            ty: &ty_elt,
            refs: Some(&refs),
            binder: None,
            alias: None,
        },
        &|aloc| reader.loc_of_aloc(aloc),
        &opts,
    );
    let refs = augment_refs_with_summaries(cx, genv, ref_type_bodies_tbl, &refs);
    let prop_docs = extract_prop_docs(reader, ast, &ty_elt);
    response::infer_type_of_name::T {
        loc,
        type_: type_str,
        refs,
        actual_name: actual_name.to_string(),
        documentation,
        prop_docs,
        source,
    }
}

fn type_of_name_from_artifacts(
    doc_at_loc: &dyn Fn(
        &mut CheckContentsCache,
        &Transaction,
        &FileArtifacts,
        &ast::Program<Loc, Loc>,
        &FileKey,
        i32,
        i32,
    ) -> Option<String>,
    cache: &mut CheckContentsCache,
    reader: &Transaction,
    check_result: &FileArtifacts,
    expand_component_props: bool,
    actual_name: &str,
    source: export_index::Source,
    aloc: &ALoc,
    type_: &Type,
) -> Result<response::infer_type_of_name::T, String> {
    let (parse_artifacts, typecheck_artifacts) = check_result;
    let ParseArtifacts { ast, .. } = parse_artifacts.as_ref();
    let loc = reader.loc_of_aloc(aloc);
    let documentation = {
        let file = typecheck_artifacts.cx.with_cx(cache, |cx| cx.file().dupe());
        let line = loc.start.line;
        let column = loc.start.column;
        doc_at_loc(cache, reader, check_result, ast, &file, line, column)
    };
    typecheck_artifacts.cx.with_cx(cache, |cx| {
        let (genv, ref_type_bodies_tbl) = mk_normalizer_genv(expand_component_props, check_result);
        match ty_normalizer_flow::from_type(cx, &genv, type_) {
            Ok(ty_elt) => Ok(format_ty_elt_response(
                cx,
                reader,
                &genv,
                &ref_type_bodies_tbl,
                loc,
                documentation,
                ast,
                actual_name,
                source,
                ty_elt,
            )),
            Err(e) => Err(format!("normalizer error {}", e)),
        }
    })
}

fn member_doc_from_def_locs(
    reader: &flow_heap::parsing_heaps::Transaction,
    ast: &flow_parser::ast::Program<Loc, Loc>,
    def_locs: &[ALoc],
) -> Option<String> {
    let get_ast_from_heap =
        |file_key: &flow_parser::file_key::FileKey| -> Option<flow_parser::ast::Program<Loc, Loc>> {
            reader.get_ast(file_key).map(|arc| (*arc).clone())
        };
    match def_locs {
        [aloc, ..] => {
            let loc = reader.loc_of_aloc(aloc);
            flow_services_autocomplete::find_documentation::jsdoc_of_getdef_loc(
                ast,
                &get_ast_from_heap,
                loc,
            )
            .and_then(|jsdoc| {
                flow_services_autocomplete::find_documentation::documentation_of_jsdoc(&jsdoc)
            })
        }
        [] => None,
    }
}

fn type_of_name_member(
    cache: &mut CheckContentsCache,
    reader: &Transaction,
    check_result: &FileArtifacts,
    full_name: &str,
    member_name: &str,
    source: export_index::Source,
    type_: &Type,
) -> Result<response::infer_type_of_name::T, String> {
    let (_, typecheck_artifacts) = check_result;
    typecheck_artifacts.cx.with_cx(cache, |cx| {
        type_of_name_member_in_cx(
            cx,
            reader,
            check_result,
            full_name,
            member_name,
            source,
            type_,
        )
    })
}

fn type_of_name_member_in_cx<'cx>(
    cx: &mut Context<'cx>,
    reader: &Transaction,
    check_result: &FileArtifacts,
    full_name: &str,
    member_name: &str,
    source: export_index::Source,
    type_: &Type,
) -> Result<response::infer_type_of_name::T, String> {
    let (parse_artifacts, typecheck_artifacts) = check_result;
    let ParseArtifacts { file_sig, ast, .. } = parse_artifacts.as_ref();
    let TypecheckArtifacts { typed_ast, .. } = typecheck_artifacts.as_ref();
    let (genv, ref_type_bodies_tbl) = mk_normalizer_genv(true, check_result);
    match ty_normalizer_flow::from_type(cx, &genv, type_) {
        Err(e) => Err(format!("normalizer error {}", e)),
        Ok(ty_elt) => {
            let format_member_ty = |cx: &mut Context<'cx>,
                                    member_ty: Arc<Ty<ALoc>>,
                                    def_locs: Vec<ALoc>|
             -> Result<response::infer_type_of_name::T, String> {
                let (loc, documentation) = match def_locs.as_slice() {
                    [aloc, ..] => {
                        let loc = reader.loc_of_aloc(aloc);
                        let documentation = member_doc_from_def_locs(reader, ast, &def_locs);
                        (loc, documentation)
                    }
                    [] => (Loc::none(), None),
                };
                Ok(format_ty_elt_response(
                    cx,
                    reader,
                    &genv,
                    &ref_type_bodies_tbl,
                    loc,
                    documentation,
                    ast,
                    full_name,
                    source.clone(),
                    Elt::Type(member_ty),
                ))
            };
            //   First try Ty-layer lookup (components, objects, type aliases)
            match lookup_member_in_elt(member_name, &ty_elt) {
                Some((member_ty, def_locs)) => format_member_ty(cx, member_ty, def_locs),
                None => {
                    // Fall back to Ty_members.extract for classes/interfaces
                    match ty_members::extract(
                        true,
                        Some(vec![reason::Name::new(member_name)]),
                        None,
                        cx,
                        Some(typed_ast),
                        file_sig.dupe(),
                        type_,
                    ) {
                        Ok(ty_members::TyMembers { members, .. }) => {
                            let key = reason::Name::new(member_name);
                            match members.get(&key) {
                                Some(member_info) => format_member_ty(
                                    cx,
                                    member_info.ty.dupe(),
                                    member_info.def_locs.clone(),
                                ),
                                None => Err(format!("member '{}' not found on type", member_name)),
                            }
                        }
                        Err(_) => Err(format!("member '{}' not found on type", member_name)),
                    }
                }
            }
        }
    }
}

fn get_server_exports(
    env: &flow_server_env::server_env::Env,
) -> Result<&flow_services_export::export_search::ExportSearch, String> {
    match &env.exports {
        Some(exports) => Ok(exports),
        None => Err(
            "No server exports found (make sure you're not using '--no-autoimport')".to_string(),
        ),
    }
}

fn find_first_match<'a>(
    exact_match_only: bool,
    target_name: &str,
    results: &'a [flow_services_export::export_search_types::SearchResultScored],
) -> Option<&'a flow_services_export::export_search_types::SearchResultScored> {
    results.iter().find(|scored| {
        let result = &scored.search_result;
        match &result.source {
            flow_services_export::export_index::Source::FileKey(_)
            | flow_services_export::export_index::Source::Global => {
                // NOTE the target name might change due to fuzzy finding
                !exact_match_only || result.name.as_str() == target_name
            }
            _ => false,
        }
    })
}

fn resolve_name_from_index(
    orchestrator: Option<&ServerOrchestratorHandle>,
    cache: &mut CheckContentsCache,
    options: &Options,
    reader: Arc<Transaction>,
    env: &flow_server_env::server_env::Env,
    exact_match_only: bool,
    target_name: &str,
    file_key: FileKey,
) -> Result<(String, export_index::Source, FileArtifacts, ALoc, Type), String> {
    let exports = get_server_exports(env)?;
    let mut search_options = export_search::default_options();
    search_options.max_results = 100;
    let num_cpus = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);
    search_options.num_threads = std::cmp::max(1, num_cpus.saturating_sub(2));
    search_options.weighted = true;
    let export_search_types::SearchResults {
        results,
        is_incomplete: _,
    } = export_search::search_both_values_and_types(Some(&search_options), target_name, exports);
    let (actual_name, source, kind) =
        match find_first_match(exact_match_only, target_name, &results) {
            Some(scored) => {
                let r = &scored.search_result;
                (r.name.dupe(), r.source.clone(), r.kind.clone())
            }
            None => return Err(format!("'{}' not found", target_name)),
        };
    // Create contents of the form
    //
    //   import NAME from FILE;
    //   import {NAME} from FILE;
    //   import type NAME from FILE;
    //   import type {NAME} from FILE;
    //
    // dependending on the kind of export that was selected. Note that this import
    // does not appear in the current file. This means that the necessary dependencies
    // might not have even been merged.
    let contents_body = match (&source, &kind) {
        (export_index::Source::FileKey(s), _) => {
            let thing = match kind {
                export_index::Kind::DefaultType => format!("type {}", actual_name),
                export_index::Kind::Default => actual_name.to_string(),
                export_index::Kind::Named => format!("{{{}}}", actual_name),
                export_index::Kind::NamedType => format!("type {{{}}}", actual_name),
                export_index::Kind::Namespace => actual_name.to_string(),
            };
            let specifier = s.to_absolute().replace('\\', "/");
            Ok(format!("import {} from '{}';", thing, specifier))
        }
        (export_index::Source::Global, export_index::Kind::DefaultType)
        | (export_index::Source::Global, export_index::Kind::NamedType) => {
            Ok(format!("declare var _: {};", actual_name))
        }
        (export_index::Source::Global, export_index::Kind::Default)
        | (export_index::Source::Global, export_index::Kind::Named)
        | (export_index::Source::Global, export_index::Kind::Namespace) => {
            Ok(format!("{};", actual_name))
        }
        (export_index::Source::Builtin(_), _) => Err("builtin lookup not supported".to_string()),
    }?;
    let contents = format!("/* @flow */ {}", contents_body);
    let parse_result =
        type_contents::parse_contents(options, env.configured_libs(), &contents, &file_key);
    match type_contents::type_parse_artifacts(
        orchestrator,
        cache,
        options,
        env.configured_libs(),
        reader.dupe(),
        env.master_cx.dupe(),
        file_key.dupe(),
        parse_result,
    ) {
        Err(_errors) => Err("Parse or typing errors on index".to_string()),
        Ok(check_result) => {
            let (_, typecheck_artifacts) = &check_result;
            let TypecheckArtifacts { typed_ast, .. } = typecheck_artifacts.as_ref();
            match find_identifier_and_type(actual_name.as_str(), typed_ast) {
                Some((aloc, type_)) => {
                    Ok((actual_name.to_string(), source, check_result, aloc, type_))
                }
                None => {
                    Err("Unexpected: no type found for identifier in phony program".to_string())
                }
            }
        }
    }
}

fn type_of_name_from_index(
    orchestrator: Option<&ServerOrchestratorHandle>,
    cache: &mut CheckContentsCache,
    doc_at_loc: &dyn Fn(
        &mut CheckContentsCache,
        &Transaction,
        &FileArtifacts,
        &ast::Program<Loc, Loc>,
        &FileKey,
        i32,
        i32,
    ) -> Option<String>,
    options: &Options,
    reader: Arc<Transaction>,
    env: &flow_server_env::server_env::Env,
    expand_component_props: bool,
    exact_match_only: bool,
    target_name: &str,
    file_key: FileKey,
) -> Result<response::infer_type_of_name::T, String> {
    let (actual_name, source, check_result, aloc, type_) = resolve_name_from_index(
        orchestrator,
        cache,
        options,
        reader.dupe(),
        env,
        exact_match_only,
        target_name,
        file_key,
    )?;
    let result = type_of_name_from_artifacts(
        doc_at_loc,
        cache,
        &reader,
        &check_result,
        expand_component_props,
        &actual_name,
        source,
        &aloc,
        &type_,
    );
    // Use def-loc of type in the response, but pass `loc` above to
    // `type_of_name_from_artifacts` so that we properly compute documentation
    let def_loc = reader.loc_of_aloc(type_util::def_loc_of_t(&type_));
    result.map(|mut r| {
        r.loc = def_loc;
        r
    })
}

fn type_of_name_single(
    orchestrator: Option<&ServerOrchestratorHandle>,
    cache: &mut CheckContentsCache,
    options: &Options,
    reader: Arc<Transaction>,
    env: &flow_server_env::server_env::Env,
    doc_at_loc: &dyn Fn(
        &mut CheckContentsCache,
        &Transaction,
        &FileArtifacts,
        &ast::Program<Loc, Loc>,
        &FileKey,
        i32,
        i32,
    ) -> Option<String>,
    expand_component_props: bool,
    exact_match_only: bool,
    target_name: &str,
    file_key: FileKey,
    check_result: &FileArtifacts,
) -> response::InferTypeOfNameResponse {
    let parts: Vec<&str> = target_name.split('.').collect();
    match parts.as_slice() {
        [] => Err("empty name".to_string()),
        [_] => {
            // No dot — existing logic
            let (_, typecheck_artifacts) = check_result;
            let TypecheckArtifacts { typed_ast, .. } = typecheck_artifacts.as_ref();
            match find_identifier_and_type(target_name, typed_ast) {
                Some((aloc, type_)) => type_of_name_from_artifacts(
                    doc_at_loc,
                    cache,
                    &reader,
                    check_result,
                    expand_component_props,
                    target_name,
                    export_index::Source::FileKey(file_key),
                    &aloc,
                    &type_,
                ),
                None => type_of_name_from_index(
                    orchestrator,
                    cache,
                    doc_at_loc,
                    options,
                    reader,
                    env,
                    expand_component_props,
                    exact_match_only,
                    target_name,
                    file_key,
                ),
            }
        }
        [base_name, member_parts @ ..] => {
            let member_name = member_parts.join(".");
            let full_name = target_name;
            let mut resolve_member_from_check_result =
                |base_check_result: &FileArtifacts,
                 base_source: export_index::Source,
                 type_: &Type| {
                    type_of_name_member(
                        cache,
                        &reader,
                        base_check_result,
                        full_name,
                        &member_name,
                        base_source,
                        type_,
                    )
                };
            let (_, typecheck_artifacts) = check_result;
            let TypecheckArtifacts { typed_ast, .. } = typecheck_artifacts.as_ref();
            match find_identifier_and_type(base_name, typed_ast) {
                Some((_aloc, type_)) => resolve_member_from_check_result(
                    check_result,
                    export_index::Source::FileKey(file_key),
                    &type_,
                ),
                None => {
                    let (actual_base_name, source, index_check_result, _aloc, type_) =
                        resolve_name_from_index(
                            orchestrator,
                            cache,
                            options,
                            reader.dupe(),
                            env,
                            exact_match_only,
                            base_name,
                            file_key,
                        )?;
                    let actual_full_name = format!("{}.{}", actual_base_name, member_name);
                    type_of_name_member(
                        cache,
                        &reader,
                        &index_check_result,
                        &actual_full_name,
                        &member_name,
                        source,
                        &type_,
                    )
                }
            }
        }
    }
}

pub fn type_of_name(
    orchestrator: Option<&ServerOrchestratorHandle>,
    cache: &mut CheckContentsCache,
    options: &Options,
    reader: Arc<Transaction>,
    env: &flow_server_env::server_env::Env,
    doc_at_loc: &dyn Fn(
        &mut CheckContentsCache,
        &Transaction,
        &FileArtifacts,
        &ast::Program<Loc, Loc>,
        &FileKey,
        i32,
        i32,
    ) -> Option<String>,
    file_key: FileKey,
    input: &type_of_name_options::T,
    check_result: &FileArtifacts,
) -> Vec<response::InferTypeOfNameResponse> {
    let type_of_name_options::T {
        input: _file_input,
        names,
        verbose: _,
        expand_component_props,
        exact_match_only,
        wait_for_recheck: _,
        strip_root: _,
    } = input;
    names
        .iter()
        .map(|target_name| {
            type_of_name_single(
                orchestrator,
                cache,
                options,
                reader.dupe(),
                env,
                doc_at_loc,
                *expand_component_props,
                *exact_match_only,
                target_name,
                file_key.dupe(),
                check_result,
            )
        })
        .collect()
}
