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
import {t} from 'flow-transform';
import {codemod} from '../Types';

const UTILITY_TYPES = new Set([
  'React$ComponentType',
  'React$AbstractComponent',
  'React$Element',
  'React$MixedElement',
  'React$ElementType',
  'React$Key',
  'React$Ref',
  'React$RefSetter',
  'React$Node',
  'React$Context',
  'React$Portal',
  'React$ElementProps',
  'React$ElementConfig',
  'React$ElementRef',
  'React$Config',
  'React$Config',
  'React$RefObject',
]);

/**
 * This codemod will replace `React$AbstractComponent` with `React.AbstractComponent`.
 */
export default codemod({
  title: 'Replace React$SomeUtilityType with React.SomeUtilityType.',
  transform: context => {
    return {
      GenericTypeAnnotation(node) {
        if (typeof node.id.name !== 'string') {
          return;
        }
        const utilTypeName = node.id.name;
        if (!UTILITY_TYPES.has(utilTypeName)) {
          return;
        }
        context.modifyNodeInPlace(node, {
          id: t.QualifiedTypeIdentifier({
            id: t.Identifier({
              name: utilTypeName.substring('React$'.length),
            }),
            qualification: t.Identifier({name: 'React'}),
          }),
        });
      },
    };
  },
}) as CodemodModule;
