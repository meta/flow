/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @format
 */

'use strict';

const pluginTester = require('babel-plugin-tester').default;
const plugin = require('../index');

pluginTester({
  plugin,
  pluginName: 'transform-flow-enums',
  tests: {
    boolean: {
      code: `enum E {A = true, B = false}`,
      output: `const E = require("flow-enums-runtime")({
  A: true,
  B: false
});`,
    },
    number: {
      code: `enum E {A = 1, B = 2}`,
      output: `const E = require("flow-enums-runtime")({
  A: 1,
  B: 2
});`,
    },
    'string-initialized': {
      code: `enum E {A = 'a', B = 'b'}`,
      output: `const E = require("flow-enums-runtime")({
  A: 'a',
  B: 'b'
});`,
    },
    'string-defaulted': {
      code: `enum E {A, B}`,
      output: `const E = require("flow-enums-runtime").Mirrored(["A", "B"]);`,
    },
    symbol: {
      code: `enum E of symbol {A, B}`,
      output: `const E = require("flow-enums-runtime")({
  A: Symbol("A"),
  B: Symbol("B")
});`,
    },
    export: {
      code: `export enum E {A = 1, B = 2}`,
      output: `export const E = require("flow-enums-runtime")({
  A: 1,
  B: 2
});`,
    },
    export_default: {
      code: `export default enum E {A = 1, B = 2}`,
      output: `const E = require("flow-enums-runtime")({
  A: 1,
  B: 2
});

export default E;`,
    },
  },
});

// Test `getRuntime` plugin option
pluginTester({
  plugin,
  pluginName: 'transform-flow-enums',
  pluginOptions: {
    getRuntime: t => t.identifier('Enum'),
  },
  tests: {
    getRuntime: {
      code: `enum E {A = 1, B = 2}`,
      output: `const E = Enum({
  A: 1,
  B: 2
});`,
    },
  },
});

// Test running the plugin twice in one pass, as when both a preset and the
// project's own config include it
pluginTester({
  plugin,
  pluginName: 'transform-flow-enums',
  babelOptions: {
    plugins: [[plugin, {}, 'transform-flow-enums-duplicate']],
  },
  tests: {
    duplicate: {
      code: `enum E {A = 1, B = 2}`,
      output: `const E = require("flow-enums-runtime")({
  A: 1,
  B: 2
});`,
    },
    duplicate_export: {
      code: `export enum E {A = 1, B = 2}`,
      output: `export const E = require("flow-enums-runtime")({
  A: 1,
  B: 2
});`,
    },
    duplicate_export_default: {
      code: `export default enum E {A = 1, B = 2}`,
      output: `const E = require("flow-enums-runtime")({
  A: 1,
  B: 2
});

export default E;`,
    },
  },
});
