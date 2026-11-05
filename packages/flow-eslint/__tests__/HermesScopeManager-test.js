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

import type {ScopeManager} from '../src/scope-manager/ScopeManager';

import {parseForESLint} from '../src';
import {DefinitionType, ScopeType} from '../src';
import {verifyHasScopes} from '../__test_utils__/verifyHasScopes';
import {PatternVisitor} from '../src/scope-manager/referencer/PatternVisitor';

// Pattern traversal visits sparse-array identifiers without treating holes as nodes.
test('pattern visitor accepts sparse array expressions', () => {
  const {ast} = parseForESLint('const value = [first,,...rest];');
  const declaration = ast.body[0];
  if (declaration.type !== 'VariableDeclaration') {
    throw new Error('Expected a variable declaration');
  }
  const array = declaration.declarations[0].init;
  if (array?.type !== 'ArrayExpression') {
    throw new Error('Expected an array expression');
  }
  const names = [];
  const visitor = new PatternVisitor(
    {},
    array,
    identifier => {
      names.push(identifier.name);
    },
    () => {},
  );
  visitor.visit(array);
  expect(names).toEqual(['first', 'rest']); // OK
  expect(array.elements[1]).toBeNull(); // OK
});

test.each([
  ['type Alias = Box<Outer<Inner>>;', ['Box', 'Outer', 'Inner']],
  ['interface I extends Base<Outer<Inner>> {}', ['Base', 'Outer', 'Inner']],
  ['class C implements I<Outer<Inner>> {}', ['I', 'Outer', 'Inner']],
  [
    'declare class C extends Base<Outer<Inner>> implements I<Outer<Inner>> {}',
    ['Base', 'Outer', 'Inner', 'I', 'Outer', 'Inner'],
  ],
])('nested type arguments are type references: %s', (code, expected) => {
  const {scopeManager} = parseForESLint(code, {flow: 'all'});
  const references = scopeManager.scopes.flatMap(scope =>
    scope.references.filter(reference => reference.isTypeReference),
  );
  expect(references.map(reference => reference.identifier.name)).toEqual(
    expected,
  );
});

describe('Source type option', () => {
  test('script', () => {
    const {ast, scopeManager} = parseForESLint('Foo', {sourceType: 'script'});

    expect(ast.sourceType).toEqual('script');
    expect(scopeManager.scopes).toHaveLength(1);
    expect(scopeManager.scopes[0].type).toEqual(ScopeType.Global);
  });

  test('module', () => {
    const {ast, scopeManager} = parseForESLint('Foo', {sourceType: 'module'});

    expect(ast.sourceType).toEqual('module');
    expect(scopeManager.scopes).toHaveLength(2);
    expect(scopeManager.scopes[0].type).toEqual(ScopeType.Global);
    expect(scopeManager.scopes[1].type).toEqual(ScopeType.Module);
  });

  test('defaults to module', () => {
    const {ast, scopeManager} = parseForESLint('Foo');

    expect(ast.sourceType).toEqual('module');
    expect(scopeManager.scopes).toHaveLength(2);
    expect(scopeManager.scopes[0].type).toEqual(ScopeType.Global);
    expect(scopeManager.scopes[1].type).toEqual(ScopeType.Module);
  });
});

describe('Type and value references', () => {
  function verifyValueAndTypeReferences(
    code: string,
    name: string,
    definitionType: 'ClassName' | 'Enum',
  ) {
    test(code, () => {
      const {scopeManager} = parseForESLint(code);

      // Verify that scope contains a single variable
      const scope = scopeManager.scopes[1];
      expect(scope.variables).toHaveLength(1);
      const variable = scope.variables[0];

      // Verify that variable has correct name and definition type
      expect(variable.name).toEqual(name);
      expect(variable.defs).toHaveLength(1);
      expect(variable.defs[0].type).toEqual(definitionType);

      // Variable has both a value and type reference
      expect(variable.references).toHaveLength(2);
      expect(variable.references[0].isValueReference).toBe(true);
      expect(variable.references[0].isTypeReference).toBe(false);
      expect(variable.references[1].isValueReference).toBe(false);
      expect(variable.references[1].isTypeReference).toBe(true);
    });
  }

  verifyValueAndTypeReferences(
    `
      class C {}
      (C: Class<C>);
    `,
    'C',
    DefinitionType.ClassName,
  );
  verifyValueAndTypeReferences(
    `
      enum E {
        A
      }
      (E.A: E);
    `,
    'E',
    DefinitionType.Enum,
  );
});

describe('Type definitions', () => {
  function verifyTypeDefinition(scopeManager: ScopeManager) {
    // Verify there is a module scope, variable, and reference
    expect(scopeManager.scopes).toHaveLength(2);

    const scope = scopeManager.scopes[1];
    expect(scope.type).toEqual(ScopeType.Module);
    expect(scope.variables).toHaveLength(1);
    expect(scope.references).toHaveLength(1);

    const variable = scope.variables[0];
    const reference = scope.references[0];
    expect(variable.name).toEqual('T');

    // Verify that reference is resolved
    expect(variable.references).toHaveLength(1);
    expect(variable.references[0]).toBe(reference);
    expect(reference.resolved).toBe(variable);
    expect(reference.isTypeReference).toBe(true);
    expect(reference.isReadOnly()).toBe(true);

    // Verify there is one TypeDefinition
    expect(variable.defs).toHaveLength(1);
    expect(variable.defs[0].type).toEqual(DefinitionType.Type);
  }

  describe('TypeAlias', () => {
    const {scopeManager} = parseForESLint(`
      type T = number;
      (1: T);
    `);
    verifyTypeDefinition(scopeManager);
  });

  describe('OpaqueType', () => {
    const {scopeManager} = parseForESLint(`
      opaque type T = number;
      (1: T);
    `);
    verifyTypeDefinition(scopeManager);
  });

  describe('InterfaceDeclaration', () => {
    const {scopeManager} = parseForESLint(`
      interface T {}
      (1: T);
    `);
    verifyTypeDefinition(scopeManager);
  });

  // Default interfaces define and reference types without introducing value reads.
  test('default interface exports keep type-only scope semantics', () => {
    const {ast, scopeManager} = parseForESLint(
      'export default interface T {} (1: T);',
    );
    expect(ast.body[0]).toMatchObject({
      type: 'ExportDefaultDeclaration',
      exportKind: 'type',
      declaration: {type: 'InterfaceDeclaration'},
    }); // OK
    verifyTypeDefinition(scopeManager);
    expect(scopeManager.scopes[1].references[0].isValueReference).toBe(false); // OK
  });
});

