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

import type {ScopeManager} from 'flow-eslint';
import type {Program, ESNode, StringLiteral} from 'flow-estree';

import {SimpleTransform} from 'flow-parser';

export type SourceMapperParams = {
  module: string,
};

export type MapperOptions = Readonly<{
  sourceMapper: (v: SourceMapperParams) => string,
}>;

export function flowImportTo(
  ast: Program,
  _code: string,
  _scopeManager: ScopeManager,
  opts: MapperOptions,
): Program {
  function mapSource(source: StringLiteral): StringLiteral {
    const resultValue = opts.sourceMapper({module: source.value});
    if (resultValue === source.value) {
      return source;
    }

    return SimpleTransform.nodeWith(source, {
      value: resultValue,
      raw: `"${resultValue}"`,
    });
  }

  const result = SimpleTransform.transform(ast, {
    transform(node: ESNode): null | ESNode {
      switch (node.type) {
        case 'ImportDeclaration': {
          return SimpleTransform.nodeWith(node, {
            source: mapSource(node.source),
          });
        }
        case 'DeclareExportAllDeclaration':
        case 'ExportAllDeclaration':
        case 'DeclareExportDeclaration':
        case 'ExportNamedDeclaration': {
          if (node.source != null) {
            return SimpleTransform.nodeWith(node, {
              source: mapSource(node.source),
            });
          }
          return node;
        }
        default: {
          return node;
        }
      }
    },
  });
  if (result == null || result.type !== 'Program') {
    throw new Error('flowImportTo: Unexpected transform result.');
  }
  return result;
}
