#!/bin/bash
# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under the MIT license found in the
# LICENSE file in the root directory of this source tree.

echo "Check Existing Server With Warnings (zero exit code):"
assert_ok "$FLOW" full-check --include-warnings --lints "unnecessary-optional-chain=warn"
printf "\n"

echo "Check Existing Server With Warnings and --max-warnings 0 (nonzero exit code):"
assert_errors "$FLOW" full-check --max-warnings 0 --lints "unnecessary-optional-chain=warn"

echo "Check Existing Server With Errors (nonzero exit code):"
assert_errors "$FLOW" full-check --include-warnings --lints "unnecessary-optional-chain=error"
printf "\n\n"

# Status and check contents and both don't accept options flags, so they are manipulated by modifying the flowconfig

echo "Status (Running Server) With Warnings (zero exit code):"
assert_ok "$FLOW" stop
cp warn_flowconfig .flowconfig
start_flow .
assert_ok "$FLOW" status --include-warnings
assert_errors "$FLOW" status --max-warnings 0
assert_ok "$FLOW" status --max-warnings 1
printf "\n"

echo "Status (Running Server) With Errors (nonzero exit code):"
assert_ok "$FLOW" stop
cp error_flowconfig .flowconfig
start_flow .
assert_errors "$FLOW" status --include-warnings
printf "\n\n"


echo "Status (No Running Server) With Warnings (zero exit code):"
assert_ok "$FLOW" stop
cp warn_flowconfig .flowconfig
assert_ok "$FLOW" status --include-warnings
printf "\n"

echo "Status (No Running Server) With Errors (nonzero exit code):"
assert_ok "$FLOW" stop
cp error_flowconfig .flowconfig
assert_errors "$FLOW" status --include-warnings
printf "\n\n"

# Kill the server before we start messing around with the flowconfig for check-contents.
assert_ok "$FLOW" stop

echo "Check Contents With Warnings (zero exit code):"
cp warn_flowconfig .flowconfig
assert_ok "$FLOW" check-contents --include-warnings test.js < test.js
assert_errors "$FLOW" check-contents --include-warnings --max-warnings 0 test.js < test.js
printf "\n"

# Kill the server before we start messing around with the flowconfig for check-contents.
assert_ok "$FLOW" stop

echo "Check Contents With Errors (nonzero exit code):"
cp error_flowconfig .flowconfig
assert_errors "$FLOW" check-contents --include-warnings test.js < test.js
printf "\n\n"
