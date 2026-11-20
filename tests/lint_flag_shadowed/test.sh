#!/bin/bash
# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under the MIT license found in the
# LICENSE file in the root directory of this source tree.

assert_exit "$EXIT_USAGE" \
  "$FLOW" full-check . --all --lints "unnecessary-optional-chain=error,unnecessary-optional-chain=off"
