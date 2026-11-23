/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

#![feature(deref_patterns)]
#![feature(never_type)]

pub mod autocomplete_js;
pub mod autocomplete_service_js;
pub mod autocomplete_sigil;
pub mod autofix_imports;
pub mod code_action_text_edits;
pub mod find_documentation;
pub mod find_method;
pub mod insert_jsdoc;
pub mod jsdoc_stub;
pub mod keywords;
pub mod lsp_import_edits;
pub mod module_system_info;

#[cfg(test)]
mod autocomplete_sigil_tests;
