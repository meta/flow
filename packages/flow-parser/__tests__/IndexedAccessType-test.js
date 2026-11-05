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

describe('IndexedAccessType', () => {
  describe('Basic Indexed Access Type', () => {
    const testCase: AlignmentCase = {
      code: `
        type T = O[k]
      `,
      espree: {
        expectToFail: 'espree-exception',
        expectedExceptionMessage: 'Unexpected token T',
      },
      // babel: {expectToFail: false},
      babel: {
        // TODO - once we update the babel version we test against - we can enable this
        expectToFail: 'babel-exception',
        expectedExceptionMessage: 'Unexpected token, expected "]"',
      },
    };

    test('ESTree', () => {
      expect(parseForSnapshot(testCase.code)).toMatchInlineSnapshot(`
       {
         "body": [
           {
             "id": {
               "name": "T",
               "optional": false,
               "type": "Identifier",
               "typeAnnotation": null,
             },
             "right": {
               "indexType": {
                 "id": {
                   "name": "k",
                   "optional": false,
                   "type": "Identifier",
                   "typeAnnotation": null,
                 },
                 "type": "GenericTypeAnnotation",
                 "typeArguments": null,
               },
               "objectType": {
                 "id": {
                   "name": "O",
                   "optional": false,
                   "type": "Identifier",
                   "typeAnnotation": null,
                 },
                 "type": "GenericTypeAnnotation",
                 "typeArguments": null,
               },
               "type": "IndexedAccessType",
             },
             "type": "TypeAlias",
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
              "id": {
                "name": "T",
                "type": "Identifier",
              },
              "right": {
                "type": "AnyTypeAnnotation",
              },
              "type": "TypeAlias",
              "typeParameters": null,
            },
          ],
          "type": "Program",
        }
      `);
      expectBabelAlignment(testCase);
    });
  });

  describe('Optional Indexed Access Type', () => {
    const testCase: AlignmentCase = {
      code: `
        type T = O?.[k]
      `,
      espree: {
        expectToFail: 'espree-exception',
        expectedExceptionMessage: 'Unexpected token T',
      },
      // babel: {expectToFail: false},
      babel: {
        // TODO - once we update the babel version we test against - we can enable this
        expectToFail: 'babel-exception',
        expectedExceptionMessage: 'Unexpected token, expected ";"',
      },
    };

    test('ESTree', () => {
      expect(parseForSnapshot(testCase.code)).toMatchInlineSnapshot(`
       {
         "body": [
           {
             "id": {
               "name": "T",
               "optional": false,
               "type": "Identifier",
               "typeAnnotation": null,
             },
             "right": {
               "indexType": {
                 "id": {
                   "name": "k",
                   "optional": false,
                   "type": "Identifier",
                   "typeAnnotation": null,
                 },
                 "type": "GenericTypeAnnotation",
                 "typeArguments": null,
               },
               "objectType": {
                 "id": {
                   "name": "O",
                   "optional": false,
                   "type": "Identifier",
                   "typeAnnotation": null,
                 },
                 "type": "GenericTypeAnnotation",
                 "typeArguments": null,
               },
               "optional": true,
               "type": "OptionalIndexedAccessType",
             },
             "type": "TypeAlias",
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
              "id": {
                "name": "T",
                "type": "Identifier",
              },
              "right": {
                "type": "AnyTypeAnnotation",
              },
              "type": "TypeAlias",
              "typeParameters": null,
            },
          ],
          "type": "Program",
        }
      `);
      expectBabelAlignment(testCase);
    });
  });
});
