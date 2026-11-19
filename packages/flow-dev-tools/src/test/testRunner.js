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
const {readFile, symlink} = require('fs').promises;
const {format} = require('util');
const {basename, dirname, resolve, join} = require('path');
const {spawn} = require('child_process');

const {getTestsDir} = require('../constants');
const {drain, rimraf} = require('../utils/async');
const {Builder} = require('./builder');
const {findTestsByName, findTestsByRun, loadSuite} = require('./findTests');
const {RunQueue} = require('./RunQueue');

import type {Suite} from './Suite';
import type {Args} from './testCommand';

/* We potentially have a ton of info to dump into stdout/stderr. Let's handle
 * drain events properly */
async function write(
  writer: stream$Writable | tty$WriteStream,
  fmt: string,
  ...args: Array<unknown>
): Promise<void> {
  if (false == writer.write(format(fmt, ...args))) {
    await drain(writer);
  }
}

async function runOnce(suites: {[suiteName: string]: Suite}, args: Args) {
  let maxErroredTests: string | number = 0;
  if (args.maxErroredTests != null) {
    maxErroredTests = args.maxErroredTests;
  }
  if (args.maxErroredTestsPct != null) {
    const maxErroredTestsPct = args.maxErroredTestsPct;
    const numTests = Object.keys(suites).length;
    maxErroredTests = Math.floor((numTests * Number(maxErroredTestsPct)) / 100);
  }
  if (Number(maxErroredTests) > 0) {
    process.stderr.write(
      format(
        'A maximum of %d suite%s allowed to error\n',
        maxErroredTests,
        maxErroredTests === 1 ? ' is' : 's are',
      ),
    );
  }

  const builder = new Builder(args.errorCheckCommand, args.testsDir);
  const runQueue = new RunQueue(
    args.bin,
    args.parallelism,
    args.fbmakeJson,
    suites,
    builder,
  );

  await runQueue.go();
  await builder.cleanup();

  const results = runQueue.results;
  // 1 - At least one test failed
  // 2 - No tests failed but too many tests errored
  // 0 - No tests failed and there were an acceptable number of errored tests
  let exitCode = 0;
  for (const suiteName of Object.keys(results).sort()) {
    const suiteResult = results[suiteName];
    if (suiteResult.type === 'exceptional') {
      maxErroredTests = Number(maxErroredTests) - 1;
      if (maxErroredTests < 0 && exitCode === 0) {
        exitCode = 2;
      }
      await write(
        process.stdout,
        chalk.bgRed(chalk.white.bold('ERRORED')) +
          chalk.grey(': suite ') +
          chalk.blue('%s') +
          '\n' +
          chalk.red('%s') +
          '\n',
        suiteName,
        suiteResult.message,
      );
    } else {
      const {testResults} = suiteResult;
      for (let testNum = 0; testNum < testResults.length; testNum++) {
        const testResult = testResults[testNum];
        for (
          let stepNum = 0;
          stepNum < testResult.stepResults.length;
          stepNum++
        ) {
          const result = testResult.stepResults[stepNum];
          if (!result.passed) {
            exitCode = 1;
            await write(
              process.stdout,
              chalk.red.bold('FAILED') +
                chalk.grey(': suite ') +
                chalk.blue('%s') +
                chalk.grey(', test ') +
                chalk.blue('%s') +
                chalk.grey(' (%d of %d), step %d of %d') +
                '\n',
              suiteName,
              testResult.name || 'unnamed test',
              testNum + 1,
              testResults.length,
              stepNum + 1,
              testResult.stepResults.length,
            );
            const messages = [];
            if (result.exception !== undefined) {
              messages.push(format('Uncaught exception: %s', result.exception));
            }
            for (const assertionResult of result.assertionResults) {
              if (assertionResult.type === 'fail') {
                messages.push(...assertionResult.messages);
              }
            }
            for (const message of messages) {
              await write(process.stdout, '%s\n', message);
            }
          }
        }
        const didTestFail = testResult.stepResults.some(r => !r.passed);
        if (didTestFail) {
          const logFile = join(
            builder.baseDirForSuite(suiteName),
            'tmp',
            String(testNum + 1),
            'test.log',
          );
          const logCommand =
            process.env['VISUAL'] || process.env['EDITOR'] || 'cat';
          let logContents = '';
          try {
            logContents = await readFile(logFile, 'utf8');
          } catch (e) {
            logContents = e.message != null ? e.message : String(e);
          }
          await write(
            process.stdout,
            chalk.grey.bold('%s %s') + '\n' + chalk.grey('%s') + '\n',
            logCommand,
            logFile,
            logContents,
          );
        }
      }
    }
  }

  const runID = builder.runID;
  const nextSteps: Array<[string, string]> = [
    ['Rerun the tests you just ran', `./tool test --rerun ${runID}`],
  ];

  if (exitCode != 0) {
    nextSteps.push([
      'Rerun the tests that just failed',
      `./tool test --rerun-failed ${runID}`,
    ]);
    nextSteps.push([
      'Record the tests that just failed',
      `./tool record --rerun-failed ${runID}`,
    ]);
  }

  await write(process.stderr, 'Possible next steps:');
  for (const [descr, cmd] of nextSteps) {
    await write(
      process.stderr,
      `
  ${descr}
    ${cmd}`,
    );
  }
  await write(process.stderr, '\n\n');

  return [exitCode, runID];
}

async function testRunner(args: Args): Promise<void> {
  process.env.IN_FLOW_TEST = '1';
  if (!process.env.hasOwnProperty('FLOW_MAX_WORKERS')) {
    process.env.FLOW_MAX_WORKERS = '2';
  }

  await write(process.stderr, `Using flow binary: ${args.bin}\n`);

  let suites;
  if (args.rerun != null) {
    suites = await findTestsByRun(args.rerun, args.failedOnly);
  } else {
    suites = await findTestsByName(args.testsDir, args.suites);
  }

  const loadedSuites: {[key: string]: Suite} = {};
  for (const suiteName of suites) {
    loadedSuites[suiteName] = loadSuite(args.testsDir, suiteName);
  }
  if (Object.keys(loadedSuites).length > 0) {
    const [exitCode, _] = await runOnce(loadedSuites, args);
    process.exit(exitCode);
  } else {
    process.stderr.write('No suites to run\n');
    process.exit(1);
  }
}

module.exports = {
  default: testRunner,
};
