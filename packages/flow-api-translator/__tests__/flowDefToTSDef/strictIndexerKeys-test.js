/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @flow strict-local
 * @format
 */

import {parse} from 'flow-transform';
import {flowDefToTSDef} from '../../src/flowDefToTSDef';

describe('strict generic indexer key translation', () => {
  // Strict translation rejects keys that would lose qualification or type arguments.
  test.each(['K', 'NS.Keys', '$Call<F>'])('rejects the key %s', key => {
    const code = `type T = {[key: ${key}]: number};`;
    const {ast, scopeManager} = parse(code);
    expect(() =>
      flowDefToTSDef(code, ast, scopeManager, {recoverFromErrors: false}),
    ).toThrow('generic indexer keys'); // ERROR
  });
});
