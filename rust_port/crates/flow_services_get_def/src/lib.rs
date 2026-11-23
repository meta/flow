/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

//! Port of `services/get_def/`

#![feature(deref_patterns)]
#![feature(never_type)]

pub mod find_refs_utils;
pub mod get_def_js;
pub mod get_def_process_location;
pub mod get_def_request;
pub mod get_def_types;
pub mod get_def_utils;
pub mod object_key_visitor;
