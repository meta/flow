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

import type {MaybeDetachedNode} from '../detachedNode';
import type {ESNode, Program} from 'flow-estree';

import {mutateESTreeASTForPrettier} from 'flow-parser';
import * as prettier from 'prettier';
import {mutateESTreeASTCommentsForPrettier} from './comments/comments';
import type {VisitorKeysType} from 'flow-parser';

/**
 * Prints the AST as source code, mutating it in place for Prettier.
 * Treat the input AST as consumed after printing.
 */
export async function print(
  ast: MaybeDetachedNode<Program>,
  originalCode: string,
  prettierOptions: {...} = {},
  visitorKeys?: ?VisitorKeysType,
): Promise<string> {
  // $FlowExpectedError[incompatible-type] This is now safe to access.
  const program: Program = ast;
  const printed = await printProgram(
    program,
    originalCode,
    prettierOptions,
    visitorKeys,
  );

  // The parser reports a leading `#!` line as `program.interpreter`, not as a
  // comment, so nothing else prints it.
  const interpreter = program.interpreter;
  if (interpreter == null) {
    return printed;
  }
  const blankLineAfter = /^\r?\n[ \t]*\r?\n/.test(
    originalCode.slice(interpreter.range[1]),
  );
  return `#!${interpreter.value}\n${blankLineAfter ? '\n' : ''}${printed}`;
}

async function printProgram(
  program: Program,
  originalCode: string,
  prettierOptions: {...},
  visitorKeys: ?VisitorKeysType,
): Promise<string> {
  // If the AST body is empty, we can skip the cost of prettier by returning a static string of the contents.
  if (program.body.length === 0) {
    // If the program had a docblock comment, we need to create the string manually.
    const docblockComment = program.docblock?.comment;
    if (docblockComment != null) {
      return '/*' + docblockComment.value + '*/\n';
    }

    return '';
  }

  // Cleanup the comments from the AST and generate the "orginal" code needed for prettier.
  const codeForPrinting = mutateESTreeASTCommentsForPrettier(
    program,
    originalCode,
  );

  // Fix up the AST to match what prettier expects.
  mutateESTreeASTForPrettier(program, visitorKeys);

  const prettierEstreePlugin = require('prettier/plugins/estree');
  const estreePrinter = prettierEstreePlugin.printers.estree;
  const pluginParserName = visitorKeys != null ? 'typescript' : 'flow';
  const pluginParser =
    visitorKeys != null
      ? require('prettier/plugins/typescript').parsers.typescript
      : require('prettier/plugins/flow').parsers.flow;

  return prettier.format(
    codeForPrinting,
    // $FlowExpectedError[incompatible-exact] - we don't want to create a dependency on the prettier types
    {
      ...prettierOptions,
      parser: pluginParserName,
      requirePragma: false,
      plugins: [
        {
          parsers: {
            [pluginParserName]: {
              ...pluginParser,
              parse() {
                return program;
              },
            },
          },
          printers: {
            ...prettierEstreePlugin.printers,
            estree: {
              ...estreePrinter,
              print(
                path: {node: ESNode},
                options: {...},
                print: (...properties: Array<string | number>) => unknown,
                args: unknown,
              ): unknown {
                if (
                  path.node.type === 'TypeParameter' &&
                  path.node.bound != null &&
                  !path.node.usesExtendsBound
                ) {
                  return estreePrinter.print(
                    path,
                    options,
                    (...properties) => {
                      const doc = print(...properties);
                      return properties[0] === 'bound' ? [': ', doc] : doc;
                    },
                    args,
                  );
                }
                return estreePrinter.print(path, options, print, args);
              },
            },
          },
        },
      ],
    },
  );
}
