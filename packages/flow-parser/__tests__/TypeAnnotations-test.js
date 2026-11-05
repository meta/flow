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
  printForSnapshotESTree,
} from '../__test_utils__/parse';

describe('Computed object type methods', () => {
  const interfaceCode = 'interface T { [Symbol.iterator](): void }';
  const objectTypeCode = 'type T = { [Symbol.iterator](): void };';

  test('interface', async () => {
    expect(parse(interfaceCode)).toMatchObject({
      body: [
        {
          type: 'InterfaceDeclaration',
          body: {
            properties: [{type: 'ObjectTypeProperty', computed: true}],
          },
        },
      ],
    });
    expect(await printForSnapshotESTree(interfaceCode)).toBe(
      'interface T {\n  [Symbol.iterator](): void;\n}',
    );
  });

  test('object type alias', async () => {
    expect(parse(objectTypeCode)).toMatchObject({
      body: [
        {
          type: 'TypeAlias',
          right: {
            properties: [{type: 'ObjectTypeProperty', computed: true}],
          },
        },
      ],
    });
    expect(await printForSnapshotESTree(objectTypeCode)).toBe(
      'type T = {[Symbol.iterator](): void};',
    );
  });
});

describe('Literal', () => {
  const testCase: AlignmentCase = {
    code: `
      type T1 = 10;
      type T2 = 0.56283;
      type T3 = "test";
      type T4 = true;
      type T5 = 4321n;
      type T6 = 12_34n;
    `,
    espree: {
      expectToFail: 'espree-exception',
      expectedExceptionMessage: 'Unexpected token T1',
    },
    babel: {expectToFail: false},
  };

  test('Emitted `.value` type is correct', () => {
    // Also assert that the literal's `.value` is the correct instance type
    expect(parse(testCase.code)).toMatchObject({
      type: 'Program',
      body: [
        {
          type: 'TypeAlias',
          right: {
            type: 'NumberLiteralTypeAnnotation',
            value: 10,
          },
        },
        {
          type: 'TypeAlias',
          right: {
            type: 'NumberLiteralTypeAnnotation',
            value: 0.56283,
          },
        },
        {
          type: 'TypeAlias',
          right: {
            type: 'StringLiteralTypeAnnotation',
            value: 'test',
          },
        },
        {
          type: 'TypeAlias',
          right: {
            type: 'BooleanLiteralTypeAnnotation',
            value: true,
          },
        },
        {
          type: 'TypeAlias',
          right: {
            type: 'BigIntLiteralTypeAnnotation',
            value: BigInt(4321),
          },
        },
        {
          type: 'TypeAlias',
          right: {
            type: 'BigIntLiteralTypeAnnotation',
            value: 1234n,
          },
        },
      ],
    });
  });

  test('ESTree', () => {
    expect(parseForSnapshot(testCase.code)).toMatchInlineSnapshot(`
      {
        "body": [
          {
            "id": {
              "name": "T1",
              "optional": false,
              "type": "Identifier",
              "typeAnnotation": null,
            },
            "right": {
              "raw": "10",
              "type": "NumberLiteralTypeAnnotation",
              "value": 10,
            },
            "type": "TypeAlias",
            "typeParameters": null,
          },
          {
            "id": {
              "name": "T2",
              "optional": false,
              "type": "Identifier",
              "typeAnnotation": null,
            },
            "right": {
              "raw": "0.56283",
              "type": "NumberLiteralTypeAnnotation",
              "value": 0.56283,
            },
            "type": "TypeAlias",
            "typeParameters": null,
          },
          {
            "id": {
              "name": "T3",
              "optional": false,
              "type": "Identifier",
              "typeAnnotation": null,
            },
            "right": {
              "raw": ""test"",
              "type": "StringLiteralTypeAnnotation",
              "value": "test",
            },
            "type": "TypeAlias",
            "typeParameters": null,
          },
          {
            "id": {
              "name": "T4",
              "optional": false,
              "type": "Identifier",
              "typeAnnotation": null,
            },
            "right": {
              "raw": "true",
              "type": "BooleanLiteralTypeAnnotation",
              "value": true,
            },
            "type": "TypeAlias",
            "typeParameters": null,
          },
          {
            "id": {
              "name": "T5",
              "optional": false,
              "type": "Identifier",
              "typeAnnotation": null,
            },
            "right": {
              "bigint": "4321",
              "raw": "4321n",
              "type": "BigIntLiteralTypeAnnotation",
              "value": 4321n,
            },
            "type": "TypeAlias",
            "typeParameters": null,
          },
          {
            "id": {
              "name": "T6",
              "optional": false,
              "type": "Identifier",
              "typeAnnotation": null,
            },
            "right": {
              "bigint": "1234",
              "raw": "12_34n",
              "type": "BigIntLiteralTypeAnnotation",
              "value": 1234n,
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
    // Babel AST literal nodes
    expect(parseForSnapshot(testCase.code, {babel: true}))
      .toMatchInlineSnapshot(`
      {
        "body": [
          {
            "id": {
              "name": "T1",
              "type": "Identifier",
            },
            "right": {
              "extra": {
                "raw": "10",
                "rawValue": 10,
              },
              "type": "NumberLiteralTypeAnnotation",
              "value": 10,
            },
            "type": "TypeAlias",
            "typeParameters": null,
          },
          {
            "id": {
              "name": "T2",
              "type": "Identifier",
            },
            "right": {
              "extra": {
                "raw": "0.56283",
                "rawValue": 0.56283,
              },
              "type": "NumberLiteralTypeAnnotation",
              "value": 0.56283,
            },
            "type": "TypeAlias",
            "typeParameters": null,
          },
          {
            "id": {
              "name": "T3",
              "type": "Identifier",
            },
            "right": {
              "extra": {
                "raw": ""test"",
                "rawValue": "test",
              },
              "type": "StringLiteralTypeAnnotation",
              "value": "test",
            },
            "type": "TypeAlias",
            "typeParameters": null,
          },
          {
            "id": {
              "name": "T4",
              "type": "Identifier",
            },
            "right": {
              "type": "BooleanLiteralTypeAnnotation",
              "value": true,
            },
            "type": "TypeAlias",
            "typeParameters": null,
          },
          {
            "id": {
              "name": "T5",
              "type": "Identifier",
            },
            "right": {
              "extra": {
                "raw": "4321n",
                "rawValue": "4321",
              },
              "type": "BigIntLiteralTypeAnnotation",
              "value": 4321n,
            },
            "type": "TypeAlias",
            "typeParameters": null,
          },
          {
            "id": {
              "name": "T6",
              "type": "Identifier",
            },
            "right": {
              "extra": {
                "raw": "12_34n",
                "rawValue": "1234",
              },
              "type": "BigIntLiteralTypeAnnotation",
              "value": 1234n,
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

describe('Keyword Types', () => {
  const testCase: AlignmentCase = {
    code: `
      type T1 = boolean;
      type T2 = string;
      type T3 = number;
      type T4 = bigint;
      type T5 = any;
      type T6 = empty;
      type T7 = symbol;
      type T8 = mixed;
      type T9 = void;
      type T0 = null;
    `,
    espree: {expectToFail: false},
    babel: {expectToFail: false},
  };

  test('Emitted `.value` type is correct', () => {
    expect(parse(testCase.code)).toMatchObject({
      type: 'Program',
      body: [
        {
          type: 'TypeAlias',
          right: {
            type: 'BooleanTypeAnnotation',
          },
        },
        {
          type: 'TypeAlias',
          right: {
            type: 'StringTypeAnnotation',
          },
        },
        {
          type: 'TypeAlias',
          right: {
            type: 'NumberTypeAnnotation',
          },
        },
        {
          type: 'TypeAlias',
          right: {
            type: 'BigIntTypeAnnotation',
          },
        },
        {
          type: 'TypeAlias',
          right: {
            type: 'AnyTypeAnnotation',
          },
        },
        {
          type: 'TypeAlias',
          right: {
            type: 'EmptyTypeAnnotation',
          },
        },
        {
          type: 'TypeAlias',
          right: {
            type: 'SymbolTypeAnnotation',
          },
        },
        {
          type: 'TypeAlias',
          right: {
            type: 'MixedTypeAnnotation',
          },
        },
        {
          type: 'TypeAlias',
          right: {
            type: 'VoidTypeAnnotation',
          },
        },
        {
          type: 'TypeAlias',
          right: {
            type: 'NullLiteralTypeAnnotation',
          },
        },
      ],
    });
  });

  it('estree AST', () => {
    expect(parseForSnapshot(testCase.code)).toMatchInlineSnapshot(`
      {
        "body": [
          {
            "id": {
              "name": "T1",
              "optional": false,
              "type": "Identifier",
              "typeAnnotation": null,
            },
            "right": {
              "type": "BooleanTypeAnnotation",
            },
            "type": "TypeAlias",
            "typeParameters": null,
          },
          {
            "id": {
              "name": "T2",
              "optional": false,
              "type": "Identifier",
              "typeAnnotation": null,
            },
            "right": {
              "type": "StringTypeAnnotation",
            },
            "type": "TypeAlias",
            "typeParameters": null,
          },
          {
            "id": {
              "name": "T3",
              "optional": false,
              "type": "Identifier",
              "typeAnnotation": null,
            },
            "right": {
              "type": "NumberTypeAnnotation",
            },
            "type": "TypeAlias",
            "typeParameters": null,
          },
          {
            "id": {
              "name": "T4",
              "optional": false,
              "type": "Identifier",
              "typeAnnotation": null,
            },
            "right": {
              "type": "BigIntTypeAnnotation",
            },
            "type": "TypeAlias",
            "typeParameters": null,
          },
          {
            "id": {
              "name": "T5",
              "optional": false,
              "type": "Identifier",
              "typeAnnotation": null,
            },
            "right": {
              "type": "AnyTypeAnnotation",
            },
            "type": "TypeAlias",
            "typeParameters": null,
          },
          {
            "id": {
              "name": "T6",
              "optional": false,
              "type": "Identifier",
              "typeAnnotation": null,
            },
            "right": {
              "type": "EmptyTypeAnnotation",
            },
            "type": "TypeAlias",
            "typeParameters": null,
          },
          {
            "id": {
              "name": "T7",
              "optional": false,
              "type": "Identifier",
              "typeAnnotation": null,
            },
            "right": {
              "type": "SymbolTypeAnnotation",
            },
            "type": "TypeAlias",
            "typeParameters": null,
          },
          {
            "id": {
              "name": "T8",
              "optional": false,
              "type": "Identifier",
              "typeAnnotation": null,
            },
            "right": {
              "type": "MixedTypeAnnotation",
            },
            "type": "TypeAlias",
            "typeParameters": null,
          },
          {
            "id": {
              "name": "T9",
              "optional": false,
              "type": "Identifier",
              "typeAnnotation": null,
            },
            "right": {
              "type": "VoidTypeAnnotation",
            },
            "type": "TypeAlias",
            "typeParameters": null,
          },
          {
            "id": {
              "name": "T0",
              "optional": false,
              "type": "Identifier",
              "typeAnnotation": null,
            },
            "right": {
              "type": "NullLiteralTypeAnnotation",
            },
            "type": "TypeAlias",
            "typeParameters": null,
          },
        ],
        "type": "Program",
      }
    `);
  });
  it(`should match babel`, () => {
    expectBabelAlignment(testCase);
  });
});

describe('TypeofTypeAnnotation', () => {
  describe('Identifier', () => {
    const testCase: AlignmentCase = {
      code: `
        type T1 = typeof a;
      `,
      espree: {
        expectToFail: 'espree-exception',
        expectedExceptionMessage: 'Unexpected token T1',
      },
      babel: {expectToFail: false},
    };

    test('ESTree', () => {
      expect(parseForSnapshot(testCase.code)).toMatchInlineSnapshot(`
        {
          "body": [
            {
              "id": {
                "name": "T1",
                "optional": false,
                "type": "Identifier",
                "typeAnnotation": null,
              },
              "right": {
                "argument": {
                  "name": "a",
                  "optional": false,
                  "type": "Identifier",
                  "typeAnnotation": null,
                },
                "type": "TypeofTypeAnnotation",
                "typeArguments": null,
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
                "name": "T1",
                "type": "Identifier",
              },
              "right": {
                "argument": {
                  "id": {
                    "name": "a",
                    "type": "Identifier",
                  },
                  "type": "GenericTypeAnnotation",
                  "typeParameters": null,
                },
                "type": "TypeofTypeAnnotation",
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

  describe('QualifiedTypeofIdentifier', () => {
    const testCase: AlignmentCase = {
      code: `
        type T1 = typeof a.a;
      `,
      espree: {
        expectToFail: 'espree-exception',
        expectedExceptionMessage: 'Unexpected token T1',
      },
      babel: {expectToFail: false},
    };

    test('ESTree', () => {
      expect(parseForSnapshot(testCase.code)).toMatchInlineSnapshot(`
        {
          "body": [
            {
              "id": {
                "name": "T1",
                "optional": false,
                "type": "Identifier",
                "typeAnnotation": null,
              },
              "right": {
                "argument": {
                  "id": {
                    "name": "a",
                    "optional": false,
                    "type": "Identifier",
                    "typeAnnotation": null,
                  },
                  "qualification": {
                    "name": "a",
                    "optional": false,
                    "type": "Identifier",
                    "typeAnnotation": null,
                  },
                  "type": "QualifiedTypeofIdentifier",
                },
                "type": "TypeofTypeAnnotation",
                "typeArguments": null,
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
                "name": "T1",
                "type": "Identifier",
              },
              "right": {
                "argument": {
                  "id": {
                    "id": {
                      "name": "a",
                      "type": "Identifier",
                    },
                    "qualification": {
                      "name": "a",
                      "type": "Identifier",
                    },
                    "type": "QualifiedTypeIdentifier",
                  },
                  "type": "GenericTypeAnnotation",
                  "typeParameters": null,
                },
                "type": "TypeofTypeAnnotation",
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

describe('TupleTypeAnnotation', () => {
  describe('normal', () => {
    const testCase: AlignmentCase = {
      code: `
        type T1 = [string];
      `,
      espree: {
        expectToFail: 'espree-exception',
        expectedExceptionMessage: 'Unexpected token T1',
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
              "id": {
                "name": "T1",
                "optional": false,
                "type": "Identifier",
                "typeAnnotation": null,
              },
              "right": {
                "elementTypes": [
                  {
                    "type": "StringTypeAnnotation",
                  },
                ],
                "inexact": false,
                "type": "TupleTypeAnnotation",
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
                "name": "T1",
                "type": "Identifier",
              },
              "right": {
                "type": "TupleTypeAnnotation",
                "types": [
                  {
                    "type": "StringTypeAnnotation",
                  },
                ],
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

  describe('TupleTypeSpreadElement', () => {
    const testCase: AlignmentCase = {
      code: `
        type T1 = [...b];
      `,
      espree: {
        expectToFail: 'espree-exception',
        expectedExceptionMessage: 'Unexpected token T1',
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
             "id": {
               "name": "T1",
               "optional": false,
               "type": "Identifier",
               "typeAnnotation": null,
             },
             "right": {
               "elementTypes": [
                 {
                   "label": null,
                   "type": "TupleTypeSpreadElement",
                   "typeAnnotation": {
                     "id": {
                       "name": "b",
                       "optional": false,
                       "type": "Identifier",
                       "typeAnnotation": null,
                     },
                     "type": "GenericTypeAnnotation",
                     "typeArguments": null,
                   },
                 },
               ],
               "inexact": false,
               "type": "TupleTypeAnnotation",
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
                "name": "T1",
                "type": "Identifier",
              },
              "right": {
                "type": "TupleTypeAnnotation",
                "types": [
                  {
                    "type": "AnyTypeAnnotation",
                  },
                ],
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

  describe('TupleTypeLabelledElement', () => {
    const testCase: AlignmentCase = {
      code: `
        type T1 = [+a?: number];
      `,
      espree: {
        expectToFail: 'espree-exception',
        expectedExceptionMessage: 'Unexpected token T1',
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
              "id": {
                "name": "T1",
                "optional": false,
                "type": "Identifier",
                "typeAnnotation": null,
              },
              "right": {
                "elementTypes": [
                  {
                    "elementType": {
                      "type": "NumberTypeAnnotation",
                    },
                    "label": {
                      "name": "a",
                      "optional": false,
                      "type": "Identifier",
                      "typeAnnotation": null,
                    },
                    "optional": true,
                    "type": "TupleTypeLabeledElement",
                    "variance": {
                      "kind": "plus",
                      "type": "Variance",
                    },
                  },
                ],
                "inexact": false,
                "type": "TupleTypeAnnotation",
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
                "name": "T1",
                "type": "Identifier",
              },
              "right": {
                "type": "TupleTypeAnnotation",
                "types": [
                  {
                    "type": "AnyTypeAnnotation",
                  },
                ],
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

  describe('Readonly variance on TupleTypeLabeledElement', () => {
    const testCase: AlignmentCase = {
      code: `
        type T1 = [readonly a: number];
      `,
      espree: {
        expectToFail: 'espree-exception',
        expectedExceptionMessage: 'Unexpected token T1',
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
             "id": {
               "name": "T1",
               "optional": false,
               "type": "Identifier",
               "typeAnnotation": null,
             },
             "right": {
               "elementTypes": [
                 {
                   "elementType": {
                     "type": "NumberTypeAnnotation",
                   },
                   "label": {
                     "name": "a",
                     "optional": false,
                     "type": "Identifier",
                     "typeAnnotation": null,
                   },
                   "optional": false,
                   "type": "TupleTypeLabeledElement",
                   "variance": {
                     "kind": "readonly",
                     "type": "Variance",
                   },
                 },
               ],
               "inexact": false,
               "type": "TupleTypeAnnotation",
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
               "name": "T1",
               "type": "Identifier",
             },
             "right": {
               "type": "TupleTypeAnnotation",
               "types": [
                 {
                   "type": "AnyTypeAnnotation",
                 },
               ],
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

  describe('Readonly variance on ObjectTypeProperty', () => {
    const testCase: AlignmentCase = {
      code: `
        type T1 = {readonly foo: string};
      `,
      espree: {
        expectToFail: 'espree-exception',
        expectedExceptionMessage: 'Unexpected token T1',
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
             "id": {
               "name": "T1",
               "optional": false,
               "type": "Identifier",
               "typeAnnotation": null,
             },
             "right": {
               "callProperties": [],
               "exact": false,
               "indexers": [],
               "inexact": false,
               "internalSlots": [],
               "properties": [
                 {
                   "key": {
                     "name": "foo",
                     "optional": false,
                     "type": "Identifier",
                     "typeAnnotation": null,
                   },
                   "kind": "init",
                   "method": false,
                   "optional": false,
                   "proto": false,
                   "static": false,
                   "type": "ObjectTypeProperty",
                   "value": {
                     "type": "StringTypeAnnotation",
                   },
                   "variance": {
                     "kind": "readonly",
                     "type": "Variance",
                   },
                 },
               ],
               "type": "ObjectTypeAnnotation",
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
               "name": "T1",
               "type": "Identifier",
             },
             "right": {
               "callProperties": [],
               "exact": false,
               "indexers": [],
               "inexact": false,
               "internalSlots": [],
               "properties": [
                 {
                   "key": {
                     "name": "foo",
                     "type": "Identifier",
                   },
                   "kind": "init",
                   "method": false,
                   "optional": false,
                   "proto": false,
                   "static": false,
                   "type": "ObjectTypeProperty",
                   "value": {
                     "type": "StringTypeAnnotation",
                   },
                   "variance": {
                     "kind": "readonly",
                     "type": "Variance",
                   },
                 },
               ],
               "type": "ObjectTypeAnnotation",
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

  describe('Readonly variance on ObjectTypeIndexer', () => {
    const testCase: AlignmentCase = {
      code: `
        type T1 = {readonly [string]: mixed};
      `,
      espree: {
        expectToFail: 'espree-exception',
        expectedExceptionMessage: 'Unexpected token T1',
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
             "id": {
               "name": "T1",
               "optional": false,
               "type": "Identifier",
               "typeAnnotation": null,
             },
             "right": {
               "callProperties": [],
               "exact": false,
               "indexers": [
                 {
                   "id": null,
                   "key": {
                     "type": "StringTypeAnnotation",
                   },
                   "optional": false,
                   "static": false,
                   "type": "ObjectTypeIndexer",
                   "value": {
                     "type": "MixedTypeAnnotation",
                   },
                   "variance": {
                     "kind": "readonly",
                     "type": "Variance",
                   },
                 },
               ],
               "inexact": false,
               "internalSlots": [],
               "properties": [],
               "type": "ObjectTypeAnnotation",
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
               "name": "T1",
               "type": "Identifier",
             },
             "right": {
               "callProperties": [],
               "exact": false,
               "indexers": [
                 {
                   "id": null,
                   "key": {
                     "type": "StringTypeAnnotation",
                   },
                   "optional": false,
                   "static": false,
                   "type": "ObjectTypeIndexer",
                   "value": {
                     "type": "MixedTypeAnnotation",
                   },
                   "variance": {
                     "kind": "readonly",
                     "type": "Variance",
                   },
                 },
               ],
               "inexact": false,
               "internalSlots": [],
               "properties": [],
               "type": "ObjectTypeAnnotation",
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

  describe('Readonly as property name (not variance)', () => {
    const testCase: AlignmentCase = {
      code: `
        type T1 = {readonly: string};
      `,
      espree: {
        expectToFail: 'espree-exception',
        expectedExceptionMessage: 'Unexpected token T1',
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
             "id": {
               "name": "T1",
               "optional": false,
               "type": "Identifier",
               "typeAnnotation": null,
             },
             "right": {
               "callProperties": [],
               "exact": false,
               "indexers": [],
               "inexact": false,
               "internalSlots": [],
               "properties": [
                 {
                   "key": {
                     "name": "readonly",
                     "optional": false,
                     "type": "Identifier",
                     "typeAnnotation": null,
                   },
                   "kind": "init",
                   "method": false,
                   "optional": false,
                   "proto": false,
                   "static": false,
                   "type": "ObjectTypeProperty",
                   "value": {
                     "type": "StringTypeAnnotation",
                   },
                   "variance": null,
                 },
               ],
               "type": "ObjectTypeAnnotation",
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
               "name": "T1",
               "type": "Identifier",
             },
             "right": {
               "callProperties": [],
               "exact": false,
               "indexers": [],
               "inexact": false,
               "internalSlots": [],
               "properties": [
                 {
                   "key": {
                     "name": "readonly",
                     "type": "Identifier",
                   },
                   "kind": "init",
                   "method": false,
                   "optional": false,
                   "proto": false,
                   "static": false,
                   "type": "ObjectTypeProperty",
                   "value": {
                     "type": "StringTypeAnnotation",
                   },
                   "variance": null,
                 },
               ],
               "type": "ObjectTypeAnnotation",
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

  describe('Readonly variance with reserved-word property name', () => {
    const testCase: AlignmentCase = {
      code: `
        type T1 = {readonly with: string};
      `,
      espree: {
        expectToFail: 'espree-exception',
        expectedExceptionMessage: 'Unexpected token T1',
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
             "id": {
               "name": "T1",
               "optional": false,
               "type": "Identifier",
               "typeAnnotation": null,
             },
             "right": {
               "callProperties": [],
               "exact": false,
               "indexers": [],
               "inexact": false,
               "internalSlots": [],
               "properties": [
                 {
                   "key": {
                     "name": "with",
                     "optional": false,
                     "type": "Identifier",
                     "typeAnnotation": null,
                   },
                   "kind": "init",
                   "method": false,
                   "optional": false,
                   "proto": false,
                   "static": false,
                   "type": "ObjectTypeProperty",
                   "value": {
                     "type": "StringTypeAnnotation",
                   },
                   "variance": {
                     "kind": "readonly",
                     "type": "Variance",
                   },
                 },
               ],
               "type": "ObjectTypeAnnotation",
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
               "name": "T1",
               "type": "Identifier",
             },
             "right": {
               "callProperties": [],
               "exact": false,
               "indexers": [],
               "inexact": false,
               "internalSlots": [],
               "properties": [
                 {
                   "key": {
                     "name": "with",
                     "type": "Identifier",
                   },
                   "kind": "init",
                   "method": false,
                   "optional": false,
                   "proto": false,
                   "static": false,
                   "type": "ObjectTypeProperty",
                   "value": {
                     "type": "StringTypeAnnotation",
                   },
                   "variance": {
                     "kind": "readonly",
                     "type": "Variance",
                   },
                 },
               ],
               "type": "ObjectTypeAnnotation",
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

  describe('Readonly variance with optional reserved-word property name', () => {
    const testCase: AlignmentCase = {
      code: `
        type T1 = {readonly default?: string};
      `,
      espree: {
        expectToFail: 'espree-exception',
        expectedExceptionMessage: 'Unexpected token T1',
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
             "id": {
               "name": "T1",
               "optional": false,
               "type": "Identifier",
               "typeAnnotation": null,
             },
             "right": {
               "callProperties": [],
               "exact": false,
               "indexers": [],
               "inexact": false,
               "internalSlots": [],
               "properties": [
                 {
                   "key": {
                     "name": "default",
                     "optional": false,
                     "type": "Identifier",
                     "typeAnnotation": null,
                   },
                   "kind": "init",
                   "method": false,
                   "optional": true,
                   "proto": false,
                   "static": false,
                   "type": "ObjectTypeProperty",
                   "value": {
                     "type": "StringTypeAnnotation",
                   },
                   "variance": {
                     "kind": "readonly",
                     "type": "Variance",
                   },
                 },
               ],
               "type": "ObjectTypeAnnotation",
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
               "name": "T1",
               "type": "Identifier",
             },
             "right": {
               "callProperties": [],
               "exact": false,
               "indexers": [],
               "inexact": false,
               "internalSlots": [],
               "properties": [
                 {
                   "key": {
                     "name": "default",
                     "type": "Identifier",
                   },
                   "kind": "init",
                   "method": false,
                   "optional": true,
                   "proto": false,
                   "static": false,
                   "type": "ObjectTypeProperty",
                   "value": {
                     "type": "StringTypeAnnotation",
                   },
                   "variance": {
                     "kind": "readonly",
                     "type": "Variance",
                   },
                 },
               ],
               "type": "ObjectTypeAnnotation",
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

  describe('inexact TupleTypeAnnotation', () => {
    const testCase: AlignmentCase = {
      code: `
        type T1 = [...];
      `,
      espree: {
        expectToFail: 'espree-exception',
        expectedExceptionMessage: 'Unexpected token T1',
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
              "id": {
                "name": "T1",
                "optional": false,
                "type": "Identifier",
                "typeAnnotation": null,
              },
              "right": {
                "elementTypes": [],
                "inexact": true,
                "type": "TupleTypeAnnotation",
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
                "name": "T1",
                "type": "Identifier",
              },
              "right": {
                "type": "TupleTypeAnnotation",
                "types": [],
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

describe('TypePredicate', () => {
  const testCase: AlignmentCase = {
    code: `
      function predicate(x: mixed): x is T {
         return x;
      }
    `,
    espree: {
      expectToFail: 'espree-exception',
      expectedExceptionMessage: 'Unexpected token :',
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
           "async": false,
           "body": {
             "body": [
               {
                 "argument": {
                   "name": "x",
                   "optional": false,
                   "type": "Identifier",
                   "typeAnnotation": null,
                 },
                 "type": "ReturnStatement",
               },
             ],
             "type": "BlockStatement",
           },
           "expression": false,
           "generator": false,
           "id": {
             "name": "predicate",
             "optional": false,
             "type": "Identifier",
             "typeAnnotation": null,
           },
           "params": [
             {
               "name": "x",
               "optional": false,
               "type": "Identifier",
               "typeAnnotation": {
                 "type": "TypeAnnotation",
                 "typeAnnotation": {
                   "type": "MixedTypeAnnotation",
                 },
               },
             },
           ],
           "predicate": null,
           "returnType": {
             "type": "TypeAnnotation",
             "typeAnnotation": {
               "kind": null,
               "parameterName": {
                 "name": "x",
                 "optional": false,
                 "type": "Identifier",
                 "typeAnnotation": null,
               },
               "type": "TypePredicate",
               "typeAnnotation": {
                 "id": {
                   "name": "T",
                   "optional": false,
                   "type": "Identifier",
                   "typeAnnotation": null,
                 },
                 "type": "GenericTypeAnnotation",
                 "typeArguments": null,
               },
             },
           },
           "type": "FunctionDeclaration",
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
            "async": false,
            "body": {
              "body": [
                {
                  "argument": {
                    "name": "x",
                    "type": "Identifier",
                  },
                  "type": "ReturnStatement",
                },
              ],
              "directives": [],
              "type": "BlockStatement",
            },
            "generator": false,
            "id": {
              "name": "predicate",
              "type": "Identifier",
            },
            "params": [
              {
                "name": "x",
                "type": "Identifier",
                "typeAnnotation": {
                  "type": "TypeAnnotation",
                  "typeAnnotation": {
                    "type": "MixedTypeAnnotation",
                  },
                },
              },
            ],
            "returnType": {
              "type": "TypeAnnotation",
              "typeAnnotation": {
                "type": "AnyTypeAnnotation",
              },
            },
            "type": "FunctionDeclaration",
          },
        ],
        "type": "Program",
      }
    `);
    expectBabelAlignment(testCase);
  });
});

describe('TypeCastExpression', () => {
  const testCase: AlignmentCase = {
    code: `
      (a: string);
    `,
    espree: {
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
            "directive": null,
            "expression": {
              "expression": {
                "name": "a",
                "optional": false,
                "type": "Identifier",
                "typeAnnotation": null,
              },
              "type": "TypeCastExpression",
              "typeAnnotation": {
                "type": "TypeAnnotation",
                "typeAnnotation": {
                  "type": "StringTypeAnnotation",
                },
              },
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
              "expression": {
                "name": "a",
                "type": "Identifier",
              },
              "type": "TypeCastExpression",
              "typeAnnotation": {
                "type": "TypeAnnotation",
                "typeAnnotation": {
                  "type": "StringTypeAnnotation",
                },
              },
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

describe('unknown/never/undefined', () => {
  const testCase: AlignmentCase = {
    code: `
      type T0 = unknown;
      type T1 = never;
      type T2 = undefined;
    `,
    espree: {expectToFail: false},
    babel: {expectToFail: false},
  };

  test('ESTree', () => {
    expect(parseForSnapshot(testCase.code)).toMatchInlineSnapshot(`
     {
       "body": [
         {
           "id": {
             "name": "T0",
             "optional": false,
             "type": "Identifier",
             "typeAnnotation": null,
           },
           "right": {
             "type": "UnknownTypeAnnotation",
           },
           "type": "TypeAlias",
           "typeParameters": null,
         },
         {
           "id": {
             "name": "T1",
             "optional": false,
             "type": "Identifier",
             "typeAnnotation": null,
           },
           "right": {
             "type": "NeverTypeAnnotation",
           },
           "type": "TypeAlias",
           "typeParameters": null,
         },
         {
           "id": {
             "name": "T2",
             "optional": false,
             "type": "Identifier",
             "typeAnnotation": null,
           },
           "right": {
             "type": "UndefinedTypeAnnotation",
           },
           "type": "TypeAlias",
           "typeParameters": null,
         },
       ],
       "type": "Program",
     }
    `);
  });

  test('Babel', () => {
    expect(parseForSnapshot(testCase.code, {babel: true}))
      .toMatchInlineSnapshot(`
     {
       "body": [
         {
           "id": {
             "name": "T0",
             "type": "Identifier",
           },
           "right": {
             "id": {
               "name": "unknown",
               "type": "Identifier",
             },
             "type": "GenericTypeAnnotation",
             "typeParameters": null,
           },
           "type": "TypeAlias",
           "typeParameters": null,
         },
         {
           "id": {
             "name": "T1",
             "type": "Identifier",
           },
           "right": {
             "id": {
               "name": "never",
               "type": "Identifier",
             },
             "type": "GenericTypeAnnotation",
             "typeParameters": null,
           },
           "type": "TypeAlias",
           "typeParameters": null,
         },
         {
           "id": {
             "name": "T2",
             "type": "Identifier",
           },
           "right": {
             "id": {
               "name": "undefined",
               "type": "Identifier",
             },
             "type": "GenericTypeAnnotation",
             "typeParameters": null,
           },
           "type": "TypeAlias",
           "typeParameters": null,
         },
       ],
       "type": "Program",
     }
    `);
  });
});
