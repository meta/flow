/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

#![feature(never_type)]
#![feature(deref_patterns)]

pub mod abnormal;
pub mod annotation_inference;
pub mod avar;
pub mod convert_types;
pub mod exhaustive;
pub mod members;
pub mod module_exports_checker;
pub mod predicate_kit;
pub mod speculation_flow;
pub mod strict_es6_import_export;
pub mod type_env;
pub mod type_filter;
pub mod type_guard;
pub mod type_hint;
pub mod type_operation_utils;
pub mod type_sig_merge;
pub mod typed_ast_utils;