describe('Enums', () => {
  test('Definition', () => {
    const {scopeManager} = parseForESLint(`
      enum E {}
      E;
    `);

    // Verify there is a module scope, variable, and reference
    expect(scopeManager.scopes).toHaveLength(2);

    const scope = scopeManager.scopes[1];
    expect(scope.type).toEqual(ScopeType.Module);
    expect(scope.variables).toHaveLength(1);
    expect(scope.references).toHaveLength(1);

    const variable = scope.variables[0];
    const reference = scope.references[0];
    expect(variable.name).toEqual('E');

    // Verify that reference is resolved
    expect(variable.references).toHaveLength(1);
    expect(variable.references[0]).toBe(reference);
    expect(reference.resolved).toBe(variable);
    expect(reference.isValueReference).toBe(true);

    // Verify there is one Enum definition
    expect(variable.defs).toHaveLength(1);
    expect(variable.defs[0].type).toEqual(DefinitionType.Enum);
    expect(variable.defs[0].node.type).toEqual('EnumDeclaration');
    expect(variable.defs[0].name).toMatchObject({
      type: 'Identifier',
      name: 'E',
    });
  });

  test('Declaration', () => {
    const {scopeManager} = parseForESLint(`
      declare enum E {}
      E;
    `);

    // Verify there is a module scope, variable, and reference
    expect(scopeManager.scopes).toHaveLength(2);

    const scope = scopeManager.scopes[1];
    expect(scope.type).toEqual(ScopeType.Module);
    expect(scope.variables).toHaveLength(1);
    expect(scope.references).toHaveLength(1);

    const variable = scope.variables[0];
    const reference = scope.references[0];
    expect(variable.name).toEqual('E');

    // Verify that reference is resolved
    expect(variable.references).toHaveLength(1);
    expect(variable.references[0]).toBe(reference);
    expect(reference.resolved).toBe(variable);
    expect(reference.isValueReference).toBe(true);

    // Verify there is one Enum definition
    expect(variable.defs).toHaveLength(1);
    expect(variable.defs[0].type).toEqual(DefinitionType.Enum);
    expect(variable.defs[0].node.type).toEqual('DeclareEnum');
    expect(variable.defs[0].name).toMatchObject({
      type: 'Identifier',
      name: 'E',
    });
  });
});

describe('QualifiedTypeIdentifier', () => {
  test('References values', () => {
    const {scopeManager} = parseForESLint(`
      import * as Foo from 'Foo';
      (1: Foo.Bar);
    `);

    // Verify that scope contains single value reference to 'Foo'
    const scope = scopeManager.scopes[1];
    expect(scope.variables).toHaveLength(1);

    const variable = scope.variables[0];
    expect(variable.name).toEqual('Foo');
    expect(variable.references).toHaveLength(1);
    expect(variable.references[0].isValueReference).toBe(true);
    expect(variable.references[0].isTypeReference).toBe(true);
  });
  test('References types', () => {
    const {scopeManager} = parseForESLint(`
      import type Foo from 'Foo';
      (1: Foo.Bar);
    `);

    // Verify that scope contains single value reference to 'Foo'
    const scope = scopeManager.scopes[1];
    expect(scope.variables).toHaveLength(1);

    const variable = scope.variables[0];
    expect(variable.name).toEqual('Foo');
    expect(variable.references).toHaveLength(1);
    expect(variable.references[0].isValueReference).toBe(true);
    expect(variable.references[0].isTypeReference).toBe(true);
  });
});

describe('QualifiedTypeofIdentifier', () => {
  test('References values', () => {
    const {scopeManager} = parseForESLint(`
      import foo from 'foo';
      import type bar from 'foo';
      (1: typeof foo.bar<bar>);
    `);

    // Verify that scope contains single value reference to 'foo'
    const scope = scopeManager.scopes[1];
    expect(scope.variables).toHaveLength(2);

    const foo = scope.variables[0];
    expect(foo.name).toEqual('foo');
    expect(foo.references).toHaveLength(1);
    expect(foo.references[0].isValueReference).toBe(true);
    expect(foo.references[0].isTypeReference).toBe(false);
    const bar = scope.variables[1];
    expect(bar.name).toEqual('bar');
    expect(bar.references).toHaveLength(1);
    expect(bar.references[0].isValueReference).toBe(false);
    expect(bar.references[0].isTypeReference).toBe(true);
  });
  test('Does not reference types', () => {
    const {scopeManager} = parseForESLint(`
      import type Foo from 'Foo';
      (1: typeof Foo.bar);
    `);

    // Verify that scope contains single value reference to 'foo'
    const scope = scopeManager.scopes[1];
    expect(scope.variables).toHaveLength(1);

    const variable = scope.variables[0];
    expect(variable.name).toEqual('Foo');

    // Because this syntax would be invalid, we do not expect
    // a reference here. Only value references are expected
    // as an argument for typeof.
    expect(variable.references).toHaveLength(0);
  });
});

