#!/bin/bash
# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under the MIT license found in the
# LICENSE file in the root directory of this source tree.

# Packages linked into each other's node_modules form cycles. Without the implicit root, the
# `[include]` glob covers each package, so the crawl reads them itself and doesn't follow the links.

export LC_ALL=C

# Links $2 to $1, which is relative to the directory containing $2. On Windows, MSYS and Cygwin
# create a native symlink rather than a copy.
link() {
  mkdir -p "$(dirname "$2")"
  MSYS=winsymlinks:nativestrict CYGWIN=winsymlinks:nativestrict ln -s "$1" "$2"
}

for package in a b c; do
  for other in a b c; do
    [ "$package" = "$other" ] || link "../../$other" "packages/$package/node_modules/$other"
  done
done

assert_ok "$FLOW" ls --strip-root | sort
