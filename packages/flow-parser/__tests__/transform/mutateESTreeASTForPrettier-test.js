/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @flow strict-local
 * @format
 */

import {parse as parseTS} from '@typescript-eslint/parser';
import {visitorKeys as tsVisitorKeys} from '@typescript-eslint/visitor-keys';
import {
  astNodeMutationHelpers,
  mutateESTreeASTForPrettier,
  SimpleTraverser,
} from 'flow-parser';
import {parse} from '../../__test_utils__/parse';

const versions = [undefined, '3.9.4', '3.10.0'];

function mutateForVersion(ast, version) {
  if (version == null) {
    mutateESTreeASTForPrettier(ast, undefined);
  } else {
    mutateESTreeASTForPrettier(ast, undefined, version);
  }
}

test.each(versions)(
  'preserves nested references and heritage at the printing boundary (%s)',
  version => {
    const ast = parse(`
      type Plain = Box;
      type T<X> = Box<Array<X>>;
      interface I<X> extends NS.Base<Array<X>> {}
      declare class C<X> extends NS.Base<X> implements NS.I<Array<X>> {}
    `);
    astNodeMutationHelpers.updateAllParentPointers(ast);
    const references = [];
    SimpleTraverser.traverse(ast, {
      enter(node) {
        if (
          node.type === 'GenericTypeAnnotation' ||
          node.type === 'InterfaceExtends' ||
          node.type === 'ClassImplements'
        ) {
          references.push([node, node.typeArguments]);
        }
      },
      leave() {},
    });
    mutateForVersion(ast, version);
    const argumentField =
      version === '3.10.0' ? 'typeArguments' : 'typeParameters';
    const absentField =
      version === '3.10.0' ? 'typeParameters' : 'typeArguments';
    expect(references.length).toBeGreaterThan(8); // OK
    for (const [node, args] of references) {
      expect(node[argumentField]).toBe(args); // OK
      expect(node).not.toHaveProperty(absentField); // OK
      expect(node.parent).toBeUndefined(); // OK
      if (args != null) {
        expect(args.parent).toBeUndefined(); // OK
        for (const param of args.params) {
          expect(param.parent).toBeUndefined(); // OK
        }
      }
    }
    expect(ast.body[1]).toMatchObject({
      typeParameters: {type: 'TypeParameterDeclaration'},
    }); // OK
  },
);

test.each(versions)(
  'retains existing detached-range mutation and nested arguments (%s)',
  version => {
    const ast = parse('type T = Box<{value: Inner<string>}>;');
    const alias = ast.body[0];
    if (
      alias.type !== 'TypeAlias' ||
      alias.right.type !== 'GenericTypeAnnotation'
    ) {
      throw new Error('Expected generic type alias');
    }
    const object = alias.right.typeArguments?.params[0];
    if (object?.type !== 'ObjectTypeAnnotation') {
      throw new Error('Expected object type argument');
    }
    const property = object.properties[0];
    if (property.type !== 'ObjectTypeProperty' || property.value == null) {
      throw new Error('Expected annotated property');
    }
    property.range[0] = 1;
    property.value.range[0] = 1;
    const originalValue = property.value;
    mutateForVersion(ast, version);
    const argumentField =
      version === '3.10.0' ? 'typeArguments' : 'typeParameters';
    expect(property.value).not.toBe(originalValue); // OK
    expect(property.value?.range[0]).toBe(2); // OK
    expect(property.value).toMatchObject({
      type: 'GenericTypeAnnotation',
      [argumentField]: {
        type: 'TypeParameterInstantiation',
        params: [{type: 'StringTypeAnnotation'}],
      },
    }); // OK
  },
);

test.each(versions)(
  'shared nodes retain identity across repeated mutator calls (%s)',
  version => {
    const ast = parse('type A = Box<string>; type B = Box<string>;');
    const first = ast.body[0];
    const second = ast.body[1];
    if (
      first.type !== 'TypeAlias' ||
      second.type !== 'TypeAlias' ||
      first.right.type !== 'GenericTypeAnnotation'
    ) {
      throw new Error('Expected generic type aliases');
    }
    // $FlowExpectedError[cannot-write]
    second.right = first.right;
    const shared = first.right;
    const args = shared.typeArguments;
    mutateForVersion(ast, version);
    mutateForVersion(ast, version);
    const argumentField =
      version === '3.10.0' ? 'typeArguments' : 'typeParameters';
    expect(first.right).toBe(shared); // OK
    expect(second.right).toBe(shared); // OK
    expect(shared[argumentField]).toBe(args); // OK
  },
);

test.each(versions)(
  'supplied TS visitor keys preserve argument and declaration fields (%s)',
  version => {
    const ast = parseTS('type T<X> = Box<Array<X>>;', {
      loc: true,
      range: true,
      sourceType: 'module',
    });
    mutateESTreeASTForPrettier(ast as $FlowFixMe, tsVisitorKeys, version);
    expect(ast.body[0]).toMatchObject({
      typeParameters: {type: 'TSTypeParameterDeclaration'},
      typeAnnotation: {typeArguments: {type: 'TSTypeParameterInstantiation'}},
    }); // OK
    expect(ast.body[0].typeAnnotation).not.toHaveProperty('typeParameters'); // OK
  },
);

test.each(['3.9.4', '3.10.0'])(
  'leaves current bare bounds and unified enum bodies intact (%s)',
  version => {
    const ast = parse('type T<A: string> = A; enum E of string { A, ... }');
    const alias = ast.body[0];
    const declaration = ast.body[1];
    if (
      alias.type !== 'TypeAlias' ||
      alias.typeParameters == null ||
      declaration.type !== 'EnumDeclaration'
    ) {
      throw new Error('Expected a type alias and enum declaration');
    }
    const bound = alias.typeParameters.params[0].bound;
    const body = declaration.body;
    const members = body.members;
    mutateForVersion(ast, version);
    expect(alias.typeParameters.params[0].bound).toBe(bound); // OK
    expect(bound).toMatchObject({type: 'StringTypeAnnotation'}); // OK
    expect(declaration.body).toBe(body); // OK
    expect(body).toMatchObject({
      type: 'EnumBody',
      explicitType: 'string',
      hasUnknownMembers: true,
    }); // OK
    expect(body.members).toBe(members); // OK
  },
);
