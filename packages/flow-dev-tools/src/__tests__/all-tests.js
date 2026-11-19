/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @format
 */

const babel = require('@babel/core');
const Module = require('module');

const srcDir = require('path').resolve(__dirname, '..');

const originalCompile = Module.prototype._compile;
Module.prototype._compile = function (code, filename) {
  if (filename.startsWith(srcDir) && filename.endsWith('.js')) {
    const result = babel.transformSync(code, {
      filename,
      presets: ['@babel/preset-flow'],
      plugins: ['flow-parser/babel-plugin'],
    });
    code = result.code;
  }
  return originalCompile.call(this, code, filename);
};

const {applyLabelOp, isLabelOp, labels} = require('../test/assertions/label');
const {exec} = require('../utils/async');
const {splitIntoChunks} = require('../utils/string');
const {
  describeServerCrash,
  HEAD_LINES,
  isCrash,
  isServerLogName,
  logWindow,
  TAIL_LINES,
} = require('../test/serverCrash');

class Expected {
  constructor(actualValue) {
    this.actualValue = actualValue;
  }

  toBe(expectedValue) {
    if (this.actualValue !== expectedValue) {
      throw new Error(`Expected ${this.actualValue} to be ${expectedValue}`);
    }
  }

  toEqual(expectedValue) {
    const actual = JSON.stringify(this.actualValue);
    const expected = JSON.stringify(expectedValue);
    if (actual !== expected) {
      throw new Error(`Expected ${actual} to be ${expected}`);
    }
  }

  toContain(expectedSubstring) {
    if (!this.actualValue.includes(expectedSubstring)) {
      throw new Error(
        `Expected ${JSON.stringify(this.actualValue)} to contain ${JSON.stringify(expectedSubstring)}`,
      );
    }
  }

  notToContain(unexpectedSubstring) {
    if (this.actualValue.includes(unexpectedSubstring)) {
      throw new Error(
        `Expected ${JSON.stringify(this.actualValue)} not to contain ${JSON.stringify(unexpectedSubstring)}`,
      );
    }
  }
}

function expect(v) {
  return new Expected(v);
}

function repeatString(str, times) {
  let result = '';
  for (let i = 0; i < times; i++) {
    result += str;
  }
  return result;
}

const collectedTests = [];

function test(name, fn) {
  collectedTests.push({name, fn});
}

test('exec', async () => {
  expect(await exec('echo foo')).toBe('foo\n');
  expect(await exec('cat', {stdin: 'bar'})).toBe('bar');

  expect(repeatString('foo', 2)).toBe('foofoo');

  // make the string big enough that it exceeds the chunk size for writes
  const repeatedString = repeatString('0123456789', 2000);
  expect(await exec('cat', {stdin: repeatedString})).toBe(repeatedString);
});

test('splitIntoChunks', () => {
  expect(splitIntoChunks('', 1)).toEqual([]);
  expect(splitIntoChunks('abcd', 2)).toEqual(['ab', 'cd']);
  expect(splitIntoChunks('abc', 2)).toEqual(['ab', 'c']);
  expect(splitIntoChunks('abc', 10)).toEqual(['abc']);
  expect(splitIntoChunks('abc', 1)).toEqual(['a', 'b', 'c']);
  // The check marks are multi-byte characters when encoded with UTF-8. Make sure they are treated
  // as single characters and not split up into individual bytes.
  expect(splitIntoChunks('✓✓✓✓✓', 1)).toEqual(['✓', '✓', '✓', '✓', '✓']);
});

// A panic as the server actually prints it, so the assertions below pin what a reader gets rather
// than what this file invents. Any Flow panic has this shape: a `panicked at <file>:<line>:<col>`
// header, the message, then a backtrace.
const PANIC_STDERR = `[2026-09-20 00:57:05.985] Dequeueing nonparallelizable Request 5: fox after 0.000 seconds

thread '<unnamed>' (113608) panicked at fbcode/flow/rust_port/crates/flow_monitor_rpc/src/server_status.rs:602:17:
Unexpected status transition from 'Server is free' with event 'Parsing_progress files 0/1 (0.0%)'
stack backtrace:
   0: __rustc::rust_begin_unwind
   1: core::panicking::panic_fmt
   2: flow_monitor_rpc::server_status::update
`;

