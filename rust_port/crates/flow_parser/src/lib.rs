/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

#![feature(never_type)]

#[cfg(test)]
mod assertion_loc_test;
pub mod ast;
pub mod ast_utils;
pub mod ast_visitor;
pub mod comment_attachment;
pub mod comment_utils;
mod declaration_parser;
pub mod dts_file_kind;
mod enum_common;
mod enum_parser;
pub mod estree_translator;
mod expression_parser;
pub mod file_key;
mod flow_ast_mapper_test;
pub mod flow_lexer;
#[cfg(test)]
mod flow_polymorphic_ast_mapper_test;
pub mod js_id_unicode;
pub mod jsdoc;
mod jsdoc_test;
mod jsx_parser;
pub mod lex_env;
pub mod loc;
pub mod loc_sig;
pub mod logos_tokens;
mod main_parser;
mod match_pattern_parser;
mod object_parser;
pub mod offset_utils;
pub mod parse_error;
mod parser_common;
mod parser_env;
mod parser_integration_tests;
mod pattern_cover;
mod pattern_parser;
pub mod polymorphic_ast_mapper;
pub mod source_location;
mod statement_parser;
pub mod token;
mod type_parser;
mod wtf8;

pub use main_parser::do_parse;
pub use main_parser::find_ident;
pub use main_parser::parse_annotation;
pub use main_parser::parse_expression;
pub use main_parser::parse_json_file;
pub use main_parser::parse_jsx_pragma_expression;
pub use main_parser::parse_module_file;
pub use main_parser::parse_module_file_with_errors;
pub use main_parser::parse_package_json_file;
pub use main_parser::parse_program_file;
pub use main_parser::parse_program_in_context;
pub use main_parser::parse_program_without_file;
pub use main_parser::parse_to_json;
pub use parser_env::LexMode;
pub use parser_env::PERMISSIVE_PARSE_OPTIONS;
pub use parser_env::ParenthesizedExpressionLocation;
pub use parser_env::ParseOptions;
pub use parser_env::ParserEnv;
pub use parser_env::TokenSinkResult;
pub use parser_env::init_env;
pub use statement_parser::parse_module_body_with_directives;
