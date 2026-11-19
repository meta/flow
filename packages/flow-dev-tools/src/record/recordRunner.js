/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @flow
 * @format
 */

const chalk = require('chalk');
const {isAbsolute, join} = require('path');
const {format} = require('util');

const {readFile, writeFile} = require('fs').promises;
const {default: Builder} = require('../test/builder');
const {findTestsByName, findTestsByRun} = require('../test/findTests');
const parser = require('flow-parser');
const {loadSuite} = require('../test/findTests');
const {default: RunQueue} = require('../test/RunQueue');
const {getTestsDir} = require('../constants');
const {isLabelOp} = require('../test/assertions/label');

import type {Suite} from '../test/Suite';
import type {Args} from './recordCommand';
import type {CallSuggestion} from '../test/assertions/assertionTypes';

function escapeString(str: string): string {
  // Escape backslashes first so literal backslashes in the content survive the round-trip; then
  // escape the template-literal metacharacters. Order matters: metacharacter escaping adds
  // backslashes that must not themselves be doubled.
  return str
    .replace(/\\/g, '\\\\')
    .replace(/`/g, '\\`')
    .replace(/\${/g, '\\${');
}

function indent(str: string, size: number) {
  const indent = Array(size + 1).join(' ');
  return str
    .split('\n')
    .map(line => (line.length > 0 ? indent + line : line))
    .join('\n');
}

// `null` for a comparison with a label op, which names an earlier output that a recorded literal
// would lose.
function suggestionToString(
  suggestion: CallSuggestion,
  indentSize: number,
): ?string {
  if (suggestion.args.some(isLabelOp)) {
    return null;
  }
  const args = suggestion.args
    .map(arg => {
      switch (typeof arg) {
        case 'string':
          if (arg.split('\n').length === 1) {
            return format('`%s`', escapeString(arg));
          } else {
            return format('`\n%s\n`', indent(escapeString(arg), 2));
          }
        case 'number':
          return format('%d', arg);
        case 'object':
          return format('%s', JSON.stringify(arg, null, 2));
        default:
          throw new Error('Unhandled arg type');
      }
    })
    .map(line => line + ',\n')
    .join('');
  if (suggestion.args.length === 0) {
    return format('%s()', suggestion.method);
  } else {
    return format(
      '%s(\n%s%s)',
      suggestion.method,
      indent(args, indentSize),
      Array(indentSize - 1).join(' '),
    );
  }
}

function dfsForRange(node: any, line: number, col: number): ?[number, number] {
  const todo = [];
  if (typeof node === 'object' && node != null && node.hasOwnProperty('type')) {
    if (node.type === 'CallExpression') {
      if (node.callee.type === 'MemberExpression') {
        if (
          node.callee.property.loc.start.line === line &&
          node.callee.property.loc.start.column === col - 1
        ) {
          return [node.callee.property.range[0], node.range[1]];
        }
      }
    }
    for (var prop in node) {
      todo.push(node[prop]);
    }
  } else if (Array.isArray(node)) {
    todo.push(...node);
  }

  for (const child of todo) {
    const ret = dfsForRange(child, line, col);
    if (ret != null) {
      return ret;
    }
  }
  return null;
}

async function runner(args: Args): Promise<void> {
  process.stderr.write(`Using flow binary: ${args.bin}\n`);

  const builder = new Builder(args.errorCheckCommand, args.testsDir);
  let suites;
  if (args.rerun != null) {
    suites = await findTestsByRun(args.rerun, true);
  } else {
    suites = await findTestsByName(args.testsDir, args.suites);
  }

  const loadedSuites: {[string]: Suite} = {};
  for (const suiteName of suites) {
    loadedSuites[suiteName] = loadSuite(args.testsDir, suiteName);
  }

  const runQueue = new RunQueue(
    args.bin,
    args.parallelism,
    false,
    loadedSuites,
    builder,
  );

  await runQueue.go();

  await builder.cleanup();

  let totalTests = 0,
    totalSteps = 0,
    testNum = 0,
    stepNum = 0,
    suiteName = 0;

  function printStatus(status: 'RECORDING' | 'RECORDED' | 'FAIL'): void {
    let statusText = chalk.bold('[ ] RECORDING:       ');
    let newline = '';
    if (status === 'RECORDED') {
      statusText = chalk.green.bold('[✓] RECORDED:        ');
      newline = '\n';
    } else if (status === 'FAIL') {
      statusText = chalk.red.bold('[✗] FAILED TO RECORD:');
      newline = '\n';
    }
    if (process.stdout.isTTY) {
      // $FlowFixMe[prop-missing] - Add this to lib file
      // $FlowFixMe[incompatible-type] - Add this to lib file
      process.stdout.clearLine();
      process.stdout.write(
        format(
          '\r%s  %s (%d/%d tests %d/%d steps passed)%s',
          statusText,
          suiteName,
          testNum,
          totalTests,
          stepNum,
          totalSteps,
          newline,
        ),
      );
    } else {
      if (status == 'FAIL' || status == 'RECORDED') {
        process.stdout.write(
          format(
            '%s  %s (%d/%d tests %d/%d steps passed)\n',
            statusText,
            suiteName,
            testNum,
            totalTests,
            stepNum,
            totalSteps,
          ),
        );
      }
    }
  }

  const results = runQueue.results;
  for (const suiteName in results) {
    const suiteResult = results[suiteName];
    if (suiteResult.type === 'exceptional') {
      printStatus('FAIL');
      continue;
    }
    // TODO - reorder records based on line number
    totalTests = suiteResult.testResults.length;
    for (testNum = 0; testNum < totalTests; testNum++) {
      let testFailed = false;
      let testRecorded = false;
      totalSteps =
        suiteResult.testResults[totalTests - testNum - 1].stepResults.length;
      for (stepNum = 0; stepNum < totalSteps; stepNum++) {
        printStatus('RECORDING');
        // Record starting at the end to avoid messing with line numbers
        const stepResult =
          suiteResult.testResults[totalTests - testNum - 1].stepResults[
            totalSteps - stepNum - 1
          ];
        if (!stepResult.passed) {
          // Again, start with the last assertion
          for (
            let assertionNum = stepResult.assertionResults.length - 1;
            assertionNum >= 0;
            assertionNum--
          ) {
            const result = stepResult.assertionResults[assertionNum];
            if (result.type === 'fail') {
              const suggestion = result.suggestion;
              if (suggestion.method) {
                const assertLoc = result.assertLoc;
                if (assertLoc) {
                  let filename = assertLoc.filename;
                  if (!isAbsolute(filename)) {
                    filename = join(
                      getTestsDir(args.testsDir),
                      suiteName,
                      filename,
                    );
                  }
                  const code = await readFile(filename, 'utf8');
                  const ast = parser.parse(code, {});
                  const range =
                    assertLoc &&
                    dfsForRange(ast, assertLoc.line, assertLoc.column);
                  if (range) {
                    const [start, end] = range;
                    const replacement =
                      // $FlowFixMe[incompatible-type]
                      suggestionToString(suggestion, assertLoc.column);
                    if (replacement == null) {
                      process.stderr.write(
                        'Not recording a comparison with a label op\n',
                      );
                    } else {
                      await writeFile(
                        filename,
                        code.slice(0, start) + replacement + code.slice(end),
                      );
                    }
                  } else {
                    process.stderr.write(
                      'Could not find the assertion in the code\n',
                    );
                  }
                } else {
                  process.stderr.write(
                    'Could not find the assertion in the stack\n',
                  );
                }
              } else if (suggestion.file) {
                let filename = suggestion.file;
                if (!isAbsolute(filename)) {
                  filename = join(
                    getTestsDir(args.testsDir),
                    suiteName,
                    filename,
                  );
                }
                await writeFile(filename, suggestion.contents);
              }
            }
          }
        }
      }
    }
    printStatus('RECORDED');
  }
}

module.exports = {
  default: runner,
};
