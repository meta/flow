/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::sync::Arc;

use dupe::Dupe;
use dupe::IterDupedExt;
use flow_aloc::ALoc;
use flow_analysis::scope_api::ScopeInfo;
use flow_check_cache::CheckContentsCache;
use flow_common::options::Options;
use flow_common::reason::VirtualReasonDesc;
use flow_common_errors::error_codes::ErrorCode;
use flow_common_ty::ty::Elt;
use flow_common_ty::ty::Ty;
use flow_common_ty::ty_printer;
use flow_common_ty::ty_printer::PrinterOptions;
use flow_common_ty::ty_utils;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_env_builder::env_api::WriteLoc;
use flow_heap::parsing_heaps::Transaction;
use flow_parser::ast;
use flow_parser::ast::types::TypeInner;
use flow_parser::file_key::FileKey;
use flow_parser::loc::LOC_NONE;
use flow_parser::loc::Loc;
use flow_parser::parse_error::ParseError;
use flow_parser_utils::file_sig::FileSig;
use flow_parser_utils::flow_ast_differ;
use flow_parser_utils_output::ast_diff_printer;
use flow_parser_utils_output::js_layout_generator;
use flow_parser_utils_output::replacement_printer;
use flow_server_env::lsp::loc_to_lsp_range;
use flow_server_env::lsp_helpers;
use flow_server_env::server_env::Env;
use flow_server_env::server_orchestrator::ServerOrchestratorHandle;
use flow_services_autocomplete::code_action_text_edits::CodeActionTextEdits;
use flow_services_autocomplete::insert_jsdoc;
use flow_services_autocomplete::lsp_import_edits;
use flow_services_autocomplete::module_system_info::LspModuleSystemInfo;
use flow_services_export::export_index;
use flow_services_export::export_search;
use flow_services_export::export_search::ExportSearch;
use flow_services_inference::type_contents;
use flow_services_inference_types::ParseArtifacts;
use flow_services_inference_types::TypecheckArtifacts;
use flow_type_sig::signature_error::TolerableError;
use flow_typing_context::Context;
use flow_typing_errors::error_message::BindingError;
use flow_typing_errors::error_message::EBuiltinNameLookupFailedData;
use flow_typing_errors::error_message::EClassToObjectData;
use flow_typing_errors::error_message::EDuplicateComponentPropData;
use flow_typing_errors::error_message::EIncompatibleTypesWithUseOpData;
use flow_typing_errors::error_message::EIncorrectTypeWithReplacementData;
use flow_typing_errors::error_message::EInvalidRendersTypeArgumentData;
use flow_typing_errors::error_message::EInvariantSubtypingWithUseOpData;
use flow_typing_errors::error_message::EPropNotFoundInLookupData;
use flow_typing_errors::error_message::EPropsNotFoundInInvariantSubtypingData;
use flow_typing_errors::error_message::ETSSyntaxData;
use flow_typing_errors::error_message::EVarianceKeywordData;
use flow_typing_errors::error_message::EnumErrorKind;
use flow_typing_errors::error_message::EnumInvalidMemberAccessData;
use flow_typing_errors::error_message::EnumInvalidMemberNameData;
use flow_typing_errors::error_message::EnumMemberAccess;
use flow_typing_errors::error_message::ErrorMessage;
use flow_typing_errors::error_message::ErrorTypeReferenceWithLocData;
use flow_typing_errors::error_message::FriendlyMessageRecipe;
use flow_typing_errors::error_message::IncompatibleTypeUseData;
use flow_typing_errors::error_message::MatchErrorKind;
use flow_typing_errors::error_message::MatchInvalidCaseSyntaxData;
use flow_typing_errors::error_message::MatchInvalidObjectShorthandData;
use flow_typing_errors::error_message::MatchNonExhaustiveObjectPatternData;
use flow_typing_errors::error_message::MatchNotExhaustiveData;
use flow_typing_errors::error_message::MatchUnusedPatternData;
use flow_typing_errors::error_message::ReadonlyTypeKind;
use flow_typing_errors::error_message::RecordErrorKind;
use flow_typing_errors::error_message::TSSyntaxKind;
use flow_typing_errors::error_message::UpperKind;
use flow_typing_errors::error_message::VarianceKeywordKind;
use flow_typing_errors::flow_error::ErrorSet;
use flow_typing_errors::flow_error::FlowError;
use flow_typing_errors::intermediate_error::make_intermediate_error;
use flow_typing_errors::intermediate_error_types::Explanation;
use flow_typing_errors::intermediate_error_types::ExplanationWithLazyParts;
use flow_typing_errors::intermediate_error_types::IncorrectType;
use flow_typing_errors::intermediate_error_types::InternalType;
use flow_typing_errors::intermediate_error_types::InvalidRenderTypeKind;
use flow_typing_errors::intermediate_error_types::VarianceSigilParent;
use flow_typing_type::type_::Type;
use flow_typing_type::type_::type_or_type_desc::ErrorTypeDesc;
use flow_typing_type::type_::type_or_type_desc::TypeOrTypeDescT;
use flow_typing_type::type_util::reason_of_t;
use lsp_types::CodeAction;
use lsp_types::CodeActionKind;
use lsp_types::CodeActionOrCommand;
use lsp_types::Command;
use lsp_types::Diagnostic;
use lsp_types::NumberOrString;
use lsp_types::TextEdit;
use lsp_types::Uri;
use lsp_types::WorkspaceEdit;

use crate::autofix_casting_syntax;
use crate::autofix_class_member_access;
use crate::autofix_enum_member_name;
use crate::autofix_exports;
use crate::autofix_imports;
use crate::autofix_interface;
use crate::autofix_legacy_flow_syntax;
use crate::autofix_match_syntax;
use crate::autofix_missing_local_annots;
use crate::autofix_new_to_record;
use crate::autofix_object_to_record;
use crate::autofix_optional_chaining;
use crate::autofix_prop_typo;
use crate::autofix_record_declaration;
use crate::autofix_renders_variant;
use crate::autofix_replace_type;
use crate::autofix_ts_syntax;
use crate::autofix_type_name;
use crate::autofix_type_to_value_import;
use crate::autofix_unused_promise;
use crate::code_action_utils;
use crate::convert_type_to_readonly_form;
use crate::insert_inferred_render_type;
use crate::insert_type;
use crate::refactor_add_jsx_props;
use crate::refactor_arrow_functions;
use crate::refactor_extract;
use crate::refactor_match_discriminant;
use crate::refactor_switch_to_match_statement;
use crate::stub_unbound_name;

fn add_missing_imports_kind() -> CodeActionKind {
    CodeActionKind::new("source.addMissingImports.flow")
}

fn is_kind(prefix: &CodeActionKind, kind: &CodeActionKind) -> bool {
    let prefix_str = prefix.as_str();
    let kind_str = kind.as_str();
    kind_str == prefix_str || kind_str.starts_with(&format!("{}.", prefix_str))
}

fn include_code_action(only: Option<&[CodeActionKind]>, kind: &CodeActionKind) -> bool {
    match only {
        None => true,
        Some(only) => only.iter().any(|prefix| is_kind(prefix, kind)),
    }
}

fn include_quick_fixes(only: Option<&[CodeActionKind]>) -> bool {
    include_code_action(only, &CodeActionKind::QUICKFIX)
}

fn include_extract_refactors(only: Option<&[CodeActionKind]>) -> bool {
    include_code_action(only, &CodeActionKind::REFACTOR_EXTRACT)
}

fn include_rewrite_refactors(only: Option<&[CodeActionKind]>) -> bool {
    include_code_action(only, &CodeActionKind::REFACTOR_REWRITE)
}

fn include_add_missing_imports_action(only: Option<&[CodeActionKind]>) -> bool {
    include_code_action(only, &add_missing_imports_kind())
}

fn include_organize_imports_actions(only: Option<&[CodeActionKind]>) -> bool {
    include_code_action(only, &CodeActionKind::SOURCE_ORGANIZE_IMPORTS)
}

fn flow_loc_patch_to_lsp_edits(patch: &[(Loc, String)]) -> Vec<TextEdit> {
    patch
        .iter()
        .map(|(loc, text)| TextEdit {
            range: loc_to_lsp_range(loc),
            new_text: text.clone(),
        })
        .collect()
}

fn mk_log_command(title: &str, diagnostic_title: &str) -> Command {
    Command {
        title: "".to_string(),
        command: "log".to_string(),
        arguments: Some(vec![
            serde_json::Value::String("textDocument/codeAction".to_string()),
            serde_json::Value::String(diagnostic_title.to_string()),
            serde_json::Value::String(title.to_string()),
        ]),
    }
}

fn autofix_insert_type_annotation_helper(
    options: &Options,
    ast: &ast::Program<Loc, Loc>,
    diagnostics: &[Diagnostic],
    uri: &Uri,
    new_ast: &ast::Program<Loc, Loc>,
) -> Vec<CodeActionOrCommand> {
    let diff = insert_type::mk_diff(ast, new_ast);
    let opts = code_action_utils::layout_options(options);
    let edits =
        flow_loc_patch_to_lsp_edits(&replacement_printer::mk_loc_patch_ast_differ(&opts, &diff));
    let title = "Insert type annotation to fix signature-verification-failure error";
    vec![CodeActionOrCommand::CodeAction(CodeAction {
        title: title.to_string(),
        kind: Some(CodeActionKind::QUICKFIX),
        diagnostics: Some(diagnostics.to_vec()),
        edit: Some(WorkspaceEdit {
            changes: Some(HashMap::from([(uri.clone(), edits)])),
            ..Default::default()
        }),
        command: Some(mk_log_command(
            title,
            "insert_type_for_sig_verification_failure",
        )),
        is_preferred: None,
        disabled: None,
        data: None,
    })]
}

fn autofix_exports_code_actions<'a, 'b>(
    options: &Options,
    cx: &mut Context<'a>,
    loc_of_aloc: &'b dyn Fn(&ALoc) -> Loc,
    get_ast_from_heap: &'b dyn Fn(&FileKey) -> Option<ast::Program<Loc, Loc>>,
    get_haste_module_info: &'b dyn Fn(&FileKey) -> Option<flow_common_modulename::HasteModuleInfo>,
    get_type_sig: &'b dyn Fn(
        &FileKey,
    ) -> Option<
        flow_type_sig::packed_type_sig::Module<
            flow_type_sig::compact_table::Index<flow_aloc::ALoc>,
        >,
    >,
    ast: &ast::Program<Loc, Loc>,
    file_sig: &Arc<FileSig>,
    tolerable_errors: &[TolerableError<Loc>],
    typed_ast: &ast::Program<ALoc, (ALoc, Type)>,
    diagnostics: &[Diagnostic],
    uri: &Uri,
    loc: Loc,
) -> Result<Vec<CodeActionOrCommand>, insert_type::Errors> {
    let fixable_locs =
        autofix_exports::set_of_fixable_signature_verification_locations(tolerable_errors);
    if fixable_locs.contains(&loc) {
        let new_ast = autofix_exports::fix_signature_verification_error_at_loc(
            None, // remote_converter
            cx,
            loc_of_aloc,
            get_ast_from_heap,
            get_haste_module_info,
            get_type_sig,
            file_sig,
            typed_ast,
            ast,
            loc,
        )?;
        Ok(autofix_insert_type_annotation_helper(
            options,
            ast,
            diagnostics,
            uri,
            &new_ast,
        ))
    } else {
        Ok(vec![])
    }
}

fn autofix_missing_local_annot_code_actions<'a, 'b>(
    options: &Options,
    cx: &mut Context<'a>,
    loc_of_aloc: &'b dyn Fn(&ALoc) -> Loc,
    get_ast_from_heap: &'b dyn Fn(&FileKey) -> Option<ast::Program<Loc, Loc>>,
    get_haste_module_info: &'b dyn Fn(&FileKey) -> Option<flow_common_modulename::HasteModuleInfo>,
    get_type_sig: &'b dyn Fn(
        &FileKey,
    ) -> Option<
        flow_type_sig::packed_type_sig::Module<
            flow_type_sig::compact_table::Index<flow_aloc::ALoc>,
        >,
    >,
    ast: &ast::Program<Loc, Loc>,
    file_sig: &Arc<FileSig>,
    _tolerable_errors: &[TolerableError<Loc>],
    typed_ast: &ast::Program<ALoc, (ALoc, Type)>,
    diagnostics: &[Diagnostic],
    uri: &Uri,
    loc: Loc,
) -> Result<Vec<CodeActionOrCommand>, insert_type::Errors> {
    let fixable_locs = autofix_missing_local_annots::map_of_fixable_missing_local_params(cx);
    let entry = fixable_locs
        .iter()
        .find(|(err_loc, _)| Loc::contains(err_loc, &loc));
    match entry {
        Some((_, type_t)) => {
            let new_ast = autofix_missing_local_annots::fix_missing_param_annot_at_loc(
                None, // remote_converter
                cx,
                loc_of_aloc,
                get_ast_from_heap,
                get_haste_module_info,
                get_type_sig,
                file_sig,
                typed_ast,
                ast,
                loc,
                type_t.dupe(),
            )?;
            Ok(autofix_insert_type_annotation_helper(
                options,
                ast,
                diagnostics,
                uri,
                &new_ast,
            ))
        }
        None => Ok(vec![]),
    }
}

fn code_action_insert_inferred_render_type<'a, 'b>(
    options: &Options,
    cx: &mut Context<'a>,
    loc_of_aloc: &'b dyn Fn(&ALoc) -> Loc,
    get_ast_from_heap: &'b dyn Fn(&FileKey) -> Option<ast::Program<Loc, Loc>>,
    get_haste_module_info: &'b dyn Fn(&FileKey) -> Option<flow_common_modulename::HasteModuleInfo>,
    get_type_sig: &'b dyn Fn(
        &FileKey,
    ) -> Option<
        flow_type_sig::packed_type_sig::Module<
            flow_type_sig::compact_table::Index<flow_aloc::ALoc>,
        >,
    >,
    ast: &ast::Program<Loc, Loc>,
    file_sig: &Arc<FileSig>,
    typed_ast: &ast::Program<ALoc, (ALoc, Type)>,
    uri: &Uri,
    loc: Loc,
) -> Result<Vec<CodeActionOrCommand>, String> {
    let result = insert_inferred_render_type::insert_render_type_at_loc(
        None, // remote_converter
        cx,
        loc_of_aloc,
        get_ast_from_heap,
        get_haste_module_info,
        get_type_sig,
        file_sig,
        typed_ast,
        ast,
        loc,
    )
    .map_err(|e| format!("{:?}", e))?;
    match result {
        Some(new_ast_result) => {
            let new_ast = new_ast_result.map_err(|e| format!("{:?}", e))?;
            let diff = insert_type::mk_diff(ast, &new_ast);
            let opts = code_action_utils::layout_options(options);
            let edits = flow_loc_patch_to_lsp_edits(&replacement_printer::mk_loc_patch_ast_differ(
                &opts, &diff,
            ));
            let title = "Insert inferred render type";
            Ok(vec![CodeActionOrCommand::CodeAction(CodeAction {
                title: title.to_string(),
                kind: Some(CodeActionKind::REFACTOR),
                diagnostics: Some(vec![]),
                edit: Some(WorkspaceEdit {
                    changes: Some(HashMap::from([(uri.clone(), edits)])),
                    ..Default::default()
                }),
                command: Some(mk_log_command(title, "insert_inferred_render_type")),
                is_preferred: None,
                disabled: None,
                data: None,
            })])
        }
        None => Ok(vec![]),
    }
}

