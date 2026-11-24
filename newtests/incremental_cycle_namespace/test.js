/*
 * @flow
 */

import type {SuiteType} from '../Tester';
const {suite, test} = require('../Tester');

// Each case is an import cycle whose files expose each other's namespace, and a
// consumer, `c.js`, of the cycle's `value`. Changing `value` in `a.js` must
// recheck the consumer. `modifyFile` force-rechecks on the harness's server.
const STATUS = ['status', '--strip-root', '.'];

module.exports = (suite(({addFiles, modifyFile}) => [
  test('Two-file ES module cycle', [
    addFiles('es/a.js', 'es/b.js', 'es/c.js')
      .flowCmd(STATUS)
      .stdout('No errors!'),
    modifyFile('es/a.js', 'value: 1}', "value: 'one'}")
      .flowCmd(STATUS)
      .stdout(
        `
          Error ----------------------------------------------------------------------------------------------------- es/c.js:2:26
          Cannot assign \`obj.value\` to \`y\` because \`string\` [1] is incompatible with \`number\` [2]. [incompatible-type]
             es/c.js:2:26
             2| export const y: number = obj.value;
                                         ^^^^^^^^^
          References:
             es/a.js:2:34
             2| export const obj = {peer, value: 'one'};
                                                 ^^^^^ [1]
             es/c.js:2:17
             2| export const y: number = obj.value;
                                ^^^^^^ [2]
          Found 1 error
        `,
      ),
    modifyFile('es/a.js', "value: 'one'}", 'value: 1}')
      .flowCmd(STATUS)
      .stdout('No errors!'),
  ]),
  test('Two-file CommonJS cycle', [
    addFiles('cjs/a.js', 'cjs/b.js', 'cjs/c.js')
      .flowCmd(STATUS)
      .stdout('No errors!'),
    modifyFile('cjs/a.js', 'value: 1}', "value: 'one'}")
      .flowCmd(STATUS)
      .stdout(
        `
          Error ---------------------------------------------------------------------------------------------------- cjs/c.js:2:19
          Cannot assign \`a.value\` to \`y\` because \`string\` [1] is incompatible with \`number\` [2]. [incompatible-type]
             cjs/c.js:2:19
             2| const y: number = a.value;
                                  ^^^^^^^
          References:
             cjs/a.js:2:32
             2| module.exports = {peer, value: 'one'};
                                               ^^^^^ [1]
             cjs/c.js:2:10
             2| const y: number = a.value;
                         ^^^^^^ [2]
          Found 1 error
        `,
      ),
    modifyFile('cjs/a.js', "value: 'one'}", 'value: 1}')
      .flowCmd(STATUS)
      .stdout('No errors!'),
  ]),
  test('Three-file ES module cycle', [
    addFiles('three/a.js', 'three/b.js', 'three/c.js', 'three/d.js')
      .flowCmd(STATUS)
      .stdout('No errors!'),
    modifyFile('three/a.js', 'value: 1}', "value: 'one'}")
      .flowCmd(STATUS)
      .stdout(
        `
          Error -------------------------------------------------------------------------------------------------- three/c.js:2:26
          Cannot assign \`obj.value\` to \`y\` because \`string\` [1] is incompatible with \`number\` [2]. [incompatible-type]
             three/c.js:2:26
             2| export const y: number = obj.value;
                                         ^^^^^^^^^
          References:
             three/a.js:2:34
             2| export const obj = {peer, value: 'one'};
                                                 ^^^^^ [1]
             three/c.js:2:17
             2| export const y: number = obj.value;
                                ^^^^^^ [2]
          Found 1 error
        `,
      ),
    modifyFile('three/a.js', "value: 'one'}", 'value: 1}')
      .flowCmd(STATUS)
      .stdout('No errors!'),
  ]),
]): SuiteType);
