/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @flow strict-local
 * @format
 */

import {parse as parseTS} from '@typescript-eslint/parser';
import {translateFlowDefToTSDef, translateFlowToTSDef} from '../../src';
// $FlowExpectedError[cannot-resolve-module]
import prettierConfig from '../../../.prettierrc.json';

describe.each([
  ['Flow definitions', translateFlowDefToTSDef],
  ['Flow source', translateFlowToTSDef],
])('%s mapped optionality', (_name, translate) => {
  // Inherited, unsigned, and signed optionality retain distinct TS modifiers.
  test.each([
    ['', false],
    ['?', true],
    ['+?', '+'],
    ['-?', '-'],
  ])(
    'preserves the modifier %s with readonly and remapped keys',
    async (modifier, optional) => {
      const source = `
      export type Input = {x?: number};
      export type Output = {
        +readonly [K in keyof Input as \`get_\${K}\`]${modifier}: Input[K],
      };
    `;
      const result = await translate(source, prettierConfig);
      const ast = parseTS(result, {
        loc: true,
        range: true,
        sourceType: 'module',
      });

      expect(result).toContain(
        `+readonly [K in keyof Input as \`get_\${K}\`]${modifier}: Input[K]`,
      ); // OK
      expect(ast.body[1]).toMatchObject({
        type: 'ExportNamedDeclaration',
        declaration: {
          type: 'TSTypeAliasDeclaration',
          typeAnnotation: {type: 'TSMappedType', readonly: '+', optional},
        },
      }); // OK
    },
  );
});