fn refactor_extract_and_stub_out_code_actions<'a, 'b>(
    options: &Options,
    support_experimental_snippet_text_edit: bool,
    file_contents: &str,
    ast: &ast::Program<Loc, Loc>,
    cx: &mut Context<'a>,
    file_sig: &Arc<FileSig>,
    typed_ast: &ast::Program<ALoc, (ALoc, Type)>,
    loc_of_aloc: &'b dyn Fn(&ALoc) -> Loc,
    get_ast_from_heap: &'b dyn Fn(&FileKey) -> Option<ast::Program<Loc, Loc>>,
    get_haste_module_info: &'b dyn Fn(&FileKey) -> Option<flow_common_modulename::HasteModuleInfo>,
    get_type_sig: &'b dyn Fn(
        &FileKey,
    ) -> Option<
        flow_type_sig::packed_type_sig::Module<
            flow_type_sig::compact_table::Index<flow_aloc::ALoc>,
        >,
    >,
    only: Option<&[CodeActionKind]>,
    uri: &Uri,
    loc: Loc,
) -> Vec<CodeActionOrCommand> {
    match loc.source {
        None => vec![],
        Some(ref file) => {
            let lsp_action_from_refactor = |diagnostic_title: &str,
                                            kind: CodeActionKind,
                                            refactor: &refactor_extract::Refactor|
             -> CodeActionOrCommand {
                let diff = insert_type::mk_diff(ast, &refactor.new_ast);
                let opts = code_action_utils::layout_options(options);
                let mut patch = autofix_imports::add_imports(&opts, &refactor.added_imports, ast);
                patch.extend(replacement_printer::mk_loc_patch_ast_differ(&opts, &diff));
                let edits = flow_loc_patch_to_lsp_edits(&patch);
                CodeActionOrCommand::CodeAction(CodeAction {
                    title: refactor.title.clone(),
                    kind: Some(kind),
                    diagnostics: Some(vec![]),
                    edit: Some(WorkspaceEdit {
                        changes: Some(HashMap::from([(uri.clone(), edits)])),
                        ..Default::default()
                    }),
                    command: Some(mk_log_command(&refactor.title, diagnostic_title)),
                    is_preferred: None,
                    disabled: None,
                    data: None,
                })
            };
            let mut refactors: Vec<CodeActionOrCommand> =
                if !include_extract_refactors(only) || loc.start == loc.end {
                    vec![]
                } else {
                    let use_strict = options.modules_are_use_strict;
                    let module_ref_prefix = options.haste_module_ref_prefix.clone();
                    let parse_options = flow_parser::ParseOptions {
                        use_strict,
                        module_ref_prefix,
                        ..flow_parser::PERMISSIVE_PARSE_OPTIONS
                    };
                    let tokens = crate::ast_extraction_utils::ast_extractor::tokens(
                        Some(parse_options),
                        Some(file.dupe()),
                        file_contents,
                    );
                    refactor_extract::provide_available_refactors(
                        &tokens,
                        ast,
                        cx,
                        file,
                        file_sig.dupe(),
                        typed_ast,
                        loc_of_aloc,
                        get_ast_from_heap,
                        get_haste_module_info,
                        get_type_sig,
                        support_experimental_snippet_text_edit,
                        &loc,
                    )
                    .iter()
                    .map(|r| {
                        lsp_action_from_refactor(
                            "refactor_extract",
                            CodeActionKind::REFACTOR_EXTRACT,
                            r,
                        )
                    })
                    .collect()
                };
            match stub_unbound_name::stub(
                ast,
                cx,
                &file.dupe(),
                file_sig,
                typed_ast,
                loc_of_aloc,
                get_ast_from_heap,
                get_haste_module_info,
                get_type_sig,
                loc.dupe(),
            ) {
                None => {}
                Some(r) => {
                    refactors.insert(
                        0,
                        lsp_action_from_refactor("stub_out", CodeActionKind::QUICKFIX, &r),
                    );
                }
            }
            refactors
        }
    }
}

fn insert_inferred_type_as_cast_code_actions<'a>(
    options: &Options,
    file_contents: &str,
    ast: &ast::Program<Loc, Loc>,
    cx: &mut Context<'a>,
    file_sig: &Arc<FileSig>,
    typed_ast: &ast::Program<ALoc, (ALoc, Type)>,
    loc_of_aloc: Arc<dyn Fn(&ALoc) -> Loc>,
    get_ast_from_heap: Arc<dyn Fn(&FileKey) -> Option<ast::Program<Loc, Loc>>>,
    get_haste_module_info: Arc<dyn Fn(&FileKey) -> Option<flow_common_modulename::HasteModuleInfo>>,
    get_type_sig: Arc<
        dyn Fn(
            &FileKey,
        ) -> Option<
            flow_type_sig::packed_type_sig::Module<flow_type_sig::compact_table::Index<ALoc>>,
        >,
    >,
    uri: &Uri,
    loc: Loc,
) -> Vec<CodeActionOrCommand> {
    if loc.start == loc.end {
        return vec![];
    }
    match loc.source {
        None => vec![],
        Some(ref file) => {
            let use_strict = options.modules_are_use_strict;
            let module_ref_prefix = options.haste_module_ref_prefix.clone();
            let parse_options = flow_parser::ParseOptions {
                use_strict,
                module_ref_prefix,
                ..flow_parser::PERMISSIVE_PARSE_OPTIONS
            };
            let tokens = crate::ast_extraction_utils::ast_extractor::tokens(
                Some(parse_options),
                Some(file.dupe()),
                file_contents,
            );
            let extraction_result =
                crate::ast_extraction_utils::ast_extractor::extract(&tokens, ast, loc.dupe());
            match extraction_result.extracted_expression {
                None => vec![],
                Some(ref extracted) => {
                    let expression_loc = extracted.expression.loc().dupe();
                    let loc_of_aloc_rc = loc_of_aloc.clone();
                    let get_haste_module_info_rc = get_haste_module_info.clone();
                    let get_type_sig_rc = get_type_sig.clone();
                    let remote_converter =
                        crate::insert_type_imports::imports_helper::RemoteConverter::new(
                            Box::new(move |aloc| loc_of_aloc_rc(aloc)),
                            options.file_options.dupe(),
                            Box::new(move |fk| get_haste_module_info_rc(fk)),
                            Box::new(move |fk| get_type_sig_rc(fk)),
                            0, // iteration
                            file.dupe(),
                            Default::default(),
                        );
                    let mut remote_converter = remote_converter;
                    match insert_type::insert_type(
                        cx,
                        &*loc_of_aloc,
                        &*get_ast_from_heap,
                        &*get_haste_module_info,
                        &*get_type_sig,
                        file_sig,
                        typed_ast,
                        false, // omit_targ_defaults
                        false, // strict
                        Some(&mut remote_converter),
                        ast,
                        expression_loc,
                    ) {
                        Err(_) => vec![],
                        Ok(new_ast) => {
                            let added_imports = remote_converter.to_import_bindings();
                            let diff = insert_type::mk_diff(ast, &new_ast);
                            let opts = code_action_utils::layout_options(options);
                            let mut patch =
                                autofix_imports::add_imports(&opts, &added_imports, ast);
                            patch
                                .extend(replacement_printer::mk_loc_patch_ast_differ(&opts, &diff));
                            let edits = flow_loc_patch_to_lsp_edits(&patch);
                            let title = "Insert inferred type as a type cast";
                            let code_action = CodeActionOrCommand::CodeAction(CodeAction {
                                title: title.to_string(),
                                kind: Some(CodeActionKind::REFACTOR),
                                diagnostics: Some(vec![]),
                                edit: Some(WorkspaceEdit {
                                    changes: Some(HashMap::from([(uri.clone(), edits)])),
                                    ..Default::default()
                                }),
                                command: Some(mk_log_command(
                                    title,
                                    "insert_inferred_type_as_cast",
                                )),
                                is_preferred: None,
                                disabled: None,
                                data: None,
                            });
                            vec![code_action]
                        }
                    }
                }
            }
        }
    }
}

fn insert_jsdoc_code_actions(
    options: &Options,
    ast: &ast::Program<Loc, Loc>,
    uri: &Uri,
    loc: Loc,
) -> Vec<CodeActionOrCommand> {
    match insert_jsdoc::insert_stub_for_target(false, loc, ast) {
        Some((ast_prime, _)) => {
            let diff = flow_ast_differ::program(ast, &ast_prime);
            let opts = code_action_utils::layout_options(options);
            let edits = flow_loc_patch_to_lsp_edits(&replacement_printer::mk_loc_patch_ast_differ(
                &opts, &diff,
            ));
            let edits = edits
                .into_iter()
                .map(|edit| TextEdit {
                    new_text: format!("{}\n", edit.new_text),
                    ..edit
                })
                .collect::<Vec<_>>();
            let title = "Add JSDoc documentation";
            vec![CodeActionOrCommand::CodeAction(CodeAction {
                title: title.to_string(),
                kind: Some(CodeActionKind::REFACTOR),
                diagnostics: Some(vec![]),
                edit: Some(WorkspaceEdit {
                    changes: Some(HashMap::from([(uri.clone(), edits)])),
                    ..Default::default()
                }),
                command: Some(mk_log_command(title, "add_jsdocs")),
                is_preferred: None,
                disabled: None,
                data: None,
            })]
        }
        _ => vec![],
    }
}

fn convert_type_to_readonly_form_code_actions(
    options: &Options,
    ast: &ast::Program<Loc, Loc>,
    only: Option<&[CodeActionKind]>,
    uri: &Uri,
    loc: Loc,
) -> Vec<CodeActionOrCommand> {
    if include_rewrite_refactors(only) {
        match convert_type_to_readonly_form::convert(ast, loc) {
            None => vec![],
            Some((ast_prime, conversion_kind)) => {
                let diff = flow_ast_differ::program(ast, &ast_prime);
                let opts = code_action_utils::layout_options(options);
                let edits = flow_loc_patch_to_lsp_edits(
                    &replacement_printer::mk_loc_patch_ast_differ(&opts, &diff),
                );
                let title = match conversion_kind {
                    convert_type_to_readonly_form::ConversionKind::ConversionToReadOnlyArray => {
                        "Make the array readonly"
                    }
                    convert_type_to_readonly_form::ConversionKind::ConversionToReadOnlyObject => {
                        "Make the object readonly"
                    }
                    convert_type_to_readonly_form::ConversionKind::ConversionToReadOnlyMap => {
                        "Make the Map readonly"
                    }
                    convert_type_to_readonly_form::ConversionKind::ConversionToReadOnlySet => {
                        "Make the Set readonly"
                    }
                };
                vec![CodeActionOrCommand::CodeAction(CodeAction {
                    title: title.to_string(),
                    kind: Some(CodeActionKind::REFACTOR_REWRITE),
                    diagnostics: Some(vec![]),
                    edit: Some(WorkspaceEdit {
                        changes: Some(HashMap::from([(uri.clone(), edits)])),
                        ..Default::default()
                    }),
                    command: Some(mk_log_command(
                        title,
                        "refactor_rewrite_readonly_conversion",
                    )),
                    is_preferred: None,
                    disabled: None,
                    data: None,
                })]
            }
        }
    } else {
        vec![]
    }
}

fn refactor_arrow_function_code_actions(
    ast: &ast::Program<Loc, Loc>,
    scope_info: &ScopeInfo<Loc>,
    options: &Options,
    only: Option<&[CodeActionKind]>,
    uri: &Uri,
    loc: Loc,
) -> Vec<CodeActionOrCommand> {
    if include_rewrite_refactors(only) {
        match refactor_arrow_functions::add_or_remove_braces(ast, scope_info, &loc) {
            Some((ast_prime, title)) => {
                let diff = flow_ast_differ::program(ast, &ast_prime);
                let opts = code_action_utils::layout_options(options);
                let edits = flow_loc_patch_to_lsp_edits(
                    &replacement_printer::mk_loc_patch_ast_differ(&opts, &diff),
                );
                vec![CodeActionOrCommand::CodeAction(CodeAction {
                    title: title.clone(),
                    kind: Some(CodeActionKind::REFACTOR_REWRITE),
                    diagnostics: Some(vec![]),
                    edit: Some(WorkspaceEdit {
                        changes: Some(HashMap::from([(uri.clone(), edits)])),
                        ..Default::default()
                    }),
                    command: Some(mk_log_command(&title, "refactor_arrow_function")),
                    is_preferred: None,
                    disabled: None,
                    data: None,
                })]
            }
            None => vec![],
        }
    } else {
        vec![]
    }
}

fn refactor_switch_to_match_statement_actions(
    cx: &mut Context,
    ast: &ast::Program<Loc, Loc>,
    options: &Options,
    only: Option<&[CodeActionKind]>,
    uri: &Uri,
    loc: Loc,
) -> Vec<CodeActionOrCommand> {
    if cx.enable_pattern_matching()
        && (include_quick_fixes(only) || include_rewrite_refactors(only))
    {
        match refactor_switch_to_match_statement::refactor(ast, loc) {
            Some(ast_prime) => {
                let diff = flow_ast_differ::program(ast, &ast_prime);
                let opts = code_action_utils::layout_options(options);
                let edits = flow_loc_patch_to_lsp_edits(
                    &replacement_printer::mk_loc_patch_ast_differ(&opts, &diff),
                );
                let title = "Refactor `switch` to `match`";
                vec![CodeActionOrCommand::CodeAction(CodeAction {
                    title: title.to_string(),
                    kind: Some(CodeActionKind::QUICKFIX),
                    diagnostics: Some(vec![]),
                    edit: Some(WorkspaceEdit {
                        changes: Some(HashMap::from([(uri.clone(), edits)])),
                        ..Default::default()
                    }),
                    command: Some(mk_log_command(title, "refactor_switch_to_match")),
                    is_preferred: None,
                    disabled: None,
                    data: None,
                })]
            }
            None => vec![],
        }
    } else {
        vec![]
    }
}

fn refactor_match_coded_like_switch(
    cx: &mut Context,
    ast: &ast::Program<Loc, Loc>,
    options: &Options,
    only: Option<&[CodeActionKind]>,
    loc_of_aloc: &dyn Fn(&ALoc) -> Loc,
    uri: &Uri,
    loc: Loc,
) -> Vec<CodeActionOrCommand> {
    if cx.enable_pattern_matching()
        && (include_quick_fixes(only) || include_rewrite_refactors(only))
        && cx.errors().exists(|error| {
            let intermediate = make_intermediate_error(loc_of_aloc, false, error, |file| {
                cx.is_global_libdef(file)
            });
            Loc::contains(&intermediate.loc, &loc)
        })
    {
        match refactor_match_discriminant::refactor(ast, loc) {
            Some(ast_prime) => {
                let diff = flow_ast_differ::program(ast, &ast_prime);
                let opts = code_action_utils::layout_options(options);
                let edits = flow_loc_patch_to_lsp_edits(
                    &replacement_printer::mk_loc_patch_ast_differ(&opts, &diff),
                );
                let title = "Refactor `match` coded like a switch";
                vec![CodeActionOrCommand::CodeAction(CodeAction {
                    title: title.to_string(),
                    kind: Some(CodeActionKind::QUICKFIX),
                    diagnostics: Some(vec![]),
                    edit: Some(WorkspaceEdit {
                        changes: Some(HashMap::from([(uri.clone(), edits)])),
                        ..Default::default()
                    }),
                    command: Some(mk_log_command(title, "refactor_switch_to_match")),
                    is_preferred: None,
                    disabled: None,
                    data: None,
                })]
            }
            None => vec![],
        }
    } else {
        vec![]
    }
}

