/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::collections::BTreeSet;
use std::collections::HashSet;
use std::collections::VecDeque;
use std::path::Path;
use std::sync::Arc;

use dupe::Dupe;
use flow_aloc::ALoc;
use flow_analysis::scope_builder;
use flow_common::flow_import_specifier::Userland;
use flow_common::options::ReactRuntime;
use flow_common::reason::Name;
use flow_common::reason::VirtualReasonDesc;
use flow_common_errors::error_codes::ErrorCode;
use flow_common_errors::error_utils;
use flow_common_ty::ty;
use flow_common_ty::ty::Decl;
use flow_common_ty::ty::DeclEnumDeclData;
use flow_common_ty::ty::DeclModuleDeclData;
use flow_common_ty::ty::DeclNamespaceDeclData;
use flow_common_ty::ty::DeclNominalComponentDeclData;
use flow_common_ty::ty::DeclTypeAliasDeclData;
use flow_common_ty::ty::Elt;
use flow_common_ty::ty::Ty;
use flow_common_ty::ty_printer;
use flow_common_ty::ty_utils;
use flow_env_builder::env_api::DefLocType;
use flow_lsp::lsp::lsp_position_to_flow_position;
use flow_parser::ast;
use flow_parser::ast_visitor::AstVisitor;
use flow_parser::file_key::FileKey;
use flow_parser::js_id_unicode::string_is_valid_identifier_name;
use flow_parser::loc::Loc;
use flow_parser_utils::ast_builder;
use flow_parser_utils::file_sig::FileSig;
use flow_parser_utils::file_sig::RequireBindings;
use flow_parser_utils_output::js_layout_generator;
use flow_parser_utils_output::pretty_printer;
use flow_services_export::export_index;
use flow_services_export::export_search_types;
use flow_services_export::fuzzy_path;
use flow_services_get_def::get_def_js;
use flow_services_get_def::get_def_js::GetDefResult;
use flow_services_get_def::get_def_types;
use flow_typing::ty_members;
use flow_typing::ty_normalizer_flow;
use flow_typing_context::Context;
use flow_typing_errors::intermediate_error;
use flow_typing_flow_common::flow_js_utils;
use flow_typing_type::type_::DefTInner;
use flow_typing_type::type_::InstanceKind;
use flow_typing_type::type_::ModuleType;
use flow_typing_type::type_::Type;
use flow_typing_type::type_::TypeInner;
use flow_typing_type::type_::constraint::Constraints;
use flow_typing_type::type_::mixed_t;
use flow_typing_type::type_util;
use flow_typing_utils::type_env;
use flow_typing_utils::typed_ast_utils::AvailableAst;
use lsp_types::CompletionItemKind as LspCompletionItemKind;
use lsp_types::CompletionItemTag as LspCompletionItemTag;
use lsp_types::InsertTextFormat as LspInsertTextFormat;
use lsp_types::TextEdit as LspTextEdit;

use crate::autocomplete_js;
use crate::autocomplete_sigil;
use crate::find_documentation;
use crate::find_method;
use crate::insert_jsdoc;
use crate::keywords;
use crate::lsp_import_edits;
use crate::module_system_info::LspModuleSystemInfo;

pub type DocumentationAndTagsValue = (Option<String>, Option<Vec<lsp_types::CompletionItemTag>>);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DocumentationAndTags {
    Empty,
    Value(DocumentationAndTagsValue),
    DefLoc(ALoc),
    Loc(Loc),
}

impl DocumentationAndTags {
    pub fn into_value(self) -> DocumentationAndTagsValue {
        match self {
            DocumentationAndTags::Empty => (None, None),
            DocumentationAndTags::Value(documentation_and_tags) => documentation_and_tags,
            DocumentationAndTags::DefLoc(_) | DocumentationAndTags::Loc(_) => (None, None),
        }
    }
}

pub mod ac_completion {
    use flow_parser::loc::Loc;

    use super::DocumentationAndTags;
    use super::LspCompletionItemKind;
    use super::LspInsertTextFormat;

    #[derive(Debug, Clone, PartialEq, Eq)]
    #[allow(non_snake_case)]
    pub struct InsertReplaceEdit {
        pub newText: String,
        pub insert: Loc,
        pub replace: Loc,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    #[allow(non_snake_case)]
    pub struct CompletionItem {
        pub kind: Option<LspCompletionItemKind>,
        pub name: String,
        pub labelDetail: Option<String>,
        pub description: Option<String>,
        pub itemDetail: Option<String>,
        pub text_edit: Option<InsertReplaceEdit>,
        pub additional_text_edits: Vec<(Loc, String)>,
        pub sort_text: Option<String>,
        pub preselect: bool,
        pub documentation_and_tags: DocumentationAndTags,
        pub log_info: String,
        pub insert_text_format: LspInsertTextFormat,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct T {
        pub items: Vec<CompletionItem>,
        pub is_incomplete: bool,
    }

    pub fn empty_documentation_and_tags() -> DocumentationAndTags {
        DocumentationAndTags::Empty
    }

    pub fn of_keyword(edit_locs: &(Loc, Loc), keyword: &str) -> CompletionItem {
        let (insert, replace) = edit_locs;
        CompletionItem {
            kind: Some(LspCompletionItemKind::KEYWORD),
            name: keyword.to_string(),
            labelDetail: None,
            description: None,
            itemDetail: None,
            text_edit: Some(InsertReplaceEdit {
                newText: keyword.to_string(),
                insert: insert.clone(),
                replace: replace.clone(),
            }),
            additional_text_edits: Vec::new(),
            sort_text: Some(format!("{:020}", 0)),
            preselect: false,
            documentation_and_tags: empty_documentation_and_tags(),
            log_info: "keyword".to_string(),
            insert_text_format: LspInsertTextFormat::PLAIN_TEXT,
        }
    }
}

pub struct AcOptions {
    pub imports: bool,
    pub imports_min_characters: i32,
    pub imports_ranked_usage: bool,
    pub imports_ranked_usage_boost_exact_match_min_length: usize,
    pub show_ranking_info: bool,
}

fn printer_options(prefer_single_quotes: bool) -> ty_printer::PrinterOptions {
    ty_printer::PrinterOptions {
        prefer_single_quotes,
        ..Default::default()
    }
}

fn lsp_completion_of_type(ty_: &Ty<ALoc>) -> LspCompletionItemKind {
    match ty_ {
        Ty::InlineInterface(_) => LspCompletionItemKind::INTERFACE,
        Ty::StrLit(_) | Ty::NumLit(_) | Ty::BigIntLit(_) | Ty::BoolLit(_) => {
            LspCompletionItemKind::VALUE
        }
        Ty::Fun(_) => LspCompletionItemKind::FUNCTION,
        Ty::Union(_, _, _, _) => LspCompletionItemKind::ENUM,
        Ty::Tup {
            elements: _,
            inexact: _,
        }
        | Ty::Bot(_)
        | Ty::Null
        | Ty::Obj(_)
        | Ty::Inter(..)
        | Ty::Bound(_)
        | Ty::Generic(_)
        | Ty::Truncated
        | Ty::Any(_)
        | Ty::Top
        | Ty::Void
        | Ty::Symbol
        | Ty::UniqueSymbol
        | Ty::Num
        | Ty::Str
        | Ty::Bool
        | Ty::BigInt
        | Ty::Arr(_)
        | Ty::TypeOf(..)
        | Ty::Utility(_)
        | Ty::IndexedAccess {
            _object: _,
            index: _,
            optional: _,
        }
        | Ty::Conditional {
            check_type: _,
            extends_type: _,
            true_type: _,
            false_type: _,
        }
        | Ty::Component {
            regular_props: _,
            renders: _,
        }
        | Ty::Infer(..)
        | Ty::Renders(..)
        | Ty::TemplateLiteral { .. } => LspCompletionItemKind::VARIABLE,
    }
}

fn lsp_completion_of_decl(decl: &Decl<ALoc>) -> LspCompletionItemKind {
    match decl {
        Decl::VariableDecl(box (_, ty_)) => lsp_completion_of_type(ty_),
        Decl::TypeAliasDecl(box DeclTypeAliasDeclData { type_, .. }) => match type_ {
            Some(type_) => lsp_completion_of_type(type_),
            None => LspCompletionItemKind::ENUM,
        },
        Decl::ClassDecl(..) => LspCompletionItemKind::CLASS,
        Decl::InterfaceDecl(..) => LspCompletionItemKind::INTERFACE,
        Decl::RecordDecl(..) => LspCompletionItemKind::STRUCT,
        Decl::EnumDecl(..) => LspCompletionItemKind::ENUM,
        Decl::NominalComponentDecl(..) => LspCompletionItemKind::VARIABLE,
        Decl::NamespaceDecl(..) | Decl::ModuleDecl(..) => LspCompletionItemKind::MODULE,
    }
}

fn sort_text_of_rank(rank: usize) -> Option<String> {
    Some(format!("{rank:020}"))
}

fn text_edit(
    insert_text: Option<&str>,
    name: &str,
    edit_locs: &(Loc, Loc),
) -> ac_completion::InsertReplaceEdit {
    let (insert, replace) = edit_locs;
    ac_completion::InsertReplaceEdit {
        newText: insert_text.unwrap_or(name).to_string(),
        insert: insert.clone(),
        replace: replace.clone(),
    }
}

fn detail_of_ty(optional: bool, ty_: &Ty<ALoc>) -> (Option<String>, Option<String>) {
    let type_ = ty_printer::string_of_t_single_line(ty_, &printer_options(false));
    let detail = format!("{}: {}", if optional { "?" } else { "" }, type_);
    (Some(type_), Some(detail))
}

fn detail_of_ty_decl(decl: &Decl<ALoc>) -> (Option<String>, Option<String>) {
    let type_ = ty_printer::string_of_decl_single_line(
        decl,
        &ty_printer::PrinterOptions {
            prefer_single_quotes: false,
            ..Default::default()
        },
    );
    let detail = match decl {
        Decl::ClassDecl(..)
        | Decl::EnumDecl(..)
        | Decl::InterfaceDecl(..)
        | Decl::RecordDecl(..)
        | Decl::NominalComponentDecl(..)
        | Decl::NamespaceDecl(..)
        | Decl::ModuleDecl(..)
        | Decl::TypeAliasDecl(..) => None,
        Decl::VariableDecl(box (_, ty_)) => detail_of_ty(false, ty_).1,
    };
    (Some(type_), detail)
}

fn autocomplete_create_result(
    insert_text: Option<&str>,
    rank: usize,
    preselect: bool,
    snippet: bool,
    documentation_and_tags: DocumentationAndTags,
    log_info: &str,
    name: &str,
    edit_locs: &(Loc, Loc),
    optional: bool,
    ty_: &Ty<ALoc>,
) -> ac_completion::CompletionItem {
    let (item_detail, label_detail) = detail_of_ty(optional, ty_);
    ac_completion::CompletionItem {
        kind: Some(lsp_completion_of_type(ty_)),
        name: name.to_string(),
        labelDetail: label_detail,
        description: None,
        itemDetail: item_detail,
        text_edit: Some(text_edit(insert_text, name, edit_locs)),
        additional_text_edits: Vec::new(),
        sort_text: sort_text_of_rank(rank),
        preselect,
        documentation_and_tags,
        log_info: log_info.to_string(),
        insert_text_format: if snippet {
            LspInsertTextFormat::SNIPPET
        } else {
            LspInsertTextFormat::PLAIN_TEXT
        },
    }
}

fn autocomplete_create_result_decl(
    insert_text: Option<&str>,
    rank: usize,
    preselect: bool,
    snippet: bool,
    documentation_and_tags: DocumentationAndTags,
    log_info: &str,
    name: &str,
    edit_locs: &(Loc, Loc),
    decl: &Decl<ALoc>,
) -> ac_completion::CompletionItem {
    let kind = Some(lsp_completion_of_decl(decl));
    let (item_detail, label_detail) = detail_of_ty_decl(decl);
    ac_completion::CompletionItem {
        kind,
        name: name.to_string(),
        labelDetail: label_detail,
        description: None,
        itemDetail: item_detail,
        text_edit: Some(text_edit(insert_text, name, edit_locs)),
        additional_text_edits: Vec::new(),
        sort_text: sort_text_of_rank(rank),
        preselect,
        documentation_and_tags,
        log_info: log_info.to_string(),
        insert_text_format: if snippet {
            LspInsertTextFormat::SNIPPET
        } else {
            LspInsertTextFormat::PLAIN_TEXT
        },
    }
}

fn autocomplete_create_result_elt(
    insert_text: Option<&str>,
    rank: usize,
    preselect: bool,
    documentation_and_tags: DocumentationAndTags,
    log_info: &str,
    name: &str,
    edit_locs: &(Loc, Loc),
    elt: &Elt<ALoc>,
) -> ac_completion::CompletionItem {
    match elt {
        Elt::Type(ty_) => autocomplete_create_result(
            insert_text,
            rank,
            preselect,
            false,
            documentation_and_tags,
            log_info,
            name,
            edit_locs,
            false,
            ty_,
        ),
        Elt::Decl(decl) => autocomplete_create_result_decl(
            insert_text,
            rank,
            preselect,
            false,
            documentation_and_tags,
            log_info,
            name,
            edit_locs,
            decl,
        ),
    }
}

fn ty_normalizer_options() -> flow_typing_ty_normalizer::env::Options {
    flow_typing_ty_normalizer::env::Options {
        expand_internal_types: true,
        ..Default::default()
    }
}

pub struct Typing<'a, 'cx: 'a> {
    pub layout_options: &'a flow_parser_utils_output::js_layout_generator::Opts,
    pub loc_of_aloc: Box<dyn Fn(&ALoc) -> Loc + 'a>,
    pub get_ast_from_heap: Box<dyn Fn(&FileKey) -> Option<ast::Program<Loc, Loc>> + 'a>,
    pub module_system_info: &'a LspModuleSystemInfo,
    pub search_exported_values:
        Box<dyn Fn(&AcOptions, &str) -> export_search_types::SearchResults + 'a>,
    pub search_exported_types:
        Box<dyn Fn(&AcOptions, &str) -> export_search_types::SearchResults + 'a>,
    pub cx: &'a mut Context<'cx>,
    pub file_sig: Arc<FileSig>,
    pub ast: Arc<ast::Program<Loc, Loc>>,
    pub canonical: Option<&'a autocomplete_sigil::canonical::Token>,
}

impl<'a, 'cx: 'a> Typing<'a, 'cx> {
    fn norm_genv(&self) -> flow_typing_ty_normalizer::env::Genv<'a, 'cx> {
        ty_normalizer_flow::mk_genv(ty_normalizer_options(), None, self.file_sig.dupe())
    }
}

