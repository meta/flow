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

import type {AlignmentCase} from '../__test_utils__/alignment-utils';

import {
  expectBabelAlignment,
  expectEspreeAlignment,
} from '../__test_utils__/alignment-utils';
import {
  parse,
  parseForSnapshot,
  printForSnapshotBabel,
  printForSnapshotESTree,
} from '../__test_utils__/parse';

describe('Definite assignment assertions', () => {
  describe('Variable', () => {
    const testCase: AlignmentCase = {
      code: `
        let count!: number;
      `,
      espree: {
        expectToFail: 'espree-exception',
        expectedExceptionMessage: 'Unexpected token !',
      },
      babel: {
        expectToFail: 'babel-exception',
        expectedExceptionMessage: 'Unexpected token, expected ";"',
      },
    };

    test('ESTree', async () => {
      expect(parseForSnapshot(testCase.code)).toMatchInlineSnapshot(`
       {
         "body": [
           {
             "declarations": [
               {
                 "definite": true,
                 "id": {
                   "name": "count",
                   "optional": false,
                   "type": "Identifier",
                   "typeAnnotation": {
                     "type": "TypeAnnotation",
                     "typeAnnotation": {
                       "type": "NumberTypeAnnotation",
                     },
                   },
                 },
                 "init": null,
                 "type": "VariableDeclarator",
               },
             ],
             "kind": "let",
             "type": "VariableDeclaration",
           },
         ],
         "type": "Program",
       }
      `);
      expect(await printForSnapshotESTree(testCase.code)).toBe(
        testCase.code.trim(),
      );
      expectEspreeAlignment(testCase);
    });

    test('Babel', async () => {
      expect(parseForSnapshot(testCase.code, {babel: true}))
        .toMatchInlineSnapshot(`
       {
         "body": [
           {
             "declarations": [
               {
                 "id": {
                   "name": "count",
                   "type": "Identifier",
                   "typeAnnotation": {
                     "type": "TypeAnnotation",
                     "typeAnnotation": {
                       "type": "NumberTypeAnnotation",
                     },
                   },
                 },
                 "init": null,
                 "type": "VariableDeclarator",
               },
             ],
             "kind": "let",
             "type": "VariableDeclaration",
           },
         ],
         "type": "Program",
       }
      `);
      expect(await printForSnapshotBabel(testCase.code)).toMatchInlineSnapshot(
        `"let count: number;"`,
      );
      expectBabelAlignment(testCase);
    });
  });

  describe('Class fields', () => {
    const testCase: AlignmentCase = {
      code: `
        class Box {
          value!: number;
          #secret!: string;
        }
      `,
      espree: {
        expectToFail: 'espree-exception',
        expectedExceptionMessage: 'Unexpected token !',
      },
      babel: {
        expectToFail: 'babel-exception',
        expectedExceptionMessage: 'Unexpected token',
      },
    };

    test('ESTree', async () => {
      expect(parseForSnapshot(testCase.code)).toMatchInlineSnapshot(`
       {
         "body": [
           {
             "body": {
               "body": [
                 {
                   "computed": false,
                   "declare": false,
                   "decorators": [],
                   "definite": true,
                   "key": {
                     "name": "value",
                     "optional": false,
                     "type": "Identifier",
                     "typeAnnotation": null,
                   },
                   "optional": false,
                   "override": false,
                   "static": false,
                   "tsAccessibility": null,
                   "type": "PropertyDefinition",
                   "typeAnnotation": {
                     "type": "TypeAnnotation",
                     "typeAnnotation": {
                       "type": "NumberTypeAnnotation",
                     },
                   },
                   "value": null,
                   "variance": null,
                 },
                 {
                   "computed": false,
                   "declare": false,
                   "decorators": [],
                   "definite": true,
                   "key": {
                     "name": "secret",
                     "optional": false,
                     "type": "PrivateIdentifier",
                     "typeAnnotation": null,
                   },
                   "optional": false,
                   "override": false,
                   "static": false,
                   "tsAccessibility": null,
                   "type": "PropertyDefinition",
                   "typeAnnotation": {
                     "type": "TypeAnnotation",
                     "typeAnnotation": {
                       "type": "StringTypeAnnotation",
                     },
                   },
                   "value": null,
                   "variance": null,
                 },
               ],
               "type": "ClassBody",
             },
             "decorators": [],
             "id": {
               "name": "Box",
               "optional": false,
               "type": "Identifier",
               "typeAnnotation": null,
             },
             "implements": [],
             "superClass": null,
             "superTypeArguments": null,
             "type": "ClassDeclaration",
             "typeParameters": null,
           },
         ],
         "type": "Program",
       }
      `);
      expect(await printForSnapshotESTree(testCase.code))
        .toMatchInlineSnapshot(`
       "class Box {
         value!: number;
         #secret!: string;
       }"
      `);
      expectEspreeAlignment(testCase);
    });

    test('Babel', async () => {
      expect(parseForSnapshot(testCase.code, {babel: true}))
        .toMatchInlineSnapshot(`
       {
         "body": [
           {
             "body": {
               "body": [
                 {
                   "computed": false,
                   "key": {
                     "name": "value",
                     "type": "Identifier",
                   },
                   "static": false,
                   "type": "ClassProperty",
                   "typeAnnotation": {
                     "type": "TypeAnnotation",
                     "typeAnnotation": {
                       "type": "NumberTypeAnnotation",
                     },
                   },
                   "value": null,
                   "variance": null,
                 },
                 {
                   "key": {
                     "id": {
                       "name": "secret",
                       "type": "Identifier",
                     },
                     "type": "PrivateName",
                   },
                   "static": false,
                   "type": "ClassPrivateProperty",
                   "typeAnnotation": {
                     "end": 73,
                     "loc": {
                       "end": {
                         "column": 26,
                         "line": 4,
                       },
                       "start": {
                         "column": 18,
                         "line": 4,
                       },
                     },
                     "start": 65,
                     "type": "TypeAnnotation",
                     "typeAnnotation": {
                       "end": 73,
                       "loc": {
                         "end": {
                           "column": 26,
                           "line": 4,
                         },
                         "start": {
                           "column": 20,
                           "line": 4,
                         },
                       },
                       "start": 67,
                       "type": "StringTypeAnnotation",
                     },
                   },
                   "value": null,
                   "variance": null,
                 },
               ],
               "type": "ClassBody",
             },
             "id": {
               "name": "Box",
               "type": "Identifier",
             },
             "superClass": null,
             "type": "ClassDeclaration",
           },
         ],
         "type": "Program",
       }
      `);
      expect(await printForSnapshotBabel(testCase.code)).toMatchInlineSnapshot(`
       "class Box {
         value: number;
         #secret: string;
       }"
      `);
      expectBabelAlignment(testCase);
    });
  });

  test('No `definite` property without `!`', () => {
    const ast = parse('let count: number; class Box { value: number; }', {});
    const [declaration, classDeclaration] = ast.body;
    if (
      declaration.type !== 'VariableDeclaration' ||
      classDeclaration.type !== 'ClassDeclaration'
    ) {
      throw new Error('expected a variable declaration and a class');
    }
    expect(declaration.declarations[0]).not.toHaveProperty('definite');
    expect(classDeclaration.body.body[0]).not.toHaveProperty('definite');
  });

  test.each([
    [
      'let count!: number = 1;',
      'Declarations with initializers cannot also have definite assignment assertions.',
    ],
    [
      'let count!;',
      'Declarations with definite assignment assertions must also have type annotations.',
    ],
    [
      'class Box { static total!: number; }',
      'A definite assignment assertion `!` is not permitted in this context.',
    ],
  ])('Error: %s', (code, message) => {
    expect(() => parse(code, {})).toThrow(message);
  });
});