describe('Identifiers not mistakenly treated as references', () => {
  function verifyHasReferences(
    code: string,
    references: Array<{count: number, name: string}>,
  ) {
    const {scopeManager} = parseForESLint(code);

    // Module scope should contain variables with the given reference counts
    const scope = scopeManager.scopes[1];
    expect(scope.variables).toHaveLength(references.length);

    for (let i = 0; i < references.length; i++) {
      const {name, count} = references[i];
      const variable = scope.variables[i];

      expect(variable.name).toEqual(name);
      expect({
        name: variable.name,
        count: variable.references.length,
      }).toEqual({
        name,
        count,
      });
    }
  }

  describe('Enum body', () => {
    verifyHasReferences(
      `
        import Foo from 'Foo';
        enum E {
          Foo
        }
      `,
      [
        {name: 'Foo', count: 0},
        {name: 'E', count: 0},
      ],
    );
  });

  describe('QualifiedTypeIdentifier', () => {
    verifyHasReferences(
      `
        import * as Foo from 'Foo';
        import Bar from 'Bar';
        import Baz from 'Baz';

        (1: Foo.Bar.Baz);
      `,
      [
        {name: 'Foo', count: 1},
        {name: 'Bar', count: 0},
        {name: 'Baz', count: 0},
      ],
    );
  });

  describe('ObjectTypeProperty', () => {
    verifyHasReferences(
      `
        import Foo from 'Foo';
        import type Bar from 'Bar';
        (1: { Foo: Bar });
      `,
      [
        {name: 'Foo', count: 0},
        {name: 'Bar', count: 1},
      ],
    );
  });

  describe('ObjectTypeIndexer', () => {
    verifyHasReferences(
      `
        import Foo from 'Foo';
        import type Bar from 'Bar';
        (1: { [Foo: Bar]: string });
      `,
      [
        {name: 'Foo', count: 0},
        {name: 'Bar', count: 1},
      ],
    );
  });

  describe('ObjectTypeInternalSlot', () => {
    verifyHasReferences(
      `
        import Foo from 'Foo';
        import type Bar from 'Bar';
        (1: { [[Foo]]: Bar });
      `,
      [
        {name: 'Foo', count: 0},
        {name: 'Bar', count: 1},
      ],
    );
  });

  describe('FunctionTypeParam', () => {
    verifyHasReferences(
      `
        import Foo from 'Foo';
        import type Bar from 'Bar';
        (1: (Foo: Bar) => void);
      `,
      [
        {name: 'Foo', count: 0},
        {name: 'Bar', count: 1},
      ],
    );
  });

  describe('MemberExpression', () => {
    verifyHasReferences(
      `
        import Foo from 'Foo';
        import Bar from 'Bar';
        Foo.Bar;
      `,
      [
        {name: 'Foo', count: 1},
        {name: 'Bar', count: 0},
      ],
    );
  });

  describe('OptionalMemberExpression', () => {
    verifyHasReferences(
      `
        import Foo from 'Foo';
        import Bar from 'Bar';
        Foo?.Bar;
      `,
      [
        {name: 'Foo', count: 1},
        {name: 'Bar', count: 0},
      ],
    );
  });

  describe('CallExpression', () => {
    verifyHasReferences(
      `
        import Foo from 'Foo';
        import Bar from 'Bar';
        import type Baz from 'Baz';

        Foo.Bar<Baz>();
        Bar<Baz>();
      `,
      [
        {name: 'Foo', count: 1},
        {name: 'Bar', count: 1},
        {name: 'Baz', count: 2},
      ],
    );
  });

  describe('OptionalCallExpression', () => {
    verifyHasReferences(
      `
        import Foo from 'Foo';
        import Bar from 'Bar';
        import type Baz from 'Baz';

        Foo.Bar?.<Baz>();
        Bar?.<Baz>();
      `,
      [
        {name: 'Foo', count: 1},
        {name: 'Bar', count: 1},
        {name: 'Baz', count: 2},
      ],
    );
  });

  describe('PropertyDefinition', () => {
    verifyHasReferences(
      `
        import Foo from 'Foo';
        import type Bar from 'Bar';
        class C {
          Foo: Bar;
        }
      `,
      [
        {name: 'Foo', count: 0},
        {name: 'Bar', count: 1},
        {name: 'C', count: 0},
      ],
    );
  });
});

describe('Type parameters', () => {
  test('Definition creates Identifier node', () => {
    const {scopeManager} = parseForESLint(`type Foo<T> = T;`);

    // Type parameter defined in type scope
    const scope = scopeManager.scopes[2];
    expect(scope.type).toEqual(ScopeType.Type);

    // Definition contains Identifier with correct name and location
    const id = scope.variables[0].defs[0].name;
    expect(id.type === 'Identifier' && id.name).toEqual('T');
    expect(id.loc).toMatchObject({
      start: {
        line: 1,
        column: 9,
      },
      end: {
        line: 1,
        column: 10,
      },
    });
    expect(id.range).toEqual([9, 10]);
    expect(id.parent.type).toEqual('TypeParameter');
  });

  test('TypeScope not created if there are no type parameters', () => {
    const {scopeManager} = parseForESLint(`type T = T;`);

    expect(scopeManager.scopes).toHaveLength(2);
    expect(scopeManager.scopes[0].type).toEqual(ScopeType.Global);
    expect(scopeManager.scopes[1].type).toEqual(ScopeType.Module);
  });

  describe('TypeAlias', () => {
    // Type alias contains type parameter in Type scope
    verifyHasScopes(`type Foo<T> = T;`, [
      {
        type: ScopeType.Module,
        variables: [
          {
            name: 'Foo',
            type: DefinitionType.Type,
            referenceCount: 0,
          },
        ],
      },
      {
        type: ScopeType.Type,
        variables: [
          {
            name: 'T',
            type: DefinitionType.TypeParameter,
            referenceCount: 1,
          },
        ],
      },
    ]);
  });

  describe('OpaqueType', () => {
    // Opaque type contains type parameter in Type scope
    verifyHasScopes(`opaque type Foo<T> = T;`, [
      {
        type: ScopeType.Module,
        variables: [
          {
            name: 'Foo',
            type: DefinitionType.Type,
            referenceCount: 0,
          },
        ],
      },
      {
        type: ScopeType.Type,
        variables: [
          {
            name: 'T',
            type: DefinitionType.TypeParameter,
            referenceCount: 1,
          },
        ],
      },
    ]);
  });

  describe('InterfaceDeclaration', () => {
    // Interface declaration contains type parameter in Type scope
    verifyHasScopes(`interface Foo<T> { prop: T }`, [
      {
        type: ScopeType.Module,
        variables: [
          {
            name: 'Foo',
            type: DefinitionType.Type,
            referenceCount: 0,
          },
        ],
      },
      {
        type: ScopeType.Type,
        variables: [
          {
            name: 'T',
            type: DefinitionType.TypeParameter,
            referenceCount: 1,
          },
        ],
      },
    ]);
  });

  describe('FunctionTypeAnnotation', () => {
    // FunctionTypeAnnotation contains type parameter in Type scope
    verifyHasScopes(`(1: <T>(T) => void);`, [
      {
        type: ScopeType.Module,
        variables: [],
      },
      {
        type: ScopeType.Type,
        variables: [
          {
            name: 'T',
            type: DefinitionType.TypeParameter,
            referenceCount: 1,
          },
        ],
      },
    ]);
  });

  describe('DeclareClass', () => {
    // DeclareClass contains type parameter in Type scope
    verifyHasScopes(`declare class C<T> { prop: T }`, [
      {
        type: ScopeType.Module,
        variables: [
          {
            name: 'C',
            type: DefinitionType.ClassName,
            referenceCount: null,
          },
        ],
      },
      {
        type: ScopeType.Type,
        variables: [
          {
            name: 'T',
            type: DefinitionType.TypeParameter,
            referenceCount: 1,
          },
        ],
      },
    ]);
  });

  describe('FunctionDeclaration', () => {
    // Function contains type parameter in Function scope alongside value parameter
    verifyHasScopes(
      `
        function foo<T>(x) {
          (x: T);
        }
      `,
      [
        {
          type: ScopeType.Module,
          variables: [
            {
              name: 'foo',
              type: DefinitionType.FunctionName,
              referenceCount: 0,
            },
          ],
        },
        {
          type: ScopeType.Function,
          variables: [
            {
              type: null,
              name: 'arguments',
              referenceCount: 0,
            },
            {
              name: 'T',
              type: DefinitionType.TypeParameter,
              referenceCount: 1,
            },
            {
              name: 'x',
              type: DefinitionType.Parameter,
              referenceCount: 1,
            },
          ],
        },
      ],
    );
  });

  describe('FunctionExpression', () => {
    verifyHasScopes(
      `
        (function foo<T>(x) {
          (x: T);
        });
      `,
      [
        {
          type: ScopeType.Module,
          variables: [],
        },
        {
          type: ScopeType.FunctionExpressionName,
          variables: [
            {
              name: 'foo',
              type: DefinitionType.FunctionName,
              referenceCount: 0,
            },
          ],
        },
        {
          type: ScopeType.Function,
          variables: [
            {
              name: 'arguments',
              type: null,
              referenceCount: 0,
            },
            {
              name: 'T',
              type: DefinitionType.TypeParameter,
              referenceCount: 1,
            },
            {
              name: 'x',
              type: DefinitionType.Parameter,
              referenceCount: 1,
            },
          ],
        },
      ],
    );
  });

  describe('Class', () => {
    // Class contains type parameter in Class scope
    verifyHasScopes(
      `
        class C<T> {
          prop: T;
        }
      `,
      [
        {
          type: ScopeType.Module,
          variables: [
            {
              name: 'C',
              type: DefinitionType.ClassName,
              referenceCount: 0,
            },
          ],
        },
        {
          type: ScopeType.Class,
          variables: [
            {
              name: 'C',
              type: DefinitionType.ClassName,
              referenceCount: 0,
            },
            {
              name: 'T',
              type: DefinitionType.TypeParameter,
              referenceCount: 1,
            },
          ],
        },
      ],
    );
  });
});

