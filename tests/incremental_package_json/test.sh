#!/bin/bash
# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under the MIT license found in the
# LICENSE file in the root directory of this source tree.

# Every step checks status with --no-auto-start: the server must survive each
# package.json change and recheck it incrementally.

recheck() {
  assert_ok "$FLOW" force-recheck --no-auto-start "$@"
  assert_errors "$FLOW" status --no-auto-start .
}

printf "\nInitial state:\n"
assert_errors "$FLOW" status --no-auto-start .

printf "\nChange dep's main to b.js:\n"
echo '{"name": "dep", "main": "b.js"}' > node_modules/dep/package.json
recheck node_modules/dep/package.json

printf "\nRename the Haste package pkg to other:\n"
echo '{"name": "other", "main": "index.js"}' > pkg/package.json
recheck pkg/package.json

printf "\nMake pkg/package.json invalid:\n"
echo '{' > pkg/package.json
recheck pkg/package.json

printf "\nName the Haste package pkg again:\n"
echo '{"name": "pkg", "main": "index.js"}' > pkg/package.json
recheck pkg/package.json

printf "\nDelete dep's package.json:\n"
rm node_modules/dep/package.json
recheck node_modules/dep/package.json

printf "\nAdd dep's package.json back with main a.js:\n"
echo '{"name": "dep", "main": "a.js"}' > node_modules/dep/package.json
recheck node_modules/dep/package.json

printf "\nAdd another Haste package named pkg, which becomes the provider:\n"
mkdir alt
echo "export const y: string = '';" > alt/main.js
echo '{"name": "pkg", "main": "main.js"}' > alt/package.json
recheck alt/package.json alt/main.js

printf "\nRemove the other Haste package:\n"
rm -r alt
recheck alt/package.json alt/main.js
