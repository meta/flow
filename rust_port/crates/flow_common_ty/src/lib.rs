/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

#![feature(deref_patterns)]

pub mod ty;
pub mod ty_ancestors;
pub mod ty_debug;
pub mod ty_printer;
pub mod ty_serializer;
pub mod ty_symbol;
pub mod ty_utils;

#[cfg(test)]
mod ty_printer_test;
#[cfg(test)]
mod ty_simplifier_test;