fn add_jsx_props_code_actions(
    snippets_enabled: bool,
    cx: &mut Context,
    ast: &ast::Program<Loc, Loc>,
    typed_ast: &ast::Program<ALoc, (ALoc, Type)>,
    options: &Options,
    uri: &Uri,
    loc: Loc,
) -> Vec<CodeActionOrCommand> {
    match refactor_add_jsx_props::fill_props(cx, snippets_enabled, ast, typed_ast, loc) {
        None => vec![],
        Some(xs) => {
            let patch: Vec<(Loc, String)> = xs
                .into_iter()
                .map(|(loc, attr)| {
                    let opts = code_action_utils::layout_options(options);
                    let text = ast_diff_printer::text_of_layout(
                        js_layout_generator::jsx_opening_attr(&opts, &attr),
                    );
                    let text = format!(" {}", text);
                    (loc, text)
                })
                .collect();
            let edits = flow_loc_patch_to_lsp_edits(&patch);
            let title = "Add missing attributes";
            vec![CodeActionOrCommand::CodeAction(CodeAction {
                title: title.to_string(),
                kind: Some(CodeActionKind::QUICKFIX),
                diagnostics: Some(vec![]),
                edit: Some(WorkspaceEdit {
                    changes: Some(HashMap::from([(uri.clone(), edits)])),
                    ..Default::default()
                }),
                command: Some(mk_log_command(title, "add_missing_jsx_props")),
                is_preferred: None,
                disabled: None,
                data: None,
            })]
        }
    }
}

fn preferred_import(
    ast: &ast::Program<Loc, Loc>,
    exports: &ExportSearch,
    name: &str,
    loc: Loc,
) -> Option<export_index::Export> {
    let files = if autofix_imports::loc_is_type(ast, loc) {
        export_search::get_types(name, exports)
    } else {
        export_search::get_values(name, exports)
    };
    if files.len() == 1 {
        files.into_keys().next()
    } else {
        None
    }
}

fn maybe_sort_by_usage(
    imports_ranked_usage: bool,
    mut imports: Vec<(export_index::Export, i32)>,
) -> Vec<(export_index::Export, i32)> {
    if imports_ranked_usage {
        imports.sort_by(|(_, a), (_, b)| b.cmp(a));
    }
    imports
}

fn find_unbound_names_from_scope(
    loc_of_aloc: &dyn Fn(&ALoc) -> Loc,
    cx: &mut Context,
    query_loc: Loc,
) -> Vec<(Loc, FlowSmolStr)> {
    let already_handled_unbound_names_with_error =
        cx.errors()
            .fold(BTreeSet::<Loc>::new(), |mut loc_set, error| {
                let error_message = ErrorMessage::map_loc_of_error_message(
                    |aloc: ALoc| loc_of_aloc(&aloc),
                    error.msg_of_error().clone(),
                );
                match error_message {
                    ErrorMessage::EBuiltinNameLookupFailed(box EBuiltinNameLookupFailedData {
                        loc: error_loc,
                        name: _,
                    }) => {
                        loc_set.insert(error_loc);
                        loc_set
                    }
                    _ => loc_set,
                }
            });
    let env = cx.environment();
    let env_values = &env.var_info.env_values;
    let mut acc = vec![];
    for (aloc, read) in env_values.iter_ordinary() {
        match read.write_locs.as_slice() {
            [WriteLoc::Global(name)] => {
                let loc = loc_of_aloc(aloc);
                if loc.intersects(&query_loc)
                    && !already_handled_unbound_names_with_error.contains(&loc)
                {
                    acc.push((loc, name.dupe()));
                }
            }
            _ => {}
        }
    }
    acc
}

fn suggest_imports(
    cx: &mut Context,
    layout_options: &js_layout_generator::Opts,
    module_system_info: &LspModuleSystemInfo,
    src_dir: Option<&str>,
    ast: &ast::Program<Loc, Loc>,
    diagnostics: &[Diagnostic],
    imports_ranked_usage: bool,
    exports: &ExportSearch,
    name: &str,
    uri: &Uri,
    loc: Loc,
) -> Vec<CodeActionOrCommand> {
    let files = if autofix_imports::loc_is_type(ast, loc.dupe()) {
        export_search::get_types(name, exports)
    } else {
        export_search::get_values(name, exports)
    };
    if files.is_empty() {
        return vec![];
    }
    let error_range = loc_to_lsp_range(&loc);
    let lsp_code = ErrorCode::CannotResolveName.as_str();
    let relevant_diagnostics: Vec<Diagnostic> = diagnostics
        .iter()
        .filter(|d| {
            d.source.as_deref() == Some("Flow")
                && d.code == Some(NumberOrString::String(lsp_code.to_string()))
                && lsp_helpers::ranges_overlap(&d.range, &error_range)
        })
        .cloned()
        .collect();
    let is_available_autoimport_result = lsp_import_edits::is_available_autoimport_result(cx);
    let mut filtered: Vec<(export_index::Export, i32)> = files
        .into_iter()
        .filter(|(export_index::Export(source, _), _)| is_available_autoimport_result(name, source))
        .collect();
    filtered = maybe_sort_by_usage(imports_ranked_usage, filtered);
    let mut acc: Vec<CodeActionOrCommand> = vec![];
    for (export_index::Export(source, export_kind), _num) in filtered {
        match lsp_import_edits::text_edits_of_import(
            layout_options,
            module_system_info,
            src_dir,
            ast,
            &export_kind,
            name,
            &source,
        ) {
            None => {}
            Some(CodeActionTextEdits {
                edits,
                title,
                from: _,
            }) => {
                let command = CodeActionOrCommand::CodeAction(CodeAction {
                    title: title.clone(),
                    kind: Some(CodeActionKind::QUICKFIX),
                    diagnostics: Some(relevant_diagnostics.clone()),
                    edit: Some(WorkspaceEdit {
                        changes: Some(HashMap::from([(uri.clone(), edits)])),
                        ..Default::default()
                    }),
                    command: Some(mk_log_command(&title, "import")),
                    ..Default::default()
                });
                acc.push(command);
            }
        }
    }
    acc
}

pub type AstTransform = Box<
    dyn Fn(
        &mut Context,
        Arc<FileSig>,
        &ast::Program<Loc, Loc>,
        &ast::Program<ALoc, (ALoc, Type)>,
        Loc,
    ) -> Option<ast::Program<Loc, Loc>>,
>;

pub enum QuickfixConfidence {
    WillFixErrorAndSafeForRunningOnSave,
    BestEffort,
}

pub struct AstTransformOfError {
    pub title: String,
    pub diagnostic_title: String,
    pub transform: AstTransform,
    pub target_loc: Loc,
    pub confidence: QuickfixConfidence,
}

pub fn untyped_ast_transform(
    transform: Box<dyn Fn(&ast::Program<Loc, Loc>, Loc) -> ast::Program<Loc, Loc>>,
) -> AstTransform {
    Box::new(
        move |_cx: &mut Context, _file_sig: Arc<FileSig>, ast, _typed_ast, loc| {
            Some(transform(ast, loc))
        },
    )
}

fn autofix_in_upstream_file(
    cx: &mut Context,
    get_ast_from_heap: &dyn Fn(&FileKey) -> Option<ast::Program<Loc, Loc>>,
    file_sig: Arc<FileSig>,
    diagnostics: &[Diagnostic],
    ast: &ast::Program<Loc, Loc>,
    typed_ast: &ast::Program<ALoc, (ALoc, Type)>,
    options: &Options,
    title: &str,
    transform: &AstTransform,
    diagnostic_title: &str,
    uri: &Uri,
    loc: Loc,
) -> Option<CodeActionOrCommand> {
    let src = loc.source.dupe();
    let ast_src = &ast.loc.source;
    let (ast, uri) = if *ast_src != src {
        {
            let source_file = src?;
            match get_ast_from_heap(&source_file) {
                None => (ast.clone(), uri.clone()),
                Some(upstream_ast) => {
                    let file_path = source_file.to_path_buf();
                    let file_uri =
                        lsp_helpers::path_to_lsp_uri(file_path.to_string_lossy().as_ref(), "");
                    (upstream_ast, file_uri)
                }
            }
        }
    } else {
        (ast.clone(), uri.clone())
    };
    let new_ast = transform(cx, file_sig, &ast, typed_ast, loc)?;
    let diff = flow_ast_differ::program(&ast, &new_ast);
    let opts = code_action_utils::layout_options(options);
    let patch = replacement_printer::mk_loc_patch_ast_differ(&opts, &diff);
    let edits = flow_loc_patch_to_lsp_edits(&patch);
    match edits.as_slice() {
        [] => None,
        _ => Some(CodeActionOrCommand::CodeAction(CodeAction {
            title: title.to_string(),
            kind: Some(CodeActionKind::QUICKFIX),
            diagnostics: Some(diagnostics.to_vec()),
            edit: Some(WorkspaceEdit {
                changes: Some(HashMap::from([(uri, edits)])),
                ..Default::default()
            }),
            command: Some(mk_log_command(title, diagnostic_title)),
            is_preferred: None,
            disabled: None,
            data: None,
        })),
    }
}

fn loc_opt_intersects(loc: Option<Loc>, error_loc: Loc) -> bool {
    match loc {
        None => true,
        Some(loc) => Loc::intersects(&error_loc, &loc),
    }
}

