#!/bin/bash
# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under the MIT license found in the
# LICENSE file in the root directory of this source tree.

write_flowconfig() {
  local root=$1
  mkdir -p "$root"
  cat > "$root/.flowconfig" <<'EOF'
[options]
all=true
builtin_lib=core-only
include_warnings=true
EOF
}

print_file() {
  local file=$1
  printf "%s\n" ">>> $file"
  sed -n '1,120p' "$file"
}

flow_bin_arg() {
  if command -v cygpath > /dev/null 2>&1; then
    cygpath -m "$FLOW"
  else
    printf "%s\n" "$FLOW"
  fi
}

FLOW_BIN=$(flow_bin_arg)

printf "=== update-suppressions preserves eslint suppressions and adds missing ones ===\n"
write_flowconfig update_suppressions
cat > update_suppressions/test.js <<'EOF'
// $FlowFixMe[incompatible-type] eslint-disable-next-line no-fallthrough
const ok: number = 1;

function takesNumber(x: number) {}
takesNumber("nope");
EOF
assert_ok "$FLOW" dev-tools update-suppressions --check check update_suppressions
print_file update_suppressions/test.js

printf "\n=== update-suppressions reads errors from the server ===\n"
write_flowconfig update_suppressions_status
cat > update_suppressions_status/test.js <<'EOF'
function takesNumber(x: number) {}
takesNumber("nope");
EOF
assert_ok "$FLOW" dev-tools update-suppressions --check status update_suppressions_status
assert_ok "$FLOW" stop update_suppressions_status >/dev/null 2>&1
print_file update_suppressions_status/test.js

printf "\n=== update-suppressions uses the upgrade server in FLOW_TEMP_DIR ===\n"
mkdir -p update_suppressions_upgrade
cat > update_suppressions_upgrade/.flowconfig <<'EOF'
[options]
all=true
builtin_lib=core-only
include_warnings=true
lazy_mode=fs
EOF
cat > update_suppressions_upgrade/test.js <<'EOF'
function takesNumber(x: number) {}
takesNumber("nope");
EOF
TEST_FLOW_TEMP_DIR=$FLOW_TEMP_DIR
FLOW_TEMP_DIR="$FLOW_TEMP_DIR/upgrade"
export FLOW_TEMP_DIR
assert_ok "$FLOW" start --wait --lazy-mode none --merge-timeout 0 update_suppressions_upgrade
assert_ok "$FLOW" dev-tools update-suppressions --only add --check status --flowconfig-name .flowconfig update_suppressions_upgrade
assert_ok "$FLOW" stop update_suppressions_upgrade >/dev/null 2>&1
FLOW_TEMP_DIR=$TEST_FLOW_TEMP_DIR
export FLOW_TEMP_DIR
print_file update_suppressions_upgrade/test.js

printf "\n=== update-suppressions updates site annotations across roots ===\n"
mkdir -p update_sites/foo update_sites/bar
cat > update_sites/foo/.flowconfig <<'EOF'
[include]
../shared.js

[options]
all=true
builtin_lib=core-only
include_warnings=true

[lints]
all=off
EOF
cat > update_sites/bar/.flowconfig <<'EOF'
[include]
../shared.js

[options]
all=true
builtin_lib=core-only
include_warnings=true

[lints]
all=error
EOF
cat > update_sites/shared.js <<'EOF'
function f(x: {a: number}) {
  // $FlowFixMe[unnecessary-optional-chain](site=bar,foo)
  x?.a;
}
EOF
assert_ok "$FLOW" dev-tools update-suppressions --check check --sites foo,bar update_sites/foo update_sites/bar
print_file update_sites/shared.js

printf "\n=== add-comments adds suppressions for errors ===\n"
write_flowconfig add_comments
cat > add_comments/test.js <<'EOF'
function takesNumber(x: number) {}
takesNumber("nope");
EOF
assert_ok "$FLOW" dev-tools add-comments --bin "$FLOW_BIN" --check full-check --comment e2e add_comments
print_file add_comments/test.js
