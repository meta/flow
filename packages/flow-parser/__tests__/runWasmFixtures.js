/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @format
 */

'use strict';

const {test} = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const util = require('node:util');

// Bypasses the public `../dist/index.js` entry on purpose. The
// fixtures under flow/src/parser/test/flow/ exercise the parser boundary with
// fixture-supplied parser options, so we go straight to the underlying
// deserializer here. Don't "fix" this back to `require('..')`.
// We reach into the babel-stripped dist/ build (populated by the workspace
// `yarn build` step in runWasmFixtures.sh) because oxidized-src/FlowParser.js
// uses ES module + `import type` syntax and node's native `--test` runner has
// no babel transform. Drop-in / contract parity with hermes-parser is
// exercised separately by the jest tests under this directory; that suite goes
// through index.js.
const FlowParser = require('../dist/FlowParser');

// Upstream hermes-parser package tests snapshot the public package AST surface
// directly and record public parse errors explicitly. This full Flow
// fixture-corpus runner is not a package-surface oracle; it verifies that every
// source fixture has an explicit package-parser outcome: normally the
// `.tree.json` error list, with package-specific parser errors recorded below.
// A serializer/deserializer exception is always a harness failure.
const EXPECTED_PACKAGE_ERRORS = new Map([
  // Legacy Flow comment syntax is not enabled on the package parser boundary.
  ['types/annotations_in_comments_invalid/migrated_0000', []],
  ['types/annotations_in_comments_invalid/migrated_0001', []],
  ['types/annotations_in_comments_invalid/migrated_0002', []],
  ['types/annotations_in_comments_invalid/migrated_0003', []],
  ['types/annotations_in_comments_invalid/migrated_0004', []],
  [
    'types/annotations_in_comments_invalid/migrated_0006',
    [
      {
        loc: {
          start: {line: 2, column: 0},
          end: {line: 2, column: 0},
          source: null,
        },
        message: 'Unexpected token ILLEGAL',
        range: [5, 5],
      },
    ],
  ],
  [
    'types/annotations_in_comments_invalid/migrated_0007',
    [
      {
        loc: {
          start: {line: 1, column: 51},
          end: {line: 1, column: 52},
          source: null,
        },
        message: 'Unexpected token `*`',
        range: [51, 52],
      },
      {
        loc: {
          start: {line: 1, column: 53},
          end: {line: 1, column: 54},
          source: null,
        },
        message: 'Invalid regular expression: missing /',
        range: [53, 54],
      },
    ],
  ],
  // `oxc_type_feature_parser_rules` is not exposed on the package parser
  // boundary, so these fixtures parse there under the default rules.
  ...[
    'arrow_function_annotations',
    'declare_class_separator',
    'declare_function_param_names',
    'function_type_in_union',
    'function_type_param_names',
    'indexer_key_names',
    'mapped_type_separator',
    'method_param_names',
    'setters',
    'typescript_file',
  ].map(name => [`oxc_type_feature_parser_rules/${name}`, []]),
  [
    'oxc_type_feature_parser_rules/newlines',
    [
      {
        loc: {
          start: {line: 3, column: 5},
          end: {line: 3, column: 6},
          source: null,
        },
        message: 'Unexpected token `(`, expected the token `,`',
        range: [26, 27],
      },
    ],
  ],
  [
    'oxc_type_feature_parser_rules/object_type_asi',
    [
      {
        loc: {
          start: {line: 3, column: 2},
          end: {line: 3, column: 3},
          source: null,
        },
        message: 'Unexpected identifier, expected the token `,`',
        range: [25, 26],
      },
      {
        loc: {
          start: {line: 7, column: 2},
          end: {line: 7, column: 3},
          source: null,
        },
        message: 'Unexpected identifier, expected the token `,`',
        range: [75, 76],
      },
    ],
  ],
]);

