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

import type {ArrayExpression} from 'flow-estree';
import type {AlignmentCase} from '../__test_utils__/alignment-utils';

import {
  expectBabelAlignment,
  expectEspreeAlignment,
} from '../__test_utils__/alignment-utils';
import {parse, parseForSnapshot} from '../__test_utils__/parse';
import {SimpleTraverser} from '../oxidized-src/traverse/SimpleTraverser';
import {nodeWith} from '../oxidized-src/transform/astNodeMutationHelpers';

const testCase: AlignmentCase = {
  code: 'const [a,,b] = [1,,2];',
  espree: {expectToFail: false},
  babel: {expectToFail: false},
};

describe('Array', () => {
  // Elisions occupy null entries; a trailing comma alone does not add a hole.
  const hole: ArrayExpression['elements'][number] = null;
  test.each([
    ['[]', [], false],
    ['[,]', [hole], false],
    ['[,,]', [hole, hole], false],
    ['[,1]', [hole, 'Literal'], false],
    ['[1,,3]', ['Literal', hole, 'Literal'], false],
    ['[1,,,3]', ['Literal', hole, hole, 'Literal'], false],
    ['[1,,]', ['Literal', hole], false],
    ['[1,]', ['Literal'], true],
    ['[1,,...rest]', ['Literal', hole, 'SpreadElement'], false],
  ])('preserves array holes in %s', (literal, kinds, trailingComma) => {
    const declaration = parse(`const value = ${literal};`).body[0];
    if (declaration.type !== 'VariableDeclaration') {
      throw new Error('Expected a variable declaration');
    }
    const array = declaration.declarations[0].init;
    if (array?.type !== 'ArrayExpression') {
      throw new Error('Expected an array expression');
    }
    expect(
      array.elements.map(element => (element === null ? null : element.type)),
    ).toEqual(kinds); // OK
    expect(array.trailingComma).toBe(trailingComma); // OK
  });

  // Expression and destructuring arrays use the same null representation.
  test('aligns expression and pattern holes', () => {
    const declaration = parse(testCase.code).body[0];
    if (declaration.type !== 'VariableDeclaration') {
      throw new Error('Expected a variable declaration');
    }
    const {id, init} = declaration.declarations[0];
    if (id.type !== 'ArrayPattern' || init?.type !== 'ArrayExpression') {
      throw new Error('Expected an array pattern and expression');
    }
    expect(id.elements[1]).toBe(hole); // OK
    expect(init.elements[1]).toBe(hole); // OK
  });

  // Array-node cloning retains holes while updating the existing child parents.
  test('clones sparse arrays without dereferencing holes', () => {
    const declaration = parse('const value = [1,,3];').body[0];
    if (declaration.type !== 'VariableDeclaration') {
      throw new Error('Expected a variable declaration');
    }
    const array = declaration.declarations[0].init;
    if (array?.type !== 'ArrayExpression') {
      throw new Error('Expected an array expression');
    }
    const clone = nodeWith(array, {trailingComma: true});
    expect(clone.elements).toHaveLength(3); // OK
    expect(clone.elements[1]).toBe(hole); // OK
    expect(clone.elements[0]?.parent).toBe(clone); // OK
    expect(clone.elements[2]?.parent).toBe(clone); // OK
  });

  // Traversal skips holes while visiting the expression and spread elements.
  test('traverses sparse arrays', () => {
    const children = [];
    SimpleTraverser.traverse(parse('const value = [1,,...rest];'), {
      enter(node, parent) {
        if (parent?.type === 'ArrayExpression') {
          children.push(node.type);
        }
      },
      leave() {},
    });
    expect(children).toEqual(['Literal', 'SpreadElement']); // OK
  });

  test('ESTree', () => {
    expect(parseForSnapshot(testCase.code)).toMatchInlineSnapshot(`
     {
       "body": [
         {
           "declarations": [
             {
               "id": {
                 "elements": [
                   {
                     "name": "a",
                     "optional": false,
                     "type": "Identifier",
                     "typeAnnotation": null,
                   },
                   null,
                   {
                     "name": "b",
                     "optional": false,
                     "type": "Identifier",
                     "typeAnnotation": null,
                   },
                 ],
                 "type": "ArrayPattern",
                 "typeAnnotation": null,
               },
               "init": {
                 "elements": [
                   {
                     "literalType": "numeric",
                     "raw": "1",
                     "type": "Literal",
                     "value": 1,
                   },
                   null,
                   {
                     "literalType": "numeric",
                     "raw": "2",
                     "type": "Literal",
                     "value": 2,
                   },
                 ],
                 "trailingComma": false,
                 "type": "ArrayExpression",
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
    expectEspreeAlignment(testCase);
  });

  test('Babel', () => {
    // Babel AST array nodes
    expect(parseForSnapshot(testCase.code, {babel: true}))
      .toMatchInlineSnapshot(`
     {
       "body": [
         {
           "declarations": [
             {
               "id": {
                 "elements": [
                   {
                     "name": "a",
                     "type": "Identifier",
                   },
                   null,
                   {
                     "name": "b",
                     "type": "Identifier",
                   },
                 ],
                 "type": "ArrayPattern",
               },
               "init": {
                 "elements": [
                   {
                     "extra": {
                       "raw": "1",
                       "rawValue": 1,
                     },
                     "type": "NumericLiteral",
                     "value": 1,
                   },
                   null,
                   {
                     "extra": {
                       "raw": "2",
                       "rawValue": 2,
                     },
                     "type": "NumericLiteral",
                     "value": 2,
                   },
                 ],
                 "type": "ArrayExpression",
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
    expectBabelAlignment(testCase);
  });
});
