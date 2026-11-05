/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @flow strict-local
 * @format
 */

import * as t from '../src/generated/node-types';
import {parse} from 'flow-parser';
import {asDetachedNode} from '../src/detachedNode';

// Fresh and detached interface declarations keep type-only default export kinds.
test('derives default-interface builder kinds from the declaration', () => {
  const parsed = parse('export default interface I {}', {flow: 'all'}).body[0];
  if (
    parsed.type !== 'ExportDefaultDeclaration' ||
    parsed.declaration.type !== 'InterfaceDeclaration'
  ) {
    throw new Error('Expected a default interface export');
  }
  for (const declaration of [
    parsed.declaration,
    asDetachedNode(parsed.declaration),
  ]) {
    const exported = t.ExportDefaultDeclaration({declaration});
    expect(exported).toHaveProperty('exportKind', 'type'); // OK
    expect(exported).toHaveProperty('declaration.parent', exported); // OK
  }
  expect(asDetachedNode(parsed)).toHaveProperty('exportKind', 'type'); // OK
});

describe('generated node builders', () => {
  // Detached programs retain their empty token-array default.
  test('defaults program tokens to an empty array', () => {
    expect(t.Program({body: []})).toHaveProperty('tokens', []); // OK
  });

  // Variance builders retain the combined directional kind.
  test('builds in-out variance', () => {
    expect(t.Variance({kind: 'in out'})).toMatchObject({
      type: 'Variance',
      kind: 'in out',
    }); // OK
  });

  // Type-operator builders preserve readonly operands and their parent pointers.
  test('builds readonly type operators', () => {
    const operator = t.TypeOperator({
      operator: 'readonly',
      typeAnnotation: t.ArrayTypeAnnotation({
        elementType: t.StringTypeAnnotation(),
      }),
    });
    expect(operator).toMatchObject({
      type: 'TypeOperator',
      operator: 'readonly',
      typeAnnotation: {type: 'ArrayTypeAnnotation'},
    }); // OK
    expect(operator).toHaveProperty('typeAnnotation.parent', operator); // OK
  });

  // Namespaced attribute builders parent the compound name like ordinary names.
  test('builds JSX namespaced attributes', () => {
    const attribute = t.JSXAttribute({
      name: t.JSXNamespacedName({
        namespace: t.JSXIdentifier({name: 'ns'}),
        name: t.JSXIdentifier({name: 'flag'}),
      }),
      value: t.StringLiteral({value: 'yes', raw: '"yes"'}),
    });
    expect(attribute).toHaveProperty('name.type', 'JSXNamespacedName'); // OK
    expect(attribute).toHaveProperty('name.parent', attribute); // OK
  });

  // Sparse-array builders preserve holes and parent only the existing nodes.
  test('builds array expressions with null holes', () => {
    const array = t.ArrayExpression({
      elements: [
        t.NumericLiteral({value: 1, raw: '1'}),
        null,
        t.SpreadElement({argument: t.Identifier({name: 'rest'})}),
      ],
      trailingComma: false,
    });
    expect(array).toMatchObject({
      type: 'ArrayExpression',
      elements: [{type: 'Literal', value: 1}, null, {type: 'SpreadElement'}],
    }); // OK
    expect(array).toHaveProperty('elements.0.parent', array); // OK
    expect(array).toHaveProperty('elements.2.parent', array); // OK
  });

  // Destructuring-array builders preserve the same null-hole representation.
  test('builds array patterns with null holes', () => {
    const array = t.ArrayPattern({
      elements: [
        t.Identifier({name: 'first'}),
        null,
        t.Identifier({name: 'last'}),
      ],
      typeAnnotation: null,
    });
    expect(array).toHaveProperty('elements.1', null); // OK
    expect(array).toHaveProperty('elements.0.parent', array); // OK
    expect(array).toHaveProperty('elements.2.parent', array); // OK
  });

  // Nullable field values remain null; annotated values still gain a parent.
  test('builds object type properties with and without annotations', () => {
    const unannotated = t.ObjectTypePropertySignature({
      key: t.Identifier({name: 'value'}),
      value: null,
      optional: false,
    });
    expect(unannotated).toMatchObject({value: null}); // OK

    const annotated = t.ObjectTypePropertySignature({
      key: t.Identifier({name: 'value'}),
      value: t.NumberTypeAnnotation(),
      optional: false,
    });
    expect(annotated).toHaveProperty('value.parent', annotated); // OK
  });

  test('parents abstract property type annotations', () => {
    const property = t.AbstractPropertyDefinition({
      key: t.Identifier({name: 'value'}),
      typeAnnotation: t.TypeAnnotation({
        typeAnnotation: t.NumberTypeAnnotation(),
      }),
      computed: false,
    });

    expect(property.value).toBeNull();
    expect(property.typeAnnotation?.parent).toBe(property);
  });

  test('preserves export kinds', () => {
    const id = t.Identifier({name: 'Foo'});

    expect(
      t.ExportSpecifier({
        exported: id,
        local: id,
        exportKind: 'type',
      }),
    ).toMatchObject({exportKind: 'type'});

    expect(
      t.ExportDefaultDeclaration({
        declaration: id,
      }),
    ).toMatchObject({exportKind: 'value'});
  });

  test('preserves optional object type indexers', () => {
    expect(
      t.ObjectTypeIndexer({
        id: null,
        key: t.StringTypeAnnotation(),
        value: t.NumberTypeAnnotation(),
        static: false,
        variance: null,
        optional: true,
      }),
    ).toMatchObject({optional: true});
  });

  test('builds declare-class extends calls', () => {
    expect(
      t.DeclareClassExtendsCall({
        callee: t.GenericTypeAnnotation({
          id: t.Identifier({name: 'Base'}),
        }),
        argument: t.InterfaceExtends({
          id: t.Identifier({name: 'Mixin'}),
        }),
      }),
    ).toMatchObject({
      type: 'DeclareClassExtendsCall',
      callee: {type: 'GenericTypeAnnotation'},
      argument: {type: 'InterfaceExtends'},
    });
  });

  test('builds namespace export declarations', () => {
    expect(
      t.NamespaceExportDeclaration({
        id: t.Identifier({name: 'Namespace'}),
      }),
    ).toMatchObject({
      type: 'NamespaceExportDeclaration',
      id: {type: 'Identifier', name: 'Namespace'},
    });
  });

  test('builds private object type fields', () => {
    expect(
      t.ObjectTypePrivateField({
        key: t.PrivateIdentifier({name: 'field'}),
      }),
    ).toMatchObject({
      type: 'ObjectTypePrivateField',
      key: {
        type: 'PrivateIdentifier',
        name: 'field',
        typeAnnotation: null,
        optional: false,
      },
    });
  });

  test('builds tagged templates with type arguments', () => {
    const template = t.TaggedTemplateExpression({
      tag: t.Identifier({name: 'tag'}),
      quasi: t.TemplateLiteral({quasis: [], expressions: []}),
      typeArguments: t.TypeParameterInstantiation({
        params: [t.StringTypeAnnotation()],
      }),
    });
    expect(template).toMatchObject({
      typeArguments: {
        type: 'TypeParameterInstantiation',
      },
    });

    expect(template).toHaveProperty('typeArguments.parent', template);
  });

  test('builds nodes with required compatibility fields', () => {
    const body = t.BlockStatement({body: []});

    expect(
      t.ArrowFunctionExpression({params: [], body, async: false}),
    ).toMatchObject({generator: false});
    expect(
      t.FunctionExpression({
        id: null,
        params: [],
        body,
        typeParameters: null,
        returnType: null,
        predicate: null,
        generator: false,
        async: false,
      }),
    ).toMatchObject({expression: false});
    expect(
      t.ImportDeclaration({
        specifiers: [],
        source: t.StringLiteral({value: 'module', raw: '"module"'}),
        attributes: [],
        importKind: 'value',
        phase: null,
      }),
    ).toMatchObject({phase: null});
  });

  test('preserves optional pattern and const enum fields', () => {
    expect(
      t.ArrayPattern({
        elements: [],
        typeAnnotation: null,
        optional: true,
      }),
    ).toMatchObject({optional: true});
    expect(
      t.ObjectPattern({
        properties: [],
        typeAnnotation: null,
        optional: true,
      }),
    ).toMatchObject({optional: true});

    const enumBody = () =>
      t.EnumBody({
        members: [],
        explicitType: 'number',
        hasUnknownMembers: false,
      });
    expect(
      t.EnumDeclaration({
        id: t.Identifier({name: 'E'}),
        body: enumBody(),
        const: true,
      }),
    ).toMatchObject({const: true});
    expect(
      t.DeclareEnum({
        id: t.Identifier({name: 'D'}),
        body: enumBody(),
        const: true,
      }),
    ).toMatchObject({const: true});
  });

  test('builds optional tuple type elements', () => {
    expect(
      t.TupleTypeElement({
        elementType: t.StringTypeAnnotation(),
        optional: true,
      }),
    ).toMatchObject({
      type: 'TupleTypeElement',
      elementType: {type: 'StringTypeAnnotation'},
      optional: true,
    });
  });
});
