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
import type {
  FunctionTypeAnnotation,
  FunctionTypeParam,
  HookTypeAnnotation,
} from 'flow-estree';

import {codemod} from '../Types';
import {t} from 'flow-transform';

export default codemod({
  title: 'Name function type params',
  describe:
    'Name every unnamed param in function types, including `declare function` signatures: `v` if it is the only param, `v0`, `v1`, ... if there are several, and `rest` for a rest param.',
  transform: context => {
    function nameParams(
      node: FunctionTypeAnnotation | HookTypeAnnotation,
    ): void {
      const {params, rest} = node;
      const usedNames = new Set<string>();
      for (const param of params) {
        if (param.name != null) {
          usedNames.add(param.name.name);
        }
      }
      if (rest?.name != null) {
        usedNames.add(rest.name.name);
      }

      function nameParam(param: FunctionTypeParam, baseName: string): void {
        let name = baseName;
        while (usedNames.has(name)) {
          name = `${name}_`;
        }
        usedNames.add(name);
        context.modifyNodeInPlace(param, {name: t.Identifier({name})});
      }

      params.forEach((param, index) => {
        if (param.name == null) {
          nameParam(param, params.length === 1 ? 'v' : `v${index}`);
        }
      });
      if (rest != null && rest.name == null) {
        nameParam(rest, 'rest');
      }
    }

    return {
      FunctionTypeAnnotation: nameParams,
      HookTypeAnnotation: nameParams,
    };
  },
}) as CodemodModule;