describe('Flow type nodes in Patterns', () => {
  describe('Identifier', () => {
    verifyHasScopes(
      `
        type T = string;
        const A: T = '';
        (B: T) => {};
      `,
      [
        {
          type: ScopeType.Module,
          variables: [
            {
              name: 'T',
              type: DefinitionType.Type,
              // In variable declaration and function parameter
              referenceCount: 2,
            },
            {
              type: DefinitionType.Variable,
              name: 'A',
              referenceCount: 1,
            },
          ],
        },
        {
          type: ScopeType.Function,
          variables: [
            {
              type: DefinitionType.Parameter,
              name: 'B',
              referenceCount: 0,
            },
          ],
        },
      ],
    );
  });

  describe('ArrayPattern', () => {
    verifyHasScopes(
      `
        type T = string;
        const [A]: T = '';
        ([B]: T) => {};
      `,
      [
        {
          type: ScopeType.Module,
          variables: [
            {
              name: 'T',
              type: DefinitionType.Type,
              // In variable declaration and function parameter
              referenceCount: 2,
            },
            {
              type: DefinitionType.Variable,
              name: 'A',
              referenceCount: 1,
            },
          ],
        },
        {
          type: ScopeType.Function,
          variables: [
            {
              type: DefinitionType.Parameter,
              name: 'B',
              referenceCount: 0,
            },
          ],
        },
      ],
    );
  });

  describe('ObjectPattern', () => {
    verifyHasScopes(
      `
        type T = string;
        const {A}: T = '';
        ({B}: T) => {};
      `,
      [
        {
          type: ScopeType.Module,
          variables: [
            {
              name: 'T',
              type: DefinitionType.Type,
              // In variable declaration and function parameter
              referenceCount: 2,
            },
            {
              type: DefinitionType.Variable,
              name: 'A',
              referenceCount: 1,
            },
          ],
        },
        {
          type: ScopeType.Function,
          variables: [
            {
              type: DefinitionType.Parameter,
              name: 'B',
              referenceCount: 0,
            },
          ],
        },
      ],
    );
  });

  describe('RestElement', () => {
    verifyHasScopes(
      `
        type T = string;
        const [...A: T] = [];
        const {...B: T} = {};
      `,
      [
        {
          type: ScopeType.Module,
          variables: [
            {
              name: 'T',
              type: DefinitionType.Type,
              referenceCount: 2,
            },
            {
              type: DefinitionType.Variable,
              name: 'A',
              referenceCount: 1,
            },
            {
              type: DefinitionType.Variable,
              name: 'B',
              referenceCount: 1,
            },
          ],
        },
      ],
    );
  });

  describe('Nested patterns', () => {
    verifyHasScopes(
      `
        type T = string;
        const [A: T, B = (1: T), {C}: T]: T = [];
      `,
      [
        {
          type: ScopeType.Module,
          variables: [
            {
              name: 'T',
              type: DefinitionType.Type,
              referenceCount: 4,
            },
            {
              type: DefinitionType.Variable,
              name: 'A',
              referenceCount: 1,
            },
            {
              type: DefinitionType.Variable,
              name: 'B',
              // has 2 because it's being written to
              referenceCount: 2,
            },
            {
              type: DefinitionType.Variable,
              name: 'C',
              referenceCount: 1,
            },
          ],
        },
      ],
    );
  });
});

