#!/bin/bash
# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under the MIT license found in the
# LICENSE file in the root directory of this source tree.

: > .flow.saved_state_file_changes

"$FLOW" start --saved-state-fetcher none --wait
"$FLOW" save-state --out .flow.saved_state >> /dev/null
"$FLOW" stop

echo '{"main": "b.js"}' > pkg/package.json
echo 'pkg/package.json' > .flow.saved_state_file_changes

printf "Start from the saved state after changing pkg's main to b.js:\n"
assert_ok "$FLOW" start --saved-state-fetcher local --saved-state-no-fallback --wait
assert_ok "$FLOW" status --no-auto-start --strip-root
assert_ok "$FLOW" stop
