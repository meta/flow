#!/bin/bash
# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under the MIT license found in the
# LICENSE file in the root directory of this source tree.

queries_in_file "type-at-pos" "annot_t.js"
queries_in_file "type-at-pos" "class-0.js"
queries_in_file "type-at-pos" "class-1.js"
queries_in_file "type-at-pos" "class-2.js"
queries_in_file "type-at-pos" "class-3.js"
queries_in_file "type-at-pos" "class-4.js"
queries_in_file "type-at-pos" "class-bound.js"
queries_in_file "type-at-pos" "class-getters-setters.js"
queries_in_file "type-at-pos" "class-poly-0.js"
# TODO constructor
# printf "class-poly-0.js:14:10 = "
# assert_ok "$FLOW" type-at-pos class-poly-0.js 14 10 --strip-root
queries_in_file "type-at-pos" "class-poly-1.js"
queries_in_file "type-at-pos" "class-statics.js"
queries_in_file "type-at-pos" "class-statics-poly.js"
queries_in_file "type-at-pos" "declare_class.js"
queries_in_file "type-at-pos" "FluxStore.js"
queries_in_file "type-at-pos" "implements.js"
queries_in_file "type-at-pos" "multi-inheritance.js"
queries_in_file "type-at-pos" "type-utils.js"
queries_in_file "type-at-pos" "default.js"
queries_in_file "type-at-pos" "constructor.js"
queries_in_file "type-at-pos" "this.js"
queries_in_file "type-at-pos" "function_this.js"
queries_in_file "type-at-pos" "declare_class_members.js"
queries_in_file "type-at-pos" "member_reference.js"
queries_in_file "type-at-pos" "member_reference_eval.js"
queries_in_file "type-at-pos" "private_members.js"
