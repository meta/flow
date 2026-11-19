/*
 * @flow
 * @format
 */

import type {SuiteType} from '../Tester';
const {suite, test} = require('../Tester');

const graphOutput =
  '{"$defs":{"0":{"def":{"kind":"NumGeneral"},"kind":"Def","reason":"main.js:1:1: number"}},"schema":{"graph":true,"json_direct":true},"types":[{"end":1,"endline":1,"line":1,"loc":{"end":{"column":1,"line":1,"offset":1},"source":"main.js","start":{"column":1,"line":1,"offset":0},"type":"SourceFile"},"path":"main.js","reasons":[],"start":1,"type":{"expression_type":{"$ref":0}}}]}';

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
  test('--graph contradicts a --for-tool DEPTH', [
    addFile('main.js')
      .flowCmd(['dump-types', '--graph', '--strip-root', 'main.js'])
      .stderr('dump-types: --graph requires --for-tool and --json')
      .exitCodes([64]),
    flowCmd([
      'dump-types',
      '--for-tool',
      '5',
      '--json',
      '--graph',
      '--strip-root',
      'main.js',
    ])
      .stderr(
        'dump-types: --graph has no depth limit, so it cannot be combined with a --for-tool DEPTH',
      )
      .exitCodes([64]),
    flowCmd([
      'dump-types',
      '--for-tool',
      '--json',
      '--graph-node-cap',
      '2',
      '--strip-root',
      'main.js',
    ])
      .stderr('dump-types: --graph-node-cap requires --graph')
      .exitCodes([64]),
  ]),
  test(
    '--graph prints the same bytes with any --dedup threshold, which it ignores',
    [
      addFile('main.js')
        .flowCmd([
          'dump-types',
          '--for-tool',
          '--json',
          '--graph',
          '--json-direct',
          '--strip-root',
          'main.js',
        ])
        .stdout(graphOutput)
        .exitCodes([0]),
      flowCmd([
        'dump-types',
        '--for-tool',
        '--json',
        '--graph',
        '--dedup',
        '--json-direct',
        '--strip-root',
        'main.js',
      ])
        .stdout(graphOutput)
        .exitCodes([0]),
      flowCmd([
        'dump-types',
        '--for-tool',
        '--json',
        '--graph',
        '--dedup',
        '0',
        '--json-direct',
        '--strip-root',
        'main.js',
      ])
        .stdout(graphOutput)
        .exitCodes([0]),
      flowCmd([
        'dump-types',
        '--for-tool',
        '--json',
        '--graph',
        '--dedup',
        '1000000',
        '--json-direct',
        '--strip-root',
        'main.js',
      ])
        .stdout(graphOutput)
        .exitCodes([0]),
    ],
  ),
  test('--path applies only to stdin', [
    addFile('main.js')
      .flowCmd(['dump-types', '--path', 'other.js', 'main.js'])
      .stderr(
        'dump-types: --path applies only to a file read from stdin, not to a FILE argument',
      )
      .exitCodes([64]),
  ]),
]): SuiteType);
