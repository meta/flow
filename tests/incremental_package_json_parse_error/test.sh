#!/bin/bash
# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under the MIT license found in the
# LICENSE file in the root directory of this source tree.

# Changes between invalid package.json files keep the server running, so the
# recheck has to report their parse errors the way a full check does.

printf "\nInitial state:\n"
assert_ok "$FLOW" status --no-auto-start .

printf "\nAdd an invalid bad/package.json:\n"
mkdir bad
echo '{' > bad/package.json
assert_ok "$FLOW" force-recheck --no-auto-start bad/package.json
assert_errors "$FLOW" status --no-auto-start .

printf "\nMake it invalid in another way:\n"
printf '{\n  "name": ]\n}\n' > bad/package.json
assert_ok "$FLOW" force-recheck --no-auto-start bad/package.json
assert_errors "$FLOW" status --no-auto-start .
