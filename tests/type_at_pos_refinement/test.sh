#!/bin/bash
# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under the MIT license found in the
# LICENSE file in the root directory of this source tree.

queries_in_file "type-at-pos" "predicates.js"
# printf "predicates.js:4:12 (null) = "
# assert_ok "$FLOW" type-at-pos predicates.js 4 12 --strip-root
queries_in_file "type-at-pos" "refinement.js"
