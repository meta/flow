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

import {parse} from '../../__test_utils__/parse';
import {print} from 'flow-transform';
// $FlowExpectedError[cannot-resolve-module]
import {createContext, runInNewContext, SourceTextModule} from 'vm';
import {transformProgram as stripFlowTypes} from '../../oxidized-src/estree/StripFlowTypes';

describe('StripFlowTypes', () => {
  // Imports without bindings still load the module for its side effects.
  test.each(["import 'side-effect';", "import {} from 'side-effect';"])(
    'preserves side effects for %s',
    async code => {
      const stripped = stripFlowTypes(parse(code), {});
      expect(stripped.body).toHaveLength(1); // OK
      expect(stripped.body[0]).toMatchObject({
        type: 'ImportDeclaration',
        importKind: 'value',
        specifiers: [],
        source: {value: 'side-effect'},
      }); // OK

      const output = await print(stripped, code);
      const context = createContext({calls: 0});
      const module = new SourceTextModule(output, {context});
      await module.link(() => new SourceTextModule('calls++;', {context}));
      await module.evaluate();

      expect(context.calls).toBe(1); // OK
    },
  );

  // Erasing all type bindings must not turn a type-only import into a bare import.
  test.each([
    "import type T from 'types';",
    "import typeof T from 'types';",
    "import type {T} from 'types';",
    "import typeof {T} from 'types';",
    "import {type T} from 'types';",
    "import {typeof T} from 'types';",
    "import {type T, typeof U} from 'types';",
  ])('still removes type-only imports in %s', code => {
    const stripped = stripFlowTypes(parse(code), {});

    expect(stripped.body).toEqual([]); // OK
  });

  // Mixed imports retain their value bindings while removing type/typeof bindings.
  test('retains value specifiers in mixed imports', async () => {
    const code = "import value, {type T, typeof U, other} from 'mixed';";
    const stripped = stripFlowTypes(parse(code), {});

    expect((await print(stripped, code)).trim()).toBe(
      'import value, { other } from "mixed";',
    ); // OK
  });

  // Defaults survive for every function form and for destructured bindings.
  test.each([
    'function f(x: number = 42): number { return x; }',
    'const f = function(x: number = 42): number { return x; };',
    'const f = (x: number = 42): number => x;',
    'function f(x? = 42): number { return x; }',
    'function f(x?: number = 42): number { return x; }',
    'function f({x}: {x: number} = {x: 42}): number { return x; }',
    'function f([x]: Array<number> = [42]): number { return x; }',
    'function f(this: mixed, x: number = 42): number { return x; }',
  ])('preserves parameter defaults in %s', async code => {
    const stripped = stripFlowTypes(parse(code), {});
    const output = await print(stripped, code);

    expect(runInNewContext(`${output}\nf()`)).toBe(42); // OK
    expect(runInNewContext(`${output}\nf(undefined)`)).toBe(42); // OK
    expect(runInNewContext(`${output}\nf.length`)).toBe(0); // OK
  });

  test('preserves default side effects and strips types in initializers', async () => {
    // Initializers run only for missing/undefined arguments, not explicit values.
    const code = `
      let calls = 0;
      function f(x: number = (++calls: number)): number { return x; }
    `;
    const stripped = stripFlowTypes(parse(code), {});
    const output = await print(stripped, code);

    expect(
      runInNewContext(`${output}\n[f(7), calls, f(), f(undefined), calls]`),
    ).toEqual([7, 0, 1, 2, 2]); // OK
  });

  test('still strips this and optional parameters', async () => {
    // Type-only parameters and optional markers do not survive erasure.
    const code = 'function f(this: mixed, x?: number): void {}';
    const stripped = stripFlowTypes(parse(code), {});

    expect((await print(stripped, code)).trim()).toBe('function f(x) {}'); // OK
  });

  test('strips non-null expressions', () => {
    // Removing the assertion preserves the expression and its original location.
    const ast = parse('value!;');
    const stripped = stripFlowTypes(ast, {});

    expect(stripped.body[0]).toMatchObject({
      type: 'ExpressionStatement',
      expression: {type: 'Identifier', name: 'value', range: [0, 5]},
    }); // OK
  });

  test.each([
    ['foo?.bar!;', 'foo?.bar;'],
    ['foo?.bar!!;', 'foo?.bar;'],
    ['value!!;', 'value;'],
    ['foo?.bar!();', 'foo?.bar();'],
    ['foo?.bar!.baz;', 'foo?.bar.baz;'],
    ['foo?.bar![key];', 'foo?.bar[key];'],
    ['foo?.bar!.baz.qux;', 'foo?.bar.baz.qux;'],
    ['foo?.bar!.baz();', 'foo?.bar.baz();'],
    ['foo?.bar!.baz!();', 'foo?.bar.baz();'],
    ['foo?.bar!!();', 'foo?.bar();'],
    ['foo?.bar!!.baz;', 'foo?.bar.baz;'],
    ['foo?.bar!?.baz;', 'foo?.bar?.baz;'],
    ['(foo?.bar)!();', '(foo?.bar)();'],
    ['(foo?.bar)!.baz;', '(foo?.bar).baz;'],
    ['(foo?.bar!)!();', '(foo?.bar)();'],
    ['(foo?.bar!)?.baz;', 'foo?.bar?.baz;'],
    ['(foo?.bar)!?.baz;', 'foo?.bar?.baz;'],
    ['foo?.[key!]!(value!);', 'foo?.[key](value);'],
    ['(foo?.bar!.baz).qux;', '(foo?.bar.baz).qux;'],
  ])(
    'preserves optional-chain boundaries when stripping %s',
    async (code, result) => {
      // Assertions do not end a chain; parentheses still do.
      const stripped = stripFlowTypes(parse(code), {});

      expect((await print(stripped, code)).trim()).toBe(result); // OK
    },
  );

  // Nested chains survive erasure, even if the printer can omit redundant parentheses.
  test.each(['(foo?.bar!)?.baz;', '(foo?.bar)!?.baz;'])(
    'retains the parenthesized receiver AST for %s',
    code => {
      const stripped = stripFlowTypes(parse(code), {});
      expect(stripped.body[0]).toMatchObject({
        type: 'ExpressionStatement',
        expression: {
          type: 'ChainExpression',
          expression: {
            type: 'MemberExpression',
            object: {type: 'ChainExpression'},
          },
        },
      }); // OK
    },
  );

  test('strips tagged-template type arguments', () => {
    const ast = parse('tag<T>`value`;');
    const stripped = stripFlowTypes(ast, {});
    const statement = stripped.body[0];
    if (
      statement.type !== 'ExpressionStatement' ||
      statement.expression.type !== 'TaggedTemplateExpression'
    ) {
      throw new Error('expected a tagged template expression');
    }

    expect(statement.expression.typeArguments).toBeNull();
  });
});
