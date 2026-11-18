#!/bin/bash
# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under the MIT license found in the
# LICENSE file in the root directory of this source tree.

# The crawl must still follow these symlinks, as it doesn't read their targets, or doesn't report
# the files under them by their real paths.

export LC_ALL=C

link() {
  MSYS=winsymlinks:nativestrict CYGWIN=winsymlinks:nativestrict ln -s "$1" "$2"
}

# `../shared/*` doesn't cover `../shared/sub/deep.js`.
link ../shared/sub sublink
# `../root-lib` is outside the root, though its path starts with the root's.
link ../root-lib root-lib
# `../other/sub` is outside the root, so the symlink inside it isn't reported by its real path.
mkdir -p ../other/sub
link ../other/sub other_sub
link ../../third ../other/sub/nested
# A symlink outside the root, to a directory under it.
link ../root/pkg ../other/to_root

assert_ok "$FLOW" ls --strip-root | sort
