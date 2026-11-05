/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @flow strict-local
 * @format
 */

import {parse} from '../../__test_utils__/parse';
import {SimpleTraverser} from '../../oxidized-src/traverse/SimpleTraverser';
import {transformProgram} from '../../oxidized-src/estree/StripFlowTypesForBabel';

it('Babel stripping constructs canonical ESTree symbol and bigint references', () => {
  const code = 'type S = symbol; type B = bigint;';
  const ast = transformProgram(parse(code), {});
  const names = [];
  SimpleTraverser.traverse(ast, {
    enter(node, parent) {
      if (node.type === 'GenericTypeAnnotation') {
        expect(node.typeArguments).toBeNull();
        expect(node).not.toHaveProperty('typeParameters');
        if (node.id.type === 'Identifier') {
          names.push(node.id.name);
        }
      }
      if (parent != null) {
        expect(node.parent).toBe(parent);
      }
    },
    leave() {},
  });
  expect(names).toEqual(['symbol', 'bigint']);
  const babel = parse(code, {babel: true});
  expect(babel.program.body.map(node => node.right)).toEqual([
    expect.objectContaining({
      type: 'GenericTypeAnnotation',
      id: expect.objectContaining({name: 'symbol'}),
      typeParameters: null,
    }),
    expect.objectContaining({
      type: 'GenericTypeAnnotation',
      id: expect.objectContaining({name: 'bigint'}),
      typeParameters: null,
    }),
  ]);
  for (const node of babel.program.body) {
    expect(node.right).not.toHaveProperty('typeArguments');
  }
});