// Walk directory tree returning sorted list of files relative to root.
function listFiles(root, dir) {
  const files = fs.readdirSync(dir ? path.join(root, dir) : root);
  const result = [];
  for (const f of files) {
    const rel = dir ? path.join(dir, f) : f;
    const stats = fs.statSync(path.join(root, rel));
    if (stats.isDirectory()) {
      result.push(...listFiles(root, rel));
    } else {
      result.push(rel);
    }
  }
  return result.sort();
}

// Group files into per-section, per-case structures matching the OCaml/JS
// hardcoded test runner discovery.
function getTests(rootDir) {
  const files = listFiles(rootDir);
  const tests = {};
  for (const file of files) {
    const sectionName = path.dirname(file);
    const caseParts = path.basename(file).split('.');
    const caseName = caseParts[0];

    // Hidden files
    if (caseName === '') {
      continue;
    }

    if (!tests[sectionName]) {
      tests[sectionName] = {};
    }
    if (!tests[sectionName][caseName]) {
      tests[sectionName][caseName] = {};
    }
    const c = tests[sectionName][caseName];

    const content = fs.readFileSync(path.join(rootDir, file), {
      encoding: 'utf8',
    });
    const ext = caseParts[caseParts.length - 1];
    const kind = caseParts.length > 2 ? caseParts[caseParts.length - 2] : null;

    if (ext === 'js') {
      c.content = content;
    } else if (ext === 'flow') {
      c.content = content;
      c.filename = path.join(rootDir, file);
    } else if (ext === 'json' && kind === 'tree') {
      c.expected_ast = JSON.parse(content);
    } else if (
      ext === 'ts' ||
      ext === 'tsx' ||
      ext === 'mts' ||
      ext === 'cts'
    ) {
      c.content = content;
      c.filename = path.join(rootDir, file);
    } else if (ext === 'json' && kind === 'options') {
      c.options = JSON.parse(content);
    }
  }
  return tests;
}

// Comparison ported verbatim from
// flow/packages/flow-parser/test/hardcoded_test_runner.js so the wasm tests
// apply the same diffing rules the existing OCaml-backed flow_parser tests
// use.
function newEnv() {
  const diffs = {};
  const path = ['root'];
  return {
    push_path: path.push.bind(path),
    pop_path: path.pop.bind(path),
    diff(type, expected, actual) {
      const pathStr = path.join('.');
      diffs[pathStr] = {
        path: pathStr,
        type,
        expected,
        actual,
      };
    },
    get_diffs() {
      const ret = [];
      for (const prop in diffs) {
        if (Object.prototype.hasOwnProperty.call(diffs, prop)) {
          ret.push(diffs[prop]);
        }
      }
      return ret;
    },
  };
}

function diffToString(diff) {
  let expectedStr = '';
  if (typeof diff.expected !== 'undefined') {
    expectedStr =
      '. Expected ' +
      JSON.stringify(diff.expected) +
      ', got ' +
      JSON.stringify(diff.actual);
  }
  return diff.path + ': ' + diff.type + expectedStr;
}

