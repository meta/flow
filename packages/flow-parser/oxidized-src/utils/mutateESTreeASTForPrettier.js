/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @flow strict
 * @format
 */

'use strict';

import type {ESNode, Program, Comment} from 'flow-estree';
import type {VisitorKeysType} from '../traverse/getVisitorKeys';
import {SimpleTransform} from '../transform/SimpleTransform';
import {SimpleTraverser} from '../traverse/SimpleTraverser';

// https://github.com/prettier/prettier/blob/d962466a828f8ef51435e3e8840178d90b7ec6cd/src/language-js/parse/postprocess/index.js#L161-L182
function transformChainExpression(
  node: ESNode,
  comments: ?ReadonlyArray<Comment>,
): ESNode {
  if (comments != null) {
    // $FlowExpectedError[prop-missing]
    const joinedComments = comments.concat(node.comments ?? []);
    // $FlowExpectedError[prop-missing]
    // $FlowFixMe[cannot-write]
    node.comments = joinedComments;
  }
  switch (node.type) {
    case 'CallExpression':
      // $FlowExpectedError[cannot-spread-interface]
      return {
        ...node,
        type: 'OptionalCallExpression',
        callee: transformChainExpression(node.callee),
      };

    case 'MemberExpression':
      // $FlowExpectedError[cannot-spread-interface]
      return {
        ...node,
        type: 'OptionalMemberExpression',
        object: transformChainExpression(node.object),
      };
    // No default
  }

  return node;
}

export default function mutate(
  rootNode: Program,
  visitorKeys: ?VisitorKeysType,
  prettierVersion?: string,
): void {
  // Since we don't return the result of `transform` we need to be careful not to replace the Program root node.
  SimpleTransform.transform(rootNode, {
    transform(node): ESNode | null {
      // prettier fully expects the parent pointers are NOT set and
      // certain cases can crash due to prettier infinite-looping
      // whilst naively traversing the parent property
      // https://github.com/prettier/prettier/issues/11793
      // Note: Only needed for prettier V2, this is supported in V3
      if (node.parent) {
        // $FlowExpectedError[cannot-write]
        delete node.parent;
      }

      // prettier currently relies on the AST being in the old-school, deprecated AST format for optional chaining
      // so we have to apply their transform to our AST so it can actually format it.
      // Note: Only needed for prettier V2, this is supported in V3
      if (node.type === 'ChainExpression') {
        // $FlowFixMe[prop-missing]
        return transformChainExpression(node.expression, node?.comments);
      }

      // Prettier currently relies on comparing the `node` vs `node.value` start positions to know if an
      // `ObjectTypeProperty` is a method or not (instead of using the `node.method` boolean). To correctly print
      // the node when its not a method we need the start position to be different from the `node.value`s start
      // position.
      if (node.type === 'ObjectTypeProperty') {
        if (
          node.method === false &&
          node.kind === 'init' &&
          node.value != null &&
          node.range[0] === 1 &&
          node.value.range[0] === 1
        ) {
          // $FlowExpectedError[cannot-write]
          // $FlowExpectedError[cannot-spread-interface]
          node.value = {
            ...node.value,
            range: [2, node.value.range[1]],
          };
        }
        return node;
      }

      // Prettier currently relies on comparing the the start positions to know if the import/export specifier should have a
      // rename (eg `Name` vs `Name as Name`) when the name is exactly the same
      // So we need to ensure that the range is always the same to avoid the useless code printing
      if (node.type === 'ImportSpecifier') {
        if (node.local.name === node.imported.name) {
          if (node.local.range == null) {
            // for our TS-ast printing which has no locs
            // $FlowExpectedError[cannot-write]
            node.local.range = [0, 0];
          }
          // $FlowExpectedError[cannot-write]
          node.imported.range = [...node.local.range];
        }
        return node;
      }

      if (node.type === 'ExportSpecifier') {
        if (node.local.name === node.exported.name) {
          if (node.local.range == null) {
            // for our TS-ast printing which has no locs
            // $FlowExpectedError[cannot-write]
            node.local.range = [0, 0];
          }
          // $FlowExpectedError[cannot-write]
          node.exported.range = [...node.local.range];
        }
        return node;
      }

      return node;
    },
    visitorKeys,
  });

  if (prettierVersion != null) {
    const [major, minor] = prettierVersion.split('.').map(Number);
    if (major > 3 || (major === 3 && minor >= 10)) {
      return;
    }
  }

  SimpleTraverser.traverse(rootNode, {
    enter() {},
    leave(node) {
      switch (node.type) {
        case 'GenericTypeAnnotation':
        case 'InterfaceExtends':
        case 'ClassImplements':
          if (!Object.prototype.hasOwnProperty.call(node, 'typeArguments')) {
            break;
          }
          // $FlowExpectedError[prop-missing] Prettier's private AST field.
          // $FlowExpectedError[cannot-write]
          node.typeParameters = node.typeArguments;
          // $FlowExpectedError[cannot-write]
          delete node.typeArguments;
          break;
      }
    },
    visitorKeys,
  });
}
