#!/bin/bash
# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under the MIT license found in the
# LICENSE file in the root directory of this source tree.

# The crawl reports these files only through the symlinks to them: `../shared/*` includes
# `../shared/top.js` but not `../shared/sub/deep.js`, and `../root-lib` is outside the root, though
# its path starts with the root's.

export LC_ALL=C

MSYS=winsymlinks:nativestrict CYGWIN=winsymlinks:nativestrict ln -s ../shared/sub sublink
MSYS=winsymlinks:nativestrict CYGWIN=winsymlinks:nativestrict ln -s ../root-lib root-lib

assert_ok "$FLOW" ls --strip-root | sort
