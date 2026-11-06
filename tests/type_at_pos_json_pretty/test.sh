#!/bin/bash
# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under the MIT license found in the
# LICENSE file in the root directory of this source tree.

# This is the one place that pins the JSON envelope of `type-at-pos`
# (`--pretty` implies `--json`). Everywhere else type-at-pos is tested in
# friendly/hover mode. Keep this small: one case per envelope shape.
queries_in_file "type-at-pos" "basic.js"
# Null envelope: position past EOF has no type
printf "basic.js:12:5 = "
assert_ok "$FLOW" type-at-pos basic.js 12 5 --strip-root --pretty
queries_in_file "type-at-pos" "docs.js"
queries_in_file "type-at-pos" "spread.js"
queries_in_file "type-at-pos" "main.js"