pub fn ast_transforms_of_error(
    loc_of_aloc: Arc<dyn Fn(&ALoc) -> Loc>,
    lazy_error_loc: Option<Loc>,
    get_ast_from_heap: Arc<dyn Fn(&FileKey) -> Option<ast::Program<Loc, Loc>>>,
    get_haste_module_info: Arc<dyn Fn(&FileKey) -> Option<flow_common_modulename::HasteModuleInfo>>,
    get_type_sig: Arc<
        dyn Fn(
            &FileKey,
        ) -> Option<
            flow_type_sig::packed_type_sig::Module<
                flow_type_sig::compact_table::Index<flow_aloc::ALoc>,
            >,
        >,
    >,
    loc: Option<Loc>,
    error_message: &ErrorMessage<Loc>,
) -> Vec<AstTransformOfError> {
    let optional_chaining_actions = |error_loc: Loc, reason_desc: &VirtualReasonDesc<Loc>| match (
        loc_opt_intersects(loc.dupe(), error_loc.dupe()),
        reason_desc,
    ) {
        (
            true,
            r @ (VirtualReasonDesc::RVoid
            | VirtualReasonDesc::RTsVoid
            | VirtualReasonDesc::RNull
            | VirtualReasonDesc::RVoidedNull
            | VirtualReasonDesc::RNullOrVoid),
        ) => {
            let desc_str = flow_common::reason::string_of_desc::<Loc>(r);
            let title = format!(
                "Add optional chaining for object that might be `{}`",
                desc_str
            );
            vec![AstTransformOfError {
                title,
                diagnostic_title: "add_optional_chaining".to_string(),
                transform: untyped_ast_transform(Box::new(|ast, loc| {
                    autofix_optional_chaining::add_optional_chaining(ast, loc)
                })),
                target_loc: error_loc,
                confidence: QuickfixConfidence::BestEffort,
            }]
        }
        _ => vec![],
    };
    let invariant_subtyping_actions = |lower_loc: Loc,
                                       upper_loc: Loc,
                                       upper_ty: &flow_common_ty::ty::ALocTy,
                                       lazy_error_loc: &Option<Loc>,
                                       loc: Option<Loc>|
     -> Vec<AstTransformOfError> {
        let error_loc_opt = match lazy_error_loc {
            Some(error_loc) if loc_opt_intersects(loc.dupe(), error_loc.dupe()) => {
                Some(error_loc.dupe())
            }
            _ => {
                if loc_opt_intersects(loc.dupe(), lower_loc.dupe()) {
                    Some(lower_loc.dupe())
                } else {
                    None
                }
            }
        };
        let make_readonly_code_action =
            |upper_ty: &flow_common_ty::ty::Ty<ALoc>, upper_loc: Loc| -> Vec<AstTransformOfError> {
                let transform: AstTransform = Box::new(
                    |_cx: &mut Context, _file_sig: Arc<FileSig>, ast, _typed_ast, loc| {
                        convert_type_to_readonly_form::convert(ast, loc).map(|(ast, _kind)| ast)
                    },
                );
                let opts = PrinterOptions::default();
                vec![AstTransformOfError {
                    title: format!(
                        "Make `{}` readonly",
                        ty_printer::string_of_t_single_line(upper_ty, &opts)
                    ),
                    diagnostic_title: "fix_invariant_subtyping_error_with_readonly_conversion"
                        .to_string(),
                    transform,
                    target_loc: upper_loc,
                    confidence: QuickfixConfidence::BestEffort,
                }]
            };
        match error_loc_opt {
            None => vec![],
            Some(_) => {
                let upper_ty = ty_utils::simplify_type(true, None, upper_ty.dupe());
                let lower_loc_clone = lower_loc.dupe();
                let upper_ty_clone = upper_ty.dupe();
                let loc_of_aloc_clone = loc_of_aloc.dupe();
                let get_ast_from_heap_clone = get_ast_from_heap.dupe();
                let get_haste_module_info_clone = get_haste_module_info.dupe();
                let get_type_sig_clone = get_type_sig.dupe();
                let transform: AstTransform = Box::new(
                    move |cx: &mut Context,
                          file_sig: Arc<FileSig>,
                          ast: &ast::Program<Loc, Loc>,
                          typed_ast: &ast::Program<ALoc, (ALoc, Type)>,
                          _loc| {
                        let result = insert_type::insert_type_ty(
                            cx,
                            &*loc_of_aloc_clone,
                            &*get_ast_from_heap_clone,
                            &*get_haste_module_info_clone,
                            &*get_type_sig_clone,
                            &file_sig,
                            typed_ast,
                            false,
                            None, // no remote_converter
                            ast,
                            lower_loc_clone.dupe(),
                            upper_ty_clone.dupe(),
                        );
                        result.ok()
                    },
                );
                let mut actions = vec![AstTransformOfError {
                    title: "Add suggested annotation to the literal".to_string(),
                    diagnostic_title: "fix_invariant_subtyping_error_with_annot".to_string(),
                    transform,
                    target_loc: lower_loc,
                    confidence: QuickfixConfidence::BestEffort,
                }];
                actions.extend(make_readonly_code_action(&upper_ty, upper_loc));
                actions
            }
        }
    };
    match error_message {
        ErrorMessage::EDeprecatedBool(error_loc) => {
            if loc_opt_intersects(loc, error_loc.dupe()) {
                vec![AstTransformOfError {
                    title: "Replace `bool` with `boolean`".to_string(),
                    diagnostic_title: "replace_bool".to_string(),
                    transform: untyped_ast_transform(Box::new(|ast, loc| {
                        autofix_replace_type::replace_type(
                            &|t: &TypeInner<Loc, Loc>| match t {
                                TypeInner::Boolean {
                                    raw: _,
                                    comments,
                                    loc,
                                } => TypeInner::Boolean {
                                    loc: loc.dupe(),
                                    raw: ast::types::BooleanRaw::Boolean,
                                    comments: comments.clone(),
                                },
                                unexpected => unexpected.clone(),
                            },
                            ast,
                            loc,
                        )
                    })),
                    target_loc: error_loc.dupe(),
                    confidence: QuickfixConfidence::WillFixErrorAndSafeForRunningOnSave,
                }]
            } else {
                vec![]
            }
        }
        ErrorMessage::EDuplicateComponentProp(box EDuplicateComponentPropData {
            spread: error_loc,
            duplicates,
        }) => {
            // Component prop keys are always string-named. A `unique symbol` key
            // has no faithful string-literal spelling for the `Omit<...>`
            // annotation, so offer the quickfix only when every duplicate is a
            // string key; otherwise the `Omit` could not fully remove them.
            let string_duplicates: Option<Vec<(Loc, FlowSmolStr, Loc)>> = duplicates
                .iter()
                .map(|(l1, x, l2)| {
                    x.as_smol_str_opt()
                        .map(|s| (l1.dupe(), s.dupe(), l2.dupe()))
                })
                .collect();
            if loc_opt_intersects(loc, error_loc.dupe())
                && let Some(duplicates) = string_duplicates
            {
                vec![AstTransformOfError {
                    title: "Wrap spread prop with Omit".to_string(),
                    diagnostic_title: "wrap_spread_prop_with_omit".to_string(),
                    transform: untyped_ast_transform(Box::new(move |ast, loc| {
                        use std::sync::Arc;
                        let duplicates = duplicates.clone();
                        autofix_replace_type::replace_type(
                            &move |t: &TypeInner<Loc, Loc>| {
                                let string_lit_annot_of_duplicate =
                                    |(_loc, n, _loc2): &(Loc, FlowSmolStr, Loc)| -> ast::types::Type<Loc, Loc> {
                                        let n = n.to_string();
                                        ast::types::Type::new(TypeInner::StringLiteral {
                                            loc: LOC_NONE,
                                            literal: ast::StringLiteral {
                                                value: n.clone().into(),
                                                raw: format!("\"{}\"", n).into(),
                                                comments: None,
                                            },
                                        })
                                    };
                                let n1 = &duplicates[0];
                                let rest = &duplicates[1..];
                                let omitted_keys_annot = if rest.is_empty() {
                                    string_lit_annot_of_duplicate(n1)
                                } else {
                                    let first = string_lit_annot_of_duplicate(n1);
                                    let second = string_lit_annot_of_duplicate(&rest[0]);
                                    let remaining: Vec<_> = rest[1..]
                                        .iter()
                                        .map(string_lit_annot_of_duplicate)
                                        .collect();
                                    ast::types::Type::new(TypeInner::Union {
                                        loc: LOC_NONE,
                                        inner: Arc::new(ast::types::Union {
                                            types: (first, second, remaining),
                                            comments: None,
                                        }),
                                    })
                                };
                                TypeInner::Generic {
                                    loc: LOC_NONE,
                                    inner: Arc::new(ast::types::Generic {
                                        id: ast::types::generic::Identifier::Unqualified(
                                            ast::Identifier(Arc::new(ast::IdentifierInner {
                                                loc: LOC_NONE,
                                                name: "Omit".into(),
                                                comments: None,
                                            })),
                                        ),
                                        targs: Some(ast::types::TypeArgs {
                                            loc: LOC_NONE,
                                            arguments: Arc::from(vec![
                                                ast::types::Type::new(t.clone()),
                                                omitted_keys_annot,
                                            ]),
                                            comments: None,
                                        }),
                                        comments: None,
                                    }),
                                }
                            },
                            ast,
                            loc,
                        )
                    })),
                    target_loc: error_loc.dupe(),
                    confidence: QuickfixConfidence::WillFixErrorAndSafeForRunningOnSave,
                }]
            } else {
                vec![]
            }
        }
        ErrorMessage::EEnumError(EnumErrorKind::EnumInvalidMemberAccess(
            box EnumInvalidMemberAccessData {
                member_loc: reason_loc,
                member:
                    EnumMemberAccess::Named {
                        name: member_name,
                        suggestion: Some(fixed_prop_name),
                    },
                ..
            },
        )) => {
            if loc_opt_intersects(loc, reason_loc.dupe()) {
                let original_prop_name = member_name.display_smol_str();
                let title = format!(
                    "Replace `{}` with `{}`",
                    original_prop_name, fixed_prop_name
                );
                let fixed_prop_name: FlowSmolStr = fixed_prop_name.as_str().into();
                vec![AstTransformOfError {
                    title,
                    diagnostic_title: "replace_enum_prop_typo_at_target".to_string(),
                    transform: untyped_ast_transform(Box::new(move |ast, loc| {
                        autofix_prop_typo::replace_prop_typo_at_target(
                            fixed_prop_name.dupe(),
                            ast,
                            loc,
                        )
                    })),
                    target_loc: reason_loc.dupe(),
                    confidence: QuickfixConfidence::BestEffort,
                }]
            } else {
                vec![]
            }
        }
        ErrorMessage::EEnumError(EnumErrorKind::EnumInvalidMemberName(
            box EnumInvalidMemberNameData {
                loc: error_loc,
                member_name,
                ..
            },
        )) => {
            if loc_opt_intersects(loc, error_loc.dupe()) {
                let mut chars = member_name.chars();
                let fixed_name = match chars.next() {
                    None => String::new(),
                    Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
                };
                let title = format!("Replace `{}` with `{}`", member_name, fixed_name);
                let fixed_name: FlowSmolStr = fixed_name.into();
                vec![AstTransformOfError {
                    title,
                    diagnostic_title: "capitalize_enum_member".to_string(),
                    transform: untyped_ast_transform(Box::new(move |ast, loc| {
                        autofix_enum_member_name::capitalize_at_target(fixed_name.dupe(), ast, loc)
                    })),
                    target_loc: error_loc.dupe(),
                    confidence: QuickfixConfidence::WillFixErrorAndSafeForRunningOnSave,
                }]
            } else {
                vec![]
            }
        }
        ErrorMessage::EClassToObject(box EClassToObjectData { lower, upper, .. }) => {
            let error_loc = lower.loc.dupe();
            if loc_opt_intersects(loc, error_loc.dupe()) {
                let obj_loc = upper.definition_loc.dupe();
                let original = match &upper.type_desc {
                    TypeOrTypeDescT::Type(t) => reason_of_t(t)
                        .desc
                        .as_ref()
                        .map(flow_common::reason::string_of_desc)
                        .unwrap_or_default(),
                    TypeOrTypeDescT::TypeDesc(ErrorTypeDesc::ReasonDesc(desc)) => {
                        flow_common::reason::string_of_desc::<Loc>(desc)
                    }
                    TypeOrTypeDescT::TypeDesc(ErrorTypeDesc::ObjectLiteral) => {
                        "object literal".to_string()
                    }
                    TypeOrTypeDescT::TypeDesc(ErrorTypeDesc::ArrayLiteral) => {
                        "array literal".to_string()
                    }
                    TypeOrTypeDescT::TypeDesc(ErrorTypeDesc::Normalized(Elt::Type(ty)))
                        if matches!(ty.as_ref(), Ty::Obj(_)) =>
                    {
                        "object type".to_string()
                    }
                    TypeOrTypeDescT::TypeDesc(ErrorTypeDesc::Normalized(elt)) => format!(
                        "`{}`",
                        ty_printer::string_of_elt_single_line(elt, &PrinterOptions::default())
                    ),
                    TypeOrTypeDescT::TypeDesc(ErrorTypeDesc::Unknown) => "`?`".to_string(),
                };
                let title = format!("Rewrite {} as an interface", original);
                vec![AstTransformOfError {
                    title,
                    diagnostic_title: "replace_obj_with_interface".to_string(),
                    transform: untyped_ast_transform(Box::new(|ast, loc| {
                        autofix_interface::replace_object_at_target(ast, loc)
                    })),
                    target_loc: obj_loc,
                    confidence: QuickfixConfidence::BestEffort,
                }]
            } else {
                vec![]
            }
        }
        ErrorMessage::EUnusedPromise {
            loc: error_loc,
            async_,
        } => {
            if loc_opt_intersects(loc, error_loc.dupe()) {
                let insert_async = AstTransformOfError {
                    title: "Insert `await`".to_string(),
                    diagnostic_title: "insert_await".to_string(),
                    transform: untyped_ast_transform(Box::new(|ast, loc| {
                        autofix_unused_promise::insert_await(ast, loc)
                    })),
                    target_loc: error_loc.dupe(),
                    confidence: QuickfixConfidence::BestEffort,
                };
                let insert_void = AstTransformOfError {
                    title: "Insert `void`".to_string(),
                    diagnostic_title: "insert_void".to_string(),
                    transform: untyped_ast_transform(Box::new(|ast, loc| {
                        autofix_unused_promise::insert_void(ast, loc)
                    })),
                    target_loc: error_loc.dupe(),
                    confidence: QuickfixConfidence::BestEffort,
                };
                if *async_ {
                    vec![insert_async, insert_void]
                } else {
                    vec![insert_void]
                }
            } else {
                vec![]
            }
        }
        ErrorMessage::ETSSyntax(box ETSSyntaxData {
            kind: TSSyntaxKind::TSNever,
            loc: error_loc,
        }) => {
            if loc_opt_intersects(loc, error_loc.dupe()) {
                vec![AstTransformOfError {
                    title: "Convert to `empty`".to_string(),
                    diagnostic_title: "convert_never_type".to_string(),
                    transform: untyped_ast_transform(Box::new(|ast, loc| {
                        autofix_ts_syntax::convert_never_type(ast, loc)
                    })),
                    target_loc: error_loc.dupe(),
                    confidence: QuickfixConfidence::WillFixErrorAndSafeForRunningOnSave,
                }]
            } else {
                vec![]
            }
        }
        ErrorMessage::ETSSyntax(box ETSSyntaxData {
            kind: TSSyntaxKind::TSUndefined,
            loc: error_loc,
        }) => {
            if loc_opt_intersects(loc, error_loc.dupe()) {
                vec![AstTransformOfError {
                    title: "Convert to `void`".to_string(),
                    diagnostic_title: "convert_undefined_type".to_string(),
                    transform: untyped_ast_transform(Box::new(|ast, loc| {
                        autofix_ts_syntax::convert_undefined_type(ast, loc)
                    })),
                    target_loc: error_loc.dupe(),
                    confidence: QuickfixConfidence::WillFixErrorAndSafeForRunningOnSave,
                }]
            } else {
                vec![]
            }
        }
        ErrorMessage::ETSSyntax(box ETSSyntaxData {
            kind: TSSyntaxKind::DeprecatedTypeParamColon,
            loc: error_loc,
        }) => {
            if loc_opt_intersects(loc, error_loc.dupe()) {
                vec![AstTransformOfError {
                    title: "Convert to `extends T`".to_string(),
                    diagnostic_title: "convert_type_param_colon".to_string(),
                    transform: untyped_ast_transform(Box::new(|ast, loc| {
                        autofix_ts_syntax::convert_type_param_colon(ast, loc)
                    })),
                    target_loc: error_loc.dupe(),
                    confidence: QuickfixConfidence::WillFixErrorAndSafeForRunningOnSave,
                }]
            } else {
                vec![]
            }
        }
        ErrorMessage::ETSSyntax(box ETSSyntaxData {
            kind: TSSyntaxKind::TSInOutVariance,
            loc: error_loc,
        }) => {
            if loc_opt_intersects(loc, error_loc.dupe()) {
                vec![AstTransformOfError {
                    title: "Remove".to_string(),
                    diagnostic_title: "remove_in_out_variance".to_string(),
                    transform: untyped_ast_transform(Box::new(|ast, loc| {
                        autofix_ts_syntax::remove_in_out_variance(ast, loc)
                    })),
                    target_loc: error_loc.dupe(),
                    confidence: QuickfixConfidence::WillFixErrorAndSafeForRunningOnSave,
                }]
            } else {
                vec![]
            }
        }
        ErrorMessage::EVarianceKeyword(box EVarianceKeywordData {
            kind: VarianceKeywordKind::Plus(VarianceSigilParent::Property),
            loc: error_loc,
        }) => {
            if loc_opt_intersects(loc, error_loc.dupe()) {
                vec![AstTransformOfError {
                    title: "Convert to `readonly`".to_string(),
                    diagnostic_title: "convert_plus_sigil_to_readonly".to_string(),
                    transform: untyped_ast_transform(Box::new(|ast, loc| {
                        autofix_ts_syntax::convert_plus_sigil_to_readonly(ast, loc)
                    })),
                    target_loc: error_loc.dupe(),
                    confidence: QuickfixConfidence::WillFixErrorAndSafeForRunningOnSave,
                }]
            } else {
                vec![]
            }
        }
        ErrorMessage::EVarianceKeyword(box EVarianceKeywordData {
            kind: VarianceKeywordKind::Plus(VarianceSigilParent::TypeParam),
            loc: error_loc,
        }) => {
            if loc_opt_intersects(loc, error_loc.dupe()) {
                vec![AstTransformOfError {
                    title: "Convert to `out`".to_string(),
                    diagnostic_title: "convert_plus_sigil_to_out".to_string(),
                    transform: untyped_ast_transform(Box::new(|ast, loc| {
                        autofix_ts_syntax::convert_plus_sigil_to_out(ast, loc)
                    })),
                    target_loc: error_loc.dupe(),
                    confidence: QuickfixConfidence::WillFixErrorAndSafeForRunningOnSave,
                }]
            } else {
                vec![]
            }
        }
        ErrorMessage::EVarianceKeyword(box EVarianceKeywordData {
            kind: VarianceKeywordKind::Minus(VarianceSigilParent::Property),
            loc: error_loc,
        }) => {
            if loc_opt_intersects(loc, error_loc.dupe()) {
                vec![AstTransformOfError {
                    title: "Convert to `writeonly`".to_string(),
                    diagnostic_title: "convert_minus_sigil_to_writeonly".to_string(),
                    transform: untyped_ast_transform(Box::new(|ast, loc| {
                        autofix_ts_syntax::convert_minus_sigil_to_writeonly(ast, loc)
                    })),
                    target_loc: error_loc.dupe(),
                    confidence: QuickfixConfidence::WillFixErrorAndSafeForRunningOnSave,
                }]
            } else {
                vec![]
            }
        }
        ErrorMessage::EVarianceKeyword(box EVarianceKeywordData {
            kind: VarianceKeywordKind::Minus(VarianceSigilParent::TypeParam),
            loc: error_loc,
        }) => {
            if loc_opt_intersects(loc, error_loc.dupe()) {
                vec![AstTransformOfError {
                    title: "Convert to `in`".to_string(),
                    diagnostic_title: "convert_minus_sigil_to_in".to_string(),
                    transform: untyped_ast_transform(Box::new(|ast, loc| {
                        autofix_ts_syntax::convert_minus_sigil_to_in(ast, loc)
                    })),
                    target_loc: error_loc.dupe(),
                    confidence: QuickfixConfidence::WillFixErrorAndSafeForRunningOnSave,
                }]
            } else {
                vec![]
            }
        }
        ErrorMessage::ETSSyntax(box ETSSyntaxData {
            kind: TSSyntaxKind::TSReadonlyType(Some(ReadonlyTypeKind::Array)),
            loc: error_loc,
        }) => {
            if loc_opt_intersects(loc, error_loc.dupe()) {
                vec![AstTransformOfError {
                    title: "Convert to `ReadonlyArray`".to_string(),
                    diagnostic_title: "convert_readonly_array_type".to_string(),
                    transform: untyped_ast_transform(Box::new(|ast, loc| {
                        autofix_ts_syntax::convert_readonly_array_type(ast, loc)
                    })),
                    target_loc: error_loc.dupe(),
                    confidence: QuickfixConfidence::WillFixErrorAndSafeForRunningOnSave,
                }]
            } else {
                vec![]
            }
        }
        ErrorMessage::ETSSyntax(box ETSSyntaxData {
            kind: TSSyntaxKind::TSReadonlyType(Some(ReadonlyTypeKind::Tuple)),
            loc: error_loc,
        }) => {
            if loc_opt_intersects(loc, error_loc.dupe()) {
                vec![AstTransformOfError {
                    title: "Convert to `Readonly`".to_string(),
                    diagnostic_title: "convert_readonly_tuple_type".to_string(),
                    transform: untyped_ast_transform(Box::new(|ast, loc| {
                        autofix_ts_syntax::convert_readonly_tuple_type(ast, loc)
                    })),
                    target_loc: error_loc.dupe(),
                    confidence: QuickfixConfidence::WillFixErrorAndSafeForRunningOnSave,
                }]
            } else {
                vec![]
            }
        }
        ErrorMessage::EInvalidTypeCastSyntax { loc: error_loc } => {
            if loc_opt_intersects(loc, error_loc.dupe()) {
                let title = "Convert to `as` expression `<expr> as <type>`";
                let diagnostic_title = "convert_colon_cast";
                vec![AstTransformOfError {
                    title: title.to_string(),
                    diagnostic_title: diagnostic_title.to_string(),
                    transform: untyped_ast_transform(Box::new(|ast, loc| {
                        autofix_casting_syntax::convert_colon_cast(ast, loc)
                    })),
                    target_loc: error_loc.dupe(),
                    confidence: QuickfixConfidence::WillFixErrorAndSafeForRunningOnSave,
                }]
            } else {
                vec![]
            }
        }
        ErrorMessage::EIncorrectTypeWithReplacement(box EIncorrectTypeWithReplacementData {
            kind: IncorrectType::DollarKeys,
            loc: error_loc,
        }) => {
            if loc_opt_intersects(loc, error_loc.dupe()) {
                vec![AstTransformOfError {
                    title: "Convert to `keyof`".to_string(),
                    diagnostic_title: "convert_dollar_keys_type".to_string(),
                    transform: untyped_ast_transform(Box::new(|ast, loc| {
                        autofix_legacy_flow_syntax::convert_dollar_keys_type(ast, loc)
                    })),
                    target_loc: error_loc.dupe(),
                    confidence: QuickfixConfidence::WillFixErrorAndSafeForRunningOnSave,
                }]
            } else {
                vec![]
            }
        }
        ErrorMessage::EIncorrectTypeWithReplacement(box EIncorrectTypeWithReplacementData {
            kind,
            loc: error_loc,
        }) => {
            let incorrect_name = kind.incorrect_of_kind();
            let replacement_name = kind.replacement_of_kind();
            let title = format!("Convert to `{}`", replacement_name);
            let diagnostic_title = format!("convert_{}_type", incorrect_name);
            if loc_opt_intersects(loc, error_loc.dupe()) {
                let kind = *kind;
                vec![AstTransformOfError {
                    title,
                    diagnostic_title,
                    transform: untyped_ast_transform(Box::new(move |ast, loc| {
                        autofix_type_name::convert_incorrect_type(kind, ast, loc)
                    })),
                    target_loc: error_loc.dupe(),
                    confidence: QuickfixConfidence::WillFixErrorAndSafeForRunningOnSave,
                }]
            } else {
                vec![]
            }
        }
        ErrorMessage::EInternalType(
            error_loc,
            InternalType::DollarUtilityTypeWithNonDollarAliases(replacement_name),
        ) => {
            let incorrect_name = format!("${}", replacement_name);
            let title = format!("Convert to `{}`", replacement_name);
            let diagnostic_title = format!("convert_dollar_{}_type", replacement_name);
            if loc_opt_intersects(loc, error_loc.dupe()) {
                let incorrect_name: FlowSmolStr = incorrect_name.into();
                let replacement_name: FlowSmolStr = replacement_name.as_str().into();
                vec![AstTransformOfError {
                    title,
                    diagnostic_title,
                    transform: untyped_ast_transform(Box::new(move |ast, loc| {
                        autofix_type_name::convert_type(
                            incorrect_name.dupe(),
                            replacement_name.dupe(),
                            ast,
                            loc,
                        )
                    })),
                    target_loc: error_loc.dupe(),
                    confidence: QuickfixConfidence::WillFixErrorAndSafeForRunningOnSave,
                }]
            } else {
                vec![]
            }
        }
        ErrorMessage::EInternalType(
            error_loc,
            InternalType::ReactDollarUtilityTypesWithNonDollarAliases(
                replacement_name_without_react,
            ),
        ) => {
            let incorrect_name = format!("React${}", replacement_name_without_react);
            let replacement_name = format!("React.{}", replacement_name_without_react);
            let title = format!("Convert to `{}`", replacement_name);
            let diagnostic_title = format!(
                "convert_react_dollar_{}_type",
                replacement_name_without_react
            );
            if loc_opt_intersects(loc, error_loc.dupe()) {
                let incorrect_name: FlowSmolStr = incorrect_name.into();
                let replacement_name: FlowSmolStr = replacement_name.into();
                vec![AstTransformOfError {
                    title,
                    diagnostic_title,
                    transform: untyped_ast_transform(Box::new(move |ast, loc| {
                        autofix_type_name::convert_type(
                            incorrect_name.dupe(),
                            replacement_name.dupe(),
                            ast,
                            loc,
                        )
                    })),
                    target_loc: error_loc.dupe(),
                    confidence: QuickfixConfidence::WillFixErrorAndSafeForRunningOnSave,
                }]
            } else {
                vec![]
            }
        }
        ErrorMessage::EInvalidRendersTypeArgument(box EInvalidRendersTypeArgumentData {
            loc: error_loc,
            renders_variant,
            invalid_render_type_kind,
            ..
        }) => {
            if loc_opt_intersects(loc, error_loc.dupe()) {
                match (renders_variant, invalid_render_type_kind) {
                    (
                        ast::types::RendersVariant::Star,
                        InvalidRenderTypeKind::InvalidRendersNullVoidFalse
                        | InvalidRenderTypeKind::InvalidRendersIterable,
                    ) => vec![AstTransformOfError {
                        title: "Simplify `renders*`".to_string(),
                        diagnostic_title: "simplify_renders_star".to_string(),
                        transform: untyped_ast_transform(Box::new(|ast, loc| {
                            autofix_renders_variant::to_renders_star_with_best_effort_fixes(
                                ast, loc,
                            )
                        })),
                        target_loc: error_loc.dupe(),
                        confidence: QuickfixConfidence::WillFixErrorAndSafeForRunningOnSave,
                    }],
                    (
                        ast::types::RendersVariant::Maybe,
                        InvalidRenderTypeKind::InvalidRendersNullVoidFalse,
                    ) => vec![AstTransformOfError {
                        title: "Simplify `renders?`".to_string(),
                        diagnostic_title: "simplify_renders_maybe".to_string(),
                        transform: untyped_ast_transform(Box::new(|ast, loc| {
                            autofix_renders_variant::to_renders_maybe_with_best_effort_fixes(
                                ast, loc,
                            )
                        })),
                        target_loc: error_loc.dupe(),
                        confidence: QuickfixConfidence::WillFixErrorAndSafeForRunningOnSave,
                    }],
                    (_, InvalidRenderTypeKind::InvalidRendersNullVoidFalse) => {
                        vec![AstTransformOfError {
                            title: "Switch to `renders?`".to_string(),
                            diagnostic_title: "switch_to_renders_maybe".to_string(),
                            transform: untyped_ast_transform(Box::new(|ast, loc| {
                                autofix_renders_variant::to_renders_maybe_with_best_effort_fixes(
                                    ast, loc,
                                )
                            })),
                            target_loc: error_loc.dupe(),
                            confidence: QuickfixConfidence::WillFixErrorAndSafeForRunningOnSave,
                        }]
                    }
                    (_, InvalidRenderTypeKind::InvalidRendersIterable) => {
                        vec![AstTransformOfError {
                            title: "Switch to `renders*`".to_string(),
                            diagnostic_title: "switch_to_renders_star".to_string(),
                            transform: untyped_ast_transform(Box::new(|ast, loc| {
                                autofix_renders_variant::to_renders_star_with_best_effort_fixes(
                                    ast, loc,
                                )
                            })),
                            target_loc: error_loc.dupe(),
                            confidence: QuickfixConfidence::WillFixErrorAndSafeForRunningOnSave,
                        }]
                    }
                    _ => vec![],
                }
            } else {
                vec![]
            }
        }
        ErrorMessage::EBuiltinNameLookupFailed(box EBuiltinNameLookupFailedData {
            loc: error_loc,
            name,
        }) => {
            if loc_opt_intersects(loc, error_loc.dupe()) {
                let name = name.to_string();
                let loc_of_aloc = loc_of_aloc.dupe();
                vec![AstTransformOfError {
                    title: "Prefix with `this.`".to_string(),
                    diagnostic_title: "prefix_with_this".to_string(),
                    transform: Box::new(move |cx, file_sig: Arc<FileSig>, ast, typed_ast, loc| {
                        autofix_class_member_access::fix(
                            cx,
                            file_sig,
                            &*loc_of_aloc,
                            ast,
                            typed_ast,
                            &name,
                            loc,
                        )
                    }),
                    target_loc: error_loc.dupe(),
                    confidence: QuickfixConfidence::BestEffort,
                }]
            } else {
                vec![]
            }
        }
        ErrorMessage::EBindingError(box (
            BindingError::ETypeInValuePosition { name, .. },
            error_loc,
            _,
            _,
        )) => {
            if loc_opt_intersects(loc, error_loc.dupe()) {
                let name = name.to_string();
                vec![AstTransformOfError {
                    title: format!("Convert type import of `{}` to value import", name),
                    diagnostic_title: "convert_type_to_value_import".to_string(),
                    transform: Box::new(|_cx, file_sig, ast, _typed_ast, loc| {
                        autofix_type_to_value_import::convert_type_to_value_import(
                            &file_sig, ast, loc,
                        )
                    }),
                    target_loc: error_loc.dupe(),
                    confidence: QuickfixConfidence::BestEffort,
                }]
            } else {
                vec![]
            }
        }
        ErrorMessage::EMatchError(MatchErrorKind::MatchInvalidObjectShorthand(
            box MatchInvalidObjectShorthandData {
                loc: error_loc,
                name,
                pattern_kind: _,
            },
        )) => {
            if loc_opt_intersects(loc, error_loc.dupe()) {
                vec![
                    AstTransformOfError {
                        title: format!("Convert to `const {}`", name),
                        diagnostic_title: "convert_match_object_shorthand_to_const".to_string(),
                        transform: untyped_ast_transform(Box::new(|ast, loc| {
                            autofix_match_syntax::convert_object_shorthand_to_const(ast, loc)
                        })),
                        target_loc: error_loc.dupe(),
                        confidence: QuickfixConfidence::BestEffort,
                    },
                    AstTransformOfError {
                        title: format!("Convert to `{}: {}`", name, name),
                        diagnostic_title: "convert_match_object_shorthand_to_reference".to_string(),
                        transform: untyped_ast_transform(Box::new(|ast, loc| {
                            autofix_match_syntax::convert_object_shorthand_to_reference(ast, loc)
                        })),
                        target_loc: error_loc.dupe(),
                        confidence: QuickfixConfidence::BestEffort,
                    },
                ]
            } else {
                vec![]
            }
        }
        ErrorMessage::EMatchError(MatchErrorKind::MatchStatementInvalidBody { loc: error_loc }) => {
            if loc_opt_intersects(loc, error_loc.dupe()) {
                vec![AstTransformOfError {
                    title: "Wrap in a block".to_string(),
                    diagnostic_title: "fix_invalid_match_statement_body".to_string(),
                    transform: untyped_ast_transform(Box::new(|ast, loc| {
                        autofix_match_syntax::fix_invalid_match_statement_body(ast, loc)
                    })),
                    target_loc: error_loc.dupe(),
                    confidence: QuickfixConfidence::WillFixErrorAndSafeForRunningOnSave,
                }]
            } else {
                vec![]
            }
        }
        ErrorMessage::EMatchError(MatchErrorKind::MatchInvalidBindingKind {
            loc: error_loc,
            kind: current_kind,
        }) => {
            if loc_opt_intersects(loc, error_loc.dupe()) {
                let kind_str = flow_parser::ast_utils::string_of_variable_kind(*current_kind);
                vec![AstTransformOfError {
                    title: format!("Replace `{}` with `const`", kind_str),
                    diagnostic_title: "fix_match_invalid_binding_kind".to_string(),
                    transform: untyped_ast_transform(Box::new(|ast, loc| {
                        autofix_match_syntax::fix_invalid_binding_kind(ast, loc)
                    })),
                    target_loc: error_loc.dupe(),
                    confidence: QuickfixConfidence::WillFixErrorAndSafeForRunningOnSave,
                }]
            } else {
                vec![]
            }
        }
        ErrorMessage::EMatchError(MatchErrorKind::MatchInvalidWildcardSyntax(error_loc)) => {
            if loc_opt_intersects(loc, error_loc.dupe()) {
                vec![AstTransformOfError {
                    title: "Replace `default` with `_`".to_string(),
                    diagnostic_title: "fix_match_invalid_wildcard_syntax".to_string(),
                    transform: untyped_ast_transform(Box::new(|ast, loc| {
                        autofix_match_syntax::fix_invalid_wildcard_syntax(ast, loc)
                    })),
                    target_loc: error_loc.dupe(),
                    confidence: QuickfixConfidence::WillFixErrorAndSafeForRunningOnSave,
                }]
            } else {
                vec![]
            }
        }
        ErrorMessage::EMatchError(MatchErrorKind::MatchInvalidCaseSyntax(
            box MatchInvalidCaseSyntaxData {
                loc: error_loc,
                kind: _,
            },
        )) => {
            if loc_opt_intersects(loc, error_loc.dupe()) {
                vec![AstTransformOfError {
                    title: "Fix invalid match syntax".to_string(),
                    diagnostic_title: "fix_match_invalid_case_syntax".to_string(),
                    transform: untyped_ast_transform(Box::new(|ast, loc| {
                        autofix_match_syntax::fix_invalid_case_syntax(ast, loc)
                    })),
                    target_loc: error_loc.dupe(),
                    confidence: QuickfixConfidence::WillFixErrorAndSafeForRunningOnSave,
                }]
            } else {
                vec![]
            }
        }
        ErrorMessage::EMatchError(MatchErrorKind::MatchNonExhaustiveObjectPattern(
            box MatchNonExhaustiveObjectPatternData {
                loc: error_loc,
                rest,
                missing_props,
                pattern_kind: _,
            },
        )) => {
            if loc_opt_intersects(loc, error_loc.dupe()) {
                let add_only_rest = AstTransformOfError {
                    title: "Add rest `...` to object pattern".to_string(),
                    diagnostic_title: "fix_match_non_exhaustive_object_pattern_add_rest"
                        .to_string(),
                    transform: untyped_ast_transform(Box::new(|ast, loc| {
                        autofix_match_syntax::fix_non_exhaustive_object_pattern(
                            true,
                            vec![],
                            ast,
                            loc,
                        )
                    })),
                    target_loc: error_loc.dupe(),
                    confidence: QuickfixConfidence::BestEffort,
                };
                let missing_props_prefix_text = |props: &[_]| {
                    if props.len() == 1 {
                        "Add the missing property".to_string()
                    } else {
                        format!("Add {} missing properties", props.len())
                    }
                };
                match (missing_props.as_slice(), rest) {
                    ([], _) => vec![add_only_rest],
                    (missing_props_slice, None) if !missing_props_slice.is_empty() => {
                        let missing_props_clone: Vec<_> =
                            missing_props_slice.iter().duped().collect();
                        vec![
                            AstTransformOfError {
                                title: format!(
                                    "{} to object pattern",
                                    missing_props_prefix_text(missing_props_slice)
                                ),
                                diagnostic_title:
                                    "fix_match_non_exhaustive_object_pattern_add_props".to_string(),
                                transform: untyped_ast_transform(Box::new(move |ast, loc| {
                                    let props: Vec<String> =
                                        missing_props_clone.iter().map(|s| s.to_string()).collect();
                                    autofix_match_syntax::fix_non_exhaustive_object_pattern(
                                        false, props, ast, loc,
                                    )
                                })),
                                target_loc: error_loc.dupe(),
                                confidence: QuickfixConfidence::BestEffort,
                            },
                            add_only_rest,
                        ]
                    }
                    (missing_props_slice, Some(_)) if !missing_props_slice.is_empty() => {
                        let missing_props_clone: Vec<_> =
                            missing_props_slice.iter().duped().collect();
                        vec![
                            AstTransformOfError {
                                title: format!(
                                    "{} and rest `...` to object pattern",
                                    missing_props_prefix_text(missing_props_slice)
                                ),
                                diagnostic_title:
                                    "fix_match_non_exhaustive_object_pattern_add_props_and_rest"
                                        .to_string(),
                                transform: untyped_ast_transform(Box::new(move |ast, loc| {
                                    let props: Vec<String> =
                                        missing_props_clone.iter().map(|s| s.to_string()).collect();
                                    autofix_match_syntax::fix_non_exhaustive_object_pattern(
                                        true, props, ast, loc,
                                    )
                                })),
                                target_loc: error_loc.dupe(),
                                confidence: QuickfixConfidence::BestEffort,
                            },
                            add_only_rest,
                        ]
                    }
                    _ => vec![add_only_rest],
                }
            } else {
                vec![]
            }
        }
        ErrorMessage::EMatchError(MatchErrorKind::MatchNotExhaustive(
            box MatchNotExhaustiveData {
                loc: error_loc,
                examples,
                missing_pattern_asts,
            },
        )) => {
            if loc_opt_intersects(loc, error_loc.dupe()) {
                let num_examples = examples.len();
                let num_asts = missing_pattern_asts.len();
                let prefix = if num_asts == 1 {
                    "Add the missing case".to_string()
                } else {
                    format!("Add {} missing cases", num_asts)
                };
                let suffix = if num_asts == num_examples {
                    " to make `match` exhaustively checked".to_string()
                } else {
                    format!(" (out of a total of {})", num_examples)
                };
                let missing_pattern_asts = missing_pattern_asts.clone();
                vec![AstTransformOfError {
                    title: format!("{}{}", prefix, suffix),
                    diagnostic_title: "fix_match_not_exhaustive".to_string(),
                    transform: untyped_ast_transform(Box::new(move |ast, loc| {
                        autofix_match_syntax::fix_not_exhaustive(
                            missing_pattern_asts.clone(),
                            ast,
                            loc,
                        )
                    })),
                    target_loc: error_loc.dupe(),
                    confidence: QuickfixConfidence::BestEffort,
                }]
            } else {
                vec![]
            }
        }
        ErrorMessage::EMatchError(MatchErrorKind::MatchUnusedPattern(
            box MatchUnusedPatternData {
                pattern,
                already_seen: _,
            },
        )) => {
            let error_loc = pattern.loc.dupe();
            if loc_opt_intersects(loc, error_loc.dupe()) {
                vec![AstTransformOfError {
                    title: "Remove".to_string(),
                    diagnostic_title: "fix_match_unused_pattern".to_string(),
                    transform: untyped_ast_transform(Box::new(|ast, loc| {
                        autofix_match_syntax::remove_unused_pattern(ast, loc)
                    })),
                    target_loc: error_loc,
                    confidence: QuickfixConfidence::BestEffort,
                }]
            } else {
                vec![]
            }
        }
        ErrorMessage::ERecordError(RecordErrorKind::RecordDeclarationInvalidSyntax {
            loc: error_loc,
            kind: _,
        }) => {
            if loc_opt_intersects(loc, error_loc.dupe()) {
                vec![AstTransformOfError {
                    title: "Fix invalid record syntax".to_string(),
                    diagnostic_title: "fix_record_invalid_syntax".to_string(),
                    transform: untyped_ast_transform(Box::new(|ast, loc| {
                        autofix_record_declaration::fix_invalid_syntax(ast, loc)
                    })),
                    target_loc: error_loc.dupe(),
                    confidence: QuickfixConfidence::WillFixErrorAndSafeForRunningOnSave,
                }]
            } else {
                vec![]
            }
        }
        ErrorMessage::EIncompatibleTypesWithUseOp(box EIncompatibleTypesWithUseOpData {
            lower_loc,
            explanation:
                Some(Explanation::ExplanationObjectLiteralNeedsRecordSyntax { record_name, .. }),
            ..
        }) => {
            let error_loc = lower_loc.dupe();
            if loc_opt_intersects(loc, error_loc.dupe()) {
                let record_name_str = record_name.to_string();
                let record_name_smol: FlowSmolStr = record_name.as_str().into();
                vec![AstTransformOfError {
                    title: format!("Convert to record `{} {{...}}`", record_name_str),
                    diagnostic_title: "convert_object_literal_to_record_expression".to_string(),
                    transform: untyped_ast_transform(Box::new(move |ast, loc| {
                        autofix_object_to_record::convert_object_to_record_expression(
                            record_name_smol.dupe(),
                            ast,
                            loc,
                        )
                    })),
                    target_loc: error_loc,
                    confidence: QuickfixConfidence::BestEffort,
                }]
            } else {
                vec![]
            }
        }
        ErrorMessage::ERecordError(RecordErrorKind::RecordInvalidNew {
            record_name,
            loc: error_loc,
        }) => {
            if loc_opt_intersects(loc, error_loc.dupe()) {
                let record_name = record_name.to_string();
                vec![AstTransformOfError {
                    title: format!("Convert to record `{} {{...}}`", record_name),
                    diagnostic_title: "convert_new_to_record_expression".to_string(),
                    transform: untyped_ast_transform(Box::new(|ast, loc| {
                        autofix_new_to_record::convert_new_to_record_expression(ast, loc)
                    })),
                    target_loc: error_loc.dupe(),
                    confidence: QuickfixConfidence::BestEffort,
                }]
            } else {
                vec![]
            }
        }
        ErrorMessage::EInvariantSubtypingWithUseOp(box EInvariantSubtypingWithUseOpData {
            explanation,
            ..
        }) if {
            matches!(
                explanation,
                Some(
                    ExplanationWithLazyParts::LazyExplanationInvariantSubtypingDueToMutableArray {
                        lower_array_desc: TypeOrTypeDescT::TypeDesc(
                            ErrorTypeDesc::ObjectLiteral | ErrorTypeDesc::ArrayLiteral,
                        ),
                        upper_array_desc: TypeOrTypeDescT::TypeDesc(ErrorTypeDesc::Normalized(Elt::Type(_))),
                        ..
                    }
                    | ExplanationWithLazyParts::LazyExplanationInvariantSubtypingDueToMutableProperty {
                        lower_obj_desc: TypeOrTypeDescT::TypeDesc(
                            ErrorTypeDesc::ObjectLiteral | ErrorTypeDesc::ArrayLiteral,
                        ),
                        upper_obj_desc: TypeOrTypeDescT::TypeDesc(ErrorTypeDesc::Normalized(Elt::Type(_))),
                        ..
                    }
                    | ExplanationWithLazyParts::LazyExplanationInvariantSubtypingDueToMutableProperties {
                        lower_obj_desc: TypeOrTypeDescT::TypeDesc(
                            ErrorTypeDesc::ObjectLiteral | ErrorTypeDesc::ArrayLiteral,
                        ),
                        upper_obj_desc: TypeOrTypeDescT::TypeDesc(ErrorTypeDesc::Normalized(Elt::Type(_))),
                        ..
                    }
                )
            )
        } =>
        {
            let (lower_loc, upper_loc, upper_ty) = if let Some(
                ExplanationWithLazyParts::LazyExplanationInvariantSubtypingDueToMutableArray {
                    lower_array_loc,
                    upper_array_loc,
                    upper_array_desc:
                        TypeOrTypeDescT::TypeDesc(ErrorTypeDesc::Normalized(Elt::Type(upper_ty))),
                    ..
                },
            ) = explanation.as_ref()
            {
                (
                    lower_array_loc.dupe(),
                    upper_array_loc.dupe(),
                    upper_ty.dupe(),
                )
            } else if let Some(
                ExplanationWithLazyParts::LazyExplanationInvariantSubtypingDueToMutableProperty {
                    lower_obj_loc,
                    upper_obj_loc,
                    upper_obj_desc:
                        TypeOrTypeDescT::TypeDesc(ErrorTypeDesc::Normalized(Elt::Type(upper_ty))),
                    ..
                },
            ) = explanation.as_ref()
            {
                (lower_obj_loc.dupe(), upper_obj_loc.dupe(), upper_ty.dupe())
            } else if let Some(
                ExplanationWithLazyParts::LazyExplanationInvariantSubtypingDueToMutableProperties {
                    lower_obj_loc,
                    upper_obj_loc,
                    upper_obj_desc:
                        TypeOrTypeDescT::TypeDesc(ErrorTypeDesc::Normalized(Elt::Type(upper_ty))),
                    ..
                },
            ) = explanation.as_ref()
            {
                (lower_obj_loc.dupe(), upper_obj_loc.dupe(), upper_ty.dupe())
            } else {
                return vec![];
            };
            invariant_subtyping_actions(lower_loc, upper_loc, &upper_ty, &lazy_error_loc, loc)
        }
        ErrorMessage::EPropsNotFoundInInvariantSubtyping(
            box EPropsNotFoundInInvariantSubtypingData {
                lower:
                    ErrorTypeReferenceWithLocData {
                        definition_loc: lower_obj_loc,
                        type_desc:
                            TypeOrTypeDescT::TypeDesc(
                                ErrorTypeDesc::ObjectLiteral | ErrorTypeDesc::ArrayLiteral,
                            ),
                        ..
                    },
                upper:
                    ErrorTypeReferenceWithLocData {
                        definition_loc: upper_obj_loc,
                        type_desc:
                            TypeOrTypeDescT::TypeDesc(ErrorTypeDesc::Normalized(Elt::Type(upper_ty))),
                        ..
                    },
                ..
            },
        ) => invariant_subtyping_actions(
            lower_obj_loc.dupe(),
            upper_obj_loc.dupe(),
            upper_ty,
            &lazy_error_loc,
            loc,
        ),
        ErrorMessage::EPropNotFoundInLookup(box EPropNotFoundInLookupData {
            prop_loc: error_loc,
            suggestion: Some(suggestion),
            prop_name: Some(prop_name),
            ..
        }) => {
            if loc_opt_intersects(loc, error_loc.dupe()) {
                let title = format!(
                    "Replace `{}` with `{}`",
                    prop_name.display_smol_str(),
                    suggestion
                );
                let suggestion: FlowSmolStr = suggestion.as_str().into();
                vec![AstTransformOfError {
                    title,
                    diagnostic_title: "replace_prop_typo_at_target".to_string(),
                    transform: untyped_ast_transform(Box::new(move |ast, loc| {
                        autofix_prop_typo::replace_prop_typo_at_target(suggestion.dupe(), ast, loc)
                    })),
                    target_loc: error_loc.dupe(),
                    confidence: QuickfixConfidence::BestEffort,
                }]
            } else {
                vec![]
            }
        }
        error_message => match error_message.clone().friendly_message_of_msg() {
            FriendlyMessageRecipe::IncompatibleTypeUse(box IncompatibleTypeUseData {
                loc: error_loc,
                upper_kind: UpperKind::IncompatibleGetPropT(..),
                lower_desc: ErrorTypeDesc::ReasonDesc(reason_desc),
                ..
            }) => optional_chaining_actions(error_loc, &reason_desc),
            FriendlyMessageRecipe::IncompatibleTypeUse(box IncompatibleTypeUseData {
                loc: error_loc,
                upper_kind: UpperKind::IncompatibleGetPropT(..),
                lower_desc: ErrorTypeDesc::Normalized(Elt::Type(lower_ty)),
                ..
            }) => match lower_ty.as_ref() {
                Ty::Void | Ty::Undefined => {
                    optional_chaining_actions(error_loc, &VirtualReasonDesc::RVoid)
                }
                Ty::TsVoid => optional_chaining_actions(error_loc, &VirtualReasonDesc::RTsVoid),
                Ty::Null => optional_chaining_actions(error_loc, &VirtualReasonDesc::RNull),
                _ => vec![],
            },
            _ => vec![],
        },
    }
}

