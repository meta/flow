/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @flow strict-local
 * @format
 */

import {parse, print} from '../../src';

function semanticAST(value: unknown): unknown {
  if (Array.isArray(value)) {
    return value.map(semanticAST);
  }
  if (value == null || typeof value !== 'object') {
    return value;
  }
  return Object.fromEntries(
    Object.entries(value)
      .filter(
        ([key]) =>
          ![
            'parent',
            'loc',
            'range',
            'raw',
            'comments',
            'docblock',
            'tokens',
          ].includes(key),
      )
      .map(([key, child]) => [key, semanticAST(child)]),
  );
}

test.each([
  'type B<T: NS.Box<Map<string, Array<number>>>> = ReadonlyArray<T>;',
  'type B<T extends NS.Box<Array<string>>> = Array<T>;',
  'interface I<T> extends NS.Base<Array<T>> {} declare class C<T> extends NS.Base<T> implements NS.I<Map<string, T>> {}',
  'type I<T> = interface extends NS.Base<Array<T>> {};',
  '/** @flow */ type T = Box</* arg */ Array<string>>;',
  'enum E { A, B, ... }',
  'enum E of string { A = "a", B = "b", ... }',
  'enum E of number { A = 1, B = 2 }',
  'enum E of boolean { A = true, B = false }',
  'enum E of bigint { A = 1n }',
  'enum E of symbol {}',
  'declare enum E of string { A = "a" }',
])('the current parser/printer preserves full semantics: %s', async code => {
  const expected = semanticAST(parse(code).ast);
  const output = await print(parse(code).ast, code, {});
  expect(semanticAST(parse(output).ast)).toEqual(expected); // OK
  expect(await print(parse(output).ast, output, {})).toBe(output); // OK
  if (code.includes('/* arg */')) {
    expect(output).toContain('/* arg */'); // OK
  }
});
