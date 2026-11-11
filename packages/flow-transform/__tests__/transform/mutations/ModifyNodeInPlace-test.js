/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @flow strict-local
 * @format
 */

import type {Identifier, Program} from 'flow-estree';

import * as t from '../../../src/generated/node-types';
import {
  createModifyNodeInPlaceMutation,
  performModifyNodeInPlaceMutation,
} from '../../../src/transform/mutations/ModifyNodeInPlace';
import {MutationContext} from '../../../src/transform/MutationContext';
import {parseAndGetAstAndNode} from './test-utils';
import {cleanASTForSnapshot} from 'flow-parser/__test_utils__/parse';

describe('ReplaceNode', () => {
  it('Identifier', async () => {
    const {ast, target} = await parseAndGetAstAndNode<Identifier>(
      'Identifier',
      'const x = 1;',
    );
    const mutation = createModifyNodeInPlaceMutation(target, {name: 'y'});
    performModifyNodeInPlaceMutation(new MutationContext(''), mutation);
    expect(cleanASTForSnapshot(ast)).toMatchInlineSnapshot(`
      {
        "body": [
          {
            "declarations": [
              {
                "id": {
                  "name": "y",
                  "optional": false,
                  "type": "Identifier",
                  "typeAnnotation": null,
                },
                "init": {
                  "literalType": "numeric",
                  "raw": "1",
                  "type": "Literal",
                  "value": 1,
                },
                "type": "VariableDeclarator",
              },
            ],
            "kind": "const",
            "type": "VariableDeclaration",
          },
        ],
        "type": "Program",
      }
    `);
  });

  it('Program', async () => {
    const {ast, target} = await parseAndGetAstAndNode<Program>('Program', '1;');
    const mutation = createModifyNodeInPlaceMutation(target, {
      docblock: {
        directives: [],
        comment: t.BlockComment({value: ' DOCBLOCK! '}),
      },
      body: [
        t.VariableDeclaration({
          declarations: [
            t.VariableDeclarator({
              id: t.Identifier({name: 'x'}),
              init: t.StringLiteral({value: '1'}),
            }),
          ],
          kind: 'const',
        }),
      ],
    });
    performModifyNodeInPlaceMutation(new MutationContext(''), mutation);

    expect(ast.docblock?.comment).toMatchObject({
      type: 'Block',
      value: ' DOCBLOCK! ',
    });
    expect(cleanASTForSnapshot(ast)).toMatchInlineSnapshot(`
      {
        "body": [
          {
            "declarations": [
              {
                "definite": undefined,
                "id": {
                  "name": "x",
                  "optional": false,
                  "type": "Identifier",
                  "typeAnnotation": null,
                },
                "init": {
                  "raw": "'1'",
                  "type": "Literal",
                  "value": "1",
                },
                "type": "VariableDeclarator",
              },
            ],
            "kind": "const",
            "type": "VariableDeclaration",
          },
        ],
        "type": "Program",
      }
    `);
  });
});