fn fix_all_in_file_code_actions(
    options: &Options,
    loc_of_aloc: &dyn Fn(&ALoc) -> Loc,
    ast: &ast::Program<Loc, Loc>,
    diagnostics: &[Diagnostic],
    errors: ErrorSet,
    loc: Loc,
    uri: &Uri,
) -> Vec<CodeActionOrCommand> {
    let has_invalid_cast_at_loc = errors.exists(|error| {
        let error_message = ErrorMessage::map_loc_of_error_message(
            |aloc: ALoc| loc_of_aloc(&aloc),
            error.msg_of_error().clone(),
        );
        match &error_message {
            ErrorMessage::EInvalidTypeCastSyntax { .. } => match error_message.loc_of_msg() {
                Some(error_loc) => Loc::intersects(&error_loc, &loc),
                None => false,
            },
            _ => false,
        }
    });
    if !has_invalid_cast_at_loc {
        vec![]
    } else {
        let new_ast = autofix_casting_syntax::convert_all_colon_casts(ast);
        if new_ast == *ast {
            return vec![];
        }
        let opts = code_action_utils::layout_options(options);
        let diff = flow_ast_differ::program(ast, &new_ast);
        let edits = flow_loc_patch_to_lsp_edits(&replacement_printer::mk_loc_patch_ast_differ(
            &opts, &diff,
        ));
        let title = "Convert all colon casts to `as` expressions";
        vec![CodeActionOrCommand::CodeAction(CodeAction {
            title: title.to_string(),
            kind: Some(CodeActionKind::QUICKFIX),
            diagnostics: Some(diagnostics.to_vec()),
            edit: Some(WorkspaceEdit {
                changes: Some(HashMap::from([(uri.clone(), edits)])),
                ..Default::default()
            }),
            command: Some(mk_log_command(title, "convert_all_colon_cast")),
            is_preferred: None,
            disabled: None,
            data: None,
        })]
    }
}

