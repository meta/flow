/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @flow
 * @format
 */

const {default: simpleDiffAssertion} = require('./simpleDiffAssertion');
const {normalizeOutput} = require('./normalize');
const {applyLabelOp, isLabelOp} = require('./label');

import type {
  AssertionLocation,
  ErrorAssertion,
  NormalizeTag,
  Suggestion,
} from './assertionTypes';
import type {LabelOpRead, LabelOpWrite} from './label';

function formatIfJSON(actual: string) {
  try {
    return JSON.stringify(JSON.parse(actual), null, 2);
  } catch (e) {
    return actual;
  }
}

function stdout(
  expected: string | LabelOpWrite | LabelOpRead,
  assertLoc: ?AssertionLocation,
  normalize?: ReadonlyArray<NormalizeTag>,
): ErrorAssertion {
  return (reason: ?string, env) => {
    // The recorded golden `expected` is already normalized, so only the actual output needs it here.
    const actual =
      normalize == null
        ? env.getStdout()
        : normalizeOutput(normalize, env.getStdout(), env.getProjectDir());
    if (isLabelOp(expected)) {
      return applyLabelOp({
        op: expected,
        recorded: env.getLabels(),
        actual,
        what: 'stdout',
        assertLoc,
        reason,
        suggestion:
          normalize == null
            ? {method: 'stdout', args: [expected]}
            : {method: 'stdout', args: [expected, {normalize}]},
      });
    }
    const formattedActual = formatIfJSON(actual);
    const suggestion: Suggestion =
      normalize == null
        ? {method: 'stdout', args: [formattedActual]}
        : {method: 'stdout', args: [formattedActual, {normalize}]};
    return simpleDiffAssertion(
      formatIfJSON(expected),
      formattedActual,
      assertLoc,
      reason,
      'stdout',
      suggestion,
    );
  };
}

module.exports = {
  default: stdout,
};
