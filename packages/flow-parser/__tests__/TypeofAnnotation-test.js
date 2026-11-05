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

import {
  parse,
  parseForSnapshotBabel,
  parseForSnapshotESTree,
  printForSnapshotBabel,
} from '../__test_utils__/parse';

// Absent arguments are omitted for each supported typeof target shape.
test.each([
  'type T = typeof f;',
  'type T = typeof NS.f;',
  "type T = typeof import('module');",
])('omits absent typeof arguments in %s', source => {
  const declaration = parse(source).body[0];
  if (
    declaration.type !== 'TypeAlias' ||
    declaration.right.type !== 'TypeofTypeAnnotation'
  ) {
    throw new Error('Expected a typeof type annotation');
  }
  expect(declaration.right).not.toHaveProperty('typeArguments'); // OK
});

// Consuming an absent argument slot does not disturb later nodes or arguments.
test('preserves decoding after omitted typeof arguments', () => {
  const ast = parse(
    'type A = typeof f; type B = typeof g<number>; const value = 1;',
  );
  expect(ast.body).toHaveLength(3); // OK
  expect(ast.body[0]).not.toHaveProperty('right.typeArguments'); // OK
  expect(ast.body[1]).toHaveProperty(
    'right.typeArguments.params.0.type',
    'NumberTypeAnnotation',
  ); // OK
  expect(ast.body[2]).toHaveProperty('declarations.0.init.value', 1); // OK
});

describe('typeof expression', () => {
  const code1 = 'type T = typeof f;';
  const code2 = 'type T = typeof f<number>;';

  test('ESTree', async () => {
    expect(await parseForSnapshotESTree(code1)).toMatchSnapshot();
    expect(await parseForSnapshotESTree(code2)).toMatchSnapshot();
  });

  test('Babel', async () => {
    expect(await parseForSnapshotBabel(code1)).toMatchSnapshot();
    expect(await parseForSnapshotBabel(code2)).toMatchSnapshot();
    expect(await printForSnapshotBabel(code1)).toMatchInlineSnapshot(
      `"type T = typeof f;"`,
    );
    expect(await printForSnapshotBabel(code2)).toMatchInlineSnapshot(
      `"type T = typeof f;"`,
    );
  });
});