fn search_with_filtered_auto_import_results<'a>(
    cx: &mut Context<'_>,
    f: &'a dyn Fn(&AcOptions, &str) -> export_search_types::SearchResults,
) -> Box<dyn Fn(&AcOptions, &str) -> export_search_types::SearchResults + 'a> {
    let is_available_autoimport_result = lsp_import_edits::is_available_autoimport_result(cx);
    Box::new(move |ac_options: &AcOptions, name: &str| {
        let results = f(ac_options, name);
        let is_incomplete = results.is_incomplete;
        let results = results
            .results
            .into_iter()
            .filter(|result| {
                let export_search_types::SearchResultScored { search_result, .. } = result;
                is_available_autoimport_result(&search_result.name, &search_result.source)
            })
            .collect();
        export_search_types::SearchResults {
            results,
            is_incomplete,
        }
    })
}

pub fn mk_typing_artifacts<'a, 'cx: 'a>(
    layout_options: &'a flow_parser_utils_output::js_layout_generator::Opts,
    loc_of_aloc: Box<dyn Fn(&ALoc) -> Loc + 'a>,
    get_ast_from_heap: Box<dyn Fn(&FileKey) -> Option<ast::Program<Loc, Loc>> + 'a>,
    module_system_info: &'a LspModuleSystemInfo,
    search_exported_values: &'a dyn Fn(&AcOptions, &str) -> export_search_types::SearchResults,
    search_exported_types: &'a dyn Fn(&AcOptions, &str) -> export_search_types::SearchResults,
    cx: &'a mut Context<'cx>,
    file_sig: Arc<FileSig>,
    ast: Arc<ast::Program<Loc, Loc>>,
    canonical: Option<&'a autocomplete_sigil::canonical::Token>,
) -> Typing<'a, 'cx> {
    Typing {
        layout_options,
        loc_of_aloc,
        get_ast_from_heap,
        module_system_info,
        search_exported_values: search_with_filtered_auto_import_results(
            cx,
            search_exported_values,
        ),
        search_exported_types: search_with_filtered_auto_import_results(cx, search_exported_types),
        cx,
        file_sig,
        ast,
        canonical,
    }
}

pub struct AcResult<R> {
    pub result: R,
    pub errors_to_log: Vec<String>,
}

pub enum AutocompleteServiceResultGeneric<R> {
    AcResult(AcResult<R>),
    AcEmpty(String),
    AcFatalError(String),
}

pub type AutocompleteServiceResult = AutocompleteServiceResultGeneric<ac_completion::T>;

fn jsdoc_of_def_loc(
    typing: &mut Typing<'_, '_>,
    def_loc: &ALoc,
) -> Option<flow_parser::jsdoc::Jsdoc> {
    let def_loc = (typing.loc_of_aloc)(def_loc);
    find_documentation::jsdoc_of_getdef_loc(&typing.ast, &*typing.get_ast_from_heap, def_loc)
}

fn jsdoc_of_loc(
    typing: &mut Typing<'_, '_>,
    loc: &Loc,
) -> Result<Option<flow_parser::jsdoc::Jsdoc>, flow_utils_concurrency::job_error::JobError> {
    let res = get_def_js::get_def(
        &*typing.loc_of_aloc,
        typing.cx,
        &typing.file_sig,
        None,
        &typing.ast,
        AvailableAst::ALocAst(flow_aloc::loc_to_aloc_ast(&typing.ast)),
        &get_def_types::Purpose::JSDoc,
        loc,
    )?;
    Ok(match res {
        GetDefResult::Def(locs, _) | GetDefResult::Partial(locs, _, _) if locs.len() == 1 => {
            let getdef_loc = locs.into_iter().next().unwrap();
            find_documentation::jsdoc_of_getdef_loc(
                &typing.ast,
                &*typing.get_ast_from_heap,
                getdef_loc,
            )
        }
        _ => None,
    })
}

fn documentation_and_tags_of_jsdoc(jsdoc: &flow_parser::jsdoc::Jsdoc) -> DocumentationAndTags {
    let docs = find_documentation::documentation_of_jsdoc(jsdoc);
    let tags = jsdoc
        .deprecated()
        .map(|_| vec![LspCompletionItemTag::DEPRECATED]);
    DocumentationAndTags::Value((docs, tags))
}

fn documentation_and_tags_of_member(
    _typing: &mut Typing<'_, '_>,
    info: &ty_members::MemberInfo<Arc<Ty<ALoc>>>,
) -> DocumentationAndTags {
    match info.def_locs.as_slice() {
        [def_loc] => DocumentationAndTags::DefLoc(def_loc.dupe()),
        _ => ac_completion::empty_documentation_and_tags(),
    }
}

fn documentation_and_tags_of_loc(loc: &Loc) -> DocumentationAndTags {
    DocumentationAndTags::Loc(loc.clone())
}

fn force_documentation_and_tags_of_loc(
    typing: &mut Typing<'_, '_>,
    loc: &Loc,
) -> Result<DocumentationAndTags, flow_utils_concurrency::job_error::JobError> {
    Ok(jsdoc_of_loc(typing, loc)?
        .as_ref()
        .map(documentation_and_tags_of_jsdoc)
        .unwrap_or_else(ac_completion::empty_documentation_and_tags))
}

fn force_documentation_and_tags_of_def_loc(
    typing: &mut Typing<'_, '_>,
    def_loc: &ALoc,
) -> DocumentationAndTags {
    jsdoc_of_def_loc(typing, def_loc)
        .as_ref()
        .map(documentation_and_tags_of_jsdoc)
        .unwrap_or_else(ac_completion::empty_documentation_and_tags)
}

fn members_of_type(
    typing: &mut Typing<'_, '_>,
    exclude_proto_members: bool,
    force_instance: bool,
    include_interface_members: bool,
    exclude_keys: &BTreeSet<String>,
    t: &Type,
) -> Result<
    (
        Vec<(
            String,
            DocumentationAndTags,
            ty_members::MemberInfo<Arc<Ty<ALoc>>>,
        )>,
        Vec<String>,
    ),
    String,
> {
    let ty_members = ty_members::extract(
        force_instance,
        None::<Vec<Name>>,
        None,
        typing.cx,
        None,
        typing.file_sig.dupe(),
        t,
    )?;
    let mut members = Vec::new();
    for (name, info) in ty_members.members {
        if (exclude_proto_members && info.inherited)
            || (!include_interface_members && info.source == ty::PropSource::Interface)
            || info.source == ty::PropSource::PrimitiveProto("Object".into())
        {
            continue;
        }
        // A symbol-typed key has no identifier spelling to complete on, so skip
        // it rather than offer a completion nobody can type.
        let Name::Str(name) = name else {
            continue;
        };
        let name = name.as_str();
        if name == "constructor" || name.starts_with('$') || exclude_keys.contains(name) {
            continue;
        }
        let documentation_and_tags = documentation_and_tags_of_member(typing, &info);
        members.push((name.to_string(), documentation_and_tags, info));
    }
    let errors = if ty_members.errors.is_empty() {
        Vec::new()
    } else {
        let mut errors = vec!["members_of_type".to_string()];
        errors.extend(ty_members.errors);
        errors
    };
    Ok((members, errors))
}

pub enum AcIdType {
    AcIdTypeNormal,
    AcIdTypeRecord {
        record_type: Type,
        defaulted_props: BTreeSet<String>,
    },
}

fn extract_record_fields(
    typing: &mut Typing<'_, '_>,
    defaulted_props: &BTreeSet<String>,
    record_type: &Type,
) -> Result<Vec<String>, String> {
    match members_of_type(typing, true, true, false, &BTreeSet::new(), record_type) {
        Err(err) => Err(err),
        Ok((mems, _errors_to_log)) => Ok(mems
            .into_iter()
            .filter_map(|(name, _, info)| match info.ty.as_ref() {
                Ty::Fun(_) => None,
                _ if defaulted_props.contains(&name) => None,
                _ => Some(name),
            })
            .collect()),
    }
}

fn autocomplete_record(
    typing: &mut Typing<'_, '_>,
    edit_locs: &(Loc, Loc),
    documentation_and_tags: DocumentationAndTags,
    record_name: &str,
    record_type: &Type,
    defaulted_props: &BTreeSet<String>,
) -> Option<ac_completion::CompletionItem> {
    let genv = typing.norm_genv();
    match ty_normalizer_flow::from_type(typing.cx, &genv, record_type) {
        Err(_) => None,
        Ok(elt) => match elt {
            Elt::Decl(record_decl @ Decl::RecordDecl(..)) => {
                match extract_record_fields(typing, defaulted_props, record_type) {
                    Err(_) => None,
                    Ok(fields) => {
                        let field_list = fields
                            .into_iter()
                            .map(|name| format!("{name}: null"))
                            .collect::<Vec<_>>()
                            .join(", ");
                        let insert_text = format!("{record_name} {{{field_list}}}");
                        Some(autocomplete_create_result_decl(
                            Some(&insert_text),
                            0,
                            false,
                            false,
                            documentation_and_tags,
                            "record",
                            record_name,
                            edit_locs,
                            &record_decl,
                        ))
                    }
                }
            }
            _ => None,
        },
    }
}

fn local_value_identifiers(
    typing: &mut Typing<'_, '_>,
    ac_loc: &Loc,
) -> Result<
    Vec<(
        (String, DocumentationAndTags, AcIdType),
        Result<Elt<ALoc>, flow_typing_ty_normalizer::normalizer::Error>,
    )>,
    flow_utils_concurrency::job_error::JobError,
> {
    let scope_info = scope_builder::program(
        typing.cx.enable_enums(),
        false,
        typing.file_sig.requires().is_empty(),
        &typing.ast,
    );
    let ac_scope_id = scope_info.closest_enclosing_scope(ac_loc, flow_common::reason::in_range);
    let names_and_locs = scope_info.fold_scope_chain(
        ac_scope_id,
        std::collections::BTreeMap::<String, Loc>::new(),
        |_, scope, mut acc| {
            let relevant_scope_vars: std::collections::BTreeMap<String, Loc> = scope
                .defs
                .iter()
                .filter(|(_, def)| def.kind.allow_forward_ref() || def.locs.first() < ac_loc)
                .map(|(name, def)| (name.to_string(), def.locs.first().clone()))
                .collect();
            for (name, loc) in relevant_scope_vars {
                acc.entry(name).or_insert(loc);
            }
            acc
        },
    );

    fn is_record_type(cx: &mut Context, t: &Type) -> Option<BTreeSet<String>> {
        let t = match &**t {
            TypeInner::OpenT(tvar) => flow_js_utils::merge_tvar(
                cx,
                false,
                |_, _| t.dupe(),
                tvar.reason(),
                tvar.id() as i32,
            ),
            _ => t.dupe(),
        };
        match &*t {
            TypeInner::DefT(_, def_t) => match &**def_t {
                DefTInner::ClassT(this) => match &**this {
                    TypeInner::ThisInstanceT(data) => match &data.instance.inst.inst_kind {
                        InstanceKind::RecordKind { defaulted_props } => {
                            Some(defaulted_props.iter().map(|s| s.to_string()).collect())
                        }
                        _ => None,
                    },
                    TypeInner::DefT(_, inner_def_t) => match &**inner_def_t {
                        DefTInner::InstanceT(instance) => match &instance.inst.inst_kind {
                            InstanceKind::RecordKind { defaulted_props } => {
                                Some(defaulted_props.iter().map(|s| s.to_string()).collect())
                            }
                            _ => None,
                        },
                        _ => None,
                    },
                    _ => None,
                },
                _ => None,
            },
            _ => None,
        }
    }

    let mut identifiers: Vec<((String, DocumentationAndTags, AcIdType), Type)> = Vec::new();
    for (name, loc) in names_and_locs {
        let type_ = match type_env::checked_find_loc_env_write_opt(
            typing.cx,
            DefLocType::OrdinaryNameLoc,
            ALoc::of_loc(loc.clone()),
        ) {
            Some(t) => t,
            None => continue,
        };
        let documentation_and_tags = documentation_and_tags_of_loc(&loc);
        let ac_id_type = match is_record_type(typing.cx, &type_) {
            Some(defaulted_props) => AcIdType::AcIdTypeRecord {
                record_type: type_.dupe(),
                defaulted_props,
            },
            None => AcIdType::AcIdTypeNormal,
        };
        identifiers.push(((name, documentation_and_tags, ac_id_type), type_));
    }
    let genv = typing.norm_genv();
    Ok(ty_normalizer_flow::from_types(
        None,
        typing.cx,
        &genv,
        identifiers,
    ))
}

