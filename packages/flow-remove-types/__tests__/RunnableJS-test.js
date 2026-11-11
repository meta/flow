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

import {spawnSync} from 'child_process';
import path from 'path';
import vm from 'vm';
import {SourceMapConsumer} from 'source-map';
import {flowToRunnableJSWithSourceMap} from '../runnable-wasm';

test('Wasm emitter lowers runtime Flow constructs and erases annotations', () => {
  const source = `enum E {A, B}
record R {value: number}
component Foo(value: number) { return R {value}; }
const result: R = Foo({value: match (E.A) { E.A => 7, _ => 0 }});
module.exports = result.value;`;
  const emitted = flowToRunnableJSWithSourceMap(source, {
    sourceFilename: 'runtime.js',
  });
  const module = {exports: null};
  vm.runInNewContext(emitted.code, {module, require});
  expect(module.exports).toBe(7);
  expect(emitted.map).toMatchObject({
    version: 3,
    file: 'runtime.js',
    sources: ['runtime.js'],
    sourcesContent: [source],
  });
});

test('Wasm emitter loads a shared enum runtime through ESM', () => {
  const emitted = flowToRunnableJSWithSourceMap(
    `enum E {A}
enum F {B}
const $$gen$enum_runtime0 = 42;
console.log(E.A, F.B, $$gen$enum_runtime0);`,
    {sourceFilename: 'runtime.mjs', sourceType: 'module'},
  );
  const result = spawnSync(
    process.execPath,
    ['--input-type=module', '-e', emitted.code],
    {cwd: path.resolve(__dirname, '..'), encoding: 'utf8'},
  );
  expect(result.stderr).toBe('');
  expect(result.status).toBe(0);
  expect(result.stdout).toBe('A B 42\n');
});

function thrownPosition(
  code: string,
  filename: string,
): {line: number, column: number} {
  try {
    vm.runInNewContext(code, {}, {filename});
  } catch (error) {
    const match = String(error.stack).match(
      new RegExp(`${filename}:(\\d+):(\\d+)`),
    );
    if (match == null) {
      throw error;
    }
    return {line: Number(match[1]), column: Number(match[2]) - 1};
  }
  throw new Error('Expected the program to throw');
}

test.each([
  '// header\ntype N = number;\nfunction fail(): void {\n const emoji = "😀"; throw new Error("boom");\n}\nfail();',
  '// header\r\ntype N = number;\r\nfunction fail(): void {\r\n const text = `😀\u2028last\u2029line`; throw new Error("boom");\r\n}\r\nfail();',
])(
  'source maps resolve real thrown frames to the original locations',
  source => {
    const emitted = flowToRunnableJSWithSourceMap(source, {
      sourceFilename: 'original.js',
    });
    const generated = thrownPosition(emitted.code, 'generated');
    // The types precede the throwing line, so removing them keeps its position.
    const expected = thrownPosition(
      source.replace('type N = number;', '').replace('(): void', '()'),
      'reference',
    );
    const consumer = new SourceMapConsumer(emitted.map);
    expect(consumer.originalPositionFor(generated)).toMatchObject({
      source: 'original.js',
      ...expected,
    });
  },
);

test('Wasm diagnostics use UTF-16 columns and remain usable after errors', () => {
  try {
    flowToRunnableJSWithSourceMap("const text = '😀'; const x = ;", {
      sourceFilename: 'bad.js',
    });
    throw new Error('Expected a syntax error');
  } catch (error) {
    expect(error).toBeInstanceOf(SyntaxError);
    expect(error.loc).toEqual({line: 1, column: 29});
  }
  const emitted = flowToRunnableJSWithSourceMap("module.exports = 'a\\0😀';", {
    sourceFilename: 'after.js',
  });
  const module = {exports: null};
  vm.runInNewContext(emitted.code, {module});
  expect(module.exports).toBe('a\0😀');
});

test('Wasm emitter preserves shebangs', () => {
  const source = '#!/usr/bin/env node\nconst value: number = 42;';
  const emitted = flowToRunnableJSWithSourceMap(source, {
    sourceFilename: 'script.js',
  });
  expect(emitted.code.startsWith('#!/usr/bin/env node\n')).toBe(true);
  expect(emitted.map.sourcesContent).toEqual([source]);
});

test('Wasm emission supports module await and dependency runtime specifiers', () => {
  const emitted = flowToRunnableJSWithSourceMap(
    'enum E {A}\nconst value: string = await Promise.resolve(E.A);\nconsole.log(value);',
    {
      sourceFilename: 'await.mjs',
      sourceType: 'module',
      enumRuntimeModule: require('url').pathToFileURL(
        require.resolve('flow-enums-runtime'),
      ).href,
    },
  );
  const result = spawnSync(
    process.execPath,
    ['--input-type=module', '-e', emitted.code],
    {encoding: 'utf8'},
  );
  expect(result.status).toBe(0);
  expect(result.stderr).toBe('');
  expect(result.stdout).toBe('A\n');
});