describe('Declare statements', () => {
  describe('DeclareTypeAlias', () => {
    verifyHasScopes(
      `
        declare type T = number;
        (1: T);
      `,
      [
        {
          type: ScopeType.Module,
          variables: [
            {
              name: 'T',
              type: DefinitionType.Type,
              referenceCount: 1,
            },
          ],
        },
      ],
    );
  });

  describe('DeclareOpaqueType', () => {
    verifyHasScopes(
      `
        declare opaque type T: number;
        (1: T);
      `,
      [
        {
          type: ScopeType.Module,
          variables: [
            {
              name: 'T',
              type: DefinitionType.Type,
              referenceCount: 1,
            },
          ],
        },
      ],
    );
  });

  describe('DeclareInterface', () => {
    verifyHasScopes(
      `
        declare interface I {};
        (1: I);
      `,
      [
        {
          type: ScopeType.Module,
          variables: [
            {
              name: 'I',
              type: DefinitionType.Type,
              referenceCount: 1,
            },
          ],
        },
      ],
    );
  });

  describe('DeclareVariable', () => {
    describe('var', () => {
      verifyHasScopes(`declare var Foo: typeof Foo;`, [
        {
          type: ScopeType.Module,
          variables: [
            {
              name: 'Foo',
              type: DefinitionType.Variable,
              referenceCount: 1,
            },
          ],
        },
      ]);
    });
    describe('let', () => {
      verifyHasScopes(`declare let Foo: typeof Foo;`, [
        {
          type: ScopeType.Module,
          variables: [
            {
              name: 'Foo',
              type: DefinitionType.Variable,
              referenceCount: 1,
            },
          ],
        },
      ]);
    });
    describe('const', () => {
      verifyHasScopes(`declare const Foo: typeof Foo;`, [
        {
          type: ScopeType.Module,
          variables: [
            {
              name: 'Foo',
              type: DefinitionType.Variable,
              referenceCount: 1,
            },
          ],
        },
      ]);
    });
  });

  describe('DeclareFunction', () => {
    verifyHasScopes(
      `
        declare function Foo(): void;
        Foo();
      `,
      [
        {
          type: ScopeType.Module,
          variables: [
            {
              name: 'Foo',
              type: DefinitionType.FunctionName,
              referenceCount: 1,
            },
          ],
        },
      ],
    );
  });

  describe('DeclareClass', () => {
    verifyHasScopes(
      `
        declare class C {}
        new C();
      `,
      [
        {
          type: ScopeType.Module,
          variables: [
            {
              name: 'C',
              type: DefinitionType.ClassName,
              referenceCount: 1,
            },
          ],
        },
      ],
    );
  });

  describe('DeclareModuleExports', () => {
    verifyHasScopes(
      `
        import type {Foo} from 'foo';
        declare module.exports: Foo;
      `,
      [
        {
          type: ScopeType.Module,
          variables: [
            {
              name: 'Foo',
              type: DefinitionType.ImportBinding,
              referenceCount: 1,
            },
          ],
        },
      ],
    );
  });

  describe('DeclareModule', () => {
    verifyHasScopes(
      `
        declare module Foo {
          declare var V: typeof V;
        }
      `,
      [
        {
          type: ScopeType.Module,
          variables: [],
        },
        {
          type: ScopeType.DeclareModule,
          variables: [
            {
              name: 'V',
              type: DefinitionType.Variable,
              referenceCount: 1,
            },
          ],
        },
      ],
    );
  });

  test('DeclareModule does not let definitions escape scope', () => {
    const {scopeManager} = parseForESLint(`
      declare module Foo {
        declare var V: string;
        declare type T = string;
      }

      (V: T);
    `);

    // All variables are defined in block scope within declare module scope
    expect(scopeManager.scopes).toHaveLength(3);

    expect(scopeManager.scopes[0].type).toEqual(ScopeType.Global);
    expect(scopeManager.scopes[0].variables).toHaveLength(0);

    expect(scopeManager.scopes[1].type).toEqual(ScopeType.Module);
    expect(scopeManager.scopes[1].variables).toHaveLength(0);

    expect(scopeManager.scopes[2].type).toEqual(ScopeType.DeclareModule);
    expect(scopeManager.scopes[2].variables).toHaveLength(2);

    // No references are resolved to the two variables in the declare module body
    const variables = scopeManager.scopes[2].variables;
    expect(variables[0].name).toEqual('V');
    expect(variables[0].references).toHaveLength(0);
    expect(variables[1].name).toEqual('T');
    expect(variables[1].references).toHaveLength(0);

    // Only the module scope contains references, however both are unresolved as they
    // cannot be resolved to the names defined within the declare module body.
    expect(scopeManager.scopes[0].references).toHaveLength(0);
    expect(scopeManager.scopes[1].references).toHaveLength(2);
    expect(scopeManager.scopes[2].references).toHaveLength(0);

    const references = scopeManager.scopes[1].references;
    expect(references[0].identifier.name).toEqual('V');
    expect(references[0].resolved).toBe(null);
    expect(references[1].identifier.name).toEqual('T');
    expect(references[1].resolved).toBe(null);
  });

  describe('DeclareNamespace', () => {
    verifyHasScopes(
      `
        declare namespace Foo {
          declare var V: typeof V;
        }
      `,
      [
        {
          type: ScopeType.Module,
          variables: [
            {
              name: 'Foo',
              type: DefinitionType.NamespaceName,
              referenceCount: 0,
            },
          ],
        },
        {
          type: ScopeType.DeclareNamespace,
          variables: [
            {
              name: 'V',
              type: DefinitionType.Variable,
              referenceCount: 1,
            },
          ],
        },
      ],
    );
  });

  test('DeclareNamespace does not let definitions escape scope', () => {
    const {scopeManager} = parseForESLint(`
      declare namespace Foo {
        declare var V: string;
        declare type T = string;
      }

      (V: T);
    `);

    // All variables are defined in block scope within declare namespace scope
    expect(scopeManager.scopes).toHaveLength(3);

    expect(scopeManager.scopes[0].type).toEqual(ScopeType.Global);
    expect(scopeManager.scopes[0].variables).toHaveLength(0);

    expect(scopeManager.scopes[1].type).toEqual(ScopeType.Module);
    expect(scopeManager.scopes[1].variables).toHaveLength(1);

    expect(scopeManager.scopes[2].type).toEqual(ScopeType.DeclareNamespace);
    expect(scopeManager.scopes[2].variables).toHaveLength(2);

    // No references are resolved to the two variables in the declare module body
    const variables = scopeManager.scopes[2].variables;
    expect(variables[0].name).toEqual('V');
    expect(variables[0].references).toHaveLength(0);
    expect(variables[1].name).toEqual('T');
    expect(variables[1].references).toHaveLength(0);

    // Only the module scope contains references, however both are unresolved as they
    // cannot be resolved to the names defined within the declare namespace body.
    expect(scopeManager.scopes[0].references).toHaveLength(0);
    expect(scopeManager.scopes[1].references).toHaveLength(2);
    expect(scopeManager.scopes[2].references).toHaveLength(0);

    const references = scopeManager.scopes[1].references;
    expect(references[0].identifier.name).toEqual('V');
    expect(references[0].resolved).toBe(null);
    expect(references[1].identifier.name).toEqual('T');
    expect(references[1].resolved).toBe(null);
  });

  describe('DeclareModule DeclareModuleExports', () => {
    verifyHasScopes(
      `
        import {module, exports} from 'Foo';
        type T = string;

        declare module Foo {
          declare module.exports: T;
        }
      `,
      [
        {
          type: ScopeType.Module,
          variables: [
            {
              name: 'module',
              type: DefinitionType.ImportBinding,
              referenceCount: 0,
            },
            {
              name: 'exports',
              type: DefinitionType.ImportBinding,
              referenceCount: 0,
            },
            {
              name: 'T',
              type: DefinitionType.Type,
              referenceCount: 1,
            },
          ],
        },
        {
          type: ScopeType.DeclareModule,
          variables: [],
        },
      ],
    );
  });

  describe('DeclareModule DeclareExportDeclaration', () => {
    // Verify that all declare export nodes introduce a definition, with a single
    // additional reference in the declare module body.
    verifyHasScopes(
      `
        declare module Foo {
          declare export type T = string;
          declare export opaque type O: string;

          declare export var V: string;
          declare export class C {}
          declare export function F(T, O): void;

          declare export {V, C, F as T};
        }
      `,
      [
        {
          type: ScopeType.Module,
          variables: [],
        },
        {
          type: ScopeType.DeclareModule,
          variables: [
            {
              name: 'T',
              type: DefinitionType.Type,
              referenceCount: 1,
            },
            {
              name: 'O',
              type: DefinitionType.Type,
              referenceCount: 1,
            },
            {
              name: 'V',
              type: DefinitionType.Variable,
              referenceCount: 1,
            },
            {
              name: 'C',
              type: DefinitionType.ClassName,
              referenceCount: 1,
            },
            {
              name: 'F',
              type: DefinitionType.FunctionName,
              referenceCount: 1,
            },
          ],
        },
      ],
    );

    verifyHasScopes(
      `
          declare module Foo {
            declare export interface I {}
          }
        `,
      [
        {
          type: ScopeType.Module,
          variables: [],
        },
        {
          type: ScopeType.DeclareModule,
          variables: [
            {
              name: 'I',
              type: DefinitionType.Type,
              referenceCount: 0,
              eslintUsed: true,
            },
          ],
        },
      ],
    );
  });
});

