/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @noflow
 * @format
 */

const assert = require('node:assert/strict');
const fs = require('node:fs');
const Module = require('node:module');
const path = require('node:path');
const {test} = require('node:test');
const {transformFileSync} = require('@babel/core');
const flowParser = require('flow-parser');

function loadModule(name, overrides = {}) {
  const filename = path.resolve(__dirname, '../src/try-flow', name);
  const compiled = new Module(filename, module);
  compiled.paths = Module._nodeModulePaths(path.dirname(filename));
  const originalRequire = compiled.require.bind(compiled);
  compiled.require = name => overrides[name] ?? originalRequire(name);
  compiled._compile(
    transformFileSync(filename, {
      configFile: false,
      babelrc: false,
      presets: ['@babel/preset-flow', '@babel/preset-react'],
      plugins: [
        'babel-plugin-syntax-hermes-parser',
        '@babel/plugin-transform-modules-commonjs',
      ],
      parserOpts: {flow: 'all'},
    }).code,
    filename,
  );
  return compiled.exports;
}

const {formatCode} = loadModule('formatCode.js');
const {createFormatterLoader, preloadFormatter, formatEditor} =
  loadModule('formatEditor.js');

function semanticAST(value) {
  if (Array.isArray(value)) return value.map(semanticAST);
  if (value == null || typeof value !== 'object') return value;
  return Object.fromEntries(
    Object.entries(value)
      .filter(
        ([key]) =>
          ![
            'loc',
            'range',
            'raw',
            'comments',
            'docblock',
            'tokens',
            'parent',
            'errors',
            'trailingComma',
          ].includes(key),
      )
      .map(([key, child]) => [key, semanticAST(child)]),
  );
}

// Formatting must preserve the AST, including modern Flow nodes and colon bounds.
const fixtures = {
  'Flow without a pragma': 'foo<string>("x"); const x:number=1;',
  'readonly and extends bounds':
    'type User={readonly name:string,readonly age:number}; function get<K extends keyof User>(user:User,key:K):User[K]{return user[key]}',
  'colon bounds': 'function f<T: number>(x:T):T { return x; }',
  'nested type arguments': 'type Alias<T> = NS.Box<Map<string, Array<T>>>;',
  'heritage type arguments':
    'interface I<T> extends NS.Base<Array<T>> {} class C<T> implements I<T> {}',
  'optional chaining and casts':
    'const x = foo?.bar?.(); const y = (x: ?number);',
  'comments and JSX':
    '/* leading */\nconst el: React.Node=<div title="hi">{name}</div>; // trailing\n',
  enums: 'enum Status { Active, Inactive }',
  components:
    'component Foo(name: string) renders React.Node { return <div>{name}</div>; }',
  'hooks and type guards':
    'hook useFoo():number {return 1;} function isNumber(x:mixed):x is number{return typeof x === "number";}',
  'mapped and conditional types':
    'type M<T> = {[K in keyof T]: T[K]}; type C<T> = T extends number ? string : boolean;',
  'pattern matching': 'const y = match (x) { 1 => 2, _ => 3 };',
  records: 'record Point {x: number, y: number}',
  'object methods and renamed imports':
    'import {x as x} from "x"; type T={f():number, g:()=>number};',
};
for (const [name, source] of Object.entries(fixtures)) {
  test(`formats ${name} without semantic changes`, async () => {
    const original = flowParser.parse(source, {flow: 'all'});
    const {formatted} = await formatCode(source, 0);
    const result = flowParser.parse(formatted, {flow: 'all'});
    assert.deepEqual(semanticAST(result), semanticAST(original)); // OK
    assert.equal((await formatCode(formatted, 0)).formatted, formatted); // OK
  });
}