fn expected_concrete_type_of_t(cx: &mut Context, lb_type: &Type) -> Type {
    fn unwrap(cx: &mut Context, seen: &mut BTreeSet<u32>, t: &Type) -> Option<Type> {
        match &**t {
            TypeInner::OpenT(tvar) => {
                let id = tvar.id();
                if seen.contains(&id) {
                    None
                } else {
                    seen.insert(id);
                    let use_ = match cx.find_graph(id as i32) {
                        Constraints::Resolved(t) => Some(t),
                        Constraints::FullyResolved(s) => Some(cx.force_fully_resolved_tvar(&s)),
                        Constraints::Unresolved(_) => None,
                    };
                    use_.and_then(|t| unwrap(cx, seen, &t))
                }
            }
            _ => Some(t.dupe()),
        }
    }

    let mut seen = BTreeSet::new();
    match unwrap(cx, &mut seen, lb_type) {
        None => mixed_t::make(type_util::reason_of_t(lb_type).dupe()),
        Some(t) => t,
    }
}

fn literals_of_ty(ty_: &Arc<Ty<ALoc>>) -> Vec<Arc<Ty<ALoc>>> {
    fn rec(acc: &mut Vec<Arc<Ty<ALoc>>>, ty_: &Arc<Ty<ALoc>>) {
        match ty_.as_ref() {
            Ty::Union(_, t1, t2, ts) => {
                rec(acc, t1);
                rec(acc, t2);
                for t in ts.iter() {
                    rec(acc, t);
                }
            }
            Ty::StrLit(_) | Ty::NumLit(_) | Ty::BoolLit(_) | Ty::Null => {
                acc.push(ty_.dupe());
            }
            Ty::Bool => {
                acc.push(Arc::new(Ty::BoolLit(false)));
                acc.push(Arc::new(Ty::BoolLit(true)));
            }
            _ => {}
        }
    }
    let mut acc = Vec::new();
    rec(&mut acc, ty_);
    acc.reverse();
    acc
}

enum QuoteKind {
    Single,
    Double,
}

fn quote_kind(token: &str) -> Option<QuoteKind> {
    if token.is_empty() {
        None
    } else {
        match token.chars().next().unwrap() {
            '\'' => Some(QuoteKind::Single),
            '"' => Some(QuoteKind::Double),
            _ => None,
        }
    }
}

fn autocomplete_create_string_literal_edit_controls(
    prefer_single_quotes: bool,
    edit_locs: &(Loc, Loc),
    token: &str,
) -> (bool, (Loc, Loc)) {
    let prefer_single_quotes = match quote_kind(token) {
        Some(QuoteKind::Single) => true,
        Some(QuoteKind::Double) => false,
        None => prefer_single_quotes,
    };
    let edit_locs = match quote_kind(token) {
        Some(_) => (edit_locs.1.clone(), edit_locs.1.clone()),
        None => edit_locs.clone(),
    };
    (prefer_single_quotes, edit_locs)
}

fn autocomplete_literals<'cx>(
    prefer_single_quotes: bool,
    cx: &mut Context<'cx>,
    genv: &flow_typing_ty_normalizer::env::Genv<'_, 'cx>,
    edit_locs: &(Loc, Loc),
    expected_type: &Type,
    token: &str,
) -> Vec<ac_completion::CompletionItem> {
    let expected_type_ty = ty_normalizer_flow::expand_literal_union(cx, genv, expected_type)
        .unwrap_or_else(|_| Arc::new(Ty::Top));
    let literals = literals_of_ty(&expected_type_ty);
    let (prefer_single_quotes, edit_locs) =
        autocomplete_create_string_literal_edit_controls(prefer_single_quotes, edit_locs, token);
    literals
        .into_iter()
        .map(|ty_| {
            let name =
                ty_printer::string_of_t_single_line(&ty_, &printer_options(prefer_single_quotes));
            autocomplete_create_result(
                Some(&name),
                0,
                true,
                false,
                ac_completion::empty_documentation_and_tags(),
                "literal from upper bound",
                &name,
                &edit_locs,
                false,
                &ty_,
            )
        })
        .collect()
}

fn src_dir_of_loc(ac_loc: &Loc) -> Option<String> {
    ac_loc.source().and_then(|key| {
        Path::new(&key.to_absolute())
            .parent()
            .map(|p| p.to_string_lossy().into_owned())
    })
}

fn flow_text_edit_of_lsp_text_edit(source: Option<&FileKey>, edit: &LspTextEdit) -> (Loc, String) {
    (
        Loc {
            source: source.cloned(),
            start: lsp_position_to_flow_position(&edit.range.start),
            end: lsp_position_to_flow_position(&edit.range.end),
        },
        edit.new_text.clone(),
    )
}

fn completion_item_of_autoimport(
    typing: &mut Typing<'_, '_>,
    src_dir: Option<&str>,
    edit_locs: &(Loc, Loc),
    ranking_info: Option<String>,
    auto_import: &export_search_types::SearchResult,
    rank: usize,
) -> ac_completion::CompletionItem {
    match lsp_import_edits::text_edits_of_import(
        typing.layout_options,
        typing.module_system_info,
        src_dir,
        &typing.ast,
        &auto_import.kind,
        auto_import.name.as_str(),
        &auto_import.source,
    ) {
        None => {
            let item_detail = Some(match ranking_info {
                Some(ranking_info) => format!("(global)\n\n{ranking_info}"),
                None => "(global)".to_string(),
            });
            ac_completion::CompletionItem {
                kind: Some(LspCompletionItemKind::VARIABLE),
                name: auto_import.name.to_string(),
                labelDetail: None,
                description: None,
                itemDetail: item_detail,
                text_edit: Some(text_edit(None, auto_import.name.as_str(), edit_locs)),
                additional_text_edits: Vec::new(),
                sort_text: sort_text_of_rank(rank),
                preselect: false,
                documentation_and_tags: ac_completion::empty_documentation_and_tags(),
                log_info: "global".to_string(),
                insert_text_format: LspInsertTextFormat::PLAIN_TEXT,
            }
        }
        Some(code_action_text_edits) => {
            let item_detail = Some(match ranking_info {
                Some(ranking_info) => format!("{}\n\n{ranking_info}", code_action_text_edits.title),
                None => code_action_text_edits.title.clone(),
            });
            let additional_text_edits = code_action_text_edits
                .edits
                .iter()
                .map(|edit| flow_text_edit_of_lsp_text_edit(edit_locs.0.source(), edit))
                .collect();
            ac_completion::CompletionItem {
                kind: Some(LspCompletionItemKind::VARIABLE),
                name: auto_import.name.to_string(),
                labelDetail: None,
                description: Some(code_action_text_edits.from),
                itemDetail: item_detail,
                text_edit: Some(text_edit(None, auto_import.name.as_str(), edit_locs)),
                additional_text_edits,
                sort_text: sort_text_of_rank(rank),
                preselect: false,
                documentation_and_tags: ac_completion::empty_documentation_and_tags(),
                log_info: "autoimport".to_string(),
                insert_text_format: LspInsertTextFormat::PLAIN_TEXT,
            }
        }
    }
}

fn add_locals<T>(f: impl Fn(&T) -> &str, locals: &[T], set: &mut HashSet<String>) {
    for local in locals {
        set.insert(f(local).to_string());
    }
}

fn set_of_locals<T>(f: impl Fn(&T) -> &str, locals: &[T]) -> HashSet<String> {
    let mut set = HashSet::with_capacity(locals.len());
    add_locals(f, locals, &mut set);
    set
}

fn is_reserved_word(name: &str) -> bool {
    matches!(
        name,
        "break"
            | "case"
            | "catch"
            | "class"
            | "const"
            | "continue"
            | "debugger"
            | "default"
            | "delete"
            | "do"
            | "else"
            | "enum"
            | "export"
            | "extends"
            | "false"
            | "finally"
            | "for"
            | "function"
            | "if"
            | "import"
            | "in"
            | "instanceof"
            | "new"
            | "null"
            | "return"
            | "super"
            | "switch"
            | "this"
            | "throw"
            | "true"
            | "try"
            | "typeof"
            | "var"
            | "void"
            | "while"
            | "with"
    )
}

fn is_strict_reserved_word(name: &str) -> bool {
    matches!(
        name,
        "implements"
            | "interface"
            | "package"
            | "private"
            | "protected"
            | "public"
            | "static"
            | "yield"
    )
}

fn is_reserved_type_word(name: &str) -> bool {
    matches!(
        name,
        "any"
            | "bigint"
            | "bool"
            | "boolean"
            | "const"
            | "empty"
            | "extends"
            | "false"
            | "function"
            | "interface"
            | "keyof"
            | "mixed"
            | "never"
            | "null"
            | "number"
            | "readonly"
            | "static"
            | "string"
            | "symbol"
            | "true"
            | "typeof"
            | "undefined"
            | "unknown"
            | "void"
    )
}

fn is_reserved(name: &str, kind: &export_index::Kind) -> bool {
    if export_index::kind_is_value(kind) {
        is_reserved_word(name) || is_strict_reserved_word(name)
    } else {
        is_reserved_type_word(name)
    }
}

fn compare_completion_items(
    a: &ac_completion::CompletionItem,
    b: &ac_completion::CompletionItem,
) -> std::cmp::Ordering {
    let rank_compare = match (&a.sort_text, &b.sort_text) {
        (Some(a), Some(b)) => a.cmp(b),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    };
    if rank_compare == std::cmp::Ordering::Equal {
        a.name
            .to_ascii_lowercase()
            .cmp(&b.name.to_ascii_lowercase())
    } else {
        rank_compare
    }
}

fn filter_by_token_and_sort(
    token: &str,
    mut items: Vec<ac_completion::CompletionItem>,
) -> Vec<ac_completion::CompletionItem> {
    let (before, _after) = autocomplete_sigil::remove(token);
    if before.is_empty() {
        items.sort_by(compare_completion_items);
        items
    } else {
        let mut scored: Vec<(i32, ac_completion::CompletionItem)> = items
            .into_iter()
            .rev()
            .filter_map(|item| {
                fuzzy_path::fuzzy_score(true, false, before, &item.name).map(|score| (score, item))
            })
            .collect();
        scored.sort_by_key(|a| a.0);
        scored.into_iter().map(|(_, item)| item).rev().collect()
    }
}

fn filter_by_token_and_sort_rev(
    token: &str,
    items: Vec<ac_completion::CompletionItem>,
    penalize_auto_import: bool,
) -> Vec<ac_completion::CompletionItem> {
    let (before, _after) = autocomplete_sigil::remove(token);
    if before.is_empty() {
        let mut items = items;
        items.sort_by(|a, b| compare_completion_items(b, a));
        items
    } else {
        let mut scored: Vec<(i32, ac_completion::CompletionItem)> = items
            .into_iter()
            .rev()
            .filter_map(|item| {
                let boost_full_match = !(penalize_auto_import && item.log_info == "autoimport");
                fuzzy_path::fuzzy_score(boost_full_match, false, before, &item.name)
                    .map(|score| (score, item))
            })
            .collect();
        scored.sort_by_key(|b| std::cmp::Reverse(b.0));
        scored.into_iter().map(|(_, item)| item).rev().collect()
    }
}

fn append_completion_items_of_autoimports(
    typing: &mut Typing<'_, '_>,
    ac_loc: &Loc,
    locals: &HashSet<String>,
    edit_locs: &(Loc, Loc),
    locals_rank: usize,
    ac_options: &AcOptions,
    auto_imports: &[export_search_types::SearchResultScored],
    token: &str,
    base_items_rev: Vec<ac_completion::CompletionItem>,
) -> Vec<ac_completion::CompletionItem> {
    let src_dir = src_dir_of_loc(ac_loc);
    let (before, _after) = autocomplete_sigil::remove(token);
    let auto_imports_items_rev: VecDeque<ac_completion::CompletionItem> = auto_imports
        .iter()
        .enumerate()
        .fold(VecDeque::new(), |mut acc, (i, scored)| {
            let rank = if ac_options.imports_ranked_usage {
                200 + i
            } else {
                200
            };
            let auto_import = &scored.search_result;
            if is_reserved(auto_import.name.as_str(), &auto_import.kind)
                || locals.contains(auto_import.name.as_str())
            {
                return acc;
            }
            let ranking_info = if ac_options.show_ranking_info {
                Some(format!("Score: {}\nUses: {}", scored.score, scored.weight))
            } else {
                None
            };
            let item = completion_item_of_autoimport(
                typing,
                src_dir.as_deref(),
                edit_locs,
                ranking_info,
                auto_import,
                rank,
            );
            acc.push_front(item);
            acc
        });
    if ac_options.imports_ranked_usage {
        let (mut exact_match_auto_imports_rev, other_auto_imports_rev): (Vec<_>, Vec<_>) =
            auto_imports_items_rev.into_iter().partition(|item| {
                item.name == before
                    && item.name.len()
                        >= ac_options.imports_ranked_usage_boost_exact_match_min_length
            });
        for item in &mut exact_match_auto_imports_rev {
            item.sort_text = sort_text_of_rank(locals_rank);
        }
        let mut items_rev = exact_match_auto_imports_rev;
        items_rev.extend(base_items_rev);
        let filtered = filter_by_token_and_sort_rev(token, items_rev, true);
        let mut result = other_auto_imports_rev;
        result.extend(filtered);
        result
    } else {
        let mut merged: Vec<_> = auto_imports_items_rev.into_iter().collect();
        merged.extend(base_items_rev);
        filter_by_token_and_sort_rev(token, merged, false)
    }
}