describe('Flow specific properties visited on non-Flow nodes', () => {
  describe('Function', () => {
    // Return type is visited, but predicate is NOT visited
    verifyHasScopes(
      `
        type T = string;
        function foo(V): T %checks(V) {}
      `,
      [
        {
          type: ScopeType.Module,
          variables: [
            {
              name: 'T',
              type: DefinitionType.Type,
              referenceCount: 1,
            },
            {
              type: DefinitionType.FunctionName,
              name: 'foo',
              referenceCount: 0,
            },
          ],
        },
        {
          type: ScopeType.Function,
          variables: [
            {
              type: null,
              name: 'arguments',
              referenceCount: null,
            },
            {
              name: 'V',
              type: DefinitionType.Parameter,
              // NOT visited
              referenceCount: 0,
            },
          ],
        },
      ],
    );
  });

  describe('Class', () => {
    // Supertype parameters and implements are visited
    verifyHasScopes(
      `
        type T = string;
        class B implements T {}
        class C extends B<T> {}
      `,
      [
        {
          type: ScopeType.Module,
          variables: [
            {
              name: 'T',
              type: DefinitionType.Type,
              referenceCount: 2,
            },
            {
              name: 'B',
              type: DefinitionType.ClassName,
              referenceCount: 1,
            },
            {
              name: 'C',
              type: DefinitionType.ClassName,
              referenceCount: 0,
            },
          ],
        },
        {
          type: ScopeType.Class,
          variables: [
            {
              type: DefinitionType.ClassName,
              name: 'B',
              referenceCount: 0,
            },
          ],
        },
        {
          type: ScopeType.Class,
          variables: [
            {
              type: DefinitionType.ClassName,
              name: 'C',
              referenceCount: 0,
            },
          ],
        },
      ],
    );
  });
});

describe('PropertyDefinition', () => {
  describe('PropertyDefinition', () => {
    verifyHasScopes(
      `
        import Foo from 'Foo';
        import type Bar from 'Bar';
        import Baz from 'Baz';
        class C {
          [Foo]: Bar = Baz;
        }
      `,
      [
        {
          type: ScopeType.Module,
          variables: [
            {
              name: 'Foo',
              type: DefinitionType.ImportBinding,
              referenceCount: 1,
            },
            {
              name: 'Bar',
              type: DefinitionType.ImportBinding,
              referenceCount: 1,
            },
            {
              name: 'Baz',
              type: DefinitionType.ImportBinding,
              referenceCount: 1,
            },
            {
              name: 'C',
              type: DefinitionType.ClassName,
              referenceCount: 0,
            },
          ],
        },
        {
          type: ScopeType.Class,
          variables: [
            {
              name: 'C',
              type: DefinitionType.ClassName,
              referenceCount: 0,
            },
          ],
        },
        {
          type: ScopeType.ClassFieldInitializer,
          variables: [],
        },
      ],
    );
  });
});

describe('FunctionExpression', () => {
  describe('Function name not referenced in return type', () => {
    verifyHasScopes(`(function foo(): foo {});`, [
      {
        type: ScopeType.Module,
        variables: [],
      },
      {
        type: ScopeType.FunctionExpressionName,
        variables: [
          {
            name: 'foo',
            type: DefinitionType.FunctionName,
            referenceCount: 0,
          },
        ],
      },
      {
        type: ScopeType.Function,
        variables: [
          {
            name: 'arguments',
            type: null,
            referenceCount: 0,
          },
        ],
      },
    ]);
  });

  /*
  // TODO - Flow behaves very inconsistently here.
  //        it's not actually possible to write error-less flow
  //        code here, so for now we'll leave this rare (if ever)
  //        edge-case broken unless there's an actual usecase for it.
  type foo = 1;
  //   ^^^ no references - shadowed by the type parameter
  (function foo<foo>(a: foo): foo {
    //                  ^^^   ^^^ refers to the type param
    //                  ^^^ refers to the type param
      return (a: foo);
    //           ^^^ refers to the function name !!!
  })
  */
  describe('Function name shadows type parameter', () => {
    verifyHasScopes(
      `
        type foo = 1;
        (function foo<foo>(a: foo): foo {
            return (a: foo);
        })
      `,
      [
        {
          type: ScopeType.Module,
          variables: [
            {
              name: 'foo',
              type: DefinitionType.Type,
              referenceCount: 0,
            },
          ],
        },
        {
          type: ScopeType.FunctionExpressionName,
          variables: [
            {
              name: 'foo',
              type: DefinitionType.FunctionName,
              referenceCount: 0,
            },
          ],
        },
        {
          type: ScopeType.Function,
          variables: [
            {
              name: 'arguments',
              type: null,
              referenceCount: 0,
            },
            {
              name: 'foo',
              type: DefinitionType.TypeParameter,
              referenceCount: 3,
            },
            {
              name: 'a',
              type: DefinitionType.Parameter,
              referenceCount: 1,
            },
          ],
        },
      ],
    );
  });
});

