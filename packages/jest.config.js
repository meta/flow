/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @noflow
 * @format
 */

'use strict';

const path = require('path');
const packageJson = require('./package.json');

const workspaceModuleNameMappers = Object.fromEntries(
  packageJson.workspaces
    .filter(
      moduleName =>
        moduleName !== 'flow-parser' && moduleName !== 'flow-upgrade',
    )
    .map(moduleName => [
      `^${moduleName}$`,
      path.resolve(__dirname, moduleName, 'src', 'index.js'),
    ]),
);

module.exports = {
  // All imported modules in your tests should be mocked automatically
  automock: false,
  // Automatically clear mock calls and instances between every test
  clearMocks: true,
  // Indicates whether the coverage information should be collected while executing the test
  collectCoverage: false,
  // Limit test discovery to packages owned by this workspace.
  roots: [
    '<rootDir>/babel-plugin-syntax-flow-parser',
    '<rootDir>/flow-api-translator',
    '<rootDir>/flow-eslint',
    '<rootDir>/flow-estree',
    '<rootDir>/flow-parser',
    '<rootDir>/flow-remove-types',
    '<rootDir>/flow-transform',
  ],
  // The glob patterns Jest uses to detect test files
  testMatch: ['**/__tests__/**/*-test.js'],
  // An array of regexp pattern strings that are matched against all test paths, matched tests are skipped
  testPathIgnorePatterns: ['/node_modules/'],
  // An array of regexp pattern strings that are matched against all source file paths, matched files will skip transformation
  transformIgnorePatterns: ['/node_modules/', '/dist/'],

  // These mappings tell jest how to find the source files directly instead of using the built `dist` files.
  // This allows us to run tests without first doing a build.
  //
  // We have to manually teach jest about the generated files that get placed in the `dist` folders.
  // Otherwise jest will look in the `src` folder and will only find the unusable `.flow` file.
  moduleNameMapper: {
    // the modules themselves
    ...workspaceModuleNameMappers,

    // The babel plugin ships as a plain re-export shim at the package root,
    // not `src/index.js` like the other workspaces.
    '^babel-plugin-syntax-flow-parser$': path.resolve(
      __dirname,
      'babel-plugin-syntax-flow-parser',
      'index.js',
    ),

    '^flow-parser$': path.resolve(
      __dirname,
      'flow-parser',
      'oxidized-src',
      'index.js',
    ),

    '^flow-parser/__test_utils__/(.*)$': path.resolve(
      __dirname,
      'flow-parser',
      '__test_utils__',
      '$1',
    ),

    '^flow-parser/babel-plugin$': path.resolve(
      __dirname,
      'flow-parser',
      'babel-plugin.js',
    ),

    // flow-parser
    '.*/FlowParserWASM$': path.resolve(
      __dirname,
      'flow-parser',
      'dist',
      'FlowParserWASM.js',
    ),

    '^flow-parser/(.*)$': path.resolve(
      __dirname,
      'flow-parser',
      'oxidized-src',
      '$1',
    ),
  },
};
