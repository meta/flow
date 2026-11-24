/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

#![feature(deref_patterns)]
#![feature(never_type)]

pub mod cycles;
mod declared_types;
pub mod env_resolution;
pub mod marked;
pub mod merge;
pub mod query_types;
pub mod ty_members;
pub mod ty_normalizer_flow;
pub mod ty_normalizer_imports;
pub mod type_inference;
pub mod typed_ast_finder;

#[cfg(test)]
mod type_hint_test;
#[cfg(test)]
mod type_test;
#[cfg(test)]
mod typed_ast_test;
