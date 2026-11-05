/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @flow strict-local
 * @format
 * @oncall flow
 */

import type {CodemodModule} from '../Types';
import {t} from 'flow-transform';
import {codemod} from '../Types';

export default codemod({
  title: 'Replace $TEMPORARY$ types',
  describe: [
    'Converts:',
    ' - `$TEMPORARY$object<{props}>` to `$ReadOnly<{props}>`',
    ' - `$TEMPORARY$array<T>` to `$ReadOnlyArray<T>`',
    ' - `$TEMPORARY$number<42>` annotations to `number`',
    ' - `$TEMPORARY$string<"foo">` annotations to `string`',
  ].join('\n'),
  transform: context => {
    return {
      GenericTypeAnnotation(node) {
        const id = node.id;
        if (
          id.type === 'Identifier' &&
          id.name === '$TEMPORARY$object' &&
          node.typeArguments != null
        ) {
          context.modifyNodeInPlace(node, {
            id: t.Identifier({
              name: '$ReadOnly',
            }),
          });
        }
        if (
          id.type === 'Identifier' &&
          id.name === '$TEMPORARY$array' &&
          node.typeArguments != null
        ) {
          context.modifyNodeInPlace(node, {
            id: t.Identifier({
              name: '$ReadOnlyArray',
            }),
          });
        }
        if (id.type === 'Identifier' && id.name === '$TEMPORARY$number') {
          const new_node = t.NumberTypeAnnotation();
          context.replaceNode(node, new_node, {keepComments: true});
        }
        if (id.type === 'Identifier' && id.name === '$TEMPORARY$string') {
          const new_node = t.StringTypeAnnotation();
          context.replaceNode(node, new_node, {keepComments: true});
        }
      },
    };
  },
}) as CodemodModule;
