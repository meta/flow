#!/bin/bash
# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under the MIT license found in the
# LICENSE file in the root directory of this source tree.

# Packages that arrive as symlinks created after the server started. File
# watchers report the files under the links' targets, never the links. Each
# step compares the server against a full check.

recheck() {
  assert_ok "$FLOW" force-recheck --no-auto-start "$@"
  "$FLOW" status --no-auto-start . > server.out || true
  "$FLOW" full-check . > full.out || true
  if diff <(grep -e '^Error -' -e '^Cannot' server.out) \
      <(grep -e '^Error -' -e '^Cannot' full.out) > /dev/null; then
    echo "server matches a full check"
  else
    echo "server does not match a full check"
  fi
  cat server.out
}

# Links $2 to $1, which is relative to the directory containing $2. On Windows, MSYS and Cygwin
# create a native symlink rather than a copy.
link() {
  mkdir -p "$(dirname "$2")"
  MSYS=winsymlinks:nativestrict CYGWIN=winsymlinks:nativestrict ln -s "$1" "$2"
}

printf "\nInitial state:\n"
assert_errors "$FLOW" status --no-auto-start .

printf "\nAdd the workspace package packages/ws and link it into node_modules:\n"
mkdir -p packages/ws
echo "export const w: string = '';" > packages/ws/main.js
echo '{"name": "ws", "main": "main.js"}' > packages/ws/package.json
link ../packages/ws node_modules/ws
recheck packages/ws/package.json packages/ws/main.js

printf "\nAdd store/b@1 and link it into a new sub/node_modules:\n"
mkdir -p store/b@1
echo 'export const v: number = 1;' > store/b@1/index.js
echo '{"name": "b", "main": "index.js"}' > store/b@1/package.json
link ../../store/b@1 sub/node_modules/b
recheck store/b@1/package.json store/b@1/index.js

printf "\nAdd store/b@2 and move the link to it:\n"
mkdir -p store/b@2
echo "export const v: string = '';" > store/b@2/index.js
echo '{"name": "b", "main": "index.js"}' > store/b@2/package.json
rm sub/node_modules/b
link ../../store/b@2 sub/node_modules/b
recheck store/b@2/package.json store/b@2/index.js

printf "\nReplace the link sub/node_modules/b with a real package:\n"
rm sub/node_modules/b
mkdir sub/node_modules/b
echo 'export const v: boolean = true;' > sub/node_modules/b/index.js
echo '{"name": "b", "main": "index.js"}' > sub/node_modules/b/package.json
recheck sub/node_modules/b/package.json sub/node_modules/b/index.js

printf "\nRemove the real package, then link store/b@3 in its place:\n"
rm -r sub/node_modules/b
recheck sub/node_modules/b/package.json sub/node_modules/b/index.js
mkdir -p store/b@3
echo "export const v: string = '';" > store/b@3/lib.js
echo '{"name": "b", "main": "lib.js"}' > store/b@3/package.json
link ../../store/b@3 sub/node_modules/b
recheck store/b@3/package.json store/b@3/lib.js

rm -f server.out full.out
