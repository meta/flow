/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

#![feature(deref_patterns)]
#![feature(never_type)]
#![allow(clippy::todo)]

pub mod class_sig;
pub mod component_declaration_config;
pub mod component_params;
pub mod component_params_intf;
pub mod component_sig;
pub mod destructuring;
pub mod exists_marker;
pub mod func_params;
pub mod func_params_intf;
pub mod func_sig;
pub mod func_stmt_config;
pub mod match_pattern;
pub mod module_info_analyzer;
pub mod react_rules;
pub mod refinement;
pub mod statement;
pub mod switch_to_match;
pub mod type_annotation;
pub mod type_annotation_cons_gen;
