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

import type {AFunction, TypeAnnotation, ESNode} from 'flow-estree';
import type {TranslationContext, Dep} from './TranslationUtils';

import {t} from 'flow-transform';
import {SimpleTraverser} from 'flow-parser';

export function analyzeFunctionReturn(func: AFunction): TypeAnnotation {
  const returnType = func.returnType;
  if (returnType != null) {
    return returnType;
  }

  const voidType = t.VoidTypeAnnotation();

  // We trust Flow has validated this function to only return void. Async
  // functions always wrap that return value in a Promise.
  // $FlowFixMe[incompatible-type]
  return t.TypeAnnotation({
    typeAnnotation: func.async
      ? t.GenericTypeAnnotation({
          id: t.Identifier({name: 'Promise'}),
          typeArguments: t.TypeParameterInstantiation({params: [voidType]}),
        })
      : voidType,
  });
}

export function analyzeTypeDependencies(
  rootNode: ESNode,
  context: TranslationContext,
): ReadonlyArray<Dep> {
  const deps = [];
  SimpleTraverser.traverse(rootNode, {
    enter(node: ESNode) {
      if (node.type === 'Identifier' || node.type === 'JSXIdentifier') {
        const variable = context.referenceMap.get(node);
        if (variable != null) {
          deps.push(variable.name);
        } else if (context.variableMap.has(node.name)) {
          // The scope manager may not track type references to value
          // variables (e.g. `const Foo = require('foo')` used as `Foo`
          // in a GenericTypeAnnotation). Fall back to variableMap.
          deps.push(node.name);
        }
      }
    },
    leave() {},
  });
  return deps;
}
