// (c) Meta Platforms, Inc. and affiliates. Confidential and proprietary.

'use strict';

const assert = require('assert');
const fs = require('fs');
const os = require('os');
const path = require('path');
const {spawnSync} = require('child_process');
const flowNode = path.resolve(__dirname, '../flow-node');
const root = fs.mkdtempSync(path.join(os.tmpdir(), 'flow-node-'));
let count = 0;

function write(filename, source) {
  const target = path.join(root, filename);
  fs.mkdirSync(path.dirname(target), {recursive: true});
  fs.writeFileSync(target, source);
  return target;
}
function run(args, input) {
  const result = spawnSync(process.execPath, [flowNode, ...args], {
    cwd: root, encoding: 'utf8', input, timeout: 30000,
  });
  if (result.error) throw result.error;
  return result;
}
function succeeds(args, stdout, input) {
  const result = run(args, input);
  assert.strictEqual(result.status, 0, result.stderr);
  assert.strictEqual(result.stdout, stdout, result.stderr);
  count++;
}
function fails(args, expected) {
  const result = run(args);
  assert.notStrictEqual(result.status, 0, result.stdout);
  assert(result.stderr.includes(expected), result.stderr);
  count++;
}
const register = path.resolve(__dirname, '../register.js');
function nodeSucceeds(args, stdout) {
  const result = spawnSync(process.execPath, args, {
    cwd: root, encoding: 'utf8', timeout: 30000,
  });
  if (result.error) throw result.error;
  assert.strictEqual(result.status, 0, result.stderr);
  assert.strictEqual(result.stdout, stdout, result.stderr);
  count++;
}

