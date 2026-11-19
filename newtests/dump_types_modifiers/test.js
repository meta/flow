/*
 * @flow
 * @format
 */

import type {SuiteType} from '../Tester';
const {suite, test} = require('../Tester');

module.exports = (suite(({addFile, flowCmd}) => [
  test('--dedup requires --for-tool and --json', [
    addFile('main.js')
      .flowCmd(['dump-types', '--dedup', '--strip-root', 'main.js'])
      .stderr('dump-types: --dedup requires --for-tool and --json')
      .exitCodes([64]),
    flowCmd(['dump-types', '--for-tool', '--dedup', '--strip-root', 'main.js'])
      .stderr('dump-types: --dedup requires --json')
      .exitCodes([64]),
    flowCmd(['dump-types', '--dedup', '--json', '--strip-root', 'main.js'])
      .stderr('dump-types: --dedup requires --for-tool')
      .exitCodes([64]),
  ]),
  test('--path applies only to stdin', [
    addFile('main.js')
      .flowCmd(['dump-types', '--path', 'other.js', 'main.js'])
      .stderr(
        'dump-types: --path applies only to a file read from stdin, not to a FILE argument',
      )
      .exitCodes([64]),
  ]),
]): SuiteType);
