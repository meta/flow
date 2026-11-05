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

import {parse, FlowVisitorKeys, SimpleTraverser} from 'flow-parser';
import * as RawFlowParser from '../oxidized-src/FlowParser';
import {expectBabelAlignment} from '../__test_utils__/alignment-utils';

const TYPE_USES = new Set([
  'GenericTypeAnnotation',
  'InterfaceExtends',
  'ClassImplements',
]);

function collectTypeUses(root: $FlowFixMe): Array<$FlowFixMe> {
  const result = [];
  function visit(value: $FlowFixMe): void {
    if (value == null || typeof value !== 'object') {
      return;
    }
    if (Array.isArray(value)) {
      value.forEach(visit);
      return;
    }
    if (TYPE_USES.has(value.type)) {
      result.push(value);
    }
    for (const key of Object.keys(value)) {
      if (key !== 'parent') {
        visit(value[key]);
      }
    }
  }
  visit(root);
  return result;
}

const cases = [
  'type Alias<T> = NS.Box<T, Pair<string, number>>; type Bare = Plain;',
  'type Alias<T> = import("module").Box<T>;',
  'interface I<T> extends NS.Base<T, Pair<number>>, Plain {}',
  'class C<T> implements NS.I<T, Pair<number>>, Plain {}',
  'declare interface I<T> extends NS.Base<T>, Plain {}',
  'declare class C<T> extends NS.Base<T> implements I<T> {}',
  'declare class C<T> mixins NS.M<T>, Plain {}',
  'declare class C<T> extends M<T>(NS.Base<T>) {}',
];

describe('Type argument field names', () => {
  test.each(cases)('ESTree: %s', source => {
    const ast = parse(source, {flow: 'all'});
    const nodes = collectTypeUses(ast);
    expect(nodes.length).toBeGreaterThan(0);
    for (const node of nodes) {
      expect(Object.hasOwn(node, 'typeArguments')).toBe(true);
      expect(Object.hasOwn(node, 'typeParameters')).toBe(false);
      if (node.typeArguments != null) {
        expect(node.typeArguments.type).toBe('TypeParameterInstantiation');
        expect(node.typeArguments.params.length).toBeGreaterThan(0);
      }
    }
    expect(ast.body[0]).toMatchObject({
      typeParameters: {
        type: 'TypeParameterDeclaration',
        params: [{name: 'T'}],
      },
    });
  });

  test.each(cases)('raw WASM boundary: %s', source => {
    const nodes = collectTypeUses(
      RawFlowParser.parse(source, {enableTypes: true}),
    );
    expect(nodes.length).toBeGreaterThan(0);
    for (const node of nodes) {
      expect(Object.hasOwn(node, 'typeArguments')).toBe(true);
      expect(Object.hasOwn(node, 'typeParameters')).toBe(false);
    }
  });

  test('absent arguments are explicit nulls', () => {
    const ast = parse(
      'type A = Plain; interface I extends Plain {} class C implements Plain {}',
      {flow: 'all'},
    );
    const nodes = collectTypeUses(ast);
    expect(nodes.map(node => node.type)).toEqual([
      'GenericTypeAnnotation',
      'InterfaceExtends',
      'ClassImplements',
    ]);
    for (const node of nodes) {
      expect(node.typeArguments).toBeNull();
      expect(Object.hasOwn(node, 'typeArguments')).toBe(true);
      expect(Object.hasOwn(node, 'typeParameters')).toBe(false);
    }
  });

  test('implicit call type arguments use the same generic-node contract', () => {
    const nodes = collectTypeUses(
      parse('const value = f<_>();', {flow: 'all'}),
    );
    expect(nodes).toHaveLength(1);
    expect(nodes[0].id.name).toBe('_');
    expect(nodes[0].typeArguments).toBeNull();
    expect(Object.hasOwn(nodes[0], 'typeArguments')).toBe(true);
    expect(Object.hasOwn(nodes[0], 'typeParameters')).toBe(false);
  });

  test.each(cases)('Babel keeps its existing schema: %s', source => {
    const nodes = collectTypeUses(parse(source, {flow: 'all', babel: true}));
    expect(nodes.length).toBeGreaterThan(0);
    for (const node of nodes) {
      expect(Object.hasOwn(node, 'typeParameters')).toBe(true);
      expect(Object.hasOwn(node, 'typeArguments')).toBe(false);
    }
  });

  test('Babel generic keyword and typeof synthesis keeps its schema', () => {
    const ast = parse(
      'type A = unknown; type B = never; type C = undefined; type D = typeof value;',
      {flow: 'all', babel: true},
    );
    const nodes = collectTypeUses(ast);
    expect(nodes.length).toBe(4);
    for (const node of nodes) {
      expect(node.typeParameters).toBeNull();
      expect(Object.hasOwn(node, 'typeParameters')).toBe(true);
      expect(Object.hasOwn(node, 'typeArguments')).toBe(false);
    }
  });

  test('Babel alignment for generic references and implementations', () => {
    expectBabelAlignment({
      code:
        'type A = NS.Box<string, Pair<string, number>>; ' +
        'class C implements I<string> {}',
      babel: {expectToFail: false},
      espree: {
        expectToFail: 'espree-exception',
        expectedExceptionMessage: 'Unexpected token A',
      },
    });
  });

  test('visitor keys traverse every argument once', () => {
    const ast = parse(
      'type A<T> = Box<T, Pair<string>>; interface I extends Base<number> {} ' +
        'class C implements I<boolean> {}',
      {flow: 'all'},
    );
    const visited = new Set();
    let argumentLists = 0;
    SimpleTraverser.traverse(ast, {
      enter(node, parent) {
        expect(visited.has(node)).toBe(false);
        visited.add(node);
        if (node.type === 'TypeParameterInstantiation') {
          argumentLists++;
          expect(parent).toHaveProperty('typeArguments', node);
        }
      },
      leave() {},
    });
    expect(argumentLists).toBe(4);
    for (const type of TYPE_USES) {
      expect(FlowVisitorKeys[type]).toEqual(['id', 'typeArguments']);
    }
  });

  test('argument locations, comments, and tokens survive Unicode input', () => {
    const source = 'type Å<T> = NS.Box</*first*/ T, /*second*/ Foo<string>>;';
    const ast = parse(source, {
      flow: 'all',
      tokens: true,
      sourceFilename: 'unicode.js',
    });
    const generic = collectTypeUses(ast)[0];
    const args = generic.typeArguments;
    expect(args.range).toEqual([
      source.indexOf('</*first*/'),
      source.lastIndexOf('>') + 1,
    ]);
    expect(args.loc.source).toBe('unicode.js');
    expect(args.params[0].range).toEqual([
      source.indexOf('T,'),
      source.indexOf('T,') + 1,
    ]);
    expect(args.params[1].typeArguments.params[0].type).toBe(
      'StringTypeAnnotation',
    );
    expect(ast.comments.map(comment => comment.value)).toEqual([
      'first',
      'second',
    ]);
    expect(ast.tokens?.length).toBeGreaterThan(0);
  });
});