try {
  write('value.cjs', '// @flow\nenum E {A}\nrecord R {value: number}\ncomponent F(value: number) { return R {value}; }\nmodule.exports = F({value: match (E.A) {E.A => 42, _ => 0}}).value;');
  const cjs = write('main.cjs', '#!/usr/bin/env node\n// @flow\nconst value: number = require("./value.cjs");\nconsole.log(value, require.main === module, process.argv.slice(2).join(","));');
  succeeds([cjs, 'a', 'b'], '42 true a,b\n');
  write('return.cjs', '// @flow\nmodule.exports = 7; return; throw new Error("unreachable");');
  succeeds(['-p', 'require("./return.cjs")'], '7\n');
  succeeds(['-p', 'const n: number = 40; n + 2'], '42\n');
  succeeds(['--no-warnings', '-p', 'process.argv.length'], '4\n');
  succeeds(['--no-warnings', cjs, '--no-warnings', 'a'], '42 true --no-warnings,a\n');
  write('plain.cjs', 'module.exports = (42: number);');
  succeeds(['-a', '-p', 'require("./plain.cjs")'], '42\n');
  fails(['-p', 'require("./plain.cjs")'], 'SyntaxError');
  const esm = write('main.mjs', '// @flow\nimport value from "./value.cjs";\nexport enum E {A}\nconst answer: number = await Promise.resolve(value);\nconsole.log(answer, E.A, import.meta.url.endsWith("main.mjs"));');
  succeeds([esm], '42 A true\n');
  write('esm/package.json', '{"type":"module"}');
  const pkgMain = write('esm/main.js', '// @flow\nimport {value} from "./value.js";\nconsole.log(value);');
  write('esm/value.js', '// @flow\nexport const value: number = 42;');
  succeeds([pkgMain], '42\n');
  write('esm/await.js', '// @flow\nexport const n: number = await Promise.resolve(42);');
  succeeds(['-c', path.join(root, 'esm/await.js')], '');
  write('cycle-a.cjs', '// @flow\nexports.before = 1; exports.result = require("./cycle-b.cjs").result;');
  write('cycle-b.cjs', '// @flow\nexports.result = require("./cycle-a.cjs").before + 41;');
  succeeds(['-p', 'require("./cycle-a.cjs").result'], '42\n');
  write('esm/cache.js', '// @flow\nglobalThis.loads = (globalThis.loads || 0) + 1; export const count: number = globalThis.loads;');
  const cache = write('esm/cache-main.js', '// @flow\nconst first = await import("./cache.js?one"); const again = await import("./cache.js?one"); const other = await import("./cache.js?two"); console.log(first === again, first.count, other.count);');
  succeeds([cache], 'true 1 2\n');
  write('esm/cycle-a.js', '// @flow\nimport {answer} from "./cycle-b.js"; export function base(): number { return 40; } export const result: number = answer();');
  write('esm/cycle-b.js', '// @flow\nimport {base} from "./cycle-a.js"; export function answer(): number { return base() + 2; }');
  const cycle = write('esm/cycle-main.js', '// @flow\nimport {result} from "./cycle-a.js"; console.log(result);');
  succeeds([cycle], '42\n');
  const component = write('component.cjs', '// @flow\ntype N = number;\ncomponent Fail(value: N) { throw new Error("component boom"); }\nFail({value: 1});');
  fails([component], 'component.cjs:3:' + ('component Fail(value: N) { throw '.length + 1));
  const unicode = write('sp ace-😀#throw.mjs', '// @flow\nfunction fail(): void { const text = `a\u2028b\u2029c`; throw new Error("unicode boom"); }\nfail();');
  fails([unicode], 'sp ace-😀#throw.mjs:4:11');
  const dynamic = write('dynamic.cjs', '// @flow\nimport("./esm/value.js").then(m => console.log(m.value));');
  succeeds([dynamic], '42\n');
  const throwSource = '// @flow\r\ntype N = number;\r\nfunction fail(): void {\r\n  const emoji: string = "😀"; throw new Error("boom");\r\n}\r\nfail();';
  const throwCjs = write('throw.cjs', throwSource);
  const throwEsm = write('throw.mjs', throwSource);
  const column = '  const emoji: string = "😀"; throw '.length + 1;
  fails([throwCjs], `at fail (${throwCjs}:4:${column})`);
  fails([throwEsm], `at fail (${throwEsm}:4:${column})`);
  const importCjs = write('import-throw.cjs', '// @flow\nrequire("./throw.cjs");');
  const importEsm = write('import-throw.mjs', '// @flow\nimport "./throw.mjs";');
  fails([importCjs], `at fail (${throwCjs}:4:${column})`);
  fails([importEsm], `at fail (${throwEsm}:4:${column})`);
  const evalSource = 'type N = number;\nfunction fail(): void { const x: N = 1; throw new Error("eval boom"); }\nfail();';
  const evalCol = 'function fail(): void { const x: N = 1; throw '.length + 1;
  fails(['-e', evalSource], `at fail ([eval]:2:${evalCol})`);
  succeeds([], '> 42\n> ', 'const n: number = 40; n + 2\n');
  if (require('module').register) {
    write('hook/dep.mjs', '// @flow\nexport const value: number = 41;');
    const hookMain = write('hook/main.mjs', '// @flow\nimport {value} from "./dep.mjs";\nconst answer: number = value + 1;\nconsole.log(answer);');
    nodeSucceeds(['-r', register, hookMain], '42\n');
    write('hook/untyped.mjs', 'export default (42: number);');
    nodeSucceeds(['-e', `require(${JSON.stringify(register)})({all: true, includes: /untyped/}); import("./hook/untyped.mjs").then(m => console.log(m.default));`], '42\n');
  }
  const bad = write('bad.mjs', '// @flow\nconst x: number = ;');
  const checked = run(['-c', bad]);
  assert.strictEqual(checked.status, 1);
  assert(checked.stdout.includes('bad.mjs:2'), checked.stdout);
  count++;
  console.log(`flow-node: ${count} integration checks passed on ${process.version}`);
} finally {
  fs.rmSync(root, {recursive: true, force: true});
}
