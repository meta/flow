/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @flow strict-local
 * @format
 */

import {parse} from '../../__test_utils__/parse';
import {SimpleTraverser} from '../../oxidized-src/traverse/SimpleTraverser';
import {transformProgram} from '../../oxidized-src/estree/TransformComponentSyntax';

it('component lowering constructs canonical argument fields and retains declaration parameters', () => {
  const ast = transformProgram(
    parse('component C<T>(value: T) { return null; }'),
    {},
  );
  expect(ast.body[0]).toMatchObject({
    type: 'FunctionDeclaration',
    typeParameters: {type: 'TypeParameterDeclaration', params: [{name: 'T'}]},
    params: [
      {
        typeAnnotation: {
          typeAnnotation: {
            type: 'GenericTypeAnnotation',
            id: {name: '$ReadOnly'},
            typeArguments: {
              type: 'TypeParameterInstantiation',
              params: [
                {
                  type: 'ObjectTypeAnnotation',
                  properties: [
                    {
                      value: {
                        type: 'GenericTypeAnnotation',
                        id: {name: 'T'},
                        typeArguments: null,
                      },
                    },
                  ],
                },
              ],
            },
          },
        },
      },
    ],
    returnType: {
      typeAnnotation: {
        type: 'GenericTypeAnnotation',
        id: {
          type: 'QualifiedTypeIdentifier',
          qualification: {name: 'React'},
          id: {name: 'Node'},
        },
        typeArguments: null,
      },
    },
  });
  let genericCount = 0;
  SimpleTraverser.traverse(ast, {
    enter(node, parent) {
      if (node.type === 'GenericTypeAnnotation') {
        genericCount++;
        expect(node).toHaveProperty('typeArguments');
        expect(node).not.toHaveProperty('typeParameters');
      }
      if (parent != null) {
        expect(node.parent).toBe(parent);
      }
    },
    leave() {},
  });
  expect(genericCount).toBe(3);
});
