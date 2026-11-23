/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

#![feature(deref_patterns)]
#![feature(never_type)]

pub mod annotate_exports;
pub mod annotate_literal_declarations;
pub mod annotate_optional_properties;
pub mod migrate_to_ts_void;
pub mod remove_react_import;
pub mod utils;
