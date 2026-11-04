/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @flow strict-local
 * @format
 */

import type {TypeNode} from '../../src/utils/ts-estree-ast-types';

import {parse} from 'flow-transform';
import {flowDefToTSDef} from '../../src/flowDefToTSDef';

function translateType(source: string): TypeNode {
  const code = `type T = ${source};`;
  const {ast, scopeManager} = parse(code);
  const [tsProgram] = flowDefToTSDef(code, ast, scopeManager, {
    recoverFromErrors: false,
  });
  const declaration = tsProgram.body[0];
  if (declaration.type !== 'TSTypeAliasDeclaration') {
    throw new Error('Expected a TypeScript type alias');
  }
  return declaration.typeAnnotation;
}

describe('inline interface type translation', () => {
  // An interface with no bases contributes only its translated object body.
  test('translates an empty inline interface directly to a type literal', () => {
    expect(translateType('interface {}')).toMatchObject({
      type: 'TSTypeLiteral',
      members: [],
    }); // OK
  });

  test('translates inline interface members without a redundant intersection', () => {
    expect(translateType('interface {foo: number}')).toMatchObject({
      type: 'TSTypeLiteral',
      members: [
        {
          type: 'TSPropertySignature',
          key: {type: 'Identifier', name: 'foo'},
          typeAnnotation: {typeAnnotation: {type: 'TSNumberKeyword'}},
        },
      ],
    }); // OK
  });

  // Qualified bases retain their namespace and generic arguments.
  test('preserves qualified interface bases in an intersection', () => {
    expect(
      translateType('interface extends A<string>, NS.B<number> {}'),
    ).toMatchObject({
      type: 'TSIntersectionType',
      types: [
        {
          type: 'TSTypeReference',
          typeName: {type: 'Identifier', name: 'A'},
          typeArguments: {params: [{type: 'TSStringKeyword'}]},
        },
        {
          type: 'TSTypeReference',
          typeName: {
            type: 'TSQualifiedName',
            left: {type: 'Identifier', name: 'NS'},
            right: {type: 'Identifier', name: 'B'},
          },
          typeArguments: {params: [{type: 'TSNumberKeyword'}]},
        },
        {type: 'TSTypeLiteral', members: []},
      ],
    }); // OK
  });
});