fn code_actions_of_errors(
    options: &Options,
    loc_of_aloc: Arc<dyn Fn(&ALoc) -> Loc>,
    get_ast_from_heap: Arc<dyn Fn(&FileKey) -> Option<ast::Program<Loc, Loc>>>,
    module_system_info: &LspModuleSystemInfo,
    get_type_sig: Arc<
        dyn Fn(
            &FileKey,
        ) -> Option<
            flow_type_sig::packed_type_sig::Module<
                flow_type_sig::compact_table::Index<flow_aloc::ALoc>,
            >,
        >,
    >,
    cx: &mut Context,
    file_sig: &Arc<FileSig>,
    env: &Env,
    ast: &ast::Program<Loc, Loc>,
    typed_ast: &ast::Program<ALoc, (ALoc, Type)>,
    diagnostics: &[Diagnostic],
    errors: ErrorSet,
    only: Option<&[CodeActionKind]>,
    imports_ranked_usage: bool,
    uri: &Uri,
    loc: Loc,
) -> Vec<CodeActionOrCommand> {
    let include_quick_fixes = include_quick_fixes(only);
    let (actions, has_missing_import) = errors.fold(
        (vec![], false),
        |(mut actions, mut has_missing_import), error| {
            let error_message = ErrorMessage::map_loc_of_error_message(
                |aloc: ALoc| loc_of_aloc(&aloc),
                error.msg_of_error().clone(),
            );
            let (suggest_imports_actions, new_has_missing_import): (
                Vec<CodeActionOrCommand>,
                bool,
            ) = match (&error_message, &env.exports) {
                (
                    ErrorMessage::EBuiltinNameLookupFailed(box EBuiltinNameLookupFailedData {
                        loc: error_loc,
                        name,
                    }),
                    Some(exports),
                ) if options.autoimports => {
                    let actions = if include_quick_fixes && Loc::intersects(error_loc, &loc) {
                        let uri_dir = lsp_helpers::lsp_uri_to_path(uri).ok().and_then(|p| {
                            std::path::Path::new(&p)
                                .parent()
                                .map(|d| d.to_string_lossy().to_string())
                        });
                        suggest_imports(
                            cx,
                            &code_action_utils::layout_options(options),
                            module_system_info,
                            uri_dir.as_deref(),
                            ast,
                            diagnostics,
                            imports_ranked_usage,
                            exports,
                            name.as_str(),
                            uri,
                            loc.dupe(),
                        )
                    } else {
                        vec![]
                    };
                    (actions, true)
                }
                _ => (vec![], has_missing_import),
            };
            has_missing_import = new_has_missing_import;
            let quick_fix_actions: Vec<CodeActionOrCommand> = if include_quick_fixes {
                let lazy_error_loc = {
                    let loc_of_aloc_ref = &loc_of_aloc;
                    let intermediate = make_intermediate_error(
                        |aloc: &ALoc| loc_of_aloc_ref(aloc),
                        false,
                        &error,
                        |file| cx.is_global_libdef(file),
                    );
                    intermediate.loc
                };
                let transforms = ast_transforms_of_error(
                    loc_of_aloc.dupe(),
                    Some(lazy_error_loc),
                    get_ast_from_heap.dupe(),
                    module_system_info.get_haste_module_info.dupe(),
                    get_type_sig.dupe(),
                    Some(loc.dupe()),
                    &error_message,
                );
                transforms
                    .iter()
                    .filter_map(|t| {
                        autofix_in_upstream_file(
                            cx,
                            &*get_ast_from_heap,
                            file_sig.dupe(),
                            diagnostics,
                            ast,
                            typed_ast,
                            options,
                            &t.title,
                            &t.transform,
                            &t.diagnostic_title,
                            uri,
                            t.target_loc.dupe(),
                        )
                    })
                    .collect()
            } else {
                vec![]
            };
            let mut result = suggest_imports_actions;
            result.extend(quick_fix_actions);
            result.extend(actions);
            actions = result;
            (actions, has_missing_import)
        },
    );
    if include_add_missing_imports_action(only) && has_missing_import {
        let mut result = vec![CodeActionOrCommand::CodeAction(CodeAction {
            title: "Add all missing imports".to_string(),
            kind: Some(add_missing_imports_kind()),
            diagnostics: Some(vec![]),
            edit: None,
            command: Some(Command {
                title: "".to_string(),
                command: "source.addMissingImports".to_string(),
                arguments: Some(vec![serde_json::json!({"uri": uri.as_str()})]),
            }),
            is_preferred: None,
            disabled: None,
            data: None,
        })];
        result.extend(actions);
        result
    } else {
        actions
    }
}

