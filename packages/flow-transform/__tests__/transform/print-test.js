/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @flow strict-local
 * @format
 */

import type {Program} from 'flow-estree';

import {SimpleTraverser} from 'flow-parser';
import * as prettier from 'prettier';
import {parse, print} from '../../src';
import {parse as parseTS} from '@typescript-eslint/parser';
import {visitorKeys as tsVisitorKeys} from '@typescript-eslint/visitor-keys';
// $FlowExpectedError[cannot-resolve-module]
import prettierConfig from '../../../.prettierrc.json';

const [prettierMajor, prettierMinor] = prettier.version.split('.').map(Number);
const modernFlowAST =
  prettierMajor > 3 || (prettierMajor === 3 && prettierMinor >= 10);

function referenceShapes(ast: Program): Array<mixed> {
  const shapes = [];
  SimpleTraverser.traverse(ast, {
    enter(node) {
      switch (node.type) {
        case 'GenericTypeAnnotation':
        case 'InterfaceExtends':
        case 'ClassImplements': {
          expect(node).not.toHaveProperty('typeParameters'); // OK
          expect(node).toHaveProperty('typeArguments'); // OK
          const names = [];
          let id = node.id;
          while (id.type === 'QualifiedTypeIdentifier') {
            names.unshift(id.id.name);
            id = id.qualification;
          }
          if (id.type === 'Identifier') {
            names.unshift(id.name);
          }
          shapes.push({
            type: node.type,
            name: names.join('.'),
            arguments:
              node.typeArguments?.params.map(param => param.type) ?? null,
          });
          break;
        }
      }
    },
    leave() {},
  });
  return shapes;
}

describe('Prettier type-argument compatibility', () => {
  // Nested, argumentless, hashbang, heritage, and inline-interface applications.
  test.each([
    'type Plain = NS.Box; type Nested<T> = NS.Box<Map<string, Array<T>>>;',
    '#!/usr/bin/env node\ntype Nested<T> = NS.Box<Map<string, Array<T>>>;',
    'interface Child<T> extends NS.Base<Array<T>> {}\ndeclare class C<T> extends NS.Base<Array<T>> implements NS.I<Map<string, T>> {}',
    'type Inline<T> = interface extends NS.Base<Array<T>> {};',
  ])('preserves references through parse-print-parse: %s', async code => {
    const {ast} = parse(code);
    const shapes = referenceShapes(ast);
    const interpreter = ast.interpreter;
    const output = await print(ast, code, prettierConfig);
    if (interpreter != null) {
      expect(output.startsWith(`#!${interpreter.value}\n`)).toBe(true); // OK
    }
    expect(referenceShapes(parse(output).ast)).toEqual(shapes); // OK
  });

  // Printing consumes the input AST rather than constructing a private copy.
  test('adapts the input AST in place for Prettier', async () => {
    const code = 'type T = Box<string>;';
    const {ast} = parse(code);
    const alias = ast.body[0];
    if (
      alias.type !== 'TypeAlias' ||
      alias.right.type !== 'GenericTypeAnnotation'
    ) {
      throw new Error('Expected a generic type alias');
    }
    const reference = alias.right;
    const argumentsNode = reference.typeArguments;
    const output = await print(ast, code, prettierConfig);
    expect(output).toBe(code + '\n'); // OK
    if (modernFlowAST) {
      expect(reference).not.toHaveProperty('typeParameters'); // OK
      expect(reference.typeArguments).toBe(argumentsNode); // OK
    } else {
      expect(reference).not.toHaveProperty('typeArguments'); // OK
      expect(
        Object.getOwnPropertyDescriptor(reference, 'typeParameters')?.value,
      ).toBe(argumentsNode); // OK
    }
  });

  // Shared references must not lose arguments on a second traversal visit.
  test('adapts a shared generic node only once', async () => {
    const code = 'type A = Box<string>; type B = Box<string>;';
    const {ast} = parse(code);
    const first = ast.body[0];
    const second = ast.body[1];
    if (first.type !== 'TypeAlias' || second.type !== 'TypeAlias') {
      throw new Error('Expected type aliases');
    }
    // $FlowExpectedError[cannot-write]
    second.right = first.right;
    const shapes = referenceShapes(ast);
    const output = await print(ast, code, prettierConfig);
    expect(output).toBe('type A = Box<string>;\ntype B = Box<string>;\n'); // OK
    expect(referenceShapes(parse(output).ast)).toEqual(shapes); // OK
  });

  // Argument comments must stay attached to nested and heritage applications.
  test('retains docblock and argument comments inside nested and heritage references', async () => {
    const code = `/** @flow */
      type Nested<T> = NS.Box</* outer */ Map<string, /* inner */ Array<T>>>;
      interface Child<T> extends NS.Base</* extends */ Array<T>> {}
      declare class C<T> implements NS.I</* implements */ Map<string, T>> {}
    `;
    const {ast} = parse(code);
    const shapes = referenceShapes(ast);
    const output = await print(ast, code, prettierConfig);
    expect(referenceShapes(parse(output).ast)).toEqual(shapes); // OK
    expect(output).toContain('NS.Box<'); // OK
    expect(output).toContain('NS.Base<'); // OK
    expect(output).toContain('NS.I<'); // OK
    for (const label of ['@flow', 'outer', 'inner', 'extends', 'implements']) {
      const marker = label === '@flow' ? '/** @flow */' : `/* ${label} */`;
      expect(output.split(marker)).toHaveLength(2); // OK
    }
    expect(output.indexOf('/* outer */')).toBeGreaterThan(
      output.indexOf('NS.Box<'),
    ); // OK
    expect(output.indexOf('/* inner */')).toBeLessThan(
      output.indexOf('Array<T>'),
    ); // OK
  });

  // Literal payloads remain printable without a graph-copying step.
  test('preserves BigInt and RegExp literals in printed output', async () => {
    const code =
      'const big = 9007199254740993n; const pattern = /a+/gi; type T = Box<Array<string>>;';
    const {ast} = parse(code);
    const shapes = referenceShapes(ast);
    const output = await print(ast, code, prettierConfig);
    expect(output).toContain('9007199254740993n'); // OK
    expect(output).toContain('/a+/gi'); // OK
    expect(referenceShapes(parse(output).ast)).toEqual(shapes); // OK
  });

  // TypeScript retains its own application and declaration field names.
  test('keeps supplied TS visitor keys and declaration parameters unchanged', async () => {
    const code = 'type T<X> = NS.Box<Map<string, X>>;';
    const ast = parseTS(code, {loc: true, range: true, sourceType: 'module'});
    const output = await print(
      ast as $FlowFixMe,
      code,
      prettierConfig,
      tsVisitorKeys,
    );
    expect(output).toBe(code + '\n'); // OK
    const reparsed = parseTS(output, {
      loc: true,
      range: true,
      sourceType: 'module',
    });
    expect(reparsed.body[0]).toMatchObject({
      typeParameters: {type: 'TSTypeParameterDeclaration'},
      typeAnnotation: {typeArguments: {type: 'TSTypeParameterInstantiation'}},
    }); // OK
  });
});
