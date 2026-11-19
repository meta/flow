/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @flow strict-local
 * @format
 */

import type {ESNode, ESQueryNodeSelectors, Program} from 'flow-estree';
import type {ScopeManager, Scope, Variable} from 'flow-eslint';
import type {EmitterListener} from './SafeEmitter';

import {codeFrameColumns} from '@babel/code-frame';
import {NodeEventGenerator} from './NodeEventGenerator';
import {SafeEmitter} from './SafeEmitter';
import {SimpleTraverser} from 'flow-parser';

export type TraversalContextBase = Readonly<{
  /**
   * Gets the variables that were declared by the given node.
   */
  getDeclaredVariables: (node: ESNode) => ReadonlyArray<Variable>,
  /**
   * Gets the nearest variable declared for the given name declare in the
   * current or an upper scope.
   */
  getBinding: (name: string) => ?Variable,
  /**
   * Gets the scope for the given node.
   * Defaults to the currently traversed node.
   */
  getScope: (node?: ESNode) => Scope,
  /**
   * Creates a full code frame for the node along with the message.
   *
   * i.e. `context.buildCodeFrame(node, 'foo')` will create a string like:
   * ```
   * 56 | function () {
   *    | ^^^^^^^^^^^^^
   * 57 | }.bind(this)
   *    | ^^ foo
   * ```
   */
  buildCodeFrame: (node: ESNode, message: string) => string,
  /**
   * Creates a simple code frame for the node along with the message.
   * Use this if you want a condensed marker for your message.
   *
   * i.e. `context.logWithNode(node, 'foo')` will create a string like:
   * ```
   * [FunctionExpression:56:44] foo
   * ```
   * (where 56:44 represents L56, Col44)
   */
  buildSimpleCodeFrame: (node: ESNode, message: string) => string,
  /**
   * Can be called at any point during the traversal to immediately stop traversal
   * entirely.
   */
  stopTraversal: () => void,
  /**
   * Can be called within the traversal "enter" function to prevent the traverser
   * from traversing the node any further, essentially culling the remainder of the
   * AST branch from traversal.
   */
  skipTraversal: () => void,
}>;
export type TraversalContext<T> = Readonly<{
  ...TraversalContextBase,
  ...T,
}>;

export type Visitor<T> = (context: TraversalContext<T>) => ESQueryNodeSelectors;

/**
 * Traverse the AST with additional context members provided by `additionalContext`.
 * @param ast the ESTree AST to traverse
 * @param scopeManager the eslint-scope compatible scope manager instance calculated using the ast
 * @param additionalContext a callback function which returns additional context members to add to the context provided to the visitor
 */
export function traverseWithContext<T = TraversalContextBase>(
  code: string,
  ast: Program,
  scopeManager: ScopeManager,
  additionalContext: (v: TraversalContextBase) => T,
  visitor: Visitor<T>,
): void {
  const emitter = new SafeEmitter();

  let currentNode: ESNode = ast;
  let shouldSkipTraversal = false;
  let shouldStopTraversal = false;

  const getScope = (givenNode: ESNode = currentNode) => {
    // On Program node, get the outermost scope to avoid return Node.js special function scope or ES modules scope.
    const inner = givenNode.type !== 'Program';

    for (let node = givenNode; node; node = node.parent) {
      const scope = scopeManager.acquire(node, inner);

      if (scope) {
        if (scope.type === 'function-expression-name') {
          return scope.childScopes[0];
        }
        return scope;
      }
    }

    return scopeManager.scopes[0];
  };

  const traversalContextBase: TraversalContextBase = Object.freeze({
    buildCodeFrame: (node: ESNode, message: string): string => {
      // babel uses 1-indexed columns
      const locForBabel = {
        start: {
          line: node.loc.start.line,
          column: node.loc.start.column + 1,
        },
        end: {
          line: node.loc.end.line,
          column: node.loc.end.column + 1,
        },
      };
      return codeFrameColumns(code, locForBabel, {
        linesAbove: 0,
        linesBelow: 0,
        highlightCode: process.env.NODE_ENV !== 'test',
        message: message,
      });
    },

    buildSimpleCodeFrame: (node: ESNode, message: string): string => {
      return `[${node.type}:${node.loc.start.line}:${node.loc.start.column}] ${message}`;
    },

    getDeclaredVariables: (node: ESNode) =>
      scopeManager.getDeclaredVariables(node),

    getBinding: (name: string) => {
      let currentScope: null | Scope = getScope();

      while (currentScope != null) {
        for (const variable of currentScope.variables) {
          if (variable.defs.length && variable.name === name) {
            return variable;
          }
        }
        currentScope = currentScope.upper;
      }

      return null;
    },

    getScope,

    stopTraversal: () => {
      shouldStopTraversal = true;
    },

    skipTraversal: () => {
      shouldSkipTraversal = true;
    },
  });

  const traversalContext: TraversalContext<T> = Object.freeze({
    ...traversalContextBase,
    ...additionalContext(traversalContextBase),
  });

  const selectors = visitor(traversalContext);

  // add all the selectors from the visitor as listeners
  Object.keys(selectors).forEach(selector => {
    // flow doesn't want us to be general here - but it's safe
    const listener: ?EmitterListener = (selectors as $FlowFixMe)[selector];
    if (listener) {
      emitter.on(selector, listener);
    }
  });

  const eventGenerator = new NodeEventGenerator(emitter);

  function checkTraversalFlags(): void {
    if (shouldStopTraversal) {
      // No need to reset the flag since we won't enter any more nodes.
      throw SimpleTraverser.Break;
    }

    if (shouldSkipTraversal) {
      shouldSkipTraversal = false;
      throw SimpleTraverser.Skip;
    }
  }

  SimpleTraverser.traverse(ast, {
    enter(node) {
      currentNode = node;
      eventGenerator.enterNode(node);
      checkTraversalFlags();
    },
    leave(node) {
      currentNode = node;
      eventGenerator.leaveNode(node);
      checkTraversalFlags();
    },
  });
}

export function traverse(
  code: string,
  ast: Program,
  scopeManager: ScopeManager,
  visitor: Visitor<void>,
): void {
  traverseWithContext(code, ast, scopeManager, () => {}, visitor);
}
