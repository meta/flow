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

export default codemod({
  title: 'Replace `T: {}` with `T: {...}` in type parameter bounds',
  describe: 'Replace `T: {}` with `T: {...}` in type parameter bounds',
  transform: context => {
    return {
      TypeParameter(node) {
        if (node.bound != null) {
          const annotation = node.bound;
          if (
            annotation.type === 'ObjectTypeAnnotation' &&
            !annotation.inexact &&
            annotation.properties.length === 0 &&
            annotation.callProperties.length === 0 &&
            annotation.indexers.length === 0 &&
            annotation.internalSlots.length === 0
          ) {
            context.replaceNode(
              annotation,
              t.ObjectTypeAnnotation({
                properties: [],
                callProperties: [],
                exact: false,
                inexact: true,
                indexers: [],
                internalSlots: [],
              }),
            );
          }
        }
      },
    };
  },
}) as CodemodModule;
