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

import type {Program} from 'flow-estree';
import type {VisitorKeysType} from 'flow-parser';
import type {PartialAnalyzeOptions, ScopeManager} from './scope-manager';

import * as HermesParser from 'flow-parser';
import {analyze} from './scope-manager';

type ParseForESLintOptions = Readonly<{
  ...PartialAnalyzeOptions,
  /**
   * Path of the file being parsed; lets the parser recognise ambient files
   * (`.d.ts`, `.js.flow`) by extension.
   */
  sourceFilename?: string,
}>;

function parse(code: string, options?: ParseForESLintOptions): Program {
  const parserOptions = {
    allowReturnOutsideFunction: true,
    flow: 'all' as 'all',
    sourceType: options?.sourceType ?? ('module' as 'module'),
    tokens: true,
    enableExperimentalComponentSyntax:
      options?.enableExperimentalComponentSyntax ?? true,
    enableExperimentalFlowMatchSyntax:
      options?.enableExperimentalFlowMatchSyntax ?? true,
    enableExperimentalFlowRecordSyntax:
      options?.enableExperimentalFlowRecordSyntax ?? true,
    ...(options?.sourceFilename != null
      ? {sourceFilename: options.sourceFilename}
      : null),
  };

  try {
    const ast = HermesParser.parse(code, parserOptions);
    return ast;
  } catch (e) {
    // Format error location for ESLint
    if (e instanceof SyntaxError) {
      // $FlowFixMe[prop-missing]
      e.lineNumber = e.loc.line;
      // $FlowFixMe[prop-missing]
      e.column = e.loc.column;
    }

    throw e;
  }
}

const VisitorKeys = HermesParser.FlowVisitorKeys;

const meta: Readonly<{name: string, version: string}> = {
  name: 'flow-eslint',
  // $FlowExpectedError[untyped-import]
  version: require('../package.json').version as string,
};

type ParseForESLintReturn = {
  ast: Program,
  scopeManager: ScopeManager,
  visitorKeys: VisitorKeysType,
};
function parseForESLint(
  code: string,
  options?: ParseForESLintOptions,
): ParseForESLintReturn {
  const ast = parse(code, options);

  // set the parent pointers
  HermesParser.SimpleTraverser.traverse(ast, {
    enter(node, parent) {
      // $FlowExpectedError[cannot-write]
      node.parent = parent;
    },
    leave() {},
  });

  const {sourceFilename: _sourceFilename, ...analyzeOptions} = options ?? {};
  const scopeManager = analyze(ast, analyzeOptions);

  return {
    ast,
    scopeManager,
    visitorKeys: VisitorKeys,
  };
}

export type * from './scope-manager';
export type {ParseForESLintOptions, ParseForESLintReturn};
export {ScopeType, DefinitionType} from './scope-manager';
export {parse, parseForESLint, VisitorKeys, meta};
