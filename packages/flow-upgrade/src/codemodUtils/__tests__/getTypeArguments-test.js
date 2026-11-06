/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @format
 */

import {getTypeArguments} from '../getTypeArguments';

const canonical = {type: 'TypeParameterInstantiation', params: []};
const legacy = {type: 'TypeParameterInstantiation', params: []};

// Read either published AST contract without changing the input node.
test.each([
  [{typeArguments: canonical}, canonical],
  [{typeParameters: legacy}, legacy],
  [{typeArguments: null}, null],
  [{typeParameters: null}, null],
  [{}, undefined],
])('reads type arguments from %p', (fields, expected) => {
  const node = Object.freeze({type: 'GenericTypeAnnotation', ...fields});
  expect(getTypeArguments(node)).toBe(expected); // OK
});

// A canonical field is authoritative, including an explicit null or undefined.
test.each([canonical, null, undefined])(
  'prefers canonical type arguments %p over a legacy field',
  typeArguments => {
    expect(
      getTypeArguments({
        type: 'GenericTypeAnnotation',
        typeArguments,
        typeParameters: legacy,
      }),
    ).toBe(typeArguments); // OK
  },
);