fn autocomplete_id(
    typing: &mut Typing<'_, '_>,
    ac_loc: &Loc,
    include_keywords: bool,
    include_super: bool,
    include_this: bool,
    ac_options: &AcOptions,
    edit_locs: &(Loc, Loc),
    token: &str,
    type_: &Type,
) -> Result<AcResult<ac_completion::T>, flow_utils_concurrency::job_error::JobError> {
    let expected_type = expected_concrete_type_of_t(typing.cx, type_);
    let prefer_single_quotes = typing.layout_options.single_quotes;
    let genv = typing.norm_genv();
    let results = autocomplete_literals(
        prefer_single_quotes,
        typing.cx,
        &genv,
        edit_locs,
        &expected_type,
        token,
    );
    let rank = if results.is_empty() { 0 } else { 1 };
    let identifiers = local_value_identifiers(typing, ac_loc)?;
    let locals = set_of_locals(|((name, _, _), _)| name.as_str(), &identifiers);
    let (mut items_rev, errors_to_log): (VecDeque<ac_completion::CompletionItem>, Vec<String>) = {
        let init: VecDeque<_> = results.into_iter().collect();
        identifiers.into_iter().fold(
            (init, Vec::new()),
            |(mut items_rev, mut errors_to_log),
             ((name, documentation_and_tags, ac_id_type), elt_result)| {
                match elt_result {
                    Ok(elt) => {
                        match ac_id_type {
                            AcIdType::AcIdTypeRecord {
                                record_type,
                                defaulted_props,
                            } => {
                                if let Some(item) = autocomplete_record(
                                    typing,
                                    edit_locs,
                                    documentation_and_tags.clone(),
                                    &name,
                                    &record_type,
                                    &defaulted_props,
                                ) {
                                    items_rev.push_front(item);
                                }
                            }
                            AcIdType::AcIdTypeNormal => {}
                        }
                        let result = autocomplete_create_result_elt(
                            Some(&name),
                            rank,
                            false,
                            documentation_and_tags,
                            "local value identifier",
                            &name,
                            edit_locs,
                            &elt,
                        );
                        items_rev.push_front(result);
                    }
                    Err(err) => {
                        errors_to_log.push(err.to_string());
                    }
                }
                (items_rev, errors_to_log)
            },
        )
    };
    if include_this {
        items_rev.push_front(ac_completion::CompletionItem {
            kind: Some(LspCompletionItemKind::VARIABLE),
            name: "this".to_string(),
            labelDetail: None,
            description: Some("this".to_string()),
            itemDetail: Some("this".to_string()),
            text_edit: Some(text_edit(None, "this", edit_locs)),
            additional_text_edits: Vec::new(),
            sort_text: sort_text_of_rank(rank),
            preselect: false,
            documentation_and_tags: ac_completion::empty_documentation_and_tags(),
            log_info: "this".to_string(),
            insert_text_format: LspInsertTextFormat::PLAIN_TEXT,
        });
    }
    if include_super {
        items_rev.push_front(ac_completion::CompletionItem {
            kind: Some(LspCompletionItemKind::VARIABLE),
            name: "super".to_string(),
            labelDetail: None,
            description: Some("super".to_string()),
            itemDetail: Some("super".to_string()),
            text_edit: Some(text_edit(None, "super", edit_locs)),
            additional_text_edits: Vec::new(),
            sort_text: sort_text_of_rank(rank),
            preselect: false,
            documentation_and_tags: ac_completion::empty_documentation_and_tags(),
            log_info: "super".to_string(),
            insert_text_format: LspInsertTextFormat::PLAIN_TEXT,
        });
    }
    if include_keywords {
        let keywords = keywords::keywords_at_loc(
            typing.cx.component_syntax(),
            typing.cx.enable_pattern_matching(),
            typing.cx.enable_records(),
            &typing.ast,
            ac_loc,
        )
        .into_iter()
        .map(|keyword| ac_completion::of_keyword(edit_locs, keyword))
        .collect::<Vec<_>>();
        for keyword in keywords {
            items_rev.push_front(keyword);
        }
    }
    let (items_rev, is_incomplete, sorted) = if ac_options.imports {
        let (before, _after) = autocomplete_sigil::remove(token);
        if before.is_empty() || before.len() < ac_options.imports_min_characters as usize {
            (items_rev, true, false)
        } else {
            let auto_imports = (typing.search_exported_values)(ac_options, before);
            let items_rev = append_completion_items_of_autoimports(
                typing,
                ac_loc,
                &locals,
                edit_locs,
                rank,
                ac_options,
                &auto_imports.results,
                token,
                items_rev.into_iter().collect(),
            );
            (
                items_rev.into_iter().collect(),
                auto_imports.is_incomplete,
                true,
            )
        }
    } else {
        (items_rev, false, false)
    };
    let mut items_rev: Vec<_> = items_rev.into_iter().collect();
    if !sorted {
        items_rev = filter_by_token_and_sort_rev(token, items_rev, false);
    }
    Ok(AcResult {
        result: ac_completion::T {
            items: items_rev.into_iter().rev().collect(),
            is_incomplete,
        },
        errors_to_log,
    })
}

enum ModuleExportKind {
    Value,
    Type,
    Either,
}

