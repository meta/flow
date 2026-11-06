#!/bin/bash
# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under the MIT license found in the
# LICENSE file in the root directory of this source tree.

queries_in_file "type-at-pos" "declare.js"
printf "declare.js:11:18 = "
assert_ok "$FLOW" type-at-pos declare.js 11 18 --strip-root
printf "declare.js:12:18 = "
assert_ok "$FLOW" type-at-pos declare.js 12 18 --strip-root
queries_in_file "type-at-pos" "function.js"
queries_in_file "type-at-pos" "default.js"
queries_in_file "type-at-pos" "function-poly-0.js"
queries_in_file "type-at-pos" "function-poly-1.js"
queries_in_file "type-at-pos" "function-poly-2.js"
queries_in_file "type-at-pos" "function-poly-4.js"
queries_in_file "type-at-pos" "function-poly-5.js"
queries_in_file "type-at-pos" "function-poly-6.js"
queries_in_file "type-at-pos" "overloads.js"
queries_in_file "type-at-pos" "function-poly-call.js"
