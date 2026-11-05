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
import {parseForSnapshot} from '../__test_utils__/parse';

describe('PropertyDefinition', () => {
  const testCase: AlignmentCase = {
    code: `
      class C {
        foo;
        bar = 1;
        static staticProp = 1;
      }
    `,
    espree: {
      expectToFail: false,
    },
    babel: {
      expectToFail: false,
    },
  };

  test('ESTree', () => {
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
                 "key": {
                   "name": "foo",
                   "optional": false,
                   "type": "Identifier",
                   "typeAnnotation": null,
                 },
                 "optional": false,
                 "override": false,
                 "static": false,
                 "tsAccessibility": null,
                 "type": "PropertyDefinition",
                 "typeAnnotation": null,
                 "value": null,
                 "variance": null,
               },
               {
                 "computed": false,
                 "declare": false,
                 "decorators": [],
                 "key": {
                   "name": "bar",
                   "optional": false,
                   "type": "Identifier",
                   "typeAnnotation": null,
                 },
                 "optional": false,
                 "override": false,
                 "static": false,
                 "tsAccessibility": null,
                 "type": "PropertyDefinition",
                 "typeAnnotation": null,
                 "value": {
                   "literalType": "numeric",
                   "raw": "1",
                   "type": "Literal",
                   "value": 1,
                 },
                 "variance": null,
               },
               {
                 "computed": false,
                 "declare": false,
                 "decorators": [],
                 "key": {
                   "name": "staticProp",
                   "optional": false,
                   "type": "Identifier",
                   "typeAnnotation": null,
                 },
                 "optional": false,
                 "override": false,
                 "static": true,
                 "tsAccessibility": null,
                 "type": "PropertyDefinition",
                 "typeAnnotation": null,
                 "value": {
                   "literalType": "numeric",
                   "raw": "1",
                   "type": "Literal",
                   "value": 1,
                 },
                 "variance": null,
               },
             ],
             "type": "ClassBody",
           },
           "decorators": [],
           "id": {
             "name": "C",
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
    expectEspreeAlignment(testCase);
  });

  test('Babel', () => {
    expectBabelAlignment(testCase);
  });

  describe('With types', () => {
    describe('Property', () => {
      const testCase: AlignmentCase = {
        code: `
          class C {
            baz: F = 1;
          }
        `,
        espree: {
          // espree doesn't support types
          expectToFail: 'espree-exception',
          expectedExceptionMessage: 'Unexpected token :',
        },
        babel: {
          expectToFail: false,
        },
      };

      test('ESTree', () => {
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
                     "key": {
                       "name": "baz",
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
                         "id": {
                           "name": "F",
                           "optional": false,
                           "type": "Identifier",
                           "typeAnnotation": null,
                         },
                         "type": "GenericTypeAnnotation",
                         "typeArguments": null,
                       },
                     },
                     "value": {
                       "literalType": "numeric",
                       "raw": "1",
                       "type": "Literal",
                       "value": 1,
                     },
                     "variance": null,
                   },
                 ],
                 "type": "ClassBody",
               },
               "decorators": [],
               "id": {
                 "name": "C",
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
        expectEspreeAlignment(testCase);
      });

      test('Babel', () => {
        expectBabelAlignment(testCase);
      });
    });
    describe('Declared Property', () => {
      const testCase: AlignmentCase = {
        code: `
          class C {
            declare bam: 1;
          }
        `,
        espree: {
          // espree doesn't support types
          expectToFail: 'espree-exception',
          expectedExceptionMessage: 'Unexpected token bam',
        },
        // babel: {expectToFail: false},
        babel: {
          // TODO - once we update the babel version we test against - we can enable this
          expectToFail: 'babel-exception',
          expectedExceptionMessage: 'Unexpected token',
        },
      };

      test('ESTree', () => {
        expect(parseForSnapshot(testCase.code)).toMatchInlineSnapshot(`
         {
           "body": [
             {
               "body": {
                 "body": [
                   {
                     "computed": false,
                     "declare": true,
                     "decorators": [],
                     "key": {
                       "name": "bam",
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
                         "raw": "1",
                         "type": "NumberLiteralTypeAnnotation",
                         "value": 1,
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
                 "name": "C",
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
        expectEspreeAlignment(testCase);
      });

      test('Babel', () => {
        expectBabelAlignment(testCase);
      });
    });

    describe('Readonly variance', () => {
      const testCase: AlignmentCase = {
        code: `
          class C {
            readonly x: number;
          }
        `,
        espree: {
          expectToFail: 'espree-exception',
          expectedExceptionMessage: 'Unexpected token x',
        },
        babel: {
          expectToFail: 'babel-exception',
          expectedExceptionMessage: 'Unexpected token',
        },
      };

      test('ESTree', () => {
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
                     "key": {
                       "name": "x",
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
                     "variance": {
                       "kind": "readonly",
                       "type": "Variance",
                     },
                   },
                 ],
                 "type": "ClassBody",
               },
               "decorators": [],
               "id": {
                 "name": "C",
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
        expectEspreeAlignment(testCase);
      });

      test('Babel', () => {
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
                       "name": "x",
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
                     "variance": {
                       "end": 41,
                       "kind": "readonly",
                       "loc": {
                         "end": {
                           "column": 20,
                           "line": 3,
                         },
                         "start": {
                           "column": 12,
                           "line": 3,
                         },
                       },
                       "start": 33,
                       "type": "Variance",
                     },
                   },
                 ],
                 "type": "ClassBody",
               },
               "id": {
                 "name": "C",
                 "type": "Identifier",
               },
               "superClass": null,
               "type": "ClassDeclaration",
             },
           ],
           "type": "Program",
         }
        `);
        expectBabelAlignment(testCase);
      });
    });
  });
});
