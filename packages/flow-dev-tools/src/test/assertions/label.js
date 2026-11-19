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

import type {
  AssertionLocation,
  ErrorAssertionResult,
  Suggestion,
} from './assertionTypes';

// As an expected value, records the output under `name`.
class LabelOpWrite {
  name: string;
  constructor(name: string) {
    this.name = name;
  }
}

// As an expected value, asserts that the output is byte for byte the one recorded under `name`.
class LabelOpRead {
  name: string;
  constructor(name: string) {
    this.name = name;
  }
}

// The only way to build a label op, so no other value is one.
const labels: Readonly<{
  write: (name: string) => LabelOpWrite,
  read: (name: string) => LabelOpRead,
}> = {
  write: name => new LabelOpWrite(name),
  read: name => new LabelOpRead(name),
};

function isLabelOp(value: unknown): value is LabelOpWrite | LabelOpRead {
  return value instanceof LabelOpWrite || value instanceof LabelOpRead;
}

// `recorded` holds what the current test has recorded. Misusing a label is an error in the test, so
// it throws rather than failing the step.
function applyLabelOp({
  op,
  recorded,
  actual,
  what,
  assertLoc,
  reason,
  suggestion,
}: Readonly<{
  op: LabelOpWrite | LabelOpRead,
  recorded: Map<string, string>,
  actual: string,
  what: string,
  assertLoc: ?AssertionLocation,
  reason: ?string,
  suggestion: Suggestion,
}>): ErrorAssertionResult {
  if (op instanceof LabelOpWrite) {
    if (recorded.has(op.name)) {
      throw new Error(`Label "${op.name}" is written twice`);
    }
    recorded.set(op.name, actual);
    return {type: 'pass'};
  }
  const expected = recorded.get(op.name);
  if (expected == null) {
    throw new Error(`Label "${op.name}" is read before it is written`);
  }
  if (expected === actual) {
    return {type: 'pass'};
  }
  const diff = simpleDiffAssertion(
    expected,
    actual,
    assertLoc,
    reason,
    what,
    suggestion,
    false,
  );
  return {
    type: 'fail',
    messages: [
      `Expected ${what} to equal label "${op.name}"`,
      ...(diff.type === 'fail'
        ? diff.messages
        : ['They differ only in leading or trailing blank lines']),
    ],
    assertLoc,
    suggestion,
  };
}

module.exports = {
  labels,
  isLabelOp,
  applyLabelOp,
};

export type {LabelOpWrite, LabelOpRead};