// Executable scripts must keep their leading interpreter directive.
test('preserves hashbangs, including comment-only scripts', async () => {
  for (const source of [
    '#!/usr/bin/env node\nconst x:number=1;',
    '#!/usr/bin/env node\n',
  ]) {
    const {formatted} = await formatCode(source, 0);
    assert.match(formatted, /^#!\/usr\/bin\/env node\n/); // OK
    assert.equal((await formatCode(formatted, 0)).formatted, formatted); // OK
    assert.deepEqual(
      semanticAST(flowParser.parse(formatted, {flow: 'all'})),
      semanticAST(flowParser.parse(source, {flow: 'all'})),
    ); // OK
  }
});

// The adapter invokes the selected package with Flow enabled even without @flow.
test('uses flow-parser explicitly and rejects invalid programs', async () => {
  let options;
  let mutations = 0;
  const {formatCode} = loadModule('formatCode.js', {
    'flow-parser': {
      ...flowParser,
      parse(code, opts) {
        options = opts;
        return flowParser.parse(code, opts);
      },
      mutateESTreeASTForPrettier(ast) {
        mutations++;
        flowParser.mutateESTreeASTForPrettier(ast);
      },
    },
  });
  assert.equal(
    (await formatCode('const x:number=1;', 0)).formatted,
    'const x: number = 1;\n',
  ); // OK
  assert.deepEqual(options, {
    flow: 'all',
    babel: false,
    throwOnParseErrors: true,
  }); // OK
  assert.equal(mutations, 1); // OK
  await assert.rejects(formatCode('const x = ;', 0), SyntaxError); // OK
});

test('preserves type arguments and the cursor inside nested references', async () => {
  const source =
    'type Alias<T>=NS.Box<Map<string,Array<T>>>; interface I<T>extends Base<T>{} class C<T>implements I<T>{}';
  const result = await formatCode(source, source.indexOf('string'));
  assert.deepEqual(
    semanticAST(flowParser.parse(result.formatted, {flow: 'all'})),
    semanticAST(flowParser.parse(source, {flow: 'all'})),
  ); // OK
  assert.match(result.formatted, /NS\.Box<Map<string,\s*Array<T>>>/); // OK
  assert.equal(
    result.formatted.slice(result.cursorOffset, result.cursorOffset + 6),
    'string',
  ); // OK
});

// Cursor offsets use UTF-16 and preserve the editor's CRLF line endings.
test('preserves comments, colon bounds, and the primary cursor', async () => {
  const {formatted} = await formatCode(fixtures['comments and JSX'], 0);
  assert.match(formatted, /\/\* leading \*\//); // OK
  assert.match(formatted, /\/\/ trailing/); // OK
  assert.match(
    (await formatCode(fixtures['colon bounds'], 0)).formatted,
    /f<T: number>/,
  ); // OK
  const source = 'const emoji="😀";\r\nconst longName:number=1;';
  const result = await formatCode(source, source.indexOf('longName'));
  assert.match(result.formatted, /\r\n/); // OK
  assert.equal(
    result.formatted.slice(result.cursorOffset, result.cursorOffset + 8),
    'longName',
  ); // OK
  assert.equal((await formatCode('   \n', 0)).formatted, ''); // OK
});

function deferred() {
  let resolve;
  let reject;
  const promise = new Promise((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return {promise, resolve, reject};
}

// Only one import is needed for concurrent and subsequent format requests.
test('caches the formatter import and permits retry after failure', async () => {
  const format = async code => ({formatted: code, cursorOffset: 0});
  let calls = 0;
  const loading = deferred();
  const load = createFormatterLoader(() => {
    calls++;
    return loading.promise;
  });
  const first = load();
  assert.equal(load(), first); // OK
  loading.resolve({formatCode: format});
  assert.equal(await first, format); // OK
  assert.equal(await load(), format); // OK
  assert.equal(calls, 1); // OK

  const retry = createFormatterLoader(async () => {
    if (calls++ === 1) throw new Error('offline');
    return {formatCode: format};
  });
  await assert.rejects(retry(), /offline/); // OK
  assert.equal(await retry(), format); // OK
  assert.equal(calls, 3); // OK
});

// Hover/focus starts only the import, shares it with clicks, and silently permits retries.
test('preloads without formatting and swallows speculative loading failures', async () => {
  let imports = 0;
  let formats = 0;
  const loading = deferred();
  const format = async code => {
    formats++;
    return {formatted: code, cursorOffset: 0};
  };
  const load = createFormatterLoader(() => {
    imports++;
    return loading.promise;
  });
  const preload = preloadFormatter(load);
  const click = load();
  loading.resolve({formatCode: format});
  await preload;
  assert.equal(await click, format); // OK
  await preloadFormatter(load);
  assert.equal(imports, 1); // OK
  assert.equal(formats, 0); // OK

  let attempts = 0;
  const retry = createFormatterLoader(async () => {
    if (attempts++ === 0) throw new Error('offline');
    return {formatCode: format};
  });
  await preloadFormatter(retry);
  assert.equal(await retry(), format); // OK
  assert.equal(attempts, 2); // OK
});

function mockEditor(initial = 'const x:number=1;') {
  let value = initial;
  let version = 1;
  let disposed = false;
  const edits = [];
  const events = [];
  const model = {
    getValue: () => value,
    getVersionId: () => version,
    getOffsetAt: () => 6,
    getPositionAt: offset => ({lineNumber: 1, column: offset + 1}),
    getFullModelRange: () => ({
      startLineNumber: 1,
      startColumn: 1,
      endLineNumber: 1,
      endColumn: value.length + 1,
    }),
    isDisposed: () => disposed,
  };
  const editor = {
    getModel: () => model,
    getPosition: () => ({lineNumber: 1, column: 7}),
    getScrollTop: () => 12,
    getScrollLeft: () => 0,
    pushUndoStop() {
      events.push('undo-stop');
    },
    executeEdits(source, changes) {
      edits.push({source, changes});
      value = changes[0].text;
      version++;
      events.push('edit');
      return true;
    },
    setPosition(position) {
      events.push(position);
    },
    setScrollPosition(scroll) {
      events.push(scroll);
    },
    focus() {
      events.push('focus');
    },
  };
  return {
    editor,
    model,
    edits,
    events,
    change(code) {
      value = code;
      version++;
    },
    dispose() {
      disposed = true;
    },
  };
}

// Loading finishes before the button switches to the formatting phase.
test('notifies the button after loading and before formatting', async () => {
  const mock = mockEditor();
  const loading = deferred();
  const phases = [];
  const pending = formatEditor(
    mock.editor,
    () => true,
    () => loading.promise,
    () => phases.push('ready'),
  );
  assert.deepEqual(phases, []); // OK
  loading.resolve(async code => {
    phases.push('format');
    return {formatted: code, cursorOffset: 0};
  });
  assert.equal(await pending, 'unchanged'); // OK
  assert.deepEqual(phases, ['ready', 'format']); // OK
});

// A single Monaco edit, bracketed by undo stops, leaves earlier undo history intact.
test('applies one edit and restores cursor, scroll, and focus', async () => {
  const {editor, edits, events, model} = mockEditor();
  const format = async (code, offset) => {
    assert.equal(code, 'const x:number=1;'); // OK
    assert.equal(offset, 6); // OK
    return {formatted: 'const x: number = 1;\n', cursorOffset: 6};
  };
  assert.equal(
    await formatEditor(
      editor,
      () => true,
      async () => format,
    ),
    'formatted',
  ); // OK
  assert.equal(model.getValue(), 'const x: number = 1;\n'); // OK
  assert.equal(edits.length, 1); // OK
  assert.equal(edits[0].source, 'prettier'); // OK
  assert.deepEqual(events, [
    'undo-stop',
    'edit',
    {lineNumber: 1, column: 7},
    'undo-stop',
    {scrollTop: 12, scrollLeft: 0},
    'focus',
  ]); // OK
});

// In-flight results must not overwrite typing, Clear/Recover, replacement, or unmount.
for (const kind of ['changed', 'replaced', 'disposed', 'unmounted']) {
  test(`discards a ${kind} editor while loading`, async () => {
    const mock = mockEditor();
    const loading = deferred();
    let mounted = true;
    const pending = formatEditor(
      mock.editor,
      () => mounted,
      () => loading.promise,
    );
    if (kind === 'changed') mock.change('new code');
    if (kind === 'replaced') mock.editor.getModel = () => ({});
    if (kind === 'disposed') mock.dispose();
    if (kind === 'unmounted') mounted = false;
    loading.resolve(async () => ({
      formatted: 'formatted old code',
      cursorOffset: 0,
    }));
    assert.equal(await pending, 'stale'); // OK
    assert.deepEqual(mock.edits, []); // OK
    assert.deepEqual(mock.events, []); // OK
  });
}

// Changes during asynchronous formatting, including edit-then-undo, invalidate the result.
test('discards edits made while the formatter is running', async () => {
  const mock = mockEditor();
  const formatted = deferred();
  const started = deferred();
  const pending = formatEditor(
    mock.editor,
    () => true,
    async () => () => {
      started.resolve();
      return formatted.promise;
    },
  );
  await started.promise;
  mock.change('temporary edit');
  mock.change('const x:number=1;');
  formatted.resolve({formatted: 'const x: number = 1;\n', cursorOffset: 6});
  assert.equal(await pending, 'stale'); // OK
  assert.deepEqual(mock.edits, []); // OK
});

// Failures and no-op formatting must leave the text and undo stack alone.
test('does not edit on failures, empty input, or unchanged output', async () => {
  const mock = mockEditor();
  await assert.rejects(
    formatEditor(
      mock.editor,
      () => true,
      async () => {
        throw new Error('offline');
      },
    ),
    /offline/,
  ); // OK
  await assert.rejects(
    formatEditor(
      mock.editor,
      () => true,
      async () => async () => {
        throw new SyntaxError('invalid');
      },
    ),
    /invalid/,
  ); // OK
  assert.equal(
    await formatEditor(
      mock.editor,
      () => true,
      async () => async code => ({formatted: code, cursorOffset: 0}),
    ),
    'unchanged',
  ); // OK
  assert.deepEqual(mock.edits, []); // OK
  assert.deepEqual(mock.events, []); // OK
  const empty = mockEditor(' \n');
  assert.equal(
    await formatEditor(
      empty.editor,
      () => true,
      () => {
        throw new Error('must not load');
      },
    ),
    'unchanged',
  ); // OK
  assert.deepEqual(empty.events, []); // OK
});

// The heavyweight imports live only beyond the dynamic import boundary.
test('keeps the parser and printer out of the editor module', () => {
  const source = fs.readFileSync(
    path.resolve(__dirname, '../src/try-flow/formatEditor.js'),
    'utf8',
  );
  assert.match(source, /import\('\.\/formatCode'\)/); // OK
  assert.doesNotMatch(
    source,
    /^import (?!type).*from ['"](?:prettier|flow-parser|\.\/formatCode)/m,
  ); // OK
});
