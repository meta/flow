#!/bin/bash
# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under the MIT license found in the
# LICENSE file in the root directory of this source tree.

echo "No errors initially:"
assert_ok "$FLOW" status .
echo ""

echo "A libdef that no longer parses is reported after the reinit:"
printf 'declare const myGlobal: number;\n<<<<<<< local\n' > lib/globals.js
assert_ok "$FLOW" force-recheck --no-auto-start lib/globals.js
assert_errors "$FLOW" status --no-auto-start .
echo ""

echo "Fixing the libdef removes the error:"
echo "declare const myGlobal: number;" > lib/globals.js
assert_ok "$FLOW" force-recheck --no-auto-start lib/globals.js
assert_ok "$FLOW" status --no-auto-start .
