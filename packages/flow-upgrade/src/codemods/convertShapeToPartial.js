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
import {getTypeArguments} from '../codemodUtils/getTypeArguments';

export default codemod({
  title: 'Convert `$Shape` to `Partial`',
  describe:
    'Converts usage of the `$Shape` utility type to `Partial`. It is possible that this will create type errors that you will have to resolve.',
  transform: context => {
    return {
      'GenericTypeAnnotation[id.name="$Shape"]'(node) {
        if (!getTypeArguments(node)?.params.length) {
          return;
        }
        context.modifyNodeInPlace(node.id, {
          name: 'Partial',
        });
      },
    };
  },
}) as CodemodModule;
