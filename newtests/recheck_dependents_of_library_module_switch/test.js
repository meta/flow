/*
 * @flow
 */

import type {SuiteType} from '../Tester';
const {suite, test} = require('../Tester');

// `decls.js`, a library, declares four modules. A file switches its `require` from one declared
// module to another, which changes the type it exports. The server runs without a file watcher,
// so only `force-recheck` tells it about the edit, and `full-check` checks the same files from
// scratch.
const START = ['start', '--wait', '--file-watcher', 'none', '.'];
const STATUS = ['status', '--no-auto-start', '--strip-root', '.'];
const FULL_CHECK = ['full-check', '--strip-root', '.'];
const LIBRARY_MODULE_ERROR = `
          Error ------------------------------------------------------------------------------------------------------ use.js:2:19
          Cannot assign \`mid\` to \`n\` because \`string\` [1] is incompatible with \`number\` [2]. [incompatible-type]
             use.js:2:19
             2| const n: number = mid;
                                  ^^^
          References:
             decls.js:5:27
             5|   declare module.exports: string;
                                          ^^^^^^ [1]
             use.js:2:10
             2| const n: number = mid;
                         ^^^^^^ [2]
          Found 1 error
        `;
const PACKAGE_ERROR = `
          Error ----------------------------------------------------------------------------------------------------- puse.js:2:19
          Cannot assign \`pmid\` to \`n\` because \`string\` [1] is incompatible with \`number\` [2]. [incompatible-type]
             puse.js:2:19
              2| const n: number = pmid;
                                   ^^^^
          References:
             decls.js:11:27
             11|   declare module.exports: string;
                                           ^^^^^^ [1]
             puse.js:2:10
              2| const n: number = pmid;
                          ^^^^^^ [2]
          Found 1 error
        `;

module.exports = (suite(({addFile, flowCmd}) => [
  test('rechecks the dependents of a file that switches between modules a library declares', [
    addFile('decls.js')
      .addFile('mid.js')
      .addFile('use.js')
      .flowCmd(START, undefined, false)
      .exitCodes([0]),
    flowCmd(STATUS, undefined, false).stdout('No errors!').exitCodes([0]),
    addFile('mid_libb.js', 'mid.js')
      .flowCmd(['force-recheck', '--no-auto-start', 'mid.js'], undefined, false)
      .exitCodes([0]),
    flowCmd(STATUS, undefined, false).stdout(LIBRARY_MODULE_ERROR).exitCodes([2]),
    flowCmd(FULL_CHECK, undefined, false).stdout(LIBRARY_MODULE_ERROR).exitCodes([2]),
  ]),
  test('rechecks the dependents of a file that switches between untyped packages a library declares', [
    addFile('decls.js')
      .addFile('pkga.js', 'node_modules/pkga/index.js')
      .addFile('pkgb.js', 'node_modules/pkgb/index.js')
      .addFile('pmid.js')
      .addFile('puse.js')
      .flowCmd(START, undefined, false)
      .exitCodes([0]),
    flowCmd(STATUS, undefined, false).stdout('No errors!').exitCodes([0]),
    addFile('pmid_pkgb.js', 'pmid.js')
      .flowCmd(['force-recheck', '--no-auto-start', 'pmid.js'], undefined, false)
      .exitCodes([0]),
    flowCmd(STATUS, undefined, false).stdout(PACKAGE_ERROR).exitCodes([2]),
    flowCmd(FULL_CHECK, undefined, false).stdout(PACKAGE_ERROR).exitCodes([2]),
  ]),
]): SuiteType);
