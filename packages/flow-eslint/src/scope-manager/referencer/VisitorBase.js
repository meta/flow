/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @flow strict
 * @noformat
 */

'use strict';

import type {ESNode} from 'flow-estree';
import type {VisitorKeysType} from 'flow-parser';

import {FlowVisitorKeys} from 'flow-parser';

type VisitorOptions = Readonly<{
  childVisitorKeys?: VisitorKeysType | null,
}>;

function isNode(node: unknown) /*: implies node is {readonly [string]: unknown} */ {
  return (
    typeof node === 'object' && node != null && typeof node.type === 'string'
  );
}
type NodeChildValue = string | boolean | number | ESNode;
type NodeChildValueOrArray = NodeChildValue | ReadonlyArray<?NodeChildValue>;

/* abstract */ class VisitorBase {
  readonly _childVisitorKeys: VisitorKeysType;
  constructor(options: VisitorOptions) {
    this._childVisitorKeys = options.childVisitorKeys ?? FlowVisitorKeys;
  }

  /**
   * Default method for visiting children.
   * @param node the node whose children should be visited
   * @param exclude a list of keys to not visit
   */
  visitChildren(node: ?ESNode, excludeArr: ReadonlyArray<string> = []): void {
    if (node == null || node.type == null) {
      return;
    }

    const exclude = new Set<string>(excludeArr.concat(['parent']));
    const children = this._childVisitorKeys[node.type] ?? Object.keys(node);
    for (const key of children) {
      if (exclude.has(key)) {
        continue;
      }

      // $FlowExpectedError[prop-missing]
      const child: ?NodeChildValueOrArray = node[key];
      if (child == null) {
        continue;
      }

      if (Array.isArray(child)) {
        for (const subChild of child) {
          if (isNode(subChild)) {
            this.visit(subChild);
          }
        }
      } else if (isNode(child)) {
        this.visit(child);
      }
    }
  }

  /**
   * Dispatching node.
   */
  visit(node: ?ESNode): void {
    if (node == null || node.type == null) {
      return;
    }

    // $FlowExpectedError[prop-missing] - subclasses declare the relevant methods
    const visitor: ?(node: ESNode) => void = this[node.type];
    if (visitor) {
      visitor.call(this, node);
      // assume the visitor function handled the children appropriately
      return;
    }

    this.visitChildren(node);
  }

  visitArray: (?ReadonlyArray<?ESNode>) => void = (arr): void => {
    if (arr == null) {
      return;
    }

    for (const child of arr) {
      this.visit(child);
    }
  };
}

export type {VisitorOptions, VisitorKeysType as VisitorKeys};
export {VisitorBase};
