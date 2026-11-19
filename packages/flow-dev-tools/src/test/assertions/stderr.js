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
const {applyLabelOp, isLabelOp} = require('./label');

import type {
  AssertionLocation,
  ErrorAssertion,
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

function stderr(
  expected: string | LabelOpWrite | LabelOpRead,
  assertLoc: ?AssertionLocation,
): ErrorAssertion {
  return (reason: ?string, env) => {
    const actual = env.getStderr();
    if (isLabelOp(expected)) {
      return applyLabelOp({
        op: expected,
        recorded: env.getLabels(),
        actual,
        what: 'stderr',
        assertLoc,
        reason,
        suggestion: {method: 'stderr', args: [expected]},
      });
    }
    const formattedActual = formatIfJSON(actual);
    const suggestion: Suggestion = {method: 'stderr', args: [formattedActual]};
    return simpleDiffAssertion(
      formatIfJSON(expected),
      formattedActual,
      assertLoc,
      reason,
      'stderr',
      suggestion,
    );
  };
}

module.exports = {
  default: stderr,
};