test('isCrash only fires on an exit nobody declared', () => {
  // The server went away on its own in the middle of a step: a crash however it died.
  expect(isCrash({harnessStoppedIt: false, stepAllowsDeath: false})).toBe(true);
  // `stopFlowServer` / `cleanup`: the harness asked, so the exit is its own doing.
  expect(isCrash({harnessStoppedIt: true, stepAllowsDeath: false})).toBe(false);
  // `dontMindServerDeath()` or `waitUntilServerStatus(_, 'stopped')`: the step is testing the
  // death, as `newtests/lsp/restartOnReinit` does for a deliberate `Exit.Restart`.
  expect(isCrash({harnessStoppedIt: false, stepAllowsDeath: true})).toBe(false);
  expect(isCrash({harnessStoppedIt: true, stepAllowsDeath: true})).toBe(false);
});

test('describeServerCrash reports the panic, not just the exit', () => {
  // `flow server` is the monitor, so the panic is in the log of the server it supervised, not in
  // the stderr of the process the harness spawned.
  const description = describeServerCrash(
    {code: 110, signal: null, stderr: 'monitor: server exited with code 110\n'},
    [{name: 'aS5.log', contents: PANIC_STDERR}],
    '/tmp/flow/tests/abc/tmp/5',
  );
  expect(description).toContain('flow server crashed: exited with code 110');
  expect(description).toContain('/tmp/flow/tests/abc/tmp/5');
  expect(description).toContain('aS5.log');
  // The two things a reader needs to act on: what failed and where.
  expect(description).toContain('Unexpected status transition');
  expect(description).toContain('server_status.rs:602:17');
});

test('describeServerCrash falls back to stderr when there is no log', () => {
  const description = describeServerCrash(
    {
      code: null,
      signal: 'SIGSEGV',
      stderr: 'died before it could open a log\n',
    },
    [],
    '/t',
  );
  expect(description).toContain('killed by signal SIGSEGV');
  expect(description).toContain('the spawned process stderr');
  expect(description).toContain('died before it could open a log');
});

test('describeServerCrash handles deaths that print nothing', () => {
  expect(
    describeServerCrash({code: 0, signal: null, stderr: '   \n'}, [], '/t'),
  ).toContain('(nothing)');
  expect(
    describeServerCrash({code: null, signal: null, stderr: 'x'}, [], '/t'),
  ).toContain('exited for a reason the harness could not read');
});

test('describeServerCrash keeps the panic header when the log is long', () => {
  // A real panic is a header, a message and then tens of backtrace frames, so a tail-only excerpt
  // would show frames and drop the two lines that say what happened and where.
  const backtrace =
    Array.from(
      {length: 200},
      (_, i) => `  ${i}: some::rust::frame\n     at ./src/thing.rs:${i}:1`,
    ).join('\n') + '\n';
  const description = describeServerCrash(
    {code: 110, signal: null, stderr: ''},
    [{name: 'server.log', contents: PANIC_STDERR + backtrace + 'goodbye\n'}],
    '/t',
  );
  expect(description).toContain('Unexpected status transition');
  expect(description).toContain('server_status.rs:602:17');
  // The end is kept too, and the middle is accounted for rather than silently dropped.
  expect(description).toContain('goodbye');
  expect(description).toContain('lines elided');
});

test('describeServerCrash bounds each log it shows', () => {
  // Numbered so the assertions can tell one part of the log from another.
  const middle =
    Array.from({length: 5000}, (_, i) => `progress line ${i}`).join('\n') +
    '\n';
  const description = describeServerCrash(
    {code: 110, signal: null, stderr: ''},
    [{name: 'server.log', contents: PANIC_STDERR + middle}],
    '/t',
  );
  // Head and tail survive, the middle does not.
  expect(description).toContain('server_status.rs:602:17');
  expect(description).toContain('progress line 4999');
  expect(description).notToContain('progress line 2500');
  expect(description.split('\n').length <= HEAD_LINES + TAIL_LINES + 6).toBe(
    true,
  );
});

test('describeServerCrash does not print one long line twice', () => {
  // A single line is all head and no tail, so an excerpt that took both from the same lines would
  // emit the same clamped text twice. It also has to keep the front, where a panic starts.
  const oneLongLine = 'panic! ' + repeatString('0123456789', 10000) + ' last!';
  const description = describeServerCrash(
    {code: 110, signal: null, stderr: ''},
    [{name: 'server.log', contents: oneLongLine}],
    '/t',
  );
  expect(description).toContain('panic! 0123456789');
  expect(description.split('0123456789').length - 1 < 500).toBe(true);
  expect(description.length < 6000).toBe(true);
});

