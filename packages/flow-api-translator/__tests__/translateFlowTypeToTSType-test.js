/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @flow strict-local
 * @format
 */

import type {TypeAnnotationType} from 'flow-estree';

import * as parser from 'flow-parser';
import {parse as parseWithScope} from 'flow-transform';
import {translateFlowDefToTSDef, translateFlowTypeToTSType} from '../src';
import {flowDefToTSDef} from '../src/flowDefToTSDef';

function parseType(source: string): TypeAnnotationType {
  const ast = parser.parse(`type T = ${source};`);
  const declaration = ast.body[0];
  if (declaration.type !== 'TypeAlias') {
    throw new Error('Expected a type alias');
  }
  return declaration.right;
}

function serializeAST(node: TypeAnnotationType): string {
  return JSON.stringify(node, (key, value) =>
    key === 'parent' ? undefined : value,
  );
}

describe('translateFlowTypeToTSType', () => {
  // A type subtree uses the same recursive translation as a definition file.
  test.each([
    ['number', 'number'],
    ['string', 'string'],
    ['boolean', 'boolean'],
    ['any', 'any'],
    ['mixed', 'unknown'],
    ["'literal'", "'literal'"],
    ['?number', 'null | undefined | number'],
    ['string | number', 'string | number'],
    ['A & B', 'A & B'],
    ['Array<NS.Value<string>>', 'Array<NS.Value<string>>'],
    ['[string, number]', '[string, number]'],
    ['<T>(value: T) => T', '<T>(value: T) => T'],
    ['{foo: number}', '{foo: number}'],
    ['{foo?: string}', '{foo?: string | undefined}'],
    ['{nested: interface {foo: number}}', '{nested: {foo: number}}'],
    ['interface {}', '{}'],
    ['interface {foo: number}', '{foo: number}'],
    ['interface extends A {foo: number}', 'A & {foo: number}'],
    [
      'interface extends A<string>, NS.B<number> {}',
      'A<string> & NS.B<number> & {}',
    ],
    ['$ReadOnly<{foo: string}>', 'Readonly<{foo: string}>'],
    ['{[key: string]: number}', '{[key: string]: number}'],
  ])('translates %s', async (source, expected) => {
    const code = `type T = ${source};`;
    const node = parseType(source);
    const result = translateFlowTypeToTSType(node);
    const {ast, scopeManager} = parseWithScope(code);
    const [tsProgram] = flowDefToTSDef(code, ast, scopeManager, {
      recoverFromErrors: false,
    });
    const declaration = tsProgram.body[0];
    if (declaration.type !== 'TSTypeAliasDeclaration') {
      throw new Error('Expected a TypeScript type alias');
    }
    expect(result).toEqual(declaration.typeAnnotation); // OK
    await expect(
      translateFlowDefToTSDef(code, {
        singleQuote: true,
        bracketSpacing: false,
      }),
    ).resolves.toBe(`type T = ${expected};\n`); // OK
  });

  // Empty inline interfaces are type literals, not one-element intersections.
  test('returns the inline interface body directly', () => {
    expect(translateFlowTypeToTSType(parseType('interface {}'))).toMatchObject({
      type: 'TSTypeLiteral',
      members: [],
    }); // OK
  });

  // The translation itself is synchronous and does not parse or print.
  test('translates an already-parsed node without calling the parser', () => {
    const node = Object.freeze(parseType('interface {foo: number}'));
    const spy = jest.spyOn(parser, 'parse');
    try {
      const result = translateFlowTypeToTSType(node);
      expect(spy).not.toHaveBeenCalled(); // OK
      expect(result).toMatchObject({
        type: 'TSTypeLiteral',
        members: [
          {
            type: 'TSPropertySignature',
            key: {type: 'Identifier', name: 'foo'},
            typeAnnotation: {
              type: 'TSTypeAnnotation',
              typeAnnotation: {type: 'TSNumberKeyword'},
            },
          },
        ],
      }); // OK
    } finally {
      spy.mockRestore();
    }
  });

  // The same node can be translated repeatedly without changing its metadata.
  test('does not mutate the input subtree or its attached comments', () => {
    const code =
      'type T = interface {/** Property documentation */ foo: number};';
    const {ast} = parseWithScope(code);
    const declaration = ast.body[0];
    if (declaration.type !== 'TypeAlias') {
      throw new Error('Expected a type alias');
    }
    const node = declaration.right;
    const before = serializeAST(node);
    const first = translateFlowTypeToTSType(node);
    const second = translateFlowTypeToTSType(node);
    expect(serializeAST(node)).toBe(before); // OK
    expect(second).toEqual(first); // OK
    expect(JSON.stringify(first)).toContain('Property documentation'); // OK
  });

  // Detached generic references do not need a parent tree or scope analysis.
  test('preserves generic names without resolving their bindings', () => {
    const detached = JSON.parse(
      serializeAST(parseType('NS.Value<string>')),
    ) as TypeAnnotationType;
    expect(translateFlowTypeToTSType(detached)).toMatchObject({
      type: 'TSTypeReference',
      typeName: {
        type: 'TSQualifiedName',
        left: {type: 'Identifier', name: 'NS'},
        right: {type: 'Identifier', name: 'Value'},
      },
      typeArguments: {params: [{type: 'TSStringKeyword'}]},
    }); // OK
  });

  // Unsupported types fail instead of silently degrading to any.
  test.each([
    'empty',
    '$Call<F>',
    '{[[slot]]: number}',
    '{nested: $ObjMap<O, F>}',
  ])('rejects unsupported translation: %s', source => {
    expect(() => translateFlowTypeToTSType(parseType(source))).toThrow(
      'Unsupported feature:',
    ); // ERROR
  });

  // Generic indexer keys need mapped-type lowering; do not truncate their types.
  test.each(['{[key: NS.Keys]: number}', '{[key: $Call<F>]: number}'])(
    'rejects generic indexer keys: %s',
    source => {
      expect(() => translateFlowTypeToTSType(parseType(source))).toThrow(
        'generic indexer keys',
      ); // ERROR
    },
  );

  // Source text is optional and is used only for richer diagnostics.
  test('reports a useful error without source text', () => {
    expect(() => translateFlowTypeToTSType(parseType('$Call<F>'))).toThrow(
      'Unsupported feature: Translating "$Call" is currently not supported.',
    ); // ERROR
  });

  test('includes a source frame when sourceText is supplied', () => {
    const sourceText = 'type T = $Call<F>;';
    expect(() =>
      translateFlowTypeToTSType(parseType('$Call<F>'), {sourceText}),
    ).toThrow('type T = $Call<F>;'); // ERROR
  });

  // A standalone node cannot carry the file-level imports generated by React types.
  test.each([
    'React.Node',
    'React$Node',
    'React.MixedElement',
    'React$MixedElement',
    '{element: React.MixedElement}',
    'component() renders React.Node',
  ])('rejects translation requiring a React import: %s', source => {
    expect(() => translateFlowTypeToTSType(parseType(source))).toThrow(
      'requires adding a React import',
    ); // ERROR
  });

  // Translation state is local, including after an unsupported or React type.
  test('keeps independent calls and the string API isolated', async () => {
    expect(() => translateFlowTypeToTSType(parseType('React.Node'))).toThrow(); // ERROR
    expect(translateFlowTypeToTSType(parseType('number')).type).toBe(
      'TSNumberKeyword',
    ); // OK
    await expect(
      translateFlowDefToTSDef('type T = React.Node;', {singleQuote: true}),
    ).resolves.toContain("import * as React from 'react';"); // OK
    expect(() => translateFlowTypeToTSType(parseType('React.Node'))).toThrow(); // ERROR
    expect(translateFlowTypeToTSType(parseType('interface {}')).type).toBe(
      'TSTypeLiteral',
    ); // OK
  });
});