fn code_action_for_parser_error_with_suggestion(
    mut acc: Vec<CodeActionOrCommand>,
    diagnostics: &[Diagnostic],
    uri: &Uri,
    error_loc: Loc,
    editor_loc: Loc,
    title: &str,
    new_text: &str,
) -> Vec<CodeActionOrCommand> {
    if Loc::intersects(&error_loc, &editor_loc) {
        let error_range = loc_to_lsp_range(&error_loc);
        let relevant_diagnostics: Vec<Diagnostic> = diagnostics
            .iter()
            .filter(|d| d.range == error_range)
            .cloned()
            .collect();
        let text_edit = TextEdit {
            range: error_range,
            new_text: new_text.to_string(),
        };
        acc.push(CodeActionOrCommand::CodeAction(CodeAction {
            title: title.to_string(),
            kind: Some(CodeActionKind::QUICKFIX),
            diagnostics: Some(relevant_diagnostics),
            edit: Some(WorkspaceEdit {
                changes: Some(HashMap::from([(uri.clone(), vec![text_edit])])),
                ..Default::default()
            }),
            command: Some(mk_log_command(title, "fix_parse_error")),
            is_preferred: None,
            disabled: None,
            data: None,
        }));
        acc
    } else {
        acc
    }
}

fn code_actions_of_parse_errors(
    diagnostics: &[Diagnostic],
    uri: &Uri,
    loc: Loc,
    parse_errors: &[(Loc, ParseError)],
) -> Vec<CodeActionOrCommand> {
    parse_errors
        .iter()
        .fold(vec![], |acc, (error_loc, parse_error)| match parse_error {
            ParseError::UnexpectedTokenWithSuggestion(token, suggestion) => {
                let title = format!("Replace `{}` with `{}`", token, suggestion);
                code_action_for_parser_error_with_suggestion(
                    acc,
                    diagnostics,
                    uri,
                    error_loc.dupe(),
                    loc.dupe(),
                    &title,
                    suggestion,
                )
            }
            ParseError::InvalidComponentRenderAnnotation {
                has_nested_render: false,
            } => {
                let title = "Replace `:` with `renders`";
                code_action_for_parser_error_with_suggestion(
                    acc,
                    diagnostics,
                    uri,
                    error_loc.dupe(),
                    loc.dupe(),
                    title,
                    " renders",
                )
            }
            ParseError::InvalidComponentRenderAnnotation {
                has_nested_render: true,
            } => {
                let title = "Remove `:`";
                code_action_for_parser_error_with_suggestion(
                    acc,
                    diagnostics,
                    uri,
                    error_loc.dupe(),
                    loc.dupe(),
                    title,
                    "",
                )
            }
            ParseError::InvalidComponentStringParameterBinding { optional, name } => {
                let title = "Use as-renaming";
                let replacement = format!(
                    " as {}{}",
                    flow_parser::parse_error::camelize(name),
                    if *optional { "?" } else { ":" }
                );
                code_action_for_parser_error_with_suggestion(
                    acc,
                    diagnostics,
                    uri,
                    error_loc.dupe(),
                    loc.dupe(),
                    title,
                    &replacement,
                )
            }
            _ => acc,
        })
}

fn supported_code_actions() -> Vec<CodeActionKind> {
    vec![
        CodeActionKind::QUICKFIX,
        add_missing_imports_kind(),
        CodeActionKind::new("source.organizeImports.flow"),
        CodeActionKind::REFACTOR_EXTRACT,
    ]
}

pub fn kind_is_supported(only: Option<&[CodeActionKind]>) -> bool {
    match only {
        None => true,
        Some(only) => {
            let supported = supported_code_actions();
            only.iter()
                .any(|kind| supported.iter().any(|s| is_kind(kind, s)))
        }
    }
}

fn organize_imports_code_action(uri: &Uri) -> CodeActionOrCommand {
    CodeActionOrCommand::CodeAction(CodeAction {
        title: "Organize imports".to_string(),
        kind: Some(CodeActionKind::new("source.organizeImports.flow")),
        diagnostics: Some(vec![]),
        edit: None,
        command: Some(Command {
            title: "".to_string(),
            command: "source.organizeImports".to_string(),
            arguments: Some(vec![serde_json::json!({"uri": uri.as_str()})]),
        }),
        is_preferred: None,
        disabled: None,
        data: None,
    })
}

