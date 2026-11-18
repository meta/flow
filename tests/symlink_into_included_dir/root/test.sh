#!/bin/bash
# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under the MIT license found in the
# LICENSE file in the root directory of this source tree.

# `../shared/*` includes `../shared/top.js` but not `../shared/sub/deep.js`, which the crawl only
# reports through the symlink to `../shared/sub`, by its real path.

export LC_ALL=C

MSYS=winsymlinks:nativestrict CYGWIN=winsymlinks:nativestrict ln -s ../shared/sub sublink

assert_ok "$FLOW" ls --strip-root | sort
