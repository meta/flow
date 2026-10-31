/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

#![feature(box_patterns)]
#![feature(deref_patterns)]
pub mod default_resolve;
pub mod flow_js;
pub mod implicit_instantiation;
pub mod intersection_kit;
pub mod marked;
pub mod natural_inference;
pub mod object_kit;
pub mod optional_chain_kit;
pub mod react_kit;
pub mod renders_kit;
pub mod slice_utils;
pub mod speculation_kit;
pub mod subtyping_kit;
pub mod template_literal_type;
pub mod tvar_resolver;
pub mod type_inference_hooks_js;
