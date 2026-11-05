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

import {
  parse,
  parseForSnapshotBabel,
  parseForSnapshotESTree,
} from '../__test_utils__/parse';

describe('Super type arguments', () => {
  const code = `class C extends A<T> { }`;

  test('ESTree', () => {
    expect(parseForSnapshotESTree(code)).toMatchInlineSnapshot(`
     {
       "body": [
         {
           "body": {
             "body": [],
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
           "superClass": {
             "name": "A",
             "optional": false,
             "type": "Identifier",
             "typeAnnotation": null,
           },
           "superTypeArguments": {
             "params": [
               {
                 "id": {
                   "name": "T",
                   "optional": false,
                   "type": "Identifier",
                   "typeAnnotation": null,
                 },
                 "type": "GenericTypeAnnotation",
                 "typeArguments": null,
               },
             ],
             "type": "TypeParameterInstantiation",
           },
           "type": "ClassDeclaration",
           "typeParameters": null,
         },
       ],
       "type": "Program",
     }
    `);
  });

  test('Babel', () => {
    expect(parseForSnapshotBabel(code)).toMatchInlineSnapshot(`
     {
       "body": [
         {
           "body": {
             "body": [],
             "type": "ClassBody",
           },
           "id": {
             "name": "C",
             "type": "Identifier",
           },
           "superClass": {
             "name": "A",
             "type": "Identifier",
           },
           "superTypeParameters": {
             "params": [
               {
                 "id": {
                   "name": "T",
                   "type": "Identifier",
                 },
                 "type": "GenericTypeAnnotation",
                 "typeParameters": null,
               },
             ],
             "type": "TypeParameterInstantiation",
           },
           "type": "ClassDeclaration",
         },
       ],
       "type": "Program",
     }
    `);
  });
});

describe('abstract classes', () => {
  test('preserves abstract class declarations', () => {
    const [declaration] = parse('abstract class Base {}').body;

    expect(declaration).toMatchObject({
      type: 'ClassDeclaration',
      abstract: true,
    });
  });

  test('preserves abstract class expressions', () => {
    const [expression] = parse('const Expression = abstract class {};').body;
    if (expression.type !== 'VariableDeclaration') {
      throw new Error('expected a variable declaration');
    }

    expect(expression.declarations[0].init).toMatchObject({
      type: 'ClassExpression',
      abstract: true,
    });
  });

  test('preserves abstract ambient classes', () => {
    const [ambient] = parse('declare abstract class Ambient {}').body;

    expect(ambient).toMatchObject({type: 'DeclareClass', abstract: true});
  });

  test('omits abstract from concrete classes', () => {
    const [concrete] = parse('class Concrete {}').body;

    expect(concrete).not.toHaveProperty('abstract');
  });

  test('preserves abstract property definitions', () => {
    const [base] = parse(
      'abstract class Base { abstract value: number; }',
    ).body;
    if (base.type !== 'ClassDeclaration') {
      throw new Error('expected a class declaration');
    }

    expect(base.body.body[0]).toMatchObject({
      type: 'AbstractPropertyDefinition',
      value: null,
      typeAnnotation: {
        type: 'TypeAnnotation',
        typeAnnotation: {type: 'NumberTypeAnnotation'},
      },
    });
    expect(base.body.body[0]).not.toHaveProperty('override');
    expect(base.body.body[0]).not.toHaveProperty('tsAccessibility');
  });

  test('preserves abstract properties without annotations', () => {
    const [base] = parse('abstract class Base { abstract value; }').body;
    if (base.type !== 'ClassDeclaration') {
      throw new Error('expected a class declaration');
    }

    expect(base.body.body[0]).toMatchObject({
      type: 'AbstractPropertyDefinition',
      value: null,
      typeAnnotation: null,
    });
  });

  test('preserves abstract method definitions', () => {
    const [base] = parse(
      'abstract class Base { abstract method(x: number): string; }',
    ).body;
    if (base.type !== 'ClassDeclaration') {
      throw new Error('expected a class declaration');
    }

    expect(base.body.body[0]).toMatchObject({type: 'AbstractMethodDefinition'});
    expect(base.body.body[0]).not.toHaveProperty('override');
    expect(base.body.body[0]).not.toHaveProperty('tsAccessibility');
  });

  test('preserves abstract property modifiers', () => {
    const [extended] = parse(
      'abstract class Extended extends Base { public abstract override value: number; }',
    ).body;
    if (extended.type !== 'ClassDeclaration') {
      throw new Error('expected a class declaration');
    }

    expect(extended.body.body[0]).toMatchObject({
      type: 'AbstractPropertyDefinition',
      override: true,
      tsAccessibility: 'public',
    });
  });

  test('preserves abstract method modifiers', () => {
    const [extended] = parse(
      'abstract class Extended extends Base { protected abstract override method(x: number): string; }',
    ).body;
    if (extended.type !== 'ClassDeclaration') {
      throw new Error('expected a class declaration');
    }

    expect(extended.body.body[0]).toMatchObject({
      type: 'AbstractMethodDefinition',
      override: true,
      tsAccessibility: 'protected',
    });
  });
});