describe('This type annotation', () => {
  describe('Is not treated as a parameter', () => {
    verifyHasScopes(
      `
        function foo(this: string, param: number) {
          return this;
        }
      `,
      [
        {
          type: ScopeType.Module,
          variables: [
            {
              name: 'foo',
              type: DefinitionType.FunctionName,
              referenceCount: 0,
            },
          ],
        },
        {
          type: ScopeType.Function,
          variables: [
            {
              name: 'arguments',
              type: null,
              referenceCount: 0,
            },
            {
              name: 'param',
              type: DefinitionType.Parameter,
              referenceCount: 0,
            },
          ],
        },
      ],
    );
  });

  describe('Type annotation is still visited', () => {
    verifyHasScopes(
      `
        type T = string;
        function foo(this: T) {}
      `,
      [
        {
          type: ScopeType.Module,
          variables: [
            {
              name: 'T',
              type: DefinitionType.Type,
              referenceCount: 1,
            },
            {
              name: 'foo',
              type: DefinitionType.FunctionName,
              referenceCount: 0,
            },
          ],
        },
        {
          type: ScopeType.Function,
          variables: [
            {
              name: 'arguments',
              type: null,
              referenceCount: 0,
            },
          ],
        },
      ],
    );
  });

  describe('this annotation can reference generics', () => {
    describe('function decl', () => {
      verifyHasScopes(
        `
          function foo<This>(this: This) {}
        `,
        [
          {
            type: ScopeType.Module,
            variables: [
              {
                name: 'foo',
                type: DefinitionType.FunctionName,
                referenceCount: 0,
              },
            ],
          },
          {
            type: ScopeType.Function,
            variables: [
              {
                name: 'arguments',
                type: null,
                referenceCount: 0,
              },
              {
                name: 'This',
                type: DefinitionType.TypeParameter,
                referenceCount: 1,
              },
            ],
          },
        ],
      );
    });
    describe('function expr', () => {
      verifyHasScopes(
        `
          const foo = function <This>(this: This) {};
        `,
        [
          {
            type: ScopeType.Module,
            variables: [
              {
                name: 'foo',
                type: DefinitionType.Variable,
                referenceCount: 1,
              },
            ],
          },
          {
            type: ScopeType.Function,
            variables: [
              {
                name: 'arguments',
                type: null,
                referenceCount: 0,
              },
              {
                name: 'This',
                type: DefinitionType.TypeParameter,
                referenceCount: 1,
              },
            ],
          },
        ],
      );
    });
    describe('function type1', () => {
      verifyHasScopes(
        `
          type foo = <This>(this: This) => void;
        `,
        [
          {
            type: ScopeType.Module,
            variables: [
              {
                name: 'foo',
                type: DefinitionType.Type,
                referenceCount: 0,
              },
            ],
          },
          {
            type: ScopeType.Type,
            variables: [
              {
                name: 'This',
                type: DefinitionType.TypeParameter,
                referenceCount: 1,
              },
            ],
          },
        ],
      );
    });
    describe('function type2', () => {
      verifyHasScopes(
        `
          type foo<This> = (this: This) => void;
        `,
        [
          {
            type: ScopeType.Module,
            variables: [
              {
                name: 'foo',
                type: DefinitionType.Type,
                referenceCount: 0,
              },
            ],
          },
          {
            type: ScopeType.Type,
            variables: [
              {
                name: 'This',
                type: DefinitionType.TypeParameter,
                referenceCount: 1,
              },
            ],
          },
        ],
      );
    });
  });
});

