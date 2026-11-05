/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @format
 * @flow
 */

import type {CodemodModule} from '../Types';

import {codemod} from '../Types';
import {t} from 'flow-transform';

/**
 * This codemod transforms legacy Flow utility types to their modern TypeScript-compatible equivalents:
 * - $NonMaybeType<T> -> NonNullable<T>
 * - $ReadOnly<T> -> Readonly<T>
 * - $ReadOnlyArray<T> -> ReadonlyArray<T>
 * - $ReadOnlyMap<K, V> -> ReadonlyMap<K, V>
 * - $ReadOnlySet<T> -> ReadonlySet<T>
 * - $Keys<T> -> keyof T
 * - $Values<T> -> Values<T>
 * - mixed -> unknown
 */
export default codemod({
  title: 'Convert legacy Flow utility types to modern equivalents',
  describe: `
Transforms legacy Flow utility types to TypeScript-compatible types:
- \`$NonMaybeType<T>\` -> \`NonNullable<T>\`
- \`$ReadOnly<T>\` -> \`Readonly<T>\`
- \`$ReadOnlyArray<T>\` -> \`ReadonlyArray<T>\`
- \`$ReadOnlyMap<K, V>\` -> \`ReadonlyMap<K, V>\`
- \`$ReadOnlySet<T>\` -> \`ReadonlySet<T>\`
- \`$Keys<T>\` -> \`keyof T\`
- \`$Values<T>\` -> \`Values<T>\`
- \`mixed\` -> \`unknown\``,
  transform: context => {
    return {
      MixedTypeAnnotation(node) {
        const newNode = t.GenericTypeAnnotation({
          id: t.Identifier({
            name: 'unknown',
          }),
        });
        context.replaceNode(node, newNode, {
          keepComments: true,
        });
      },

      GenericTypeAnnotation(node) {
        const id = node.id;
        if (id.type !== 'Identifier') {
          return;
        }

        const typeName = id.name;

        // Handle $Keys<T> -> keyof T
        if (typeName === '$Keys') {
          if (
            node.typeArguments &&
            node.typeArguments.params &&
            node.typeArguments.params.length === 1
          ) {
            const newNode = t.KeyofTypeAnnotation({
              argument: node.typeArguments.params[0],
            });
            context.replaceNode(node, newNode, {
              keepComments: true,
            });
          }
          return;
        }

        // Handle simple renames
        const typeMap = {
          $NonMaybeType: 'NonNullable',
          $ReadOnly: 'Readonly',
          $ReadOnlyArray: 'ReadonlyArray',
          $ReadOnlyMap: 'ReadonlyMap',
          $ReadOnlySet: 'ReadonlySet',
          $Values: 'Values',
        };

        const modernType = typeMap[typeName];
        if (modernType) {
          context.modifyNodeInPlace(node.id, {
            name: modernType,
          });
        }
      },
    };
  },
}) as CodemodModule;
