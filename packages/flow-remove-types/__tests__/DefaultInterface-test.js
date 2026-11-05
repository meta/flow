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

import {SourceTextModule} from 'vm';
import stripFlowTypes from '../index';

// Type-only wrappers disappear in both whitespace-preserving and compact output.
test.each([false, true])('strips default interfaces with pretty=%s', pretty => {
  const output = stripFlowTypes('export default interface I {}', {
    all: true,
    pretty,
  }).toString();

  expect(output.trim()).toBe(''); // OK
});

// Runtime defaults remain executable after an adjacent type-only default is erased.
test.each([false, true])(
  'preserves runtime defaults with pretty=%s',
  async pretty => {
    const output = stripFlowTypes(
      'export default interface I {}\nexport default 42;',
      {all: true, pretty},
    ).toString();
    const module = new SourceTextModule(output);
    await module.link(() => {
      throw new Error('Unexpected import');
    });
    await module.evaluate();

    expect(module.namespace.default).toBe(42); // OK
  },
);

// Runtime declarations still receive ordinary type erasure, not wrapper deletion.
test.each([false, true])(
  'preserves typed runtime default functions with pretty=%s',
  async pretty => {
    const output = stripFlowTypes(
      'export default function value(): number { return 42; }',
      {all: true, pretty},
    ).toString();
    const module = new SourceTextModule(output);
    await module.link(() => {
      throw new Error('Unexpected import');
    });
    await module.evaluate();

    expect(module.namespace.default()).toBe(42); // OK
  },
);
