/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @flow strict-local
 * @format
 */

'use strict';

// Phase D1 / task #13 sub-test 1: API-surface diff.
//
// `flow-parser` is intended as a drop-in replacement for
// `hermes-parser`. This suite asserts that the public export surface of our
// package is a superset of upstream: every name upstream exports is also
// exported by us, and the `typeof` of each shared export agrees.
//
// We intentionally allow our package to export *additional* names — the
// distinction between "drop-in replacement" and "exact mirror" is that callers
// who follow the upstream-documented API must continue to work; callers who
// reach into undocumented internals are not supported either way. The list of
// our package's *extra* exports (compared to upstream) is recorded as a
// snapshot so any future addition is reviewed.
//
// Pin: hermes-parser/hermes-estree are pinned to 0.35.0 in devDependencies,
// which matches the vendored upstream source under
// xplat/static_h/tools/hermes-parser/. If the pin moves, the
// adapter-fixup audit (ADAPTER_GAPS.md) needs to be re-run against the new
// upstream behavior.

type ModuleExports = interface {
  readonly [key: string]: unknown,
};

const ours: ModuleExports = require('flow-parser');
const upstream: ModuleExports = require('hermes-parser');

describe('Public export surface vs hermes-parser', () => {
  test('every upstream export name is present on flow-parser', () => {
    const upstreamNames = Object.keys(upstream).sort();
    const ourNames = new Set(Object.keys(ours));
    const missing = upstreamNames.filter(name => !ourNames.has(name));
    expect(missing).toEqual([]);
  });

  test('typeof of each shared export agrees with hermes-parser', () => {
    const upstreamNames = Object.keys(upstream).sort();
    const mismatches: Array<{
      name: string,
      upstream: string,
      ours: string,
    }> = [];
    for (const name of upstreamNames) {
      const upstreamType = typeof upstream[name];
      const ourType = typeof ours[name];
      if (upstreamType !== ourType) {
        mismatches.push({name, upstream: upstreamType, ours: ourType});
      }
    }
    expect(mismatches).toEqual([]);
  });

  test('extra exports beyond upstream are recorded explicitly', () => {
    // Capture the *delta* of names we export that upstream does not. The
    // snapshot pins this set so a new export is reviewed (it might be a
    // legitimate fork-only addition, or it might be an accidental leak).
    const upstreamNames = new Set(Object.keys(upstream));
    const extras = Object.keys(ours)
      .filter(name => !upstreamNames.has(name))
      .sort();
    expect(extras).toMatchInlineSnapshot(`[]`);
  });
});
