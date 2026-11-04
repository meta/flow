/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @flow strict-local
 * @format
 */

import {parse as parseBabelOriginal} from '@babel/parser';
import {parse as parseEspreeOriginal} from 'espree';
import {BABEL_VISITOR_KEYS, parse as parseHermesOriginal} from './parse';
import {SimpleTraverser} from '../oxidized-src/traverse/SimpleTraverser';

function cleanAstForHermes(
  ast: $FlowFixMe,
  style: 'babel' | 'estree',
): unknown {
  if (style === 'babel') {
    // Babel changes what properties are stripped by each version, to support some
    // older versions of Babel we don't exactly match the output of the latest babel
    // version in all cases. This code allows us to strip properties when comparing
    // AST's to ensure we can continue matching latest Babel in the tests.
    SimpleTraverser.traverse(ast, {
      enter(node: $FlowFixMe) {
        // Most older version of babel expect this property.
        if (node.type === 'OptionalCallExpression' && node.optional === false) {
          // $FlowExpectedError[cannot-write]
          delete node.optional;
        }
      },
      leave() {},
      visitorKeys: BABEL_VISITOR_KEYS,
    });
  }

  return JSON.parse(
    JSON.stringify(ast, (_, value) => {
      if (typeof value === 'bigint') {
        return value.toString();
      }
      return value;
    }),
  );
}

function cleanAstForEspree(ast: $FlowFixMe): unknown {
  // $FlowExpectedError[incompatible-use]
  delete ast.comments;
  // $FlowExpectedError[incompatible-use]
  delete ast.tokens;
  // $FlowExpectedError[incompatible-use]
  delete ast.errors;
  SimpleTraverser.traverse(ast, {
    enter(node: $FlowFixMe) {
      delete node.parent;
      delete node.start;
      delete node.end;
    },
    leave() {},
  });
  return JSON.parse(
    JSON.stringify(ast, (_, value) => {
      if (typeof value === 'bigint') {
        return value.toString();
      }
      return value;
    }),
  );
}

function cleanBabelAst(ast: $FlowFixMe): unknown {
  delete ast.errors;

  SimpleTraverser.traverse(ast, {
    enter(node: $FlowFixMe) {
      // Babel adds "extra" properties to capture parenthesized locations,
      // Hermes does not support these.
      if (node.extra != null && node.extra.parenthesized === true) {
        delete node.extra;
      }
    },
    leave() {},
    visitorKeys: BABEL_VISITOR_KEYS,
  });

  return JSON.parse(
    JSON.stringify(ast, (_, value) => {
      if (typeof value === 'bigint') {
        return value.toString();
      }
      return value;
    }),
  );
}

export function parseBabel(source: string): unknown {
  const ast = parseBabelOriginal(source, {
    attachComment: false,
    errorRecovery: false,
    plugins: [
      'bigInt',
      'dynamicImport',
      'classPrivateMethods',
      'classPrivateProperties',
      'privateIn',

      // from WWW config
      'jsx',
      ['flow', {enums: true}],
      'objectRestSpread',
      'classProperties',
      'numericSeparator',
      'optionalChaining',
      'optionalCatchBinding',
      'nullishCoalescingOperator',
    ],
    ranges: false,
    sourceType: 'module',
    tokens: false,
  });

  return cleanBabelAst(ast);
}

export function parseEspree(source: string): unknown {
  const ast = parseEspreeOriginal(source, {
    comment: false,
    ecmaVersion: 'latest',
    ecmaFeatures: {
      jsx: true,
    },
    loc: false,
    range: false,
    sourceType: 'module',
    tokens: false,
  });
  return cleanAstForEspree(ast);
}

export function parseHermes(
  source: string,
  style: 'babel' | 'estree',
): unknown {
  // $FlowExpectedError[incompatible-type] - the overloads confuse flow
  const ast = parseHermesOriginal(source, {
    babel: style === 'babel',
    sourceType: 'module',
    tokens: false,
  });
  return cleanAstForHermes(ast, style);
}

export type AlignmentExpectation = Readonly<
  | {
      expectToFail: false,
    }
  | {
      expectToFail: 'ast-diff',
    }
  | {
      expectToFail: 'hermes-exception' | 'espree-exception' | 'babel-exception',
      expectedExceptionMessage: string,
    },
>;
export type AlignmentCase = Readonly<{
  code: string,
  espree: AlignmentExpectation,
  babel: AlignmentExpectation,
}>;

function expectAlignment(
  hermesAst: () => unknown,
  otherAst: () => unknown,
  expectation: AlignmentExpectation,
  parserType: 'Babel' | 'ESTree',
): void {
  switch (expectation.expectToFail) {
    case false:
      if (parserType === 'Babel') {
        // Received = Hermes, Expected = Babel
        expect(hermesAst()).toEqual(otherAst());
      } else {
        // Received = Hermes, Expected = Espree
        expect(hermesAst()).toMatchObject(otherAst());
      }
      break;

    case 'ast-diff':
      if (parserType === 'Babel') {
        // Received = Hermes, Expected = Babel
        expect(hermesAst()).not.toEqual(otherAst());
      } else {
        // Received = Hermes, Expected = Espree
        expect(hermesAst()).not.toMatchObject(otherAst());
      }
      break;

    case 'hermes-exception':
      expect(hermesAst).toThrow(expectation.expectedExceptionMessage);
      break;

    case 'espree-exception':
    case 'babel-exception':
      expect(otherAst).toThrow(expectation.expectedExceptionMessage);
      break;
  }
}

export function expectEspreeAlignment(testCase: AlignmentCase): void {
  const hermesAst = () => parseHermes(testCase.code, 'estree');
  const espreeAst = () => parseEspree(testCase.code);

  expectAlignment(hermesAst, espreeAst, testCase.espree, 'ESTree');
}

export function expectBabelAlignment(testCase: AlignmentCase): void {
  const hermesAst = () => parseHermes(testCase.code, 'babel');
  const babelAst = () => parseBabel(testCase.code);

  expectAlignment(hermesAst, babelAst, testCase.babel, 'Babel');
}
