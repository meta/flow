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
const {test} = require('node:test');
const path = require('node:path');
const vm = require('node:vm');
const {transformFileSync, transformFromAstSync} = require('@babel/core');
const traverse = require('@babel/traverse').default;
const LZString = require('lz-string');

const {ast} = transformFileSync(
  path.resolve(__dirname, '../src/try-flow/TryFlow.js'),
  {
    configFile: false,
    babelrc: false,
    presets: ['@babel/preset-flow'],
    plugins: ['babel-plugin-syntax-hermes-parser'],
    parserOpts: {flow: 'all'},
    ast: true,
    code: false,
  },
);
const names = [
  'getHashedValue',
  'takeRecoveryAfterReload',
  'setHashedValue',
  'changeFlowVersion',
  'resetFromStorage',
];
const functions = [];
traverse(ast, {
  FunctionDeclaration({node}) {
    if (names.includes(node.id?.name)) functions.push(node);
  },
});
assert.equal(functions.length, names.length);
const {code} = transformFromAstSync(
  {type: 'File', program: {type: 'Program', body: functions}},
  null,
  {configFile: false, babelrc: false},
);

function handlers(overrides = {}) {
  const events = [];
  const location = {
    hash: '',
    reload() {
      events.push(['reload', this.hash]);
    },
  };
  const context = {
    LZString,
    location,
    window: {location},
    localStorage: {
      setItem(key, value) {
        events.push([key, value]);
      },
    },
    TRY_FLOW_LAST_CONTENT_STORAGE_KEY: 'tryFlowLastContent',
    TRY_FLOW_PENDING_RECOVERY_STORAGE_KEY: 'tryFlowPendingRecovery',
    initialStateFromStorage: null,
    sessionStorage: {
      value: null,
      getItem() {
        return this.value;
      },
      setItem(key, value) {
        this.value = value;
      },
      removeItem() {
        this.value = null;
      },
    },
    flowVersion: 'v0.337.0',
    defaultFlowVersion: 'master',
    flowService: {config: {enums: false}},
    initialState: {code: 'initial code', config: {enums: true}},
    editorRef: {current: {getValue: () => 'latest editor code'}},
    ...overrides,
  };
  vm.runInNewContext(code, context);
  return {
    ...context,
    events,
    savedState: () =>
      JSON.parse(
        LZString.decompressFromEncodedURIComponent(location.hash.slice(1)),
      ),
  };
}

test('version selection saves current code and config before reloading', () => {
  const h = handlers();
  h.changeFlowVersion('master');
  assert.deepEqual(h.savedState(), {
    code: 'latest editor code',
    config: {enums: false},
    version: 'master',
  }); // OK
  assert.deepEqual(h.events, [
    ['tryFlowLastContent', h.location.hash],
    ['reload', h.location.hash],
  ]); // OK
});

test('selecting the current version does not reload', () => {
  const h = handlers();
  h.changeFlowVersion('v0.337.0');
  assert.deepEqual(h.events, []); // OK
});

test('version selection preserves pending recovery separately from empty code', () => {
  const saved = {
    version: 'v0.337.0',
    code: 'recoverable code',
    config: {enums: true},
  };
  const h = handlers({
    initialStateFromStorage: saved,
    editorRef: {current: {getValue: () => ''}},
  });
  h.changeFlowVersion('master');
  assert.equal(h.savedState().code, ''); // OK
  assert.deepEqual(
    JSON.parse(JSON.stringify(h.takeRecoveryAfterReload())),
    saved,
  ); // OK
  assert.equal(h.takeRecoveryAfterReload(), null); // OK
});

test('version selection preserves empty code and state before initialization', () => {
  const h = handlers({flowService: null, editorRef: {current: null}});
  h.changeFlowVersion('v0.339.0');
  assert.deepEqual(h.savedState(), {
    code: 'initial code',
    config: {enums: true},
    version: 'v0.339.0',
  }); // OK
  h.editorRef.current = {getValue: () => ''};
  h.changeFlowVersion('master');
  assert.equal(h.savedState().code, ''); // OK
});

test('recovering another version reloads with the saved code and config', () => {
  const saved = {version: 'master', code: 'saved code', config: {enums: true}};
  const h = handlers({
    initialStateFromStorage: saved,
    monaco: {
      editor: {
        getModels: () => [
          {
            setValue() {
              assert.fail('must reload');
            },
          },
        ],
      },
    },
  });
  h.resetFromStorage();
  assert.deepEqual(h.savedState(), saved); // OK
  assert.equal(h.events[1][0], 'reload'); // OK
});

test('recovering the current version still restores without reloading', () => {
  const h = handlers({
    initialStateFromStorage: {
      version: 'v0.337.0',
      code: 'saved code',
      config: {},
    },
    monaco: {
      editor: {
        getModels: () => [
          {
            setValue(code) {
              h.events.push(code);
            },
          },
        ],
      },
    },
    setInitialStateFromStorage(value) {
      h.events.push(value);
    },
    setFlowService() {},
  });
  h.resetFromStorage();
  assert.deepEqual(h.events, [null, 'saved code']); // OK
});
