/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

#![feature(never_type)]

pub mod check_type_only_changes;
pub mod comment;
mod errors;
mod flow_result;
pub mod runtests;
pub mod update_suppressions;
mod utils;

pub use errors::ErrorCheckCommand;
