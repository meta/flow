/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @format
 */

import {mutateESTreeASTForPrettier, parse} from 'flow-parser';
import {
  formatWithCursor,
  version as prettierVersion,
} from 'prettier/standalone';
import * as estree from 'prettier/plugins/estree';

export type FormatResult = {formatted: string, cursorOffset: number};
export type Formatter = (
  code: string,
  cursorOffset: number,
) => Promise<FormatResult>;

const [prettierMajor, prettierMinor] = prettierVersion.split('.').map(Number);
const needsLegacyBoundColon =
  prettierMajor < 3 || (prettierMajor === 3 && prettierMinor < 10);
const estreePrinter = estree.printers.estree;
const flowPlugin = {
  parsers: {
    'try-flow': {
      parse(code: string): unknown {
        const ast = parse(code, {
          flow: 'all',
          babel: false,
          throwOnParseErrors: true,
        });
        mutateESTreeASTForPrettier(ast, undefined, prettierVersion);
        if (ast.interpreter != null) {
          ast.comments.unshift(ast.interpreter);
          delete ast.interpreter;
        }
        return ast;
      },
      astFormat: 'estree',
      locStart: (node: any) => node.range[0],
      locEnd: (node: any) => node.range[1],
    },
  },
  printers: {
    ...estree.printers,
    estree: {
      ...estreePrinter,
      // Match flow-transform/src/transform/print.js for legacy `T: Bound` syntax.
      print(path: any, options: any, print: any, args: any): any {
        if (
          needsLegacyBoundColon &&
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
};

export function formatCode(
  code: string,
  cursorOffset: number,
): Promise<FormatResult> {
  return formatWithCursor(code, {
    parser: 'try-flow',
    plugins: [flowPlugin, estree],
    cursorOffset,
    singleQuote: true,
    bracketSpacing: false,
    bracketSameLine: true,
    endOfLine: 'auto',
  });
}
