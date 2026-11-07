#!/usr/bin/env bash
# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under the MIT license found in the
# LICENSE file in the root directory of this source tree.

# Run all jest tests across the unified flow/packages/ workspace. Replaces the
# previous per-package orchestrator now that jest config / babel config /
# devDeps live at the workspace root.
#
# Usage:
#   ./runOxidizedJestTests.sh                       # run every package
#   ./runOxidizedJestTests.sh -- -t 'MatchExpression'  # forward jest flags

set -e -o pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"
WORKSPACE_DIR="$(cd "$SCRIPT_DIR/.." && pwd -P)"
FBCODE="$(cd "$WORKSPACE_DIR/../.." && pwd -P)"
FBSOURCE="$(cd "$FBCODE/.." && pwd -P)"

# fbsource third-party Node toolchain (24.x). The system Node may be 16.x which
# lacks `os.availableParallelism()` — required by jest 30's `getMaxWorkers`.
NODE="$FBSOURCE/xplat/third-party/node/bin/node"

PARSER_DIR="$WORKSPACE_DIR/flow-parser"

# ---- argv parsing ---------------------------------------------------------
JEST_EXTRA=()
SAW_DASHDASH=0
for arg in "$@"; do
  if [[ "$SAW_DASHDASH" == 1 ]]; then
    JEST_EXTRA+=("$arg")
  elif [[ "$arg" == "--" ]]; then
    SAW_DASHDASH=1
  fi
done

# Serialize dist/ rebuilds + reads against runContractTests.sh, which also
# rebuilds the generated parser output and then `require()`s
# `flow-parser`. Buck
# schedules `oxidized_jest_test` and `wasm_parser_contract_test` in
# parallel; without this flock, target A's jest can read a torn
# `dist/index.js` (un-stripped Flow `import type` lines from the cp -r
# step) while target B's `yarn build` is mid-flight, surfacing as
#   `SyntaxError: Cannot use import statement outside a module` or
#   `TypeError: FlowParserWASMModule is not a function`.
# Hold the lock through both `yarn build` AND the jest run so a concurrent
# target can't clobber generated output while we're reading from it.
DIST_FLOCK="$PARSER_DIR/.dist.flock"
exec 9> "$DIST_FLOCK"
echo "==> waiting for exclusive lock on $DIST_FLOCK"
flock -x 9
echo "==> acquired lock on $DIST_FLOCK"

cd "$WORKSPACE_DIR"

# yarn.lock is regenerated on first install. Subsequent runs reuse the locked
# tree. We rely on the global fbcode yarn offline mirror so --offline is the
# right mode here.
yarn install --offline

# Build output for each package (and embed the WASM parser into
# flow-parser/dist/FlowParserWASM.js). The unified jest config maps
# `*/FlowParserWASM` -> flow-parser/dist/FlowParserWASM.js so tests can
# resolve the wasm wrapper from src-level imports.
yarn build

# The upstream jest testMatch is intentionally broad. In this fork it also
# picks up standalone packages outside the oxidized workspace. Keep their
# locked dependency trees available before the root Jest invocation.
for standalone_package in \
  babel-plugin-transform-flow-enums \
  eslint-plugin-fb-flow
do
  (
    cd "$WORKSPACE_DIR/$standalone_package"
    yarn install --frozen-lockfile --offline --ignore-scripts --non-interactive
  )
done

# Run jest from the workspace root using the unified jest.config.js. The
# moduleNameMapper auto-derived from workspaces[] points each package name at
# its src/index.js; the explicit FlowParserWASM mapping points wasm requires
# at the just-built dist artifact.
"$NODE" --experimental-vm-modules \
  ./node_modules/.bin/jest --no-coverage "${JEST_EXTRA[@]}"
