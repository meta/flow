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

import type {JSXNamespacedName} from 'flow-estree';
import type {AlignmentCase} from '../__test_utils__/alignment-utils';

import {
  expectBabelAlignment,
  expectEspreeAlignment,
} from '../__test_utils__/alignment-utils';
import {parse, parseForSnapshot} from '../__test_utils__/parse';

// Namespaced attributes retain both identifiers; ordinary attributes stay identifiers.
test('preserves typed JSX namespaced attribute names', () => {
  const declaration = parse('const el = <A ns:flag="yes" flag="yes" />;')
    .body[0];
  if (declaration.type !== 'VariableDeclaration') {
    throw new Error('Expected a variable declaration');
  }
  const element = declaration.declarations[0].init;
  if (element?.type !== 'JSXElement') {
    throw new Error('Expected a JSX element');
  }
  const [namespaced, ordinary] = element.openingElement.attributes;
  if (
    namespaced.type !== 'JSXAttribute' ||
    namespaced.name.type !== 'JSXNamespacedName'
  ) {
    throw new Error('Expected a namespaced JSX attribute');
  }
  const name: JSXNamespacedName = namespaced.name;
  expect(name.namespace.name).toBe('ns'); // OK
  expect(name.name.name).toBe('flag'); // OK
  expect(ordinary).toHaveProperty('name.type', 'JSXIdentifier'); // OK
});

describe('JSXElement', () => {
  describe('With type', () => {
    const testCase: AlignmentCase = {
      code: '<Component<string> />',
      espree: {
        expectToFail: 'espree-exception',
        expectedExceptionMessage: 'Unexpected token <',
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
                    "directive": null,
                    "expression": {
                      "children": [],
                      "closingElement": null,
                      "openingElement": {
                        "attributes": [],
                        "name": {
                          "name": "Component",
                          "type": "JSXIdentifier",
                        },
                        "selfClosing": true,
                        "type": "JSXOpeningElement",
                        "typeArguments": {
                          "params": [
                            {
                              "type": "StringTypeAnnotation",
                            },
                          ],
                          "type": "TypeParameterInstantiation",
                        },
                      },
                      "type": "JSXElement",
                    },
                    "type": "ExpressionStatement",
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
                    "expression": {
                      "children": [],
                      "closingElement": null,
                      "openingElement": {
                        "attributes": [],
                        "name": {
                          "name": "Component",
                          "type": "JSXIdentifier",
                        },
                        "selfClosing": true,
                        "type": "JSXOpeningElement",
                      },
                      "type": "JSXElement",
                    },
                    "type": "ExpressionStatement",
                  },
                ],
                "type": "Program",
              }
          `);
      expectBabelAlignment(testCase);
    });
  });
  describe('With text', () => {
    const testCase: AlignmentCase = {
      code: '<Foo>text</Foo>',
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
              "directive": null,
              "expression": {
                "children": [
                  {
                    "raw": "text",
                    "type": "JSXText",
                    "value": "text",
                  },
                ],
                "closingElement": {
                  "name": {
                    "name": "Foo",
                    "type": "JSXIdentifier",
                  },
                  "type": "JSXClosingElement",
                },
                "openingElement": {
                  "attributes": [],
                  "name": {
                    "name": "Foo",
                    "type": "JSXIdentifier",
                  },
                  "selfClosing": false,
                  "type": "JSXOpeningElement",
                  "typeArguments": null,
                },
                "type": "JSXElement",
              },
              "type": "ExpressionStatement",
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
              "expression": {
                "children": [
                  {
                    "extra": {
                      "raw": "text",
                      "rawValue": "text",
                    },
                    "type": "JSXText",
                    "value": "text",
                  },
                ],
                "closingElement": {
                  "name": {
                    "name": "Foo",
                    "type": "JSXIdentifier",
                  },
                  "type": "JSXClosingElement",
                },
                "openingElement": {
                  "attributes": [],
                  "name": {
                    "name": "Foo",
                    "type": "JSXIdentifier",
                  },
                  "selfClosing": false,
                  "type": "JSXOpeningElement",
                },
                "type": "JSXElement",
              },
              "type": "ExpressionStatement",
            },
          ],
          "type": "Program",
        }
      `);
      expectBabelAlignment(testCase);
    });
  });
});