function executeScript(source: string): mixed {
  const module = {exports: null};
  vm.runInNewContext(source, {module});
  return JSON.parse(JSON.stringify(module.exports));
}

test.each([
  `const x = null;
module.exports = [x?.a.b(), x?.a().b, x?.["a"]?.().b];`,
  `const object = {value: 7, method() { return this.value; }};
module.exports = [object.method(), (object.method)(), object.method?.()];`,
  `let local = 7;
module.exports = [eval("local"), (0, eval)("typeof local")];`,
  `let x = 1;
const before = (x // keep suffix outside comment
)++;
module.exports = [before, x, x + +2, x - -2, (-2) ** 3, (2 ** 3) ** 2];`,
  `const value = [1, 2, 3];
const [,a,,...rest] = value;
const object = {a: 1, b: 2, c: 3};
const {a: first = 7, ...others} = object;
let target;
({b: target} = object);
module.exports = [a, rest, first, others, target, [,].length, [,,].length, [1,,].length];`,
  `const proto = {inherited: 7};
const object = {__proto__: proto, __proto__() { return 3; }, get x() { return this.value; }, set x(v) { this.value = v; }};
object.x = 4;
module.exports = [object.inherited, object.x, object.__proto__(), Object.hasOwnProperty.call(object, "__proto__")];`,
  `class Base { constructor(value) { this.value = value; } method() { return this.value; } }
class Child extends Base {
  #secret = 5;
  static total;
  static { this.total = 7; }
  constructor(value) { super(value); }
  method() { return super.method() + this.#secret; }
  secret() { return this.#secret; }
}
const value = new Child(3);
module.exports = [value.method(), Child.total, value.secret()];`,
  `function *values() { yield (// first
1); yield* [2, 3]; }
module.exports = [...values()];`,
  `let total = 0;
outer: for (let i = 0; i < 5; i++) { if (i === 1) continue outer; if (i === 4) break outer; total += i; }
do total++; while (total < 6);
try { throw {message: "failure"}; } catch ({message}) { total += message.length; } finally { total++; }
switch (total) { case 14: total += 10; break; default: total = -1; }
module.exports = total;`,
  `function value() { return (// before
7 // after
); }
function fail() { throw (// before
new Error("bad") // after
); }
let message; try { fail(); } catch (error) { message = error.message; }
module.exports = [value(), message];`,
  `function f(value) { 'use\\x20strict'; with ({x: value}) { return x; } }
module.exports = [f(7), "\\0😀\\u2028\\u2029", String.raw\`first
last\`];`,
  `const cases = [null, {a: {b() { return 7; }}}];
module.exports = cases.map(object => {
  let boundary; try { (object?.a).b(); boundary = "ok"; } catch (error) { boundary = error.name; }
  return [object?.a.b(), boundary];
});`,
])('direct AST emission preserves JavaScript execution: %s', source => {
  const emitted = flowToRunnableJSWithSourceMap(source, {
    sourceFilename: 'reference.js',
  });
  expect(executeScript(emitted.code)).toEqual(executeScript(source));
});

test('type erasure does not create a new strict directive', () => {
  const emitted = flowToRunnableJSWithSourceMap(
    `function f() { type T = number; "use strict"; with ({value: 7}) { return value; } }
module.exports = f();`,
    {sourceFilename: 'directives.js'},
  );
  expect(executeScript(emitted.code)).toBe(7);
});

test('type erasure removes definite assignment assertions', () => {
  const emitted = flowToRunnableJSWithSourceMap(
    `let count!: number;
class Box {
  value!: number;
  #secret!: string;
  secret() { return this.#secret === undefined; }
}
count = 1;
const box = new Box();
module.exports = [count, Object.keys(box), box.secret()];`,
    {sourceFilename: 'definite.js'},
  );
  expect(emitted.code).not.toContain('!');
  expect(executeScript(emitted.code)).toEqual([1, ['value'], true]);
});

test('direct string emission retains isolated UTF-16 surrogates', () => {
  const source = String.raw`module.exports = ["\ud800", "\udfff", "\ud800\udfff", "\u{d800}", "\141"];`;
  const emitted = flowToRunnableJSWithSourceMap(source, {
    sourceFilename: 'surrogates.js',
  });
  expect(executeScript(emitted.code)).toEqual(executeScript(source));
});
