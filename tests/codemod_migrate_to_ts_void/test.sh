#!/bin/bash
# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under the MIT license found in the
# LICENSE file in the root directory of this source tree.

printf "flow codemod migrate-to-ts-void --write\n"
assert_ok "$FLOW" codemod migrate-to-ts-void --strip-root --quiet --write .
printf "\ntypes.js\n"
cat types.js
printf "\nuses.js\n"
cat uses.js
printf "\nflow check under experimental.ts_void\n"
assert_ok "$FLOW" check --flowconfig-name .flowconfig.ts_void
