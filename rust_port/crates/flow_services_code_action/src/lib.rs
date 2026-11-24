/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

#![feature(deref_patterns)]
#![feature(never_type)]

//! Port of `services/code_action/`

pub mod ast_extraction_utils;
pub mod autofix_casting_syntax;
pub mod autofix_class_member_access;
pub mod autofix_enum_member_name;
pub mod autofix_exports;
pub mod autofix_imports;
pub mod autofix_interface;
pub mod autofix_legacy_flow_syntax;
pub mod autofix_match_syntax;
pub mod autofix_missing_local_annots;
pub mod autofix_new_to_record;
pub mod autofix_object_to_record;
pub mod autofix_optional_chaining;
pub mod autofix_prop_typo;
pub mod autofix_record_declaration;
pub mod autofix_renders_variant;
pub mod autofix_replace_type;
pub mod autofix_ts_syntax;
pub mod autofix_type_name;
pub mod autofix_type_to_value_import;
pub mod autofix_unused_promise;
pub mod code_action_service;
pub mod code_action_text_edits;
pub mod code_action_utils;
pub mod contains_mapper;
pub mod convert_type_to_readonly_form;
pub mod document_paste;
pub mod insert_inferred_render_type;
pub mod insert_type;
pub mod insert_type_imports;
pub mod insert_type_utils;
pub mod lsp_module_system_info;
pub mod refactor_add_jsx_props;
pub mod refactor_arrow_functions;
pub mod refactor_extract;
pub mod refactor_extract_utils;
pub mod refactor_match_discriminant;
pub mod refactor_switch_to_match_statement;
pub mod stub_unbound_name;

// Test modules (OCaml __tests__/ directory)
#[cfg(test)]
mod autofix_imports_tests;
#[cfg(test)]
mod autofix_type_to_value_import_tests;
#[cfg(test)]
mod code_action_service_tests;
#[cfg(test)]
mod insert_type_utils_tests;
#[cfg(test)]
mod refactor_extract_tests;
#[cfg(test)]
mod refactor_extract_utils_tests;
#[cfg(test)]
mod validation_tests;
