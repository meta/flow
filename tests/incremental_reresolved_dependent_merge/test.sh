#!/bin/bash
# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under the MIT license found in the
# LICENSE file in the root directory of this source tree.

printf "\nInitial state:\n"
assert_errors "$FLOW" status --no-auto-start .

# F.js's import of ./r stops resolving, while the only other signature F.js
# depends on (d.js) stays the same: F.js still has to be merged again.
printf "\nDelete r.js and edit the body of q.js in the same recheck:\n"
rm r.js
printf 'export function body(): number {\n  return 2;\n}\n' > q.js
assert_ok "$FLOW" force-recheck --no-auto-start r.js q.js
assert_errors "$FLOW" status --no-auto-start .