describe('Imports', () => {
  describe('default', () => {
    describe('import type', () => {
      verifyHasScopes(
        `
          import type RefValue from 'foo';
          import type RefType from 'foo';
          const foo = RefValue;
          type T = RefType;
        `,
        [
          {
            type: ScopeType.Module,
            variables: [
              {
                name: 'RefValue',
                type: DefinitionType.ImportBinding,
                referenceCount: 0,
              },
              {
                name: 'RefType',
                type: DefinitionType.ImportBinding,
                referenceCount: 1,
              },
              {
                name: 'foo',
                type: DefinitionType.Variable,
                referenceCount: 1,
              },
              {
                name: 'T',
                type: DefinitionType.Type,
                referenceCount: 0,
              },
            ],
          },
        ],
      );
    });

    describe('import typeof', () => {
      verifyHasScopes(
        `
          import typeof RefValue from 'foo';
          import typeof RefType from 'foo';
          const foo = RefValue;
          type T = RefType;
        `,
        [
          {
            type: ScopeType.Module,
            variables: [
              {
                name: 'RefValue',
                type: DefinitionType.ImportBinding,
                referenceCount: 0,
              },
              {
                name: 'RefType',
                type: DefinitionType.ImportBinding,
                referenceCount: 1,
              },
              {
                name: 'foo',
                type: DefinitionType.Variable,
                referenceCount: 1,
              },
              {
                name: 'T',
                type: DefinitionType.Type,
                referenceCount: 0,
              },
            ],
          },
        ],
      );
    });

    describe('import value', () => {
      verifyHasScopes(
        `
          import RefValue from 'foo';
          import RefType from 'foo';
          const foo = RefValue;
          type T = RefType;
        `,
        [
          {
            type: ScopeType.Module,
            variables: [
              {
                name: 'RefValue',
                type: DefinitionType.ImportBinding,
                referenceCount: 1,
              },
              {
                name: 'RefType',
                type: DefinitionType.ImportBinding,
                referenceCount: 1,
              },
              {
                name: 'foo',
                type: DefinitionType.Variable,
                referenceCount: 1,
              },
              {
                name: 'T',
                type: DefinitionType.Type,
                referenceCount: 0,
              },
            ],
          },
        ],
      );
    });
  });

  describe('named', () => {
    describe('top-level', () => {
      describe('import type', () => {
        verifyHasScopes(
          `
            import type {RefValue} from 'foo';
            import type {RefType} from 'foo';
            const foo = RefValue;
            type T = RefType;
          `,
          [
            {
              type: ScopeType.Module,
              variables: [
                {
                  name: 'RefValue',
                  type: DefinitionType.ImportBinding,
                  referenceCount: 0,
                },
                {
                  name: 'RefType',
                  type: DefinitionType.ImportBinding,
                  referenceCount: 1,
                },
                {
                  name: 'foo',
                  type: DefinitionType.Variable,
                  referenceCount: 1,
                },
                {
                  name: 'T',
                  type: DefinitionType.Type,
                  referenceCount: 0,
                },
              ],
            },
          ],
        );
      });

      describe('import typeof', () => {
        verifyHasScopes(
          `
            import typeof {RefValue} from 'foo';
            import typeof {RefType} from 'foo';
            const foo = RefValue;
            type T = RefType;
          `,
          [
            {
              type: ScopeType.Module,
              variables: [
                {
                  name: 'RefValue',
                  type: DefinitionType.ImportBinding,
                  referenceCount: 0,
                },
                {
                  name: 'RefType',
                  type: DefinitionType.ImportBinding,
                  referenceCount: 1,
                },
                {
                  name: 'foo',
                  type: DefinitionType.Variable,
                  referenceCount: 1,
                },
                {
                  name: 'T',
                  type: DefinitionType.Type,
                  referenceCount: 0,
                },
              ],
            },
          ],
        );
      });

      describe('import value', () => {
        verifyHasScopes(
          `
            import {RefValue} from 'foo';
            import {RefType} from 'foo';
            const foo = RefValue;
            type T = RefType;
          `,
          [
            {
              type: ScopeType.Module,
              variables: [
                {
                  name: 'RefValue',
                  type: DefinitionType.ImportBinding,
                  referenceCount: 1,
                },
                {
                  name: 'RefType',
                  type: DefinitionType.ImportBinding,
                  referenceCount: 1,
                },
                {
                  name: 'foo',
                  type: DefinitionType.Variable,
                  referenceCount: 1,
                },
                {
                  name: 'T',
                  type: DefinitionType.Type,
                  referenceCount: 0,
                },
              ],
            },
          ],
        );
      });
    });

    describe('inline', () => {
      describe('import type', () => {
        verifyHasScopes(
          `
            import {type RefValue} from 'foo';
            import {type RefType} from 'foo';
            const foo = RefValue;
            type T = RefType;
          `,
          [
            {
              type: ScopeType.Module,
              variables: [
                {
                  name: 'RefValue',
                  type: DefinitionType.ImportBinding,
                  referenceCount: 0,
                },
                {
                  name: 'RefType',
                  type: DefinitionType.ImportBinding,
                  referenceCount: 1,
                },
                {
                  name: 'foo',
                  type: DefinitionType.Variable,
                  referenceCount: 1,
                },
                {
                  name: 'T',
                  type: DefinitionType.Type,
                  referenceCount: 0,
                },
              ],
            },
          ],
        );
      });

      describe('import typeof', () => {
        verifyHasScopes(
          `
            import {typeof RefValue} from 'foo';
            import {typeof RefType} from 'foo';
            const foo = RefValue;
            type T = RefType;
          `,
          [
            {
              type: ScopeType.Module,
              variables: [
                {
                  name: 'RefValue',
                  type: DefinitionType.ImportBinding,
                  referenceCount: 0,
                },
                {
                  name: 'RefType',
                  type: DefinitionType.ImportBinding,
                  referenceCount: 1,
                },
                {
                  name: 'foo',
                  type: DefinitionType.Variable,
                  referenceCount: 1,
                },
                {
                  name: 'T',
                  type: DefinitionType.Type,
                  referenceCount: 0,
                },
              ],
            },
          ],
        );
      });
    });
  });

  describe('namespace', () => {
    verifyHasScopes(
      `
        import * as RefValue from 'foo';
        import * as RefType from 'foo';
        const foo = RefValue;
        type T = RefType;
      `,
      [
        {
          type: ScopeType.Module,
          variables: [
            {
              name: 'RefValue',
              type: DefinitionType.ImportBinding,
              referenceCount: 1,
            },
            {
              name: 'RefType',
              type: DefinitionType.ImportBinding,
              referenceCount: 1,
            },
            {
              name: 'foo',
              type: DefinitionType.Variable,
              referenceCount: 1,
            },
            {
              name: 'T',
              type: DefinitionType.Type,
              referenceCount: 0,
            },
          ],
        },
      ],
    );
  });
});

describe('TupleTypeAnnotation', () => {
  describe('references types', () => {
    verifyHasScopes(
      `
      type T = string;
      (1: [T]);
    `,
      [
        {
          type: ScopeType.Module,
          variables: [
            {
              name: 'T',
              type: DefinitionType.Type,
              referenceCount: 1,
            },
          ],
        },
      ],
    );
  });

  describe('TupleTypeLabeledElement', () => {
    verifyHasScopes(
      `
      type T = string;
      (1: [a: T]);
    `,
      [
        {
          type: ScopeType.Module,
          variables: [
            {
              name: 'T',
              type: DefinitionType.Type,
              referenceCount: 1,
            },
          ],
        },
      ],
    );
  });

  describe('TupleTypeSpreadElement with Label', () => {
    verifyHasScopes(
      `
      type T = string;
      (1: [...a: T]);
    `,
      [
        {
          type: ScopeType.Module,
          variables: [
            {
              name: 'T',
              type: DefinitionType.Type,
              referenceCount: 1,
            },
          ],
        },
      ],
    );
  });
});

describe('AsExpression', () => {
  describe('types cast to are counted as referenced', () => {
    verifyHasScopes(
      `
      type T = number;
      1 as T;
    `,
      [
        {
          type: ScopeType.Module,
          variables: [
            {
              name: 'T',
              type: DefinitionType.Type,
              referenceCount: 1,
            },
          ],
        },
      ],
    );
  });
});