function compare(env, ast, spec, skipComments) {
  if (Array.isArray(spec)) {
    if (Array.isArray(ast)) {
      if (spec.length !== ast.length) {
        env.diff('Array has wrong size', spec.length, ast.length);
      }
      for (let i = 0; i < spec.length; i++) {
        if (Object.prototype.hasOwnProperty.call(spec, i)) {
          if (Object.prototype.hasOwnProperty.call(ast, i)) {
            env.push_path(i);
            compare(env, ast[i], spec[i], skipComments);
            env.pop_path();
          }
        }
      }
    } else {
      env.diff('Not an array');
    }
  } else if (spec != null && typeof spec === 'object') {
    for (const prop in spec) {
      const isCommentsProp =
        prop === 'trailingComments' || prop === 'leadingComments';
      if (skipComments && isCommentsProp) {
        continue;
      }
      // Skip comparing loc.source when expected is null (filename comparison).
      // Matches the OCaml test runner behavior where include_filename = false.
      if (prop === 'source' && spec[prop] === null) {
        continue;
      }
      if (Object.prototype.hasOwnProperty.call(spec, prop)) {
        const segments = prop.split('.');
        let subAst = ast;
        let found = true;
        let i;
        for (i = 0; i < segments.length; i++) {
          const seg = segments[i];
          if (subAst && Object.prototype.hasOwnProperty.call(subAst, seg)) {
            subAst = subAst[seg];
          } else {
            env.diff('Missing property "' + seg + '"');
            found = false;
            break;
          }
          env.push_path(seg);
        }
        if (found) {
          compare(env, subAst, spec[prop], skipComments);
        }
        for (; i > 0; i--) {
          env.pop_path();
        }
      }
    }
  } else {
    // The OCaml/Rust source-of-truth tree.json fixtures have `value: null`
    // for `Literal` (bigint) and `BigIntLiteralTypeAnnotation` because
    // BigInt is not JSON-serializable, so the wire-format `value` slot is
    // null. The wasm deserializer constructs the host BigInt value from
    // the `bigint` slot at deserialize time (mirroring upstream Hermes'
    // HermesToESTreeAdapter.{mapBigIntLiteral, mapBigIntLiteralTypeAnnotation}).
    // Treat `BigInt(...)` vs spec=null the same way `new RegExp(...)` vs
    // spec=null is treated.
    if (
      ast !== spec &&
      !(ast instanceof RegExp && spec === null) &&
      !(typeof ast === 'bigint' && spec === null)
    ) {
      env.diff('Wrong value', spec, ast);
    }
  }
}

function escapeContent(content) {
  return content
    .replace(/[\\]/g, '\\\\')
    .replace(/[\b]/g, '\\b')
    .replace(/[\f]/g, '\\f')
    .replace(/[\n]/g, '\\n')
    .replace(/[\r]/g, '\\r')
    .replace(/[\t]/g, '\\t');
}

function runOneCase(testName, testCase, parseOptions) {
  // Discovery may emit cases that lack one half of the (.js, .tree.json) pair.
  // The OCaml/JS runners simply skip those — they have no ground truth to
  // compare against. We surface them as a hard failure so a missing fixture is
  // never silent.
  if (testCase.content == null) {
    return {
      passed: false,
      output: 'Missing source content (no .js / .flow / .ts file)',
    };
  }
  if (testCase.expected_ast == null) {
    return {
      passed: false,
      output: 'Missing expected AST (no .tree.json file)',
    };
  }

  const result = {passed: true, output: ''};
  const append = (...args) => {
    result.output += args.join(' ') + '\n';
  };

  let flowAst;
  try {
    flowAst = FlowParser.parse(testCase.content, {
      ...parseOptions,
      sourceFilename: testCase.filename,
      // The fixtures want raw `ast.errors`, not a thrown SyntaxError on the
      // first parse error — they include cases (e.g. invalid-source fixtures)
      // where the OCaml parser surfaces errors but still produces a partial
      // AST. Opt out of the public-`parse()`-style throw added in
      // FlowParser.parse for hermes-parser API parity.
      throwOnParseErrors: false,
    });
  } catch (e) {
    append('Flow exploded:', util.inspect(e, {depth: null}));
    result.passed = false;
    return result;
  }

  const env = newEnv();
  const flowErrors = flowAst.errors;
  // The wasm parser's ESTree output may not surface an `errors` array (Flow's
  // ESTree shape collects parse errors out-of-band when configured). Treat
  // missing as empty so the existing comparison logic still applies.
  const errors = Array.isArray(flowErrors) ? flowErrors : [];

  const expectedErrors =
    EXPECTED_PACKAGE_ERRORS.get(testName) ??
    (Array.isArray(testCase.expected_ast.errors)
      ? testCase.expected_ast.errors
      : []);
  compare(env, {errors}, {errors: expectedErrors}, true);

  const diffs = env.get_diffs();
  if (diffs.length > 0) {
    result.passed = false;
    append('****Unexpected Differences****');
    for (let i = 0; i < diffs.length; i++) {
      append('(#' + i + ')', diffToString(diffs[i]));
    }
  }

  return result;
}

