#!/bin/bash
# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under the MIT license found in the
# LICENSE file in the root directory of this source tree.

# Outside the root, the crawl reports what's under a symlink by the path it reached it by, so it
# still follows these: one to a directory outside the root with a symlink inside it, and one from
# outside the root to a directory under it. It stops at one to a directory it's already in.

export LC_ALL=C

# Links $2 to $1, which is relative to the directory containing $2. On Windows, MSYS and Cygwin
# create a native symlink rather than a copy.
link() {
  mkdir -p "$(dirname "$2")"
  MSYS=winsymlinks:nativestrict CYGWIN=winsymlinks:nativestrict ln -s "$1" "$2"
}

mkdir -p ../other/sub
link ../other/sub other_sub
link ../../third ../other/sub/nested
link ../root/pkg ../other/to_root
link .. ../other/sub/up

assert_ok "$FLOW" ls --strip-root | sort
