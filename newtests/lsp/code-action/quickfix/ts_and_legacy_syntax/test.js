/*
 * @flow
 * @format
 */

import type {SuiteType} from '../../../../Tester';
const path = require('path');
const {suite, test} = require('../../../../Tester');
const {generateSimpleTests} = require('../../test-utils');

module.exports = suite(
  ({
    lspStartAndConnect,
    lspStart,
    lspRequest,
    lspInitializeParams,
    lspRequestAndWaitUntilResponse,
    addFile,
    lspIgnoreStatusAndCancellation,
  }) => [
    test('provide quickfix for `$Keys`', [
      addFile('fix-dollar-keys.js.ignored', 'fix-dollar-keys.js'),
      lspStartAndConnect(),
      lspRequestAndWaitUntilResponse('textDocument/codeAction', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/fix-dollar-keys.js',
        },
        range: {
          start: {
            line: 2,
            character: 10,
          },
          end: {
            line: 2,
            character: 11,
          },
        },
        context: {
          only: ['quickfix'],
          diagnostics: [],
        },
      }).verifyLSPMessageSnapshot(
        path.join(
          __dirname,
          '__snapshots__',
          'quickfix-fix-dollar-keys-type.json',
        ),
        ['textDocument/publishDiagnostics', ...lspIgnoreStatusAndCancellation],
      ),
    ]).flowConfig('_flowconfig_ts_utility_syntax'),
    test('provide quickfix for `mixed` type', [
      addFile('fix-mixed-type.js.ignored', 'fix-mixed-type.js'),
      lspStartAndConnect(),
      lspRequestAndWaitUntilResponse('textDocument/codeAction', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/fix-mixed-type.js',
        },
        range: {
          start: {
            line: 2,
            character: 9,
          },
          end: {
            line: 2,
            character: 14,
          },
        },
        context: {
          only: ['quickfix'],
          diagnostics: [],
        },
      }).verifyLSPMessageSnapshot(
        path.join(__dirname, '__snapshots__', 'quickfix-mixed-type.json'),
        ['textDocument/publishDiagnostics', ...lspIgnoreStatusAndCancellation],
      ),
    ]).flowConfig('_flowconfig_ts_utility_syntax'),
    test('provide quickfix for `unknown` type', [
      addFile('fix-unknown-type.js.ignored', 'fix-unknown-type.js'),
      lspStartAndConnect(),
      lspRequestAndWaitUntilResponse('textDocument/codeAction', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/fix-unknown-type.js',
        },
        range: {
          start: {
            line: 2,
            character: 9,
          },
          end: {
            line: 2,
            character: 16,
          },
        },
        context: {
          only: ['quickfix'],
          diagnostics: [],
        },
      }).verifyLSPMessageSnapshot(
        path.join(__dirname, '__snapshots__', 'quickfix-unknown-type.json'),
        ['textDocument/publishDiagnostics', ...lspIgnoreStatusAndCancellation],
      ),
    ]),
    test('provide quickfix for `never` type', [
      addFile('fix-never-type.js.ignored', 'fix-never-type.js'),
      lspStartAndConnect(),
      lspRequestAndWaitUntilResponse('textDocument/codeAction', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/fix-never-type.js',
        },
        range: {
          start: {
            line: 2,
            character: 9,
          },
          end: {
            line: 2,
            character: 14,
          },
        },
        context: {
          only: ['quickfix'],
          diagnostics: [],
        },
      }).verifyLSPMessageSnapshot(
        path.join(__dirname, '__snapshots__', 'quickfix-never-type.json'),
        ['textDocument/publishDiagnostics', ...lspIgnoreStatusAndCancellation],
      ),
    ]),
    test('provide quickfix for `keyof`', [
      addFile('fix-keyof.js.ignored', 'fix-keyof.js'),
      lspStartAndConnect(),
      lspRequestAndWaitUntilResponse('textDocument/codeAction', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/fix-keyof.js',
        },
        range: {
          start: {
            line: 2,
            character: 9,
          },
          end: {
            line: 2,
            character: 16,
          },
        },
        context: {
          only: ['quickfix'],
          diagnostics: [],
        },
      }).verifyLSPMessageSnapshot(
        path.join(__dirname, '__snapshots__', 'quickfix-keyof-type.json'),
        ['textDocument/publishDiagnostics', ...lspIgnoreStatusAndCancellation],
      ),
    ]),
    test('provide quickfix for `extends` in type param', [
      addFile('fix-type-param-extends.js.ignored', 'fix-type-param-extends.js'),
      lspStartAndConnect(),
      lspRequestAndWaitUntilResponse('textDocument/codeAction', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/fix-type-param-extends.js',
        },
        range: {
          start: {
            line: 2,
            character: 7,
          },
          end: {
            line: 2,
            character: 23,
          },
        },
        context: {
          only: ['quickfix'],
          diagnostics: [],
        },
      }).verifyLSPMessageSnapshot(
        path.join(
          __dirname,
          '__snapshots__',
          'quickfix-extends-in-type-param.json',
        ),
        ['textDocument/publishDiagnostics', ...lspIgnoreStatusAndCancellation],
      ),
    ]),
    test('provide quickfix for deprecated `:` in type param', [
      addFile('fix-type-param-colon.js.ignored', 'fix-type-param-colon.js'),
      lspStartAndConnect(),
      lspRequestAndWaitUntilResponse('textDocument/codeAction', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/fix-type-param-colon.js',
        },
        range: {
          start: {
            line: 2,
            character: 7,
          },
          end: {
            line: 2,
            character: 21,
          },
        },
        context: {
          only: ['quickfix'],
          diagnostics: [],
        },
      }).verifyLSPMessageSnapshot(
        path.join(
          __dirname,
          '__snapshots__',
          'quickfix-deprecated-colon-in-type-param.json',
        ),
        ['textDocument/publishDiagnostics', ...lspIgnoreStatusAndCancellation],
      ),
    ]),
    test('provide quickfix for `readonly` variance', [
      addFile('fix-readonly-variance.js.ignored', 'fix-readonly-variance.js'),
      lspStartAndConnect(),
      lspRequestAndWaitUntilResponse('textDocument/codeAction', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/fix-readonly-variance.js',
        },
        range: {
          start: {
            line: 2,
            character: 10,
          },
          end: {
            line: 2,
            character: 18,
          },
        },
        context: {
          only: ['quickfix'],
          diagnostics: [],
        },
      }).verifyLSPMessageSnapshot(
        path.join(
          __dirname,
          '__snapshots__',
          'quickfix-readonly-variance.json',
        ),
        ['textDocument/publishDiagnostics', ...lspIgnoreStatusAndCancellation],
      ),
    ]),
    test('provide quickfix for `in` variance', [
      addFile('fix-in-variance.js.ignored', 'fix-in-variance.js'),
      lspStartAndConnect(),
      lspRequestAndWaitUntilResponse('textDocument/codeAction', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/fix-in-variance.js',
        },
        range: {
          start: {
            line: 2,
            character: 20,
          },
          end: {
            line: 2,
            character: 21,
          },
        },
        context: {
          only: ['quickfix'],
          diagnostics: [],
        },
      }).verifyLSPMessageSnapshot(
        path.join(__dirname, '__snapshots__', 'quickfix-in-variance.json'),
        ['textDocument/publishDiagnostics', ...lspIgnoreStatusAndCancellation],
      ),
    ]),
    test('provide quickfix for `out` variance', [
      addFile('fix-out-variance.js.ignored', 'fix-out-variance.js'),
      lspStartAndConnect(),
      lspRequestAndWaitUntilResponse('textDocument/codeAction', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/fix-out-variance.js',
        },
        range: {
          start: {
            line: 2,
            character: 20,
          },
          end: {
            line: 2,
            character: 21,
          },
        },
        context: {
          only: ['quickfix'],
          diagnostics: [],
        },
      }).verifyLSPMessageSnapshot(
        path.join(__dirname, '__snapshots__', 'quickfix-out-variance.json'),
        ['textDocument/publishDiagnostics', ...lspIgnoreStatusAndCancellation],
      ),
    ]),
    test('provide quickfix for `in out` variance', [
      addFile('fix-in-out-variance.js.ignored', 'fix-in-out-variance.js'),
      lspStartAndConnect(),
      lspRequestAndWaitUntilResponse('textDocument/codeAction', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/fix-in-out-variance.js',
        },
        range: {
          start: {
            line: 2,
            character: 20,
          },
          end: {
            line: 2,
            character: 21,
          },
        },
        context: {
          only: ['quickfix'],
          diagnostics: [],
        },
      }).verifyLSPMessageSnapshot(
        path.join(__dirname, '__snapshots__', 'quickfix-inout-variance.json'),
        ['textDocument/publishDiagnostics', ...lspIgnoreStatusAndCancellation],
      ),
    ]),
    test('provide quickfix for `(x: T)` type cast', [
      addFile('fix-colon-cast.js.ignored', 'fix-colon-cast.js'),
      lspStartAndConnect(),
      lspRequestAndWaitUntilResponse('textDocument/codeAction', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/fix-colon-cast.js',
        },
        range: {
          start: {
            line: 2,
            character: 3,
          },
          end: {
            line: 2,
            character: 3,
          },
        },
        context: {
          only: ['quickfix'],
          diagnostics: [],
        },
      }).verifyLSPMessageSnapshot(
        path.join(
          __dirname,
          '__snapshots__',
          'quickfix-old-flow-type-cast.json',
        ),
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),
    ]),
    test('provide quickfix for `readonly` array type', [
      addFile(
        'fix-readonly-array-type.js.ignored',
        'fix-readonly-array-type.js',
      ),
      lspStartAndConnect(),
      lspRequestAndWaitUntilResponse('textDocument/codeAction', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/fix-readonly-array-type.js',
        },
        range: {
          start: {
            line: 2,
            character: 10,
          },
          end: {
            line: 2,
            character: 26,
          },
        },
        context: {
          only: ['quickfix'],
          diagnostics: [],
        },
      }).verifyLSPMessageSnapshot(
        path.join(
          __dirname,
          '__snapshots__',
          'quickfix-readonly-array-type.json',
        ),
        ['textDocument/publishDiagnostics', ...lspIgnoreStatusAndCancellation],
      ),
    ]),
    test('provide quickfix for `readonly` tuple type', [
      addFile(
        'fix-readonly-tuple-type.js.ignored',
        'fix-readonly-tuple-type.js',
      ),
      lspStartAndConnect(),
      lspRequestAndWaitUntilResponse('textDocument/codeAction', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/fix-readonly-tuple-type.js',
        },
        range: {
          start: {
            line: 2,
            character: 10,
          },
          end: {
            line: 2,
            character: 34,
          },
        },
        context: {
          only: ['quickfix'],
          diagnostics: [],
        },
      }).verifyLSPMessageSnapshot(
        path.join(
          __dirname,
          '__snapshots__',
          'quickfix-readonly-tuple-type.json',
        ),
        ['textDocument/publishDiagnostics', ...lspIgnoreStatusAndCancellation],
      ),
    ]),
    test('provide quickfix for `Readonly` type', [
      addFile('fix-readonly-type.js.ignored', 'fix-readonly-type.js'),
      lspStartAndConnect(),
      lspRequestAndWaitUntilResponse('textDocument/codeAction', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/fix-readonly-type.js',
        },
        range: {
          start: {
            line: 1,
            character: 9,
          },
          end: {
            line: 1,
            character: 17,
          },
        },
        context: {
          only: ['quickfix'],
          diagnostics: [],
        },
      }).verifyLSPMessageSnapshot(
        path.join(__dirname, '__snapshots__', 'quickfix-ts-readonly-type.json'),
        ['textDocument/publishDiagnostics', ...lspIgnoreStatusAndCancellation],
      ),
    ]),
    test('provide quickfix for `ReadonlyArray` type', [
      addFile('fix-readonlyarray-type.js.ignored', 'fix-readonlyarray-type.js'),
      lspStartAndConnect(),
      lspRequestAndWaitUntilResponse('textDocument/codeAction', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/fix-readonlyarray-type.js',
        },
        range: {
          start: {
            line: 1,
            character: 9,
          },
          end: {
            line: 1,
            character: 22,
          },
        },
        context: {
          only: ['quickfix'],
          diagnostics: [],
        },
      }).verifyLSPMessageSnapshot(
        path.join(
          __dirname,
          '__snapshots__',
          'quickfix-readonly-array-no-dollar-type.json',
        ),
        ['textDocument/publishDiagnostics', ...lspIgnoreStatusAndCancellation],
      ),
    ]),
    test('provide quickfix for `ReadonlyMap` type', [
      addFile('fix-readonlymap-type.js.ignored', 'fix-readonlymap-type.js'),
      lspStartAndConnect(),
      lspRequestAndWaitUntilResponse('textDocument/codeAction', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/fix-readonlymap-type.js',
        },
        range: {
          start: {
            line: 1,
            character: 9,
          },
          end: {
            line: 1,
            character: 20,
          },
        },
        context: {
          only: ['quickfix'],
          diagnostics: [],
        },
      }).verifyLSPMessageSnapshot(
        path.join(
          __dirname,
          '__snapshots__',
          'quickfix-readonly-map-no-dollar-type.json',
        ),
        ['textDocument/publishDiagnostics', ...lspIgnoreStatusAndCancellation],
      ),
    ]),
    test('provide quickfix for `ReadonlySet` type', [
      addFile('fix-readonlyset-type.js.ignored', 'fix-readonlyset-type.js'),
      lspStartAndConnect(),
      lspRequestAndWaitUntilResponse('textDocument/codeAction', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/fix-readonlyset-type.js',
        },
        range: {
          start: {
            line: 1,
            character: 9,
          },
          end: {
            line: 1,
            character: 20,
          },
        },
        context: {
          only: ['quickfix'],
          diagnostics: [],
        },
      }).verifyLSPMessageSnapshot(
        path.join(
          __dirname,
          '__snapshots__',
          'quickfix-readonly-set-no-dollar-type.json',
        ),
        ['textDocument/publishDiagnostics', ...lspIgnoreStatusAndCancellation],
      ),
    ]),
    test('provide quickfix for `NonNullable` type', [
      addFile('fix-nonnullable-type.js.ignored', 'fix-nonnullable-type.js'),
      lspStartAndConnect(),
      lspRequestAndWaitUntilResponse('textDocument/codeAction', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/fix-nonnullable-type.js',
        },
        range: {
          start: {
            line: 1,
            character: 9,
          },
          end: {
            line: 1,
            character: 20,
          },
        },
        context: {
          only: ['quickfix'],
          diagnostics: [],
        },
      }).verifyLSPMessageSnapshot(
        path.join(
          __dirname,
          '__snapshots__',
          'quickfix-non-nullable-type.json',
        ),
        ['textDocument/publishDiagnostics', ...lspIgnoreStatusAndCancellation],
      ),
    ]),
    test('provide quickfix for `$Partial`', [
      addFile('fix-partial-type.js.ignored', 'fix-partial-type.js'),
      lspStartAndConnect(),
      lspRequestAndWaitUntilResponse('textDocument/codeAction', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/fix-partial-type.js',
        },
        range: {
          start: {
            line: 1,
            character: 9,
          },
          end: {
            line: 1,
            character: 17,
          },
        },
        context: {
          only: ['quickfix'],
          diagnostics: [],
        },
      }).verifyLSPMessageSnapshot(
        path.join(
          __dirname,
          '__snapshots__',
          'quickfix-dollar-partial-type.json',
        ),
        ['textDocument/publishDiagnostics', ...lspIgnoreStatusAndCancellation],
      ),
    ]),
    test('provide quickfix for `$Shape`', [
      addFile('fix-shape-type.js.ignored', 'fix-shape-type.js'),
      lspStartAndConnect(),
      lspRequestAndWaitUntilResponse('textDocument/codeAction', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/fix-shape-type.js',
        },
        range: {
          start: {
            line: 1,
            character: 9,
          },
          end: {
            line: 1,
            character: 15,
          },
        },
        context: {
          only: ['quickfix'],
          diagnostics: [],
        },
      }).verifyLSPMessageSnapshot(
        path.join(__dirname, '__snapshots__', 'quickfix-dollar-shape.json'),
        ['textDocument/publishDiagnostics', ...lspIgnoreStatusAndCancellation],
      ),
    ]),
    test(
      'provide fix-all-in-file quickfix for colon casts',
      generateSimpleTests(
        'quickfix',
        {
          addFile,
          lspIgnoreStatusAndCancellation,
          lspStartAndConnect,
          lspRequestAndWaitUntilResponse,
        },
        __dirname,
        'fix-all-colon-cast.js',
        'quickfix-fix-all-colon-cast',
      ),
    ),
    test('provide quickfix for deprecated `+` sigil on property', [
      addFile(
        'fix-deprecated-plus-sigil-prop.js.ignored',
        'fix-deprecated-plus-sigil-prop.js',
      ),
      lspStartAndConnect(),
      lspRequestAndWaitUntilResponse('textDocument/codeAction', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/fix-deprecated-plus-sigil-prop.js',
        },
        range: {
          start: {
            line: 2,
            character: 10,
          },
          end: {
            line: 2,
            character: 11,
          },
        },
        context: {
          only: ['quickfix'],
          diagnostics: [],
        },
      }).verifyLSPMessageSnapshot(
        path.join(
          __dirname,
          '__snapshots__',
          'quickfix-deprecated-plus-sigil-prop.json',
        ),
        ['textDocument/publishDiagnostics', ...lspIgnoreStatusAndCancellation],
      ),
    ]),
    test('provide quickfix for deprecated `-` sigil on property', [
      addFile(
        'fix-deprecated-minus-sigil-prop.js.ignored',
        'fix-deprecated-minus-sigil-prop.js',
      ),
      lspStartAndConnect(),
      lspRequestAndWaitUntilResponse('textDocument/codeAction', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/fix-deprecated-minus-sigil-prop.js',
        },
        range: {
          start: {
            line: 2,
            character: 10,
          },
          end: {
            line: 2,
            character: 11,
          },
        },
        context: {
          only: ['quickfix'],
          diagnostics: [],
        },
      }).verifyLSPMessageSnapshot(
        path.join(
          __dirname,
          '__snapshots__',
          'quickfix-deprecated-minus-sigil-prop.json',
        ),
        ['textDocument/publishDiagnostics', ...lspIgnoreStatusAndCancellation],
      ),
    ]),
    test('provide quickfix for deprecated `+` sigil on type parameter', [
      addFile(
        'fix-deprecated-plus-sigil-tparam.js.ignored',
        'fix-deprecated-plus-sigil-tparam.js',
      ),
      lspStartAndConnect(),
      lspRequestAndWaitUntilResponse('textDocument/codeAction', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/fix-deprecated-plus-sigil-tparam.js',
        },
        range: {
          start: {
            line: 2,
            character: 8,
          },
          end: {
            line: 2,
            character: 9,
          },
        },
        context: {
          only: ['quickfix'],
          diagnostics: [],
        },
      }).verifyLSPMessageSnapshot(
        path.join(
          __dirname,
          '__snapshots__',
          'quickfix-deprecated-plus-sigil-tparam.json',
        ),
        ['textDocument/publishDiagnostics', ...lspIgnoreStatusAndCancellation],
      ),
    ]),
    test('provide quickfix for deprecated `-` sigil on type parameter', [
      addFile(
        'fix-deprecated-minus-sigil-tparam.js.ignored',
        'fix-deprecated-minus-sigil-tparam.js',
      ),
      lspStartAndConnect(),
      lspRequestAndWaitUntilResponse('textDocument/codeAction', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/fix-deprecated-minus-sigil-tparam.js',
        },
        range: {
          start: {
            line: 2,
            character: 8,
          },
          end: {
            line: 2,
            character: 9,
          },
        },
        context: {
          only: ['quickfix'],
          diagnostics: [],
        },
      }).verifyLSPMessageSnapshot(
        path.join(
          __dirname,
          '__snapshots__',
          'quickfix-deprecated-minus-sigil-tparam.json',
        ),
        ['textDocument/publishDiagnostics', ...lspIgnoreStatusAndCancellation],
      ),
    ]),
  ],
) as SuiteType;
