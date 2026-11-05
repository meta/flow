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

it('the exported printing boundary adapts nested references and heritage after canonical traversal', () => {
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
  mutateESTreeASTForPrettier(ast);
  expect(references.length).toBeGreaterThan(8);
  for (const [node, args] of references) {
    expect(node.typeParameters).toBe(args);
    expect(node).not.toHaveProperty('typeArguments');
    expect(node.parent).toBeUndefined();
    if (args != null) {
      expect(args.parent).toBeUndefined();
      for (const param of args.params) {
        expect(param.parent).toBeUndefined();
      }
    }
  }
  expect(ast.body[1]).toMatchObject({
    typeParameters: {type: 'TypeParameterDeclaration'},
  });
});

it('postorder adaptation follows generic nodes cloned by existing detached-range mutation', () => {
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
  mutateESTreeASTForPrettier(ast);
  expect(property.value).not.toBe(originalValue);
  expect(property.value?.range[0]).toBe(2);
  expect(property.value).toMatchObject({
    type: 'GenericTypeAnnotation',
    typeParameters: {
      type: 'TypeParameterInstantiation',
      params: [{type: 'StringTypeAnnotation'}],
    },
  });
  expect(property.value).not.toHaveProperty('typeArguments');
});

it('shared nodes retain arguments across repeated exported-mutator calls', () => {
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
  mutateESTreeASTForPrettier(ast);
  mutateESTreeASTForPrettier(ast);
  expect(first.right).toBe(shared);
  expect(second.right).toBe(shared);
  expect(shared).toHaveProperty('typeParameters', args);
  expect(shared).not.toHaveProperty('typeArguments');
});

it('supplied TS visitor keys preserve TS argument and declaration fields', () => {
  const ast = parseTS('type T<X> = Box<Array<X>>;', {
    loc: true,
    range: true,
    sourceType: 'module',
  });
  mutateESTreeASTForPrettier(ast as $FlowFixMe, tsVisitorKeys);
  expect(ast.body[0]).toMatchObject({
    typeParameters: {type: 'TSTypeParameterDeclaration'},
    typeAnnotation: {typeArguments: {type: 'TSTypeParameterInstantiation'}},
  });
  expect(ast.body[0].typeAnnotation).not.toHaveProperty('typeParameters');
});
