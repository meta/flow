/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

#![feature(deref_patterns)]
#![feature(never_type)]

pub mod compact_table;
pub mod expected_annotation_sort;
pub mod packed_type_sig;
pub mod signature_error;
pub mod ts_enum_sig;
pub mod type_sig;
pub mod type_sig_hash;
mod type_sig_mark;
pub mod type_sig_options;
pub mod type_sig_pack;
mod type_sig_parse;
#[cfg(test)]
mod type_sig_tests;
pub mod type_sig_utils;
