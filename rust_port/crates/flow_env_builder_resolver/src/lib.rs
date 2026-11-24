/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

#![feature(deref_patterns)]
#![feature(never_type)]

pub mod dependency_sigs;
#[cfg(test)]
mod env_builder_refinement_test;
pub mod name_def_ordering;
#[cfg(test)]
mod name_def_test;
pub mod name_resolver;
pub mod super_call_in_derived_ctor_checker;