test('describeServerCrash keeps the end of a long tail', () => {
  // The tail exists to show the last thing the process said, so it is the end of it that has to
  // survive the character bound.
  const lines = Array.from({length: HEAD_LINES + TAIL_LINES + 10}, (_, i) =>
    i < HEAD_LINES + 10 ? `line ${i}` : `line ${i} ` + repeatString('x', 500),
  );
  const description = describeServerCrash(
    {code: 110, signal: null, stderr: ''},
    [{name: 'server.log', contents: lines.join('\n')}],
    '/t',
  );
  expect(description).toContain(`line ${HEAD_LINES + TAIL_LINES + 9}`);
});

test('logWindow returns what was appended since the mark', () => {
  const log = Buffer.from('older lines\nmore of them\nthe new part\n', 'utf8');
  const mark = Buffer.byteLength('older lines\nmore of them\n', 'utf8');
  expect(logWindow(log, mark)).toBe('the new part\n');
  // No mark yet: the whole file is new.
  expect(logWindow(log, 0)).toBe(log.toString('utf8'));
  // Nothing appended since the mark.
  expect(logWindow(log, log.length)).toBe('');
  // Shorter than its mark, so Flow rotated it and this is a fresh log: show all of it.
  expect(logWindow(Buffer.from('fresh\n', 'utf8'), 9999)).toBe('fresh\n');
});

test('isServerLogName picks Flow logs and not the harness log', () => {
  expect(isServerLogName('zStmpzSfoozS5.log')).toBe(true);
  expect(isServerLogName('zStmpzSfoozS5.monitor_log')).toBe(true);
  // The harness's own record of the actions it ran, not evidence about the crash.
  expect(isServerLogName('test.log')).toBe(false);
  // A restart rotates the dying server's log here, so it is evidence rather than noise.
  expect(isServerLogName('zStmpzSfoozS5.log.old')).toBe(true);
  expect(isServerLogName('zStmpzSfoozS5.sock')).toBe(false);
  expect(isServerLogName('zStmpzSfoozS5.pids')).toBe(false);
});

function expectThrow(fn, message) {
  try {
    fn();
  } catch (e) {
    expect(e.message).toBe(message);
    return;
  }
  throw new Error(`Expected an error: ${message}`);
}

const suggestion = {method: 'stdout', args: []};

function apply(recorded, op, actual, what = 'stdout') {
  return applyLabelOp({
    op,
    recorded,
    actual,
    what,
    assertLoc: null,
    reason: null,
    suggestion,
  });
}

test('only labels.write and labels.read make label ops', () => {
  expect(isLabelOp(labels.write('x'))).toBe(true);
  expect(isLabelOp(labels.read('x'))).toBe(true);
  expect(isLabelOp('x')).toBe(false);
  expect(isLabelOp({name: 'x'})).toBe(false);
});

test('a write records the output and passes', () => {
  const recorded = new Map();
  expect(apply(recorded, labels.write('x'), 'a\n').type).toBe('pass');
  expect(recorded.get('x')).toBe('a\n');
});

test('a read passes exactly when the bytes are the recorded ones', () => {
  for (const [written, actual] of [
    ['a\nb\n', 'a\nb\n'],
    ['a\nb\n', 'a\nb\n\n'],
    ['a\nb\n', '\na\nb\n'],
    ['a b\n', 'a  b\n'],
    ['a\n', 'b\n'],
  ]) {
    const recorded = new Map([['x', written]]);
    expect(apply(recorded, labels.read('x'), actual).type).toBe(
      written === actual ? 'pass' : 'fail',
    );
  }
});

test('a read names the label when the bytes differ', () => {
  const recorded = new Map([['before', 'a\n']]);
  const blank = apply(recorded, labels.read('before'), 'a\n\n', 'out.json');
  expect(blank.messages[0]).toBe('Expected out.json to equal label "before"');
  expect(blank.messages[1]).toBe(
    'They differ only in leading or trailing blank lines',
  );
  expect(apply(recorded, labels.read('before'), 'b\n').messages[0]).toBe(
    'Expected stdout to equal label "before"',
  );
});

test('a label is written once and read only after it is written', () => {
  const recorded = new Map();
  expectThrow(
    () => apply(recorded, labels.read('x'), 'a'),
    'Label "x" is read before it is written',
  );
  apply(recorded, labels.write('x'), 'a');
  expectThrow(
    () => apply(recorded, labels.write('x'), 'a'),
    'Label "x" is written twice',
  );
});

(async () => {
  for (const {name, fn} of collectedTests) {
    try {
      await fn();
      console.error(`[SUCCESS] ${name}`);
    } catch (e) {
      console.error(`[FAILURE] ${name}`);
      console.error(e);
      throw e;
    }
  }
})();
