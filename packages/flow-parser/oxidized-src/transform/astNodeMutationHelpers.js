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

import type {ESNode} from 'flow-estree';
import type {VisitorKeysType} from '../traverse/getVisitorKeys';

import {
  arrayIsEqual,
  removeFromArray,
  replaceInArray,
} from './astArrayMutationHelpers';
import {getVisitorKeys, isNode} from '../traverse/getVisitorKeys';
import {SimpleTraverser} from '../traverse/SimpleTraverser';

function getParentKey(
  target: ESNode,
  parent: ESNode,
  visitorKeys?: ?VisitorKeysType,
): Readonly<
  | {
      type: 'single',
      node: ESNode,
      key: string,
    }
  | {
      type: 'array',
      node: ESNode,
      key: string,
      targetIndex: number,
    },
> {
  if (parent == null) {
    throw new Error(`Expected parent node to be set on "${target.type}"`);
  }
  for (const key of getVisitorKeys(parent, visitorKeys)) {
    if (
      isNode(
        // $FlowExpectedError[prop-missing]
        parent[key],
      )
    ) {
      // $FlowFixMe[invalid-compare]
      if (parent[key] === target) {
        return {type: 'single', node: parent, key};
      }
    } else if (
      Array.isArray(
        // $FlowExpectedError[prop-missing]
        parent[key],
      )
    ) {
      for (let i = 0; i < parent[key].length; i += 1) {
        // $FlowExpectedError[invalid-tuple-index]
        const current = parent[key][i];
        // $FlowFixMe[invalid-compare]
        if (current === target) {
          return {type: 'array', node: parent, key, targetIndex: i};
        }
      }
    }
  }

  // this shouldn't happen ever
  throw new Error(
    `Expected to find the ${target.type} as a direct child of the ${parent.type}.`,
  );
}

/**
 * Replace a node with a new node within an AST (via the parent pointer).
 */
export function replaceNodeOnParent(
  originalNode: ESNode,
  originalNodeParent: ESNode,
  nodeToReplaceWith: ESNode | ReadonlyArray<ESNode>,
  visitorKeys?: ?VisitorKeysType,
): void {
  const replacementParent = getParentKey(
    originalNode,
    originalNodeParent,
    visitorKeys,
  );
  const parent = replacementParent.node;
  if (replacementParent.type === 'array') {
    // $FlowExpectedError[prop-missing]
    parent[replacementParent.key] = replaceInArray(
      // $FlowExpectedError[prop-missing]
      parent[replacementParent.key],
      replacementParent.targetIndex,
      Array.isArray(nodeToReplaceWith)
        ? nodeToReplaceWith
        : [nodeToReplaceWith],
    );
  } else {
    if (Array.isArray(nodeToReplaceWith)) {
      throw new Error(
        `Cannot insert array into non-array parent type: ${parent.type}`,
      );
    }
    // $FlowExpectedError[prop-missing]
    parent[replacementParent.key] = nodeToReplaceWith;
  }
}

/**
 * Remove a node from the AST its connected to (via the parent pointer).
 */
export function removeNodeOnParent(
  originalNode: ESNode,
  originalNodeParent: ESNode,
  visitorKeys?: ?VisitorKeysType,
): void {
  const replacementParent = getParentKey(
    originalNode,
    originalNodeParent,
    visitorKeys,
  );
  const parent = replacementParent.node;
  if (replacementParent.type === 'array') {
    // $FlowExpectedError[prop-missing]
    parent[replacementParent.key] = removeFromArray(
      // $FlowExpectedError[prop-missing]
      parent[replacementParent.key],
      replacementParent.targetIndex,
    );
  } else {
    // $FlowExpectedError[prop-missing]
    parent[replacementParent.key] = null;
  }
}

/**
 * Corrects the parent pointers in direct children of the given node.
 */
export function setParentPointersInDirectChildren(
  node: ESNode,
  visitorKeys?: ?VisitorKeysType,
): void {
  for (const key: $FlowFixMe of getVisitorKeys(node, visitorKeys)) {
    if (isNode(node[key])) {
      // $FlowExpectedError[cannot-write]
      node[key].parent = node;
    } else if (Array.isArray(node[key])) {
      for (const child of node[key]) {
        if (child != null) {
          child.parent = node;
        }
      }
    }
  }
}

/**
 * Traverses the entire subtree to ensure the parent pointers are set correctly.
 */
export function updateAllParentPointers(
  node: ESNode,
  visitorKeys?: ?VisitorKeysType,
) {
  SimpleTraverser.traverse(node, {
    enter(node, parent) {
      // $FlowExpectedError[cannot-write]
      node.parent = parent;
    },
    leave() {},
    visitorKeys,
  });
}

/**
 * Clone node and add new props.
 *
 * This will only create a new object if the overrides actually result in a change.
 */
export function nodeWith<T extends ESNode>(
  node: T,
  overrideProps: Partial<T>,
  visitorKeys?: ?VisitorKeysType,
): T {
  // Check if this will actually result in a change, maintaining referential equality is important.
  const willBeUnchanged = Object.entries(overrideProps).every(
    ([key, value]) => {
      const node_: $FlowFixMe = node;
      if (Array.isArray(value)) {
        return Array.isArray(node_[key])
          ? arrayIsEqual(node_[key], value)
          : false;
      }
      return node_[key] === value;
    },
  );
  if (willBeUnchanged) {
    return node;
  }

  // Create new node.
  // $FlowExpectedError[cannot-spread-interface]
  const newNode: T = {
    ...node,
    ...overrideProps,
  };

  // Ensure parent pointers are correctly set within this nodes children.
  setParentPointersInDirectChildren(newNode, visitorKeys);

  return newNode;
}

/**
 * Shallow clones node, providing a new reference for an existing node.
 */
export function shallowCloneNode<T extends ESNode>(
  node: T,
  visitorKeys?: ?VisitorKeysType,
): T {
  // $FlowExpectedError[cannot-spread-interface]
  const newNode: T = {...node};

  // Ensure parent pointers are correctly set within this nodes children.
  setParentPointersInDirectChildren(newNode, visitorKeys);

  return newNode;
}

/**
 * Deeply clones node and its entire tree.
 */
export function deepCloneNode<T extends ESNode>(
  node: T,
  visitorKeys?: ?VisitorKeysType,
): T {
  const clone: T = JSON.parse(
    JSON.stringify(node, (key, value) => {
      // null out parent pointers
      if (key === 'parent') {
        return undefined;
      }
      return value;
    }),
  );

  updateAllParentPointers(clone, visitorKeys);

  return clone;
}