fn exports_of_module_ty(
    edit_locs: &(Loc, Loc),
    documentation_and_tags_of_module_member: &dyn Fn(&ALoc) -> DocumentationAndTags,
    kind: ModuleExportKind,
    filter_name: Option<&dyn Fn(&str) -> bool>,
    decl: &Decl<ALoc>,
) -> Vec<ac_completion::CompletionItem> {
    let is_kind = |export_kind: ModuleExportKind| {
        matches!(kind, ModuleExportKind::Either)
            || matches!(export_kind, ModuleExportKind::Either)
            || std::mem::discriminant(&kind) == std::mem::discriminant(&export_kind)
    };
    let filter_name = |name: &str| match filter_name {
        Some(filter_name) => filter_name(name),
        None => true,
    };
    let is_ok =
        |export_kind: ModuleExportKind, name: &str| is_kind(export_kind) && filter_name(name);
    let exports = match decl {
        Decl::ModuleDecl(box DeclModuleDeclData { exports, .. })
        | Decl::NamespaceDecl(box DeclNamespaceDeclData { exports, .. }) => exports,
        _ => return Vec::new(),
    };
    let mut items = exports
        .iter()
        .filter_map(|decl| match decl {
            Decl::TypeAliasDecl(box DeclTypeAliasDeclData { name, .. })
                if is_ok(ModuleExportKind::Type, name.sym_name.as_str()) =>
            {
                let documentation_and_tags =
                    documentation_and_tags_of_module_member(&name.sym_def_loc);
                Some(autocomplete_create_result_decl(
                    None,
                    0,
                    false,
                    false,
                    documentation_and_tags,
                    "qualified type alias",
                    name.sym_name.as_str(),
                    edit_locs,
                    decl,
                ))
            }
            Decl::InterfaceDecl(box (name, _))
                if is_ok(ModuleExportKind::Type, name.sym_name.as_str()) =>
            {
                let documentation_and_tags =
                    documentation_and_tags_of_module_member(&name.sym_def_loc);
                Some(autocomplete_create_result_decl(
                    None,
                    0,
                    false,
                    false,
                    documentation_and_tags,
                    "qualified interface",
                    name.sym_name.as_str(),
                    edit_locs,
                    decl,
                ))
            }
            Decl::ClassDecl(box (name, _))
                if is_ok(ModuleExportKind::Either, name.sym_name.as_str()) =>
            {
                let documentation_and_tags =
                    documentation_and_tags_of_module_member(&name.sym_def_loc);
                Some(autocomplete_create_result_decl(
                    None,
                    0,
                    false,
                    false,
                    documentation_and_tags,
                    "qualified class",
                    name.sym_name.as_str(),
                    edit_locs,
                    decl,
                ))
            }
            Decl::RecordDecl(box (name, _))
                if is_ok(ModuleExportKind::Either, name.sym_name.as_str()) =>
            {
                let documentation_and_tags =
                    documentation_and_tags_of_module_member(&name.sym_def_loc);
                Some(autocomplete_create_result_decl(
                    None,
                    0,
                    false,
                    false,
                    documentation_and_tags,
                    "qualified record",
                    name.sym_name.as_str(),
                    edit_locs,
                    decl,
                ))
            }
            Decl::EnumDecl(box DeclEnumDeclData { name, .. })
                if is_ok(ModuleExportKind::Either, name.sym_name.as_str()) =>
            {
                let documentation_and_tags =
                    documentation_and_tags_of_module_member(&name.sym_def_loc);
                Some(autocomplete_create_result_decl(
                    None,
                    0,
                    false,
                    false,
                    documentation_and_tags,
                    "qualified enum",
                    name.sym_name.as_str(),
                    edit_locs,
                    decl,
                ))
            }
            // A symbol-named export has no identifier spelling to complete.
            Decl::VariableDecl(box (name, _))
                if let Some(name) = name.as_str_opt()
                    && is_ok(ModuleExportKind::Value, name) =>
            {
                Some(autocomplete_create_result_decl(
                    None,
                    0,
                    false,
                    false,
                    ac_completion::empty_documentation_and_tags(),
                    "qualified variable",
                    name,
                    edit_locs,
                    decl,
                ))
            }
            Decl::NominalComponentDecl(box DeclNominalComponentDeclData { name, .. })
                if is_ok(ModuleExportKind::Value, name.sym_name.as_str()) =>
            {
                let documentation_and_tags =
                    documentation_and_tags_of_module_member(&name.sym_def_loc);
                Some(autocomplete_create_result_decl(
                    None,
                    0,
                    false,
                    false,
                    documentation_and_tags,
                    "qualified component",
                    name.sym_name.as_str(),
                    edit_locs,
                    decl,
                ))
            }
            Decl::NamespaceDecl(box DeclNamespaceDeclData {
                name: Some(name), ..
            }) if is_ok(ModuleExportKind::Value, name.sym_name.as_str()) => {
                let documentation_and_tags =
                    documentation_and_tags_of_module_member(&name.sym_def_loc);
                Some(autocomplete_create_result_decl(
                    None,
                    0,
                    false,
                    false,
                    documentation_and_tags,
                    "qualified namespace",
                    name.sym_name.as_str(),
                    edit_locs,
                    decl,
                ))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    items.sort_by(|a, b| a.name.cmp(&b.name));
    for (i, item) in items.iter_mut().enumerate() {
        item.sort_text = sort_text_of_rank(i);
    }
    items
}

struct LocalTypeIdentifiersAstSearcher<'a, 'cx> {
    cx: &'a mut Context<'cx>,
    rev_ids: Vec<ast::Identifier<ALoc, (ALoc, Type)>>,
}

impl<'a, 'cx> LocalTypeIdentifiersAstSearcher<'a, 'cx> {
    fn new(cx: &'a mut Context<'cx>) -> Self {
        Self {
            cx,
            rev_ids: Vec::new(),
        }
    }

    fn add_id(&mut self, id: ast::Identifier<ALoc, (ALoc, Type)>) {
        self.rev_ids.push(id);
    }
}

impl<'ast> AstVisitor<'ast, ALoc, ALoc, &'ast ALoc, flow_utils_concurrency::job_error::JobError>
    for LocalTypeIdentifiersAstSearcher<'_, '_>
{
    fn normalize_loc(loc: &'ast ALoc) -> &'ast ALoc {
        loc
    }

    fn normalize_type(type_: &'ast ALoc) -> &'ast ALoc {
        type_
    }

    fn type_alias(
        &mut self,
        loc: &'ast ALoc,
        alias: &'ast ast::statement::TypeAlias<ALoc, ALoc>,
    ) -> Result<(), flow_utils_concurrency::job_error::JobError> {
        let (_, typed_alias) =
            flow_typing_statement::statement::type_alias(self.cx, loc.dupe(), alias)?;
        self.add_id(typed_alias.id);
        Ok(())
    }

    fn opaque_type(
        &mut self,
        loc: &'ast ALoc,
        otype: &'ast ast::statement::OpaqueType<ALoc, ALoc>,
    ) -> Result<(), flow_utils_concurrency::job_error::JobError> {
        let (_, typed_otype) =
            flow_typing_statement::statement::opaque_type(self.cx, loc.dupe(), otype)?;
        self.add_id(typed_otype.id);
        Ok(())
    }

    fn interface(
        &mut self,
        loc: &'ast ALoc,
        decl: &'ast ast::statement::Interface<ALoc, ALoc>,
    ) -> Result<(), flow_utils_concurrency::job_error::JobError> {
        let (_, typed_decl) =
            flow_typing_statement::statement::interface(self.cx, loc.dupe(), decl)?;
        self.add_id(typed_decl.id);
        Ok(())
    }

    fn import_declaration(
        &mut self,
        loc: &'ast ALoc,
        decl: &'ast ast::statement::ImportDeclaration<ALoc, ALoc>,
    ) -> Result<(), flow_utils_concurrency::job_error::JobError> {
        let typed_stmt = match flow_typing_statement::statement::statement(
            self.cx,
            &ast::statement::Statement::new(ast::statement::StatementInner::ImportDeclaration {
                loc: loc.dupe(),
                inner: decl.clone().into(),
            }),
        ) {
            Ok(s) => s,
            Err(flow_typing_utils::abnormal::CheckExprError::Canceled(c)) => {
                return Err(flow_utils_concurrency::job_error::JobError::Canceled(c));
            }
            Err(flow_typing_utils::abnormal::CheckExprError::TimedOut(t)) => {
                return Err(flow_utils_concurrency::job_error::JobError::TimedOut(t));
            }
            Err(flow_typing_utils::abnormal::CheckExprError::DebugThrow { loc }) => {
                return Err(flow_utils_concurrency::job_error::JobError::DebugThrow { loc });
            }
            Err(flow_typing_utils::abnormal::CheckExprError::Abnormal(_)) => {
                panic!("typed import declaration");
            }
        };
        let ast::statement::StatementInner::ImportDeclaration { inner, .. } = &*typed_stmt else {
            unreachable!()
        };
        let binds_type = |kind: ast::statement::ImportKind| {
            matches!(
                kind,
                ast::statement::ImportKind::ImportType | ast::statement::ImportKind::ImportTypeof
            )
        };
        let declaration_binds_type = binds_type(inner.import_kind);
        if declaration_binds_type && let Some(default) = &inner.default {
            self.add_id(default.identifier.clone());
        }
        if let Some(specifiers) = &inner.specifiers {
            match specifiers {
                ast::statement::import_declaration::Specifier::ImportNamedSpecifiers(
                    specifiers,
                ) => {
                    for specifier in specifiers {
                        let specifier_binds_type = specifier
                            .kind
                            .map(binds_type)
                            .unwrap_or(declaration_binds_type);
                        if specifier_binds_type {
                            self.add_id(
                                specifier
                                    .local
                                    .clone()
                                    .unwrap_or_else(|| specifier.remote.clone()),
                            );
                        }
                    }
                }
                ast::statement::import_declaration::Specifier::ImportNamespaceSpecifier(_) => {}
            }
        }
        Ok(())
    }
}

fn make_builtin_type(edit_locs: &(Loc, Loc), name: &str) -> ac_completion::CompletionItem {
    ac_completion::CompletionItem {
        kind: Some(LspCompletionItemKind::VARIABLE),
        name: name.to_string(),
        labelDetail: None,
        description: None,
        itemDetail: Some(name.to_string()),
        text_edit: Some(text_edit(None, name, edit_locs)),
        additional_text_edits: Vec::new(),
        sort_text: sort_text_of_rank(200),
        preselect: false,
        documentation_and_tags: ac_completion::empty_documentation_and_tags(),
        log_info: "builtin type".to_string(),
        insert_text_format: LspInsertTextFormat::PLAIN_TEXT,
    }
}

const BUILTIN_TYPES: &[&str] = &[
    "any", "boolean", "empty", "false", "unknown", "null", "number", "bigint", "string", "true",
    "void", "symbol",
];

fn make_builtin_type_operator(
    edit_locs: &(Loc, Loc),
    insert_text: Option<&str>,
    detail: Option<&str>,
    docs: Option<&str>,
    snippet: bool,
    name: &str,
) -> ac_completion::CompletionItem {
    ac_completion::CompletionItem {
        kind: Some(LspCompletionItemKind::KEYWORD),
        name: name.to_string(),
        labelDetail: detail.map(str::to_string),
        description: None,
        itemDetail: Some(name.to_string()),
        text_edit: Some(text_edit(insert_text, name, edit_locs)),
        additional_text_edits: Vec::new(),
        sort_text: sort_text_of_rank(100),
        preselect: false,
        documentation_and_tags: DocumentationAndTags::Value((docs.map(str::to_string), None)),
        log_info: "builtin type operators".to_string(),
        insert_text_format: if snippet {
            LspInsertTextFormat::SNIPPET
        } else {
            LspInsertTextFormat::PLAIN_TEXT
        },
    }
}

fn make_builtin_type_operators(edit_locs: &(Loc, Loc)) -> Vec<ac_completion::CompletionItem> {
    vec![
        make_builtin_type_operator(
            edit_locs,
            Some("component($1)"),
            None,
            Some("[component type](https://flow.org/en/docs/react/component-types/)"),
            true,
            "component",
        ),
        make_builtin_type_operator(
            edit_locs,
            Some("hook "),
            None,
            Some("[hook type](https://flow.org/en/docs/react/hook-syntax/#hook-type-annotations)"),
            false,
            "hook",
        ),
        make_builtin_type_operator(
            edit_locs,
            Some("renders "),
            None,
            Some(
                "`renders A` means that it will eventually render exactly one React element `A`. See https://flow.org/en/docs/react/render-types/ for more details.",
            ),
            false,
            "renders",
        ),
        make_builtin_type_operator(
            edit_locs,
            Some("renders? "),
            None,
            Some(
                "`renders? A` means that it will eventually render zero or one React element `A`. See https://flow.org/en/docs/react/render-types/ for more details.",
            ),
            false,
            "renders?",
        ),
        make_builtin_type_operator(
            edit_locs,
            Some("renders* "),
            None,
            Some(
                "`renders* A` means that it will eventually render any amount of `A`. See https://flow.org/en/docs/react/render-types/ for more details.",
            ),
            false,
            "renders*",
        ),
    ]
}

fn make_utility_type(edit_locs: &(Loc, Loc), name: &str) -> ac_completion::CompletionItem {
    ac_completion::CompletionItem {
        kind: Some(LspCompletionItemKind::FUNCTION),
        name: name.to_string(),
        labelDetail: None,
        description: None,
        itemDetail: Some(name.to_string()),
        text_edit: Some(text_edit(None, name, edit_locs)),
        additional_text_edits: Vec::new(),
        sort_text: sort_text_of_rank(300),
        preselect: false,
        documentation_and_tags: ac_completion::empty_documentation_and_tags(),
        log_info: "builtin type".to_string(),
        insert_text_format: LspInsertTextFormat::PLAIN_TEXT,
    }
}

const UTILITY_TYPES: &[&str] = &[
    "Class",
    "Partial",
    "Required",
    "$Exact",
    "$Exports",
    "$KeyMirror",
    "keyof",
    "NonNullable",
    "Readonly",
    "Values",
    "StringPrefix",
    "StringSuffix",
    "Uppercase",
    "Lowercase",
    "Capitalize",
    "Uncapitalize",
];

fn make_type_param(edit_locs: &(Loc, Loc), name: &str) -> ac_completion::CompletionItem {
    ac_completion::CompletionItem {
        kind: Some(LspCompletionItemKind::TYPE_PARAMETER),
        name: name.to_string(),
        labelDetail: None,
        description: None,
        itemDetail: Some(name.to_string()),
        text_edit: Some(text_edit(None, name, edit_locs)),
        additional_text_edits: Vec::new(),
        sort_text: sort_text_of_rank(0),
        preselect: false,
        documentation_and_tags: ac_completion::empty_documentation_and_tags(),
        log_info: "unqualified type parameter".to_string(),
        insert_text_format: LspInsertTextFormat::PLAIN_TEXT,
    }
}

fn local_type_identifiers(
    typing: &mut Typing<'_, '_>,
) -> Result<
    Vec<(
        (String, ALoc),
        Result<Elt<ALoc>, flow_typing_ty_normalizer::normalizer::Error>,
    )>,
    flow_utils_concurrency::job_error::JobError,
> {
    let mut search = LocalTypeIdentifiersAstSearcher::new(typing.cx);
    search.program(flow_aloc::loc_to_aloc_ast(&typing.ast))?;
    let ids = search
        .rev_ids
        .into_iter()
        .rev()
        .map(|id| ((id.name.to_string(), id.loc.0.dupe()), id.loc.1.dupe()))
        .collect();
    let genv = typing.norm_genv();
    Ok(ty_normalizer_flow::from_types(None, typing.cx, &genv, ids))
}

fn autocomplete_unqualified_type(
    typing: &mut Typing<'_, '_>,
    ac_options: &AcOptions,
    tparams_rev: &[String],
    ac_loc: &Loc,
    edit_locs: &(Loc, Loc),
    token: &str,
) -> Result<AcResult<ac_completion::T>, flow_utils_concurrency::job_error::JobError> {
    let mut items_rev: VecDeque<ac_completion::CompletionItem> = VecDeque::new();
    for name in BUILTIN_TYPES {
        items_rev.push_front(make_builtin_type(edit_locs, name));
    }
    for name in UTILITY_TYPES {
        items_rev.push_front(make_utility_type(edit_locs, name));
    }
    for op in make_builtin_type_operators(edit_locs) {
        items_rev.push_front(op);
    }
    for name in tparams_rev {
        items_rev.push_front(make_type_param(edit_locs, name));
    }
    let type_identifiers = local_type_identifiers(typing)?;
    let mut errors_to_log: Vec<String> = Vec::new();
    for ((name, aloc), ty_result) in type_identifiers {
        let documentation_and_tags = documentation_and_tags_of_loc(&(typing.loc_of_aloc)(&aloc));
        match ty_result {
            Ok(elt) => items_rev.push_front(autocomplete_create_result_elt(
                None,
                0,
                false,
                documentation_and_tags,
                "unqualified type: local type identifier",
                &name,
                edit_locs,
                &elt,
            )),
            Err(err) => errors_to_log.push(err.to_string()),
        }
    }
    let value_identifiers = local_value_identifiers(typing, ac_loc)?;
    let value_locals = set_of_locals(|((name, _, _), _)| name.as_str(), &value_identifiers);
    let type_locals = set_of_locals(
        |((name, _), _)| name.as_str(),
        &local_type_identifiers(typing)?,
    );
    for ((name, documentation_and_tags, _), ty_res) in value_identifiers {
        match ty_res {
            Err(err) => errors_to_log.push(err.to_string()),
            Ok(elt @ Elt::Decl(Decl::ClassDecl(_) | Decl::RecordDecl(_) | Decl::EnumDecl(_))) => {
                items_rev.push_front(autocomplete_create_result_elt(
                    None,
                    0,
                    false,
                    documentation_and_tags,
                    "unqualified type: class, record, enum",
                    &name,
                    edit_locs,
                    &elt,
                ));
            }
            Ok(elt @ Elt::Decl(Decl::NominalComponentDecl { .. })) => {
                items_rev.push_front(autocomplete_create_result_elt(
                    None,
                    0,
                    false,
                    documentation_and_tags,
                    "unqualified type: react element shorthand",
                    &name,
                    edit_locs,
                    &ty_utils::reinterpret_elt_as_type_identifier(elt),
                ));
            }
            Ok(ref elt @ Elt::Type(ref t))
                if matches!(
                    t.as_ref(),
                    Ty::Component {
                        renders: Some(renders),
                        ..
                    } if matches!(renders.as_ref(), Ty::Renders(..))
                ) =>
            {
                items_rev.push_front(autocomplete_create_result_elt(
                    None,
                    0,
                    false,
                    documentation_and_tags,
                    "unqualified type: react element shorthand",
                    &name,
                    edit_locs,
                    elt,
                ));
            }
            Ok(Elt::Decl(decl))
                if !exports_of_module_ty(
                    edit_locs,
                    &|_| ac_completion::empty_documentation_and_tags(),
                    ModuleExportKind::Type,
                    None,
                    &decl,
                )
                .is_empty() =>
            {
                let insert_text = format!("{name}.");
                items_rev.push_front(autocomplete_create_result_decl(
                    Some(&insert_text),
                    0,
                    false,
                    false,
                    documentation_and_tags,
                    "unqualified type -> qualified type",
                    &name,
                    edit_locs,
                    &decl,
                ));
            }
            Ok(_) => {}
        }
    }
    let (before, _after) = autocomplete_sigil::remove(token);
    let (items_rev, is_incomplete, sorted) =
        if before.len() < ac_options.imports_min_characters as usize {
            (items_rev, true, false)
        } else if ac_options.imports {
            let mut locals = type_locals;
            add_locals(
                |((name, _, _), _)| name.as_str(),
                &local_value_identifiers(typing, ac_loc)?,
                &mut locals,
            );
            let auto_imports = (typing.search_exported_types)(ac_options, before);
            let items_rev = append_completion_items_of_autoimports(
                typing,
                ac_loc,
                &locals,
                edit_locs,
                0,
                ac_options,
                &auto_imports.results,
                token,
                items_rev.into_iter().collect(),
            );
            (
                items_rev.into_iter().collect(),
                auto_imports.is_incomplete,
                true,
            )
        } else {
            let _value_locals = value_locals;
            (items_rev, false, false)
        };
    let mut items_rev: Vec<_> = items_rev.into_iter().collect();
    if !sorted {
        items_rev = filter_by_token_and_sort_rev(token, items_rev, false);
    }
    Ok(AcResult {
        result: ac_completion::T {
            items: items_rev.into_iter().rev().collect(),
            is_incomplete,
        },
        errors_to_log,
    })
}

fn fix_loc_of_string_token(token: &str, loc: &Loc) -> Loc {
    match quote_kind(token) {
        Some(_) => match autocomplete_sigil::remove_opt(token) {
            Some(("\"", "\"")) | Some(("'", "'")) => loc.clone(),
            _ => Loc {
                source: loc.source.dupe(),
                start: loc.start,
                end: loc.start,
            },
        },
        None => loc.clone(),
    }
}

fn fix_locs_of_string_token(token: &str, edit_locs: &(Loc, Loc)) -> (Loc, Loc) {
    (
        fix_loc_of_string_token(token, &edit_locs.0),
        fix_loc_of_string_token(token, &edit_locs.1),
    )
}

fn gen_layout_options(
    opts: &flow_parser_utils_output::js_layout_generator::Opts,
    token: &str,
) -> flow_parser_utils_output::js_layout_generator::Opts {
    let mut opts = opts.clone();
    match quote_kind(token) {
        Some(QuoteKind::Single) => opts.single_quotes = true,
        Some(QuoteKind::Double) => opts.single_quotes = false,
        None => {}
    }
    opts
}

fn print_name_as_indexer(
    layout_options: &flow_parser_utils_output::js_layout_generator::Opts,
    token: &str,
    name: &str,
) -> String {
    if let Some(symbol) = name.strip_prefix("@@") {
        format!("Symbol.{symbol}")
    } else {
        let opts = gen_layout_options(layout_options, token);
        js_layout_generator::quote_string(opts.single_quotes, name)
    }
}

fn print_name_as_indexer_with_edit_locs(
    layout_options: &flow_parser_utils_output::js_layout_generator::Opts,
    token: &str,
    edit_locs: &(Loc, Loc),
    name: &str,
) -> (String, (Loc, Loc)) {
    let (single_quotes, edit_locs) = autocomplete_create_string_literal_edit_controls(
        layout_options.single_quotes,
        edit_locs,
        token,
    );
    let mut layout_options = layout_options.clone();
    layout_options.single_quotes = single_quotes;
    (
        print_name_as_indexer(&layout_options, token, name),
        edit_locs,
    )
}

fn autocomplete_create_result_method(
    method_: &ast::class::Method<Loc, Loc>,
    layout_options: &flow_parser_utils_output::js_layout_generator::Opts,
    rank: usize,
    documentation_and_tags: DocumentationAndTags,
    log_info: &str,
    name: &str,
    edit_locs: &(Loc, Loc),
    ty_: &Arc<Ty<ALoc>>,
) -> ac_completion::CompletionItem {
    let method_ = ast_builder::classes::methods::with_docs(
        ast_builder::classes::methods::with_body(
            method_.clone(),
            ast_builder::functions::body(
                None,
                None,
                vec![ast_builder::statements::expression(
                    None,
                    None,
                    None,
                    ast_builder::expressions::identifier(None, None, autocomplete_sigil::SIGIL),
                )],
            ),
        ),
        None,
    );
    let insert_text = pretty_printer::print(
        true,
        &js_layout_generator::class_method(layout_options, &method_),
    )
    .contents()
    .to_string()
    .replacen(&format!("{};", autocomplete_sigil::SIGIL), "$0", 1);
    let label_detail = format!(
        "{}{{ … }}",
        pretty_printer::print(
            true,
            &js_layout_generator::function_params_and_return(layout_options, &method_.value.1),
        )
        .contents()
    );
    ac_completion::CompletionItem {
        kind: Some(lsp_completion_of_type(ty_)),
        name: name.to_string(),
        labelDetail: Some(label_detail),
        description: None,
        itemDetail: None,
        text_edit: Some(text_edit(Some(&insert_text), name, edit_locs)),
        additional_text_edits: Vec::new(),
        sort_text: sort_text_of_rank(rank),
        preselect: false,
        documentation_and_tags,
        log_info: log_info.to_string(),
        insert_text_format: LspInsertTextFormat::SNIPPET,
    }
}

fn autocomplete_member(
    typing: &mut Typing<'_, '_>,
    ac_options: &AcOptions,
    edit_locs: &(Loc, Loc),
    token: &str,
    this: &Type,
    in_optional_chain: bool,
    ac_loc: &Loc,
    tparams_rev: &[String],
    bracket_syntax: Option<autocomplete_js::BracketSyntax>,
    member_loc: Option<Loc>,
    is_type_annotation: bool,
    force_instance: bool,
) -> Result<AutocompleteServiceResult, flow_utils_concurrency::job_error::JobError> {
    let edit_locs = fix_locs_of_string_token(token, edit_locs);
    match members_of_type(typing, false, force_instance, false, &BTreeSet::new(), this) {
        Err(err) => Ok(AutocompleteServiceResultGeneric::AcFatalError(err)),
        Ok((mems, errors_to_log)) => {
            let items = mems
                .into_iter()
                .map(|(name, documentation_and_tags, info)| {
                    let rank = match info.source {
                        ty::PropSource::PrimitiveProto(_) => 1,
                        _ => 0,
                    };
                    let opt_chain_ty = ty_utils::simplify_type(
                        true,
                        None,
                        Arc::new(Ty::Union(
                            false,
                            Arc::new(Ty::Void),
                            info.ty.dupe(),
                            Arc::from([]),
                        )),
                    );
                    let name_as_indexer =
                        print_name_as_indexer(typing.layout_options, token, &name);
                    let name_is_valid_identifier = string_is_valid_identifier_name(&name);
                    let edit_loc_of_member_loc = |member_loc: &Loc| {
                        if member_loc.start.line == member_loc.end.line {
                            autocomplete_sigil::remove_from_loc(typing.canonical, member_loc)
                        } else {
                            Loc {
                                source: member_loc.source.dupe(),
                                start: member_loc.start,
                                end: member_loc.start,
                            }
                        }
                    };
                    match (
                        info.from_nullable,
                        in_optional_chain,
                        bracket_syntax.as_ref(),
                        member_loc.as_ref(),
                        name_is_valid_identifier,
                    ) {
                        (_, _, _, None, _)
                        | (false, _, None, _, true)
                        | (_, true, None, _, true) => {
                            let ty_ = if info.from_nullable && in_optional_chain {
                                &opt_chain_ty
                            } else {
                                &info.ty
                            };
                            autocomplete_create_result(
                                None,
                                rank,
                                false,
                                false,
                                documentation_and_tags,
                                "member",
                                &name,
                                &edit_locs,
                                info.optional,
                                ty_,
                            )
                        }
                        (false, _, Some(_), _, _) | (_, true, Some(_), _, _) => {
                            let (insert_text, edit_locs) = print_name_as_indexer_with_edit_locs(
                                typing.layout_options,
                                token,
                                &edit_locs,
                                &name,
                            );
                            autocomplete_create_result(
                                Some(&insert_text),
                                rank,
                                false,
                                false,
                                documentation_and_tags,
                                "bracket syntax member",
                                &insert_text,
                                &edit_locs,
                                info.optional,
                                &info.ty,
                            )
                        }
                        (false, _, None, Some(member_loc), false)
                        | (_, true, None, Some(member_loc), false) => {
                            let insert_text = format!("[{name_as_indexer}]");
                            let edit_loc = edit_loc_of_member_loc(member_loc);
                            autocomplete_create_result(
                                Some(&insert_text),
                                rank,
                                false,
                                false,
                                documentation_and_tags,
                                "dot-member switched to bracket-syntax member",
                                &insert_text,
                                &(edit_loc.clone(), edit_loc),
                                info.optional,
                                &info.ty,
                            )
                        }
                        (true, false, _, Some(member_loc), _) => {
                            let opt_chain_name =
                                match (bracket_syntax.as_ref(), name_is_valid_identifier) {
                                    (None, true) => format!("?.{name}"),
                                    _ => format!("?.[{name_as_indexer}]"),
                                };
                            let edit_loc =
                                autocomplete_sigil::remove_from_loc(typing.canonical, member_loc);
                            autocomplete_create_result(
                                Some(&opt_chain_name),
                                rank,
                                false,
                                false,
                                documentation_and_tags,
                                "start optional chain",
                                &opt_chain_name,
                                &(edit_loc.clone(), edit_loc),
                                info.optional,
                                &opt_chain_ty,
                            )
                        }
                    }
                })
                .collect::<Vec<_>>();
            match bracket_syntax {
                None => {
                    let items = filter_by_token_and_sort(token, items);
                    Ok(AutocompleteServiceResultGeneric::AcResult(AcResult {
                        result: ac_completion::T {
                            items,
                            is_incomplete: false,
                        },
                        errors_to_log,
                    }))
                }
                Some(bracket_syntax) => {
                    let id_result = if is_type_annotation {
                        autocomplete_unqualified_type(
                            typing,
                            ac_options,
                            tparams_rev,
                            ac_loc,
                            &edit_locs,
                            token,
                        )?
                    } else {
                        autocomplete_id(
                            typing,
                            ac_loc,
                            false,
                            bracket_syntax.include_super,
                            bracket_syntax.include_this,
                            ac_options,
                            &edit_locs,
                            token,
                            &bracket_syntax.type_,
                        )?
                    };
                    let ac_completion::T {
                        items: id_items,
                        is_incomplete,
                    } = id_result.result;
                    let mut rev_items: Vec<_> = id_items.into_iter().rev().collect();
                    rev_items.extend(items.into_iter().rev());
                    let items = filter_by_token_and_sort_rev(token, rev_items, false)
                        .into_iter()
                        .rev()
                        .collect();
                    Ok(AutocompleteServiceResultGeneric::AcResult(AcResult {
                        result: ac_completion::T {
                            items,
                            is_incomplete,
                        },
                        errors_to_log: [errors_to_log, id_result.errors_to_log].concat(),
                    }))
                }
            }
        }
    }
}

fn binds_react(bindings: &RequireBindings) -> bool {
    match bindings {
        RequireBindings::BindIdent(id) => id.name() == "React",
        RequireBindings::BindNamed(bindings) => {
            bindings.iter().any(|(_, local)| binds_react(local))
        }
    }
}

fn should_autoimport_react(cx: &mut Context, imports: bool, file_sig: &FileSig) -> bool {
    if !imports {
        return false;
    }
    match cx.react_runtime() {
        ReactRuntime::Automatic => false,
        ReactRuntime::Classic => {
            let requires_react = file_sig.requires().iter().any(|require| match require {
                flow_parser_utils::file_sig::Require::Require {
                    bindings: Some(bindings),
                    ..
                } => binds_react(bindings),
                flow_parser_utils::file_sig::Require::Import { ns: Some(ns), .. } => {
                    ns.name() == "React"
                }
                flow_parser_utils::file_sig::Require::Import { named, .. } => named
                    .values()
                    .any(|local| local.keys().any(|name| name.as_str() == "React")),
                _ => false,
            });
            !requires_react
        }
    }
}

fn autocomplete_jsx_intrinsic(
    typing: &mut Typing<'_, '_>,
    ac_loc: &Loc,
    edit_locs: &(Loc, Loc),
) -> Result<AcResult<ac_completion::T>, flow_utils_concurrency::job_error::JobError> {
    let reason = flow_common::reason::mk_reason(
        VirtualReasonDesc::RType("React.JSX.IntrinsicElements".into()),
        ALoc::of_loc(ac_loc.clone()),
    );
    let intrinsics_t = flow_typing_flow_js::flow_js::get_builtin_type_non_speculating(
        typing.cx,
        &reason,
        None,
        "React$JSXIntrinsicElements",
    )?;
    let (items, errors_to_log) =
        match members_of_type(typing, true, false, false, &BTreeSet::new(), &intrinsics_t) {
            Err(err) => (Vec::new(), vec![err]),
            Ok((mems, errors_to_log)) => (
                mems.into_iter()
                    .map(
                        |(name, documentation_and_tags, _)| ac_completion::CompletionItem {
                            kind: Some(LspCompletionItemKind::VARIABLE),
                            name: name.clone(),
                            labelDetail: None,
                            description: None,
                            itemDetail: Some("JSX Intrinsic".to_string()),
                            text_edit: Some(text_edit(Some(&name), &name, edit_locs)),
                            additional_text_edits: Vec::new(),
                            sort_text: sort_text_of_rank(400),
                            preselect: false,
                            documentation_and_tags,
                            log_info: "React.JSX.IntrinsicElements member".to_string(),
                            insert_text_format: LspInsertTextFormat::PLAIN_TEXT,
                        },
                    )
                    .collect(),
                errors_to_log,
            ),
        };
    Ok(AcResult {
        result: ac_completion::T {
            items,
            is_incomplete: false,
        },
        errors_to_log,
    })
}

fn autocomplete_jsx_element(
    typing: &mut Typing<'_, '_>,
    ac_loc: &Loc,
    ac_options: &AcOptions,
    edit_locs: &(Loc, Loc),
    token: &str,
    type_: &Type,
) -> Result<AutocompleteServiceResult, flow_utils_concurrency::job_error::JobError> {
    let results_id = autocomplete_id(
        typing, ac_loc, false, false, false, ac_options, edit_locs, token, type_,
    )?;
    let results_jsx = autocomplete_jsx_intrinsic(typing, ac_loc, edit_locs)?;
    let mut errors_to_log = results_id.errors_to_log.clone();
    errors_to_log.extend(results_jsx.errors_to_log.clone());
    let mut items = results_id.result.items.clone();
    items.extend(results_jsx.result.items.clone());
    let result = ac_completion::T {
        items: filter_by_token_and_sort(token, items),
        is_incomplete: results_id.result.is_incomplete || results_jsx.result.is_incomplete,
    };
    if should_autoimport_react(typing.cx, ac_options.imports, &typing.file_sig) {
        let import_edit = lsp_import_edits::text_edits_of_import(
            typing.layout_options,
            typing.module_system_info,
            src_dir_of_loc(ac_loc).as_deref(),
            &typing.ast,
            &export_index::Kind::Namespace,
            "React",
            &export_index::Source::Builtin(Userland::from_smol_str("react".into())),
        );
        match import_edit {
            None => Ok(AutocompleteServiceResultGeneric::AcResult(AcResult {
                result,
                errors_to_log,
            })),
            Some(import_edit) => {
                let edits = import_edit
                    .edits
                    .iter()
                    .map(|edit| flow_text_edit_of_lsp_text_edit(ac_loc.source(), edit))
                    .collect::<Vec<_>>();
                let items = result
                    .items
                    .into_iter()
                    .map(|mut item| {
                        item.additional_text_edits.extend(edits.clone());
                        item
                    })
                    .collect();
                Ok(AutocompleteServiceResultGeneric::AcResult(AcResult {
                    result: ac_completion::T {
                        items,
                        is_incomplete: result.is_incomplete,
                    },
                    errors_to_log,
                }))
            }
        }
    } else {
        Ok(AutocompleteServiceResultGeneric::AcResult(AcResult {
            result,
            errors_to_log,
        }))
    }
}

fn autocomplete_record_field(
    typing: &mut Typing<'_, '_>,
    used_field_names: &BTreeSet<String>,
    edit_locs: &(Loc, Loc),
    token: &str,
    record_t: &Type,
) -> AutocompleteServiceResult {
    match members_of_type(typing, true, true, false, used_field_names, record_t) {
        Err(err) => AutocompleteServiceResultGeneric::AcFatalError(err),
        Ok((mems, errors_to_log)) => {
            let items = mems
                .into_iter()
                .map(|(name, documentation_and_tags, info)| {
                    autocomplete_create_result(
                        Some(&name),
                        0,
                        false,
                        false,
                        documentation_and_tags,
                        "record field",
                        &name,
                        edit_locs,
                        info.optional,
                        &info.ty,
                    )
                })
                .collect();
            AutocompleteServiceResultGeneric::AcResult(AcResult {
                result: ac_completion::T {
                    items: filter_by_token_and_sort(token, items),
                    is_incomplete: false,
                },
                errors_to_log,
            })
        }
    }
}

fn autocomplete_jsx_attribute(
    typing: &mut Typing<'_, '_>,
    used_attr_names: &BTreeSet<String>,
    has_value: bool,
    edit_locs: &(Loc, Loc),
    token: &str,
    props: &Type,
) -> Result<AutocompleteServiceResult, flow_utils_concurrency::job_error::JobError> {
    let mut exclude_keys = used_attr_names.clone();
    exclude_keys.insert("children".to_string());
    match members_of_type(typing, true, false, false, &exclude_keys, props) {
        Err(err) => Ok(AutocompleteServiceResultGeneric::AcFatalError(err)),
        Ok((mems, errors_to_log)) => {
            let items = mems
                .into_iter()
                .map(|(name, documentation_and_tags, info)| {
                    let insert_text = if has_value {
                        name.clone()
                    } else {
                        format!("{name}=")
                    };
                    autocomplete_create_result(
                        Some(&insert_text),
                        0,
                        false,
                        false,
                        documentation_and_tags,
                        "jsx attribute",
                        &name,
                        edit_locs,
                        info.optional,
                        &info.ty,
                    )
                })
                .collect();
            Ok(AutocompleteServiceResultGeneric::AcResult(AcResult {
                result: ac_completion::T {
                    items: filter_by_token_and_sort(token, items),
                    is_incomplete: false,
                },
                errors_to_log,
            }))
        }
    }
}

enum ModuleTypeOrType {
    Module(ModuleType),
    Type(Type),
}

fn autocomplete_module_exports(
    typing: &mut Typing<'_, '_>,
    edit_locs: &(Loc, Loc),
    token: &str,
    kind: ModuleExportKind,
    filter_name: Option<&dyn Fn(&str) -> bool>,
    module_type_opt: Option<ModuleTypeOrType>,
) -> AutocompleteServiceResult {
    let documentation_and_tags_of_module_member =
        |def_loc: &ALoc| DocumentationAndTags::DefLoc(def_loc.dupe());
    let (items, errors_to_log) = match module_type_opt {
        None => (Vec::new(), Vec::new()),
        Some(ModuleTypeOrType::Module(module_type)) => {
            let genv = typing.norm_genv();
            match ty_normalizer_flow::from_module_type(typing.cx, &genv, &module_type) {
                Err(err) => (Vec::new(), vec![err.to_string()]),
                Ok(module_ty) => (
                    exports_of_module_ty(
                        edit_locs,
                        &documentation_and_tags_of_module_member,
                        kind,
                        filter_name,
                        &module_ty,
                    ),
                    Vec::new(),
                ),
            }
        }
        Some(ModuleTypeOrType::Type(t)) => {
            let genv = typing.norm_genv();
            match ty_normalizer_flow::from_type(typing.cx, &genv, &t) {
                Err(err) => (Vec::new(), vec![err.to_string()]),
                Ok(Elt::Type(_)) => (Vec::new(), Vec::new()),
                Ok(Elt::Decl(module_ty)) => (
                    exports_of_module_ty(
                        edit_locs,
                        &documentation_and_tags_of_module_member,
                        kind,
                        filter_name,
                        &module_ty,
                    ),
                    Vec::new(),
                ),
            }
        }
    };
    AutocompleteServiceResultGeneric::AcResult(AcResult {
        result: ac_completion::T {
            items: filter_by_token_and_sort(token, items),
            is_incomplete: false,
        },
        errors_to_log,
    })
}

fn unused_super_methods(
    typing: &mut Typing<'_, '_>,
    edit_locs: &(Loc, Loc),
    exclude_keys: &BTreeSet<String>,
    enclosing_class_t: &Type,
) -> Result<(Vec<ac_completion::CompletionItem>, Vec<String>), String> {
    let (mems, errors_to_log) =
        members_of_type(typing, false, true, true, exclude_keys, enclosing_class_t)?;
    let items = mems
        .into_iter()
        .filter_map(
            |(name, documentation_and_tags, info)| match info.ty.as_ref() {
                Ty::Fun(_) => info
                    .def_locs
                    .first()
                    .map(|loc| (*typing.loc_of_aloc)(loc))
                    .and_then(|loc| find_method::find(&*typing.get_ast_from_heap, &loc))
                    .map(|method_| {
                        autocomplete_create_result_method(
                            &method_,
                            typing.layout_options,
                            0,
                            documentation_and_tags,
                            "class key",
                            &name,
                            edit_locs,
                            &info.ty,
                        )
                    }),
                _ => None,
            },
        )
        .collect();
    Ok((items, errors_to_log))
}

fn autocomplete_class_key(
    typing: &mut Typing<'_, '_>,
    token: &str,
    edit_locs: &(Loc, Loc),
    enclosing_class_t: Option<Type>,
) -> AutocompleteServiceResult {
    match enclosing_class_t {
        Some(enclosing_class_t) => {
            let existing_members = members_of_type(
                typing,
                true,
                true,
                false,
                &BTreeSet::new(),
                &enclosing_class_t,
            );
            match existing_members.and_then(|(members, _)| {
                let exclude_keys = members
                    .into_iter()
                    .map(|(name, _, _)| name)
                    .collect::<BTreeSet<_>>();
                unused_super_methods(typing, edit_locs, &exclude_keys, &enclosing_class_t)
            }) {
                Err(err) => AutocompleteServiceResultGeneric::AcFatalError(err),
                Ok((items, errors_to_log)) => {
                    AutocompleteServiceResultGeneric::AcResult(AcResult {
                        result: ac_completion::T {
                            items: filter_by_token_and_sort(token, items),
                            is_incomplete: false,
                        },
                        errors_to_log,
                    })
                }
            }
        }
        None => AutocompleteServiceResultGeneric::AcResult(AcResult {
            result: ac_completion::T {
                items: Vec::new(),
                is_incomplete: false,
            },
            errors_to_log: Vec::new(),
        }),
    }
}

fn autocomplete_object_key(
    typing: &mut Typing<'_, '_>,
    edit_locs: &(Loc, Loc),
    token: &str,
    used_keys: &BTreeSet<String>,
    spreads: &[(Loc, Type)],
    obj_type: &Type,
) -> AutocompleteServiceResult {
    let edit_locs = fix_locs_of_string_token(token, edit_locs);
    let insert_loc = &edit_locs.0;
    let mut exclude_keys = used_keys.clone();
    for (spread_loc, spread_type) in spreads {
        if spread_loc < insert_loc {
            continue;
        }
        if let Ok((members, _)) =
            members_of_type(typing, true, false, false, &BTreeSet::new(), spread_type)
        {
            for (name, _, _) in members {
                exclude_keys.insert(name);
            }
        }
    }
    let expected_type = expected_concrete_type_of_t(typing.cx, obj_type);
    match unused_super_methods(typing, &edit_locs, &exclude_keys, &expected_type).and_then(
        |(methods, methods_errors_to_log)| {
            members_of_type(typing, true, false, false, &exclude_keys, &expected_type).map(
                |(mems, mems_errors_to_log)| {
                    let items = mems
                        .into_iter()
                        .map(|(name, documentation_and_tags, info)| {
                            if string_is_valid_identifier_name(&name) {
                                autocomplete_create_result(
                                    None,
                                    0,
                                    false,
                                    false,
                                    documentation_and_tags,
                                    "object key",
                                    &name,
                                    &edit_locs,
                                    info.optional,
                                    &info.ty,
                                )
                            } else {
                                let insert_text =
                                    print_name_as_indexer(typing.layout_options, token, &name);
                                autocomplete_create_result(
                                    Some(&insert_text),
                                    0,
                                    false,
                                    false,
                                    documentation_and_tags,
                                    "bracket syntax object key",
                                    &insert_text,
                                    &edit_locs,
                                    info.optional,
                                    &info.ty,
                                )
                            }
                        })
                        .chain(methods)
                        .collect();
                    (items, [methods_errors_to_log, mems_errors_to_log].concat())
                },
            )
        },
    ) {
        Err(err) => AutocompleteServiceResultGeneric::AcFatalError(err),
        Ok((items, errors_to_log)) => AutocompleteServiceResultGeneric::AcResult(AcResult {
            result: ac_completion::T {
                items: filter_by_token_and_sort(token, items),
                is_incomplete: false,
            },
            errors_to_log,
        }),
    }
}

fn applicable_error_codes(
    loc_of_aloc: &dyn Fn(&ALoc) -> Loc,
    cx: &mut Context,
    comment_loc: &ALoc,
) -> Vec<ErrorCode> {
    let comment_loc = loc_of_aloc(comment_loc);
    let next_line = Loc {
        source: comment_loc.source.dupe(),
        start: flow_parser::loc::Position {
            line: comment_loc.start.line + 1,
            column: comment_loc.start.column,
        },
        end: flow_parser::loc::Position {
            line: comment_loc.end.line + 1,
            column: comment_loc.end.column,
        },
    };
    let strip_root = Some(cx.root());
    let mut error_codes = Vec::new();
    for err in cx.errors().iter() {
        let intermediate =
            intermediate_error::make_intermediate_error(loc_of_aloc, false, err, |file| {
                cx.is_global_libdef(file)
            });
        let printable = intermediate_error::to_printable_error(
            loc_of_aloc,
            |_file_key: &FileKey| None,
            strip_root.map(|v| &**v),
            intermediate,
        );
        let loc = error_utils::loc_of_printable_error(&printable);
        if next_line.lines_intersect(&loc)
            && let Some(code) = error_utils::code_of_printable_error(&printable)
        {
            error_codes.push(code);
        }
    }
    error_codes.sort();
    error_codes.dedup();
    error_codes
}

fn has_leading(text: &str) -> bool {
    let (before, _) = autocomplete_sigil::remove(text);
    let before = before.trim_end_matches(|c: char| !c.is_whitespace());
    before.chars().any(|c| !c.is_whitespace())
}

fn autocomplete_fixme(
    loc_of_aloc: &dyn Fn(&ALoc) -> Loc,
    cx: &mut Context,
    edit_locs: &(Loc, Loc),
    token: &str,
    text: &str,
    loc: &ALoc,
) -> Vec<ac_completion::CompletionItem> {
    if has_leading(text) {
        Vec::new()
    } else {
        applicable_error_codes(loc_of_aloc, cx, loc)
            .into_iter()
            .map(|error_code| {
                let code_str = error_code.as_str();
                let name = format!("$FlowFixMe[{code_str}]");
                let insert_text = format!("\\$FlowFixMe[{code_str}] ${{1:reason for suppression}}");
                ac_completion::CompletionItem {
                    kind: Some(LspCompletionItemKind::TEXT),
                    name,
                    labelDetail: None,
                    description: None,
                    itemDetail: None,
                    text_edit: Some(text_edit(Some(&insert_text), &insert_text, edit_locs)),
                    additional_text_edits: Vec::new(),
                    sort_text: sort_text_of_rank(0),
                    preselect: false,
                    documentation_and_tags: ac_completion::empty_documentation_and_tags(),
                    log_info: "fixme comment".to_string(),
                    insert_text_format: LspInsertTextFormat::SNIPPET,
                }
            })
            .collect::<Vec<_>>()
            .pipe(|items| filter_by_token_and_sort(token, items))
    }
}

fn autocomplete_jsdoc(
    token: &str,
    ast: &ast::Program<Loc, Loc>,
    loc: &ALoc,
    canon: Option<&autocomplete_sigil::canonical::Token>,
) -> Vec<ac_completion::CompletionItem> {
    let loc = loc.to_loc_exn();
    let (before, _after) = autocomplete_sigil::remove(token);
    if before != "*" {
        Vec::new()
    } else {
        insert_jsdoc::insert_stub_in_comment(true, loc.clone(), ast)
            .map(|(_, stub)| {
                let name = "/** */".to_string();
                let loc = autocomplete_sigil::remove_from_loc(canon, loc);
                let insert_text = format!("/*{stub} */");
                vec![ac_completion::CompletionItem {
                    kind: Some(LspCompletionItemKind::TEXT),
                    name,
                    labelDetail: None,
                    description: None,
                    itemDetail: Some("JSDoc Comment".to_string()),
                    text_edit: Some(text_edit(Some(&insert_text), "/** */", &(loc.clone(), loc))),
                    additional_text_edits: Vec::new(),
                    sort_text: sort_text_of_rank(0),
                    preselect: false,
                    documentation_and_tags: ac_completion::empty_documentation_and_tags(),
                    log_info: "jsdoc".to_string(),
                    insert_text_format: LspInsertTextFormat::SNIPPET,
                }]
            })
            .unwrap_or_default()
            .pipe(|items| filter_by_token_and_sort(token, items))
    }
}

fn autocomplete_comment(
    typing: &mut Typing<'_, '_>,
    edit_locs: &(Loc, Loc),
    trigger_character: Option<&str>,
    token: &str,
    text: &str,
    loc: &ALoc,
) -> AutocompleteServiceResult {
    let items = match trigger_character {
        Some("*") | None => {
            let mut items =
                autocomplete_fixme(&*typing.loc_of_aloc, typing.cx, edit_locs, token, text, loc);
            items.extend(autocomplete_jsdoc(
                token,
                &typing.ast,
                loc,
                typing.canonical,
            ));
            items
        }
        _ => Vec::new(),
    };
    AutocompleteServiceResultGeneric::AcResult(AcResult {
        result: ac_completion::T {
            items,
            is_incomplete: false,
        },
        errors_to_log: Vec::new(),
    })
}

fn string_of_autocomplete_type(ac_type: &autocomplete_js::AutocompleteType) -> &'static str {
    match ac_type {
        autocomplete_js::AutocompleteType::AcIgnored => "Empty",
        autocomplete_js::AutocompleteType::AcBinding => "Empty",
        autocomplete_js::AutocompleteType::AcTypeBinding => "Empty",
        autocomplete_js::AutocompleteType::AcComment { .. } => "Ac_comment",
        autocomplete_js::AutocompleteType::AcEnum => "Acenum",
        autocomplete_js::AutocompleteType::AcModule => "Acmodule",
        autocomplete_js::AutocompleteType::AcType => "Actype",
        autocomplete_js::AutocompleteType::AcJsxText => "Empty",
        autocomplete_js::AutocompleteType::AcId { .. } => "Acid",
        autocomplete_js::AutocompleteType::AcClassKey { .. } => "Ac_class_key",
        autocomplete_js::AutocompleteType::AcImportSpecifier { .. } => "Ac_import_specifier",
        autocomplete_js::AutocompleteType::AcKey { .. } => "Ackey",
        autocomplete_js::AutocompleteType::AcLiteral { .. } => "Acliteral",
        autocomplete_js::AutocompleteType::AcQualifiedType(_) => "Acqualifiedtype",
        autocomplete_js::AutocompleteType::AcMember { .. } => "Acmem",
        autocomplete_js::AutocompleteType::AcJsxElement { .. } => "Ac_jsx_element",
        autocomplete_js::AutocompleteType::AcJsxAttribute { .. } => "Acjsx",
        autocomplete_js::AutocompleteType::AcRecordField { .. } => "Ac_record_field",
    }
}

trait Pipe: Sized {
    fn pipe<R>(self, f: impl FnOnce(Self) -> R) -> R {
        f(self)
    }
}

impl<T> Pipe for T {}

fn force_documentation_and_tags(
    typing: &mut Typing<'_, '_>,
    documentation_and_tags: DocumentationAndTags,
) -> Result<DocumentationAndTags, flow_utils_concurrency::job_error::JobError> {
    match documentation_and_tags {
        DocumentationAndTags::Empty => Ok(documentation_and_tags),
        DocumentationAndTags::Value(_) => Ok(documentation_and_tags),
        DocumentationAndTags::DefLoc(def_loc) => {
            Ok(force_documentation_and_tags_of_def_loc(typing, &def_loc))
        }
        DocumentationAndTags::Loc(loc) => force_documentation_and_tags_of_loc(typing, &loc),
    }
}

fn force_completion_item_documentation(
    typing: &mut Typing<'_, '_>,
    mut item: ac_completion::CompletionItem,
) -> Result<ac_completion::CompletionItem, flow_utils_concurrency::job_error::JobError> {
    item.documentation_and_tags =
        force_documentation_and_tags(typing, item.documentation_and_tags)?;
    Ok(item)
}

fn force_autocomplete_result_documentation(
    typing: &mut Typing<'_, '_>,
    result: AutocompleteServiceResult,
) -> Result<AutocompleteServiceResult, flow_utils_concurrency::job_error::JobError> {
    match result {
        AutocompleteServiceResultGeneric::AcResult(AcResult {
            result,
            errors_to_log,
        }) => {
            let ac_completion::T {
                items,
                is_incomplete,
            } = result;
            let items = items
                .into_iter()
                .map(|item| force_completion_item_documentation(typing, item))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(AutocompleteServiceResultGeneric::AcResult(AcResult {
                result: ac_completion::T {
                    items,
                    is_incomplete,
                },
                errors_to_log,
            }))
        }
        AutocompleteServiceResultGeneric::AcEmpty(reason) => {
            Ok(AutocompleteServiceResultGeneric::AcEmpty(reason))
        }
        AutocompleteServiceResultGeneric::AcFatalError(err) => {
            Ok(AutocompleteServiceResultGeneric::AcFatalError(err))
        }
    }
}

pub fn autocomplete_get_results(
    typing: &mut Typing<'_, '_>,
    ac_options: &AcOptions,
    trigger_character: Option<&str>,
    cursor: &Loc,
) -> Result<
    (
        Option<String>,
        Option<Loc>,
        String,
        AutocompleteServiceResult,
    ),
    flow_utils_concurrency::job_error::JobError,
> {
    let canon_cursor = typing.canonical.map_or_else(
        || cursor.clone(),
        |canon| autocomplete_sigil::canonical::cursor(canon).clone(),
    );
    match autocomplete_js::process_location(
        typing.cx,
        typing.file_sig.dupe(),
        trigger_character,
        &canon_cursor,
        flow_aloc::loc_to_aloc_ast(&typing.ast),
    )? {
        Err(err) => Ok((
            None,
            None,
            "None".to_string(),
            AutocompleteServiceResultGeneric::AcFatalError(err),
        )),
        Ok(None) => Ok((
            None,
            None,
            "None".to_string(),
            AutocompleteServiceResultGeneric::AcResult(AcResult {
                result: ac_completion::T {
                    items: Vec::new(),
                    is_incomplete: false,
                },
                errors_to_log: vec!["Autocomplete token not found in AST".to_string()],
            }),
        )),
        Ok(Some(processed)) => {
            let token = autocomplete_sigil::canonical::to_relative_token(
                typing.canonical,
                &processed.token,
            );
            let ac_loc = autocomplete_sigil::remove_from_loc(
                typing.canonical,
                &(typing.loc_of_aloc)(&processed.ac_loc),
            );
            let replace_loc = ac_loc.clone();
            let insert_loc = Loc {
                source: replace_loc.source.dupe(),
                start: replace_loc.start,
                end: cursor.end,
            };
            let edit_locs = (insert_loc, replace_loc);
            let autocomplete_type_string =
                string_of_autocomplete_type(&processed.autocomplete_type).to_string();
            let result = match processed.autocomplete_type {
                autocomplete_js::AutocompleteType::AcBinding => {
                    AutocompleteServiceResultGeneric::AcEmpty("Binding".to_string())
                }
                autocomplete_js::AutocompleteType::AcTypeBinding => {
                    AutocompleteServiceResultGeneric::AcEmpty("TypeBinding".to_string())
                }
                autocomplete_js::AutocompleteType::AcIgnored => {
                    AutocompleteServiceResultGeneric::AcEmpty("Ignored".to_string())
                }
                autocomplete_js::AutocompleteType::AcComment { text, loc } => {
                    autocomplete_comment(typing, &edit_locs, trigger_character, &token, &text, &loc)
                }
                autocomplete_js::AutocompleteType::AcJsxText => {
                    AutocompleteServiceResultGeneric::AcEmpty("JSXText".to_string())
                }
                autocomplete_js::AutocompleteType::AcClassKey { enclosing_class_t } => {
                    autocomplete_class_key(typing, &token, &edit_locs, enclosing_class_t)
                }
                autocomplete_js::AutocompleteType::AcModule => {
                    AutocompleteServiceResultGeneric::AcEmpty("Module".to_string())
                }
                autocomplete_js::AutocompleteType::AcImportSpecifier {
                    module_type_opt,
                    used_keys,
                    is_type,
                } => autocomplete_module_exports(
                    typing,
                    &edit_locs,
                    &token,
                    if is_type {
                        ModuleExportKind::Type
                    } else {
                        ModuleExportKind::Value
                    },
                    Some(&|name| !used_keys.contains(name)),
                    module_type_opt.map(ModuleTypeOrType::Module),
                ),
                autocomplete_js::AutocompleteType::AcEnum => {
                    AutocompleteServiceResultGeneric::AcEmpty("Enum".to_string())
                }
                autocomplete_js::AutocompleteType::AcKey {
                    obj_type,
                    used_keys,
                    spreads,
                } => autocomplete_object_key(
                    typing, &edit_locs, &token, &used_keys, &spreads, &obj_type,
                ),
                autocomplete_js::AutocompleteType::AcLiteral { lit_type: None } => {
                    AutocompleteServiceResultGeneric::AcEmpty("Literal".to_string())
                }
                autocomplete_js::AutocompleteType::AcLiteral {
                    lit_type: Some(lit_type),
                } => {
                    let expected_type = expected_concrete_type_of_t(typing.cx, &lit_type);
                    let genv = typing.norm_genv();
                    let items = filter_by_token_and_sort(
                        &token,
                        autocomplete_literals(
                            typing.layout_options.single_quotes,
                            typing.cx,
                            &genv,
                            &edit_locs,
                            &expected_type,
                            &token,
                        ),
                    );
                    AutocompleteServiceResultGeneric::AcResult(AcResult {
                        result: ac_completion::T {
                            items,
                            is_incomplete: false,
                        },
                        errors_to_log: Vec::new(),
                    })
                }
                autocomplete_js::AutocompleteType::AcId {
                    include_super,
                    include_this,
                    type_,
                    enclosing_class_t,
                } => {
                    let result_id = autocomplete_id(
                        typing,
                        &ac_loc,
                        true,
                        include_super,
                        include_this,
                        ac_options,
                        &edit_locs,
                        &token,
                        &type_,
                    )?;
                    match enclosing_class_t {
                        Some(t) => match autocomplete_member(
                            typing,
                            ac_options,
                            &edit_locs,
                            &token,
                            &t,
                            false,
                            &ac_loc,
                            &processed.tparams_rev,
                            None,
                            None,
                            false,
                            true,
                        )? {
                            AutocompleteServiceResultGeneric::AcFatalError(err) => {
                                AutocompleteServiceResultGeneric::AcFatalError(err)
                            }
                            AutocompleteServiceResultGeneric::AcEmpty(_) => {
                                AutocompleteServiceResultGeneric::AcResult(result_id)
                            }
                            AutocompleteServiceResultGeneric::AcResult(result_member) => {
                                let init_rev_items = result_id.result.items.iter().rev().cloned();
                                let rev_items: Vec<_> = result_member
                                    .result
                                    .items
                                    .into_iter()
                                    .rev()
                                    .map(|item| {
                                        let name = format!("this.{}", item.name);
                                        let mut item = item;
                                        item.name = name.clone();
                                        item.text_edit =
                                            Some(text_edit(Some(&name), &name, &edit_locs));
                                        item
                                    })
                                    .chain(init_rev_items)
                                    .collect();
                                AutocompleteServiceResultGeneric::AcResult(AcResult {
                                    result: ac_completion::T {
                                        items: filter_by_token_and_sort_rev(
                                            &token, rev_items, false,
                                        )
                                        .into_iter()
                                        .rev()
                                        .collect(),
                                        is_incomplete: result_id.result.is_incomplete
                                            || result_member.result.is_incomplete,
                                    },
                                    errors_to_log: [
                                        result_id.errors_to_log,
                                        result_member.errors_to_log,
                                    ]
                                    .concat(),
                                })
                            }
                        },
                        None => AutocompleteServiceResultGeneric::AcResult(result_id),
                    }
                }
                autocomplete_js::AutocompleteType::AcMember {
                    obj_type,
                    in_optional_chain,
                    bracket_syntax,
                    member_loc,
                    is_type_annotation,
                    is_super,
                } => autocomplete_member(
                    typing,
                    ac_options,
                    &edit_locs,
                    &token,
                    &obj_type,
                    in_optional_chain,
                    &ac_loc,
                    &processed.tparams_rev,
                    bracket_syntax,
                    member_loc,
                    is_type_annotation,
                    is_super,
                )?,
                autocomplete_js::AutocompleteType::AcJsxElement { type_ } => {
                    autocomplete_jsx_element(
                        typing, &ac_loc, ac_options, &edit_locs, &token, &type_,
                    )?
                }
                autocomplete_js::AutocompleteType::AcJsxAttribute {
                    attribute_name: _,
                    used_attr_names,
                    props,
                    has_value,
                } => autocomplete_jsx_attribute(
                    typing,
                    &used_attr_names,
                    has_value,
                    &edit_locs,
                    &token,
                    &props,
                )?,
                autocomplete_js::AutocompleteType::AcRecordField {
                    field_name: _,
                    used_field_names,
                    record_t,
                    has_value: _,
                } => autocomplete_record_field(
                    typing,
                    &used_field_names,
                    &edit_locs,
                    &token,
                    &record_t,
                ),
                autocomplete_js::AutocompleteType::AcType => {
                    AutocompleteServiceResultGeneric::AcResult(autocomplete_unqualified_type(
                        typing,
                        ac_options,
                        &processed.tparams_rev,
                        &ac_loc,
                        &edit_locs,
                        &token,
                    )?)
                }
                autocomplete_js::AutocompleteType::AcQualifiedType(qtype) => {
                    autocomplete_module_exports(
                        typing,
                        &edit_locs,
                        &token,
                        ModuleExportKind::Type,
                        None,
                        Some(ModuleTypeOrType::Type(qtype)),
                    )
                }
            };
            let result = force_autocomplete_result_documentation(typing, result)?;
            Ok((Some(token), Some(ac_loc), autocomplete_type_string, result))
        }
    }
}