pub fn code_actions_at_loc<'a>(
    options: &Options,
    lsp_init_params: &lsp_types::InitializeParams,
    imports_ranked_usage: bool,
    env: &Env,
    loc_of_aloc: Arc<dyn Fn(&ALoc) -> Loc>,
    get_ast_from_heap: Arc<dyn Fn(&FileKey) -> Option<ast::Program<Loc, Loc>>>,
    get_type_sig: Arc<
        dyn Fn(
            &FileKey,
        ) -> Option<
            flow_type_sig::packed_type_sig::Module<
                flow_type_sig::compact_table::Index<flow_aloc::ALoc>,
            >,
        >,
    >,
    module_system_info: &LspModuleSystemInfo,
    cx: &mut Context<'a>,
    file_sig: &Arc<FileSig>,
    tolerable_errors: &[TolerableError<Loc>],
    file_contents: &str,
    ast: &ast::Program<Loc, Loc>,
    typed_ast: &ast::Program<ALoc, (ALoc, Type)>,
    scope_info: &ScopeInfo<Loc>,
    parse_errors: &[(Loc, ParseError)],
    diagnostics: &[Diagnostic],
    only: Option<&[CodeActionKind]>,
    uri: &Uri,
    loc: Loc,
) -> Result<Vec<CodeActionOrCommand>, String> {
    let support_experimental_snippet_text_edit = lsp_init_params
        .capabilities
        .experimental
        .as_ref()
        .and_then(|v| v.get("snippetTextEdit"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let get_haste_module_info: &dyn Fn(
        &FileKey,
    ) -> Option<flow_common_modulename::HasteModuleInfo> =
        &*module_system_info.get_haste_module_info;
    let autofix_exports_actions = autofix_exports_code_actions(
        options,
        cx,
        &*loc_of_aloc,
        &*get_ast_from_heap,
        get_haste_module_info,
        &*get_type_sig,
        ast,
        file_sig,
        tolerable_errors,
        typed_ast,
        diagnostics,
        uri,
        loc.dupe(),
    )
    .map_err(|e| format!("{:?}", e))?;
    let autofix_missing_local_annot_actions = autofix_missing_local_annot_code_actions(
        options,
        cx,
        &*loc_of_aloc,
        &*get_ast_from_heap,
        get_haste_module_info,
        &*get_type_sig,
        ast,
        file_sig,
        tolerable_errors,
        typed_ast,
        diagnostics,
        uri,
        loc.dupe(),
    )
    .map_err(|e| format!("{:?}", e))?;
    let insert_inferred_render_type_actions = code_action_insert_inferred_render_type(
        options,
        cx,
        &*loc_of_aloc,
        &*get_ast_from_heap,
        get_haste_module_info,
        &*get_type_sig,
        ast,
        file_sig,
        typed_ast,
        uri,
        loc.dupe(),
    )?;
    let mut refactor_actions = refactor_extract_and_stub_out_code_actions(
        options,
        support_experimental_snippet_text_edit,
        file_contents,
        ast,
        cx,
        file_sig,
        typed_ast,
        &*loc_of_aloc,
        &*get_ast_from_heap,
        get_haste_module_info,
        &*get_type_sig,
        only,
        uri,
        loc.dupe(),
    );
    refactor_actions.extend(insert_jsdoc_code_actions(options, ast, uri, loc.dupe()));
    refactor_actions.extend(convert_type_to_readonly_form_code_actions(
        options,
        ast,
        only,
        uri,
        loc.dupe(),
    ));
    refactor_actions.extend(refactor_arrow_function_code_actions(
        ast,
        scope_info,
        options,
        only,
        uri,
        loc.dupe(),
    ));
    refactor_actions.extend(refactor_switch_to_match_statement_actions(
        cx,
        ast,
        options,
        only,
        uri,
        loc.dupe(),
    ));
    refactor_actions.extend(refactor_match_coded_like_switch(
        cx,
        ast,
        options,
        only,
        &*loc_of_aloc,
        uri,
        loc.dupe(),
    ));
    refactor_actions.extend(add_jsx_props_code_actions(
        support_experimental_snippet_text_edit,
        cx,
        ast,
        typed_ast,
        options,
        uri,
        loc.dupe(),
    ));
    let fix_all_in_file_actions = if include_quick_fixes(only) {
        fix_all_in_file_code_actions(
            options,
            &*loc_of_aloc,
            ast,
            diagnostics,
            cx.errors(),
            loc.dupe(),
            uri,
        )
    } else {
        vec![]
    };
    let error_fixes = code_actions_of_errors(
        options,
        loc_of_aloc.dupe(),
        get_ast_from_heap.dupe(),
        module_system_info,
        get_type_sig.dupe(),
        cx,
        file_sig,
        env,
        ast,
        typed_ast,
        diagnostics,
        cx.errors(),
        only,
        imports_ranked_usage,
        uri,
        loc.dupe(),
    );
    let scope_based_auto_import_fixes: Vec<CodeActionOrCommand> = match &env.exports {
        Some(exports) if options.autoimports && include_quick_fixes(only) => {
            let unbound_names = find_unbound_names_from_scope(&*loc_of_aloc, cx, loc.dupe());
            unbound_names
                .into_iter()
                .flat_map(|(error_loc, name)| {
                    suggest_imports(
                        cx,
                        &code_action_utils::layout_options(options),
                        module_system_info,
                        lsp_helpers::lsp_uri_to_path(uri)
                            .ok()
                            .and_then(|p| {
                                std::path::Path::new(&p)
                                    .parent()
                                    .map(|d| d.to_string_lossy().to_string())
                            })
                            .as_deref(),
                        ast,
                        diagnostics,
                        imports_ranked_usage,
                        exports,
                        name.as_str(),
                        uri,
                        error_loc,
                    )
                })
                .collect()
        }
        _ => vec![],
    };
    let inspection_related_code_actions: Vec<CodeActionOrCommand> =
        if autofix_exports_actions.is_empty() {
            insert_inferred_type_as_cast_code_actions(
                options,
                file_contents,
                ast,
                cx,
                file_sig,
                typed_ast,
                loc_of_aloc.dupe(),
                get_ast_from_heap.dupe(),
                module_system_info.get_haste_module_info.dupe(),
                get_type_sig.dupe(),
                uri,
                loc.dupe(),
            )
        } else {
            vec![]
        };
    let parse_error_fixes = code_actions_of_parse_errors(diagnostics, uri, loc, parse_errors);
    let mut actions = vec![];
    actions.extend(parse_error_fixes);
    actions.extend(autofix_exports_actions);
    actions.extend(autofix_missing_local_annot_actions);
    actions.extend(fix_all_in_file_actions);
    actions.extend(error_fixes);
    actions.extend(scope_based_auto_import_fixes);
    actions.extend(insert_inferred_render_type_actions);
    actions.extend(refactor_actions);
    actions.extend(inspection_related_code_actions);
    if include_organize_imports_actions(only) {
        let organize = organize_imports_code_action(uri);
        actions.insert(0, organize);
    }
    Ok(actions)
}

fn autofix_imports_fn(
    options: &Options,
    env: &Env,
    loc_of_aloc: &dyn Fn(&ALoc) -> Loc,
    module_system_info: &LspModuleSystemInfo,
    cx: &mut Context,
    ast: &ast::Program<Loc, Loc>,
    src_dir: Option<&str>,
) -> Vec<(Loc, String)> {
    let errors = cx.errors();
    type ExportSourceMap =
        BTreeMap<export_index::Source, BTreeMap<export_index::Kind, Vec<FlowSmolStr>>>;
    let (imports, _): (ExportSourceMap, BTreeSet<FlowSmolStr>) = errors.fold(
        (BTreeMap::new(), BTreeSet::new()),
        |(mut imports, mut unbound_names), error| {
            let error_message = ErrorMessage::map_loc_of_error_message(
                |aloc: ALoc| loc_of_aloc(&aloc),
                error.msg_of_error().clone(),
            );
            match (&error_message, &env.exports) {
                (
                    ErrorMessage::EBuiltinNameLookupFailed(box EBuiltinNameLookupFailedData {
                        loc: error_loc,
                        name,
                    }),
                    Some(exports),
                ) if options.autoimports => {
                    match preferred_import(ast, exports, name.as_str(), error_loc.dupe()) {
                        Some(export_index::Export(source, export_kind))
                            if !unbound_names.contains(name) =>
                        {
                            let bindings = imports.entry(source).or_insert_with(BTreeMap::new);
                            let names = bindings.entry(export_kind).or_insert_with(Vec::new);
                            names.push(name.dupe());
                            unbound_names.insert(name.dupe());
                            (imports, unbound_names)
                        }
                        _ => (imports, unbound_names),
                    }
                }
                _ => (imports, unbound_names),
            }
        },
    );
    let mut added_imports: Vec<(String, autofix_imports::Bindings)> = vec![];
    for (source, names_of_kinds) in &imports {
        let from = lsp_import_edits::from_of_source(module_system_info, src_dir, source);
        match from {
            None => {}
            Some(from) => {
                for (export_kind, names) in names_of_kinds {
                    match export_kind {
                        export_index::Kind::DefaultType => {
                            for name in names {
                                added_imports.push((
                                    from.clone(),
                                    autofix_imports::Bindings::DefaultType(name.to_string()),
                                ));
                            }
                        }
                        export_index::Kind::Default => {
                            for name in names {
                                added_imports.push((
                                    from.clone(),
                                    autofix_imports::Bindings::Default(name.to_string()),
                                ));
                            }
                        }
                        export_index::Kind::Named => {
                            let named_bindings: Vec<autofix_imports::NamedBinding> = names
                                .iter()
                                .map(|name| autofix_imports::NamedBinding {
                                    remote_name: name.to_string(),
                                    local_name: None,
                                })
                                .collect();
                            added_imports.push((
                                from.clone(),
                                autofix_imports::Bindings::Named(named_bindings),
                            ));
                        }
                        export_index::Kind::NamedType => {
                            let named_bindings: Vec<autofix_imports::NamedBinding> = names
                                .iter()
                                .map(|name| autofix_imports::NamedBinding {
                                    remote_name: name.to_string(),
                                    local_name: None,
                                })
                                .collect();
                            added_imports.push((
                                from.clone(),
                                autofix_imports::Bindings::NamedType(named_bindings),
                            ));
                        }
                        export_index::Kind::Namespace => {
                            for name in names {
                                added_imports.push((
                                    from.clone(),
                                    autofix_imports::Bindings::Namespace(name.to_string()),
                                ));
                            }
                        }
                    }
                }
            }
        }
    }
    let opts = code_action_utils::layout_options(options);
    autofix_imports::add_imports(&opts, &added_imports, ast)
}

fn with_type_checked_file<T>(
    orchestrator: Option<&ServerOrchestratorHandle>,
    cache: &mut CheckContentsCache,
    options: &Options,
    env: &Env,
    transaction: Arc<Transaction>,
    file_key: &FileKey,
    file_content: &str,
    f: &dyn Fn(
        &mut Context,
        &Arc<FileSig>,
        &ast::Program<Loc, Loc>,
        &ast::Program<ALoc, (ALoc, Type)>,
    ) -> Result<T, String>,
) -> Result<T, String> {
    let file_artifacts = {
        let intermediate_result =
            type_contents::parse_contents(options, env.configured_libs(), file_content, file_key);
        let (ref _parse_artifacts_opt, ref parse_errs) = intermediate_result;
        if !parse_errs.is_empty() {
            Err(flow_services_inference_types::TypeContentsError::Errors(
                parse_errs.clone(),
            ))
        } else {
            type_contents::type_parse_artifacts(
                orchestrator,
                cache,
                options,
                env.configured_libs(),
                transaction,
                env.master_cx.dupe(),
                file_key.clone(),
                intermediate_result,
            )
        }
    };
    match file_artifacts {
        Ok((parse_artifacts, typecheck_artifacts)) => typecheck_artifacts.cx.with_cx(cache, |cx| {
            f(
                cx,
                &parse_artifacts.file_sig,
                &parse_artifacts.ast,
                &typecheck_artifacts.typed_ast,
            )
        }),
        _ => Err("Failed to parse or check file".to_string()),
    }
}

pub fn autofix_errors_cli(
    cache: &mut CheckContentsCache,
    options: &Options,
    env: &Env,
    transaction: Arc<Transaction>,
    loc_of_aloc: Arc<dyn Fn(&ALoc) -> Loc>,
    get_ast_from_heap: Arc<dyn Fn(&FileKey) -> Option<ast::Program<Loc, Loc>>>,
    module_system_info: &LspModuleSystemInfo,
    get_type_sig: Arc<
        dyn Fn(
            &FileKey,
        ) -> Option<
            flow_type_sig::packed_type_sig::Module<
                flow_type_sig::compact_table::Index<flow_aloc::ALoc>,
            >,
        >,
    >,
    include_best_effort_fix: bool,
    file_key: &FileKey,
    file_content: &str,
) -> Result<Vec<(Loc, String)>, String> {
    with_type_checked_file(
        None,
        cache,
        options,
        env,
        transaction,
        file_key,
        file_content,
        &|cx: &mut Context,
          file_sig: &Arc<FileSig>,
          ast: &ast::Program<Loc, Loc>,
          typed_ast: &ast::Program<ALoc, (ALoc, Type)>| {
            let (safe_transforms, best_effort_transforms): (
                Vec<AstTransformOfError>,
                Vec<AstTransformOfError>,
            ) = cx
                .errors()
                .fold((vec![], vec![]), |(mut safe, mut best_effort), error| {
                    let lazy_error_loc = {
                        let loc_of_aloc_ref = &loc_of_aloc;
                        make_intermediate_error(
                            |aloc: &ALoc| loc_of_aloc_ref(aloc),
                            false,
                            &error,
                            |file| cx.is_global_libdef(file),
                        )
                        .loc
                    };
                    let mapped_error =
                        FlowError::map_loc_of_error(|aloc: ALoc| loc_of_aloc(&aloc), error);
                    let transforms = ast_transforms_of_error(
                        loc_of_aloc.dupe(),
                        Some(lazy_error_loc),
                        get_ast_from_heap.dupe(),
                        module_system_info.get_haste_module_info.dupe(),
                        get_type_sig.dupe(),
                        None,
                        mapped_error.msg_of_error(),
                    );
                    match transforms.into_iter().next() {
                        None => (safe, best_effort),
                        Some(transform) => {
                            match transform.confidence {
                                QuickfixConfidence::WillFixErrorAndSafeForRunningOnSave => {
                                    safe.push(transform);
                                }
                                QuickfixConfidence::BestEffort => {
                                    if include_best_effort_fix {
                                        best_effort.push(transform);
                                    }
                                }
                            }
                            (safe, best_effort)
                        }
                    }
                });
            let all_transforms: Vec<AstTransformOfError> = safe_transforms
                .into_iter()
                .chain(best_effort_transforms)
                .collect();
            let (new_ast, _) = all_transforms.into_iter().fold(
                (ast.clone(), false),
                |(current_ast, has_run_best_effort), transform| match (
                    transform.confidence,
                    has_run_best_effort,
                ) {
                    (QuickfixConfidence::BestEffort, true) => (current_ast, true),
                    (QuickfixConfidence::WillFixErrorAndSafeForRunningOnSave, _) => {
                        let result = (transform.transform)(
                            cx,
                            file_sig.dupe(),
                            &current_ast,
                            typed_ast,
                            transform.target_loc,
                        );
                        let new = result.unwrap_or(current_ast);
                        (new, has_run_best_effort)
                    }
                    (QuickfixConfidence::BestEffort, false) => {
                        let result = (transform.transform)(
                            cx,
                            file_sig.dupe(),
                            &current_ast,
                            typed_ast,
                            transform.target_loc,
                        );
                        match result {
                            Some(new) => (new, true),
                            None => (current_ast, false),
                        }
                    }
                },
            );
            let opts = code_action_utils::layout_options(options);
            let diff = flow_ast_differ::program(ast, &new_ast);
            let edits = replacement_printer::mk_loc_patch_ast_differ(&opts, &diff);
            Ok(edits)
        },
    )
}

pub fn autofix_imports_cli(
    cache: &mut CheckContentsCache,
    options: &Options,
    env: &Env,
    transaction: Arc<Transaction>,
    loc_of_aloc: &dyn Fn(&ALoc) -> Loc,
    module_system_info: &LspModuleSystemInfo,
    file_key: &FileKey,
    file_content: &str,
) -> Result<Vec<(Loc, String)>, String> {
    let file_path = file_key.to_absolute();
    let src_dir = std::path::Path::new(&file_path)
        .parent()
        .map(|p| p.to_string_lossy().to_string());
    with_type_checked_file(
        None,
        cache,
        options,
        env,
        transaction,
        file_key,
        file_content,
        &|cx: &mut Context,
          _file_sig: &Arc<FileSig>,
          ast: &ast::Program<Loc, Loc>,
          _typed_ast: &ast::Program<ALoc, (ALoc, Type)>| {
            let edits = autofix_imports_fn(
                options,
                env,
                loc_of_aloc,
                module_system_info,
                cx,
                ast,
                src_dir.as_deref(),
            );
            Ok(edits)
        },
    )
}

pub fn suggest_imports_cli(
    cache: &mut CheckContentsCache,
    options: &Options,
    env: &Env,
    transaction: Arc<Transaction>,
    loc_of_aloc: &dyn Fn(&ALoc) -> Loc,
    module_system_info: &LspModuleSystemInfo,
    file_key: &FileKey,
    file_content: &str,
) -> Result<HashMap<String, Vec<CodeActionOrCommand>>, String> {
    let uri = flow_lsp::lsp_helpers::path_to_lsp_uri(&file_key.to_absolute(), "");
    let file_path = file_key.to_absolute();
    let src_dir = std::path::Path::new(&file_path)
        .parent()
        .map(|p| p.to_string_lossy().to_string());
    with_type_checked_file(
        None,
        cache,
        options,
        env,
        transaction,
        file_key,
        file_content,
        &|cx: &mut Context,
          _file_sig: &Arc<FileSig>,
          ast: &ast::Program<Loc, Loc>,
          _typed_ast: &ast::Program<ALoc, (ALoc, Type)>| {
            let errors = cx.errors();
            let (imports, _): (
                HashMap<String, Vec<CodeActionOrCommand>>,
                BTreeSet<FlowSmolStr>,
            ) = errors.fold(
                (HashMap::new(), BTreeSet::new()),
                |(mut imports, mut unbound_names), error| {
                    let error_message = ErrorMessage::map_loc_of_error_message(
                        |aloc: ALoc| loc_of_aloc(&aloc),
                        error.msg_of_error().clone(),
                    );
                    match (&error_message, &env.exports) {
                        (
                            ErrorMessage::EBuiltinNameLookupFailed(
                                box EBuiltinNameLookupFailedData {
                                    loc: error_loc,
                                    name,
                                },
                            ),
                            Some(exports),
                        ) if options.autoimports => {
                            let ranked_imports = suggest_imports(
                                cx,
                                &code_action_utils::layout_options(options),
                                module_system_info,
                                src_dir.as_deref(),
                                ast,
                                &[],
                                true, // imports_ranked_usage
                                exports,
                                name.as_str(),
                                &uri,
                                error_loc.dupe(),
                            );
                            imports.insert(name.to_string(), ranked_imports);
                            unbound_names.insert(name.dupe());
                            (imports, unbound_names)
                        }
                        _ => (imports, unbound_names),
                    }
                },
            );
            Ok(imports)
        },
    )
}

pub fn autofix_imports_lsp(
    options: &Options,
    env: &Env,
    loc_of_aloc: &dyn Fn(&ALoc) -> Loc,
    module_system_info: &LspModuleSystemInfo,
    cx: &mut Context,
    ast: &ast::Program<Loc, Loc>,
    uri: &Uri,
) -> Vec<TextEdit> {
    let src_dir = lsp_helpers::lsp_uri_to_path(uri).ok().and_then(|p| {
        std::path::Path::new(&p)
            .parent()
            .map(|d| d.to_string_lossy().to_string())
    });
    let edits = autofix_imports_fn(
        options,
        env,
        loc_of_aloc,
        module_system_info,
        cx,
        ast,
        src_dir.as_deref(),
    );
    flow_loc_patch_to_lsp_edits(&edits)
}

pub fn autofix_exports_fn(
    cache: &mut CheckContentsCache,
    options: &Options,
    loc_of_aloc: Arc<dyn Fn(&ALoc) -> Loc>,
    get_ast_from_heap: &dyn Fn(&FileKey) -> Option<ast::Program<Loc, Loc>>,
    get_haste_module_info: Arc<dyn Fn(&FileKey) -> Option<flow_common_modulename::HasteModuleInfo>>,
    get_type_sig: Arc<
        dyn Fn(
            &FileKey,
        ) -> Option<
            flow_type_sig::packed_type_sig::Module<
                flow_type_sig::compact_table::Index<flow_aloc::ALoc>,
            >,
        >,
    >,
    file_key: FileKey,
    _file_content: &str,
    parse_artifacts: &ParseArtifacts,
    typecheck_artifacts: &TypecheckArtifacts,
) -> (Vec<(Loc, String)>, Vec<String>) {
    let ParseArtifacts {
        ast,
        file_sig,
        tolerable_errors,
        ..
    } = parse_artifacts;
    let TypecheckArtifacts { cx, typed_ast, .. } = typecheck_artifacts;
    let sv_errors =
        autofix_exports::set_of_fixable_signature_verification_locations(tolerable_errors);
    let (new_ast, it_errs) = cx.with_cx(cache, |cx| {
        autofix_exports::fix_signature_verification_errors(
            file_key,
            cx,
            &*loc_of_aloc,
            options.file_options.dupe(),
            get_ast_from_heap,
            &*get_haste_module_info,
            &*get_type_sig,
            file_sig,
            typed_ast,
            ast.dupe(),
            &sv_errors,
        )
    });
    let opts = code_action_utils::layout_options(options);
    let diff = flow_ast_differ::program(ast, &new_ast);
    let edits = replacement_printer::mk_loc_patch_ast_differ(&opts, &diff);
    (edits, it_errs)
}

pub fn autofix_missing_local_annot_fn(
    cache: &mut CheckContentsCache,
    options: &Options,
    loc_of_aloc: &dyn Fn(&ALoc) -> Loc,
    get_ast_from_heap: &dyn Fn(&FileKey) -> Option<ast::Program<Loc, Loc>>,
    get_haste_module_info: &dyn Fn(&FileKey) -> Option<flow_common_modulename::HasteModuleInfo>,
    get_type_sig: &dyn Fn(
        &FileKey,
    ) -> Option<
        flow_type_sig::packed_type_sig::Module<
            flow_type_sig::compact_table::Index<flow_aloc::ALoc>,
        >,
    >,
    _file_content: &str,
    parse_artifacts: &ParseArtifacts,
    typecheck_artifacts: &TypecheckArtifacts,
) -> Result<Vec<(Loc, String)>, String> {
    let ParseArtifacts { ast, file_sig, .. } = parse_artifacts;
    let TypecheckArtifacts { cx, typed_ast, .. } = typecheck_artifacts;
    let new_ast = cx
        .with_cx(cache, |cx| {
            autofix_missing_local_annots::fix_all_missing_param_annot_errors_in_file(
                None, // no remote_converter
                cx,
                loc_of_aloc,
                get_ast_from_heap,
                get_haste_module_info,
                get_type_sig,
                file_sig,
                typed_ast,
                ast.dupe(),
            )
        })
        .map_err(|e| format!("FailedToInsertType: {:?}", e))?;
    let opts = code_action_utils::layout_options(options);
    let diff = flow_ast_differ::program(ast, &new_ast);
    let edits = replacement_printer::mk_loc_patch_ast_differ(&opts, &diff);
    Ok(edits)
}

pub fn insert_type_fn(
    cache: &mut CheckContentsCache,
    options: &Options,
    loc_of_aloc: &dyn Fn(&ALoc) -> Loc,
    get_ast_from_heap: &dyn Fn(&FileKey) -> Option<ast::Program<Loc, Loc>>,
    get_haste_module_info: &dyn Fn(&FileKey) -> Option<flow_common_modulename::HasteModuleInfo>,
    get_type_sig: &dyn Fn(
        &FileKey,
    ) -> Option<
        flow_type_sig::packed_type_sig::Module<
            flow_type_sig::compact_table::Index<flow_aloc::ALoc>,
        >,
    >,
    _file_content: &str,
    target: Loc,
    omit_targ_defaults: bool,
    strict: bool,
    parse_artifacts: &ParseArtifacts,
    typecheck_artifacts: &TypecheckArtifacts,
) -> Result<Vec<(Loc, String)>, String> {
    let ParseArtifacts { ast, file_sig, .. } = parse_artifacts;
    let TypecheckArtifacts { cx, typed_ast, .. } = typecheck_artifacts;
    let new_ast = cx
        .with_cx(cache, |cx| {
            insert_type::insert_type(
                cx,
                loc_of_aloc,
                get_ast_from_heap,
                get_haste_module_info,
                get_type_sig,
                file_sig,
                typed_ast,
                omit_targ_defaults,
                strict,
                None, // no remote_converter
                ast,
                target,
            )
        })
        .map_err(|err| insert_type::error_to_string(&err))?;
    let opts = code_action_utils::layout_options(options);
    let diff = flow_ast_differ::program(ast, &new_ast);
    let edits = replacement_printer::mk_loc_patch_ast_differ(&opts, &diff);
    Ok(edits)
}

pub fn organize_imports_fn(options: &Options, ast: &ast::Program<Loc, Loc>) -> Vec<TextEdit> {
    let opts = code_action_utils::layout_options(options);
    flow_loc_patch_to_lsp_edits(&autofix_imports::organize_imports(&opts, ast))
}
