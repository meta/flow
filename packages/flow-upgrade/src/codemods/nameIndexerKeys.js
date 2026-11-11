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
import type {ObjectTypeIndexer} from 'flow-estree';
import type {TransformContext} from 'flow-transform';

import {codemod} from '../Types';
import {getTypeArguments} from '../codemodUtils/getTypeArguments';
import {t} from 'flow-transform';

// Flow reads an unlabeled `[K]` as a computed property key, not an index
// signature, when `K` is a string or number literal, or a name whose leftmost
// identifier binds a value other than a namespace and binds no type.
function isComputedKey(
  key: ObjectTypeIndexer['key'],
  context: TransformContext,
): boolean {
  switch (key.type) {
    case 'StringLiteralTypeAnnotation':
    case 'NumberLiteralTypeAnnotation':
      return true;
    case 'GenericTypeAnnotation': {
      if (getTypeArguments(key) != null) {
        return false;
      }
      let id = key.id;
      while (id.type === 'QualifiedTypeIdentifier') {
        id = id.qualification;
      }
      if (id.type !== 'Identifier') {
        return false;
      }
      const defs = context.getBinding(id.name)?.defs ?? [];
      if (defs.some(def => def.isTypeDefinition && !def.isVariableDefinition)) {
        return false;
      }
      return defs.some(
        def =>
          def.isVariableDefinition &&
          def.type !== 'NamespaceName' &&
          def.node.type !== 'ImportNamespaceSpecifier',
      );
    }
    default:
      return false;
  }
}

export default codemod({
  title: 'Label dictionary keys',
  describe:
    "Label every unlabeled index signature key `key`: `{[string]: T}` becomes `{[key: string]: T}`. Computed property keys, as in `{['a']: T}` or `{[K]: T}` where `K` is a value, are left alone.",
  transform: context => {
    return {
      ObjectTypeIndexer(node: ObjectTypeIndexer) {
        if (node.id != null || isComputedKey(node.key, context)) {
          return;
        }
        context.modifyNodeInPlace(node, {id: t.Identifier({name: 'key'})});
      },
    };
  },
}) as CodemodModule;
