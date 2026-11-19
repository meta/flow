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

import type {ESNode} from 'flow-estree';

import {cleanASTForSnapshot, parse} from '../../__test_utils__/parse';
import {SimpleTransform} from '../../oxidized-src/transform/SimpleTransform';

function expectTransformToEqual({
  code,
  result,
  transform,
}: Readonly<{
  code: string,
  result: string,
  transform: (v: ESNode) => ESNode | null,
}>): void {
  const codeAST = parse(code);
  const resultAST = parse(result);

  const transformedAST = SimpleTransform.transform(codeAST, {transform});
  expect(
    transformedAST == null ? null : cleanASTForSnapshot(transformedAST),
  ).toEqual(cleanASTForSnapshot(resultAST));
}

describe('SimpleTransform', () => {
  describe('Remove', () => {
    it('Statement', () => {
      expectTransformToEqual({
        code: `a; function b() {}`,
        result: `function b() {}`,
        transform(node) {
          if (node.type === 'ExpressionStatement') {
            return null;
          }
          return node;
        },
      });
    });
    it('TypeAnnotation', () => {
      expectTransformToEqual({
        code: `function b(): void {}`,
        result: `function b() {}`,
        transform(node) {
          if (node.type === 'TypeAnnotation') {
            return null;
          }
          return node;
        },
      });
    });
  });
  describe('Replace', () => {
    it('Statement', () => {
      let retraversedReplacedNode = false;
      expectTransformToEqual({
        code: `a; function b() {}`,
        result: `b; function b() {}`,
        transform(node) {
          if (node.type === 'ExpressionStatement') {
            if (
              node.expression.type === 'Identifier' &&
              node.expression.name === 'b'
            ) {
              retraversedReplacedNode = true;
              return node;
            }
            // $FlowFixMe[incompatible-type]
            return {
              type: 'ExpressionStatement',
              expression: {
                type: 'Identifier',
                name: 'b',
                typeAnnotation: null,
                optional: false,
              },
              directive: null,
            };
          }
          return node;
        },
      });
      expect(retraversedReplacedNode).toBeTruthy();
    });
    it('TypeAnnotation', () => {
      expectTransformToEqual({
        code: `function b(): void {}`,
        result: `function b(): string {}`,
        transform(node) {
          if (node.type === 'VoidTypeAnnotation') {
            // $FlowFixMe[incompatible-type]
            return {
              type: 'StringTypeAnnotation',
            };
          }
          return node;
        },
      });
    });
    it('Nested expressions', () => {
      expectTransformToEqual({
        code: `a as number as string;`,
        result: `a;`,
        transform(node) {
          if (node.type === 'AsExpression') {
            return node.expression;
          }
          return node;
        },
      });
    });
  });
});