// Discovery + registration. The fixture root is taken from
// FLOW_PARSER_FIXTURE_ROOT so the BUCK runtime can point at the materialized
// runtime_files directory; falls back to the in-tree path for local runs.
const FIXTURE_ROOT =
  process.env.FLOW_PARSER_FIXTURE_ROOT ||
  path.resolve(__dirname, '../../../src/parser/test/flow');

const sections = getTests(FIXTURE_ROOT);
const sectionNames = Object.keys(sections).sort();

if (sectionNames.length === 0) {
  throw new Error(
    'No fixture sections discovered under ' +
      FIXTURE_ROOT +
      '. Set FLOW_PARSER_FIXTURE_ROOT to point at flow/src/parser/test/flow.',
  );
}

// Translate OCaml-style fixture options (as written in `*.options.json` files
// — `components`, `enums`, `pattern_matching`, `types`, etc.) into the
// JS-layer `flow.parse` options (`enableExperimentalComponentSyntax`,
// `enableExperimentalFlowMatchSyntax`, `flow`). The OCaml runner consumes the
// fixture options directly via `Parser_env.parse_options`; the JS layer
// renamed them, so we bridge here so the same fixture files work.
function translateFixtureOptions(fixtureOptions) {
  const out = {};
  for (const k of Object.keys(fixtureOptions)) {
    const v = fixtureOptions[k];
    switch (k) {
      case 'components':
        out.enableExperimentalComponentSyntax = v;
        break;
      case 'pattern_matching':
        out.enableExperimentalFlowMatchSyntax = v;
        break;
      case 'types':
        // OCaml's `default_parse_options` sets `types = true`; fixtures that
        // exercise the no-type-grammar path (e.g. ts_syntax `as`/`satisfies`
        // disabled) opt out via `types: false`. Plumbed through the WASM C
        // ABI as `enableTypes`.
        out.enableTypes = v;
        break;
      case 'esproposal_decorators':
        // The Rust parser exposes a `enableExperimentalDecorators` flag
        // through the WASM ABI so fixtures that disable decorators (e.g.
        // `decorators/class_field_disabled`) can exercise the
        // unexpected-`@` error path.
        out.enableExperimentalDecorators = v;
        break;
      case 'enums':
        // OCaml's `default_parse_options` sets `enums = false`; fixtures
        // that exercise enum syntax opt in via `enums: true` in their
        // `.options.json`. Plumbed through the WASM C ABI as `enableEnums`.
        out.enableEnums = v;
        break;
      case 'records':
        // OCaml's `default_parse_options` sets `records = false`; fixtures
        // that exercise record syntax (`#{ ... }`) opt in via
        // `records: true`. Plumbed through the WASM C ABI as `enableRecords`.
        out.enableRecords = v;
        break;
      case 'use_strict':
      case 'intern_comments':
        // No matching JS-layer option; the wasm parser hardwires safe
        // defaults that match the fixture expectations.
        break;
      default:
        // Pass through unknown keys (sourceFilename, tokens, etc.).
        out[k] = v;
        break;
    }
  }
  return out;
}

for (const sectionName of sectionNames) {
  const cases = sections[sectionName];
  const caseNames = Object.keys(cases).sort();
  test(sectionName, async t => {
    for (const caseName of caseNames) {
      await t.test(caseName, () => {
        const c = cases[caseName];
        const sectionOptions = {};
        const caseOptions = translateFixtureOptions(c.options || {});
        const parseOptions = {...sectionOptions, ...caseOptions};
        const expectationKey = sectionName + '/' + caseName;
        const r = runOneCase(expectationKey, c, parseOptions);
        if (!r.passed) {
          assert.fail(
            'Fixture ' +
              sectionName +
              '/' +
              caseName +
              ' (source: "' +
              escapeContent(c.content || '<missing>') +
              '")\n' +
              r.output,
          );
        }
      });
    }
  });
}
