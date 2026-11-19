/*
 * @flow
 * @format
 */

import type {SuiteType} from '../Tester';
const {suite, test} = require('../Tester');

module.exports = (suite(({addFile, flowCmd, labels}) => [
  test('--json-direct prints the type string as JSON', [
    addFile('main.js')
      .flowCmd([
        'dump-types',
        '--for-tool',
        '--json',
        '--strip-root',
        'main.js',
      ])
      .stdout(
        `
          [
            {
              "end": 1,
              "endline": 1,
              "line": 1,
              "loc": {
                "end": {
                  "column": 1,
                  "line": 1,
                  "offset": 1
                },
                "source": "main.js",
                "start": {
                  "column": 1,
                  "line": 1,
                  "offset": 0
                },
                "type": "SourceFile"
              },
              "path": "main.js",
              "reasons": [],
              "start": 1,
              "type": "{\\"expression_type\\":{\\"def\\":{\\"kind\\":\\"NumGeneral\\"},\\"kind\\":\\"Def\\",\\"reason\\":\\"main.js:1:1: number\\"}}"
            }
          ]
        `,
        {normalize: ['json']},
      )
      .exitCodes([0]),
    flowCmd([
      'dump-types',
      '--for-tool',
      '--json',
      '--json-direct',
      '--strip-root',
      'main.js',
    ])
      .stdout(
        `
          {
            "schema": {
              "json_direct": true
            },
            "types": [
              {
                "end": 1,
                "endline": 1,
                "line": 1,
                "loc": {
                  "end": {
                    "column": 1,
                    "line": 1,
                    "offset": 1
                  },
                  "source": "main.js",
                  "start": {
                    "column": 1,
                    "line": 1,
                    "offset": 0
                  },
                  "type": "SourceFile"
                },
                "path": "main.js",
                "reasons": [],
                "start": 1,
                "type": {
                  "expression_type": {
                    "def": {
                      "kind": "NumGeneral"
                    },
                    "kind": "Def",
                    "reason": "main.js:1:1: number"
                  }
                }
              }
            ]
          }
        `,
        {normalize: ['json']},
      )
      .exitCodes([0]),
  ]),
  test('--force-schema lists the schema at its defaults', [
    addFile('main.js')
      .flowCmd([
        'dump-types',
        '--for-tool',
        '--json',
        '--force-schema',
        '--strip-root',
        'main.js',
      ])
      .stdout(
        `
          {
            "schema": {
              "json_direct": false
            },
            "types": [
              {
                "end": 1,
                "endline": 1,
                "line": 1,
                "loc": {
                  "end": {
                    "column": 1,
                    "line": 1,
                    "offset": 1
                  },
                  "source": "main.js",
                  "start": {
                    "column": 1,
                    "line": 1,
                    "offset": 0
                  },
                  "type": "SourceFile"
                },
                "path": "main.js",
                "reasons": [],
                "start": 1,
                "type": "{\\"expression_type\\":{\\"def\\":{\\"kind\\":\\"NumGeneral\\"},\\"kind\\":\\"Def\\",\\"reason\\":\\"main.js:1:1: number\\"}}"
              }
            ]
          }
        `,
        {normalize: ['json']},
      )
      .exitCodes([0]),
  ]),
  test('--force-schema does not change --json-direct output', [
    addFile('main.js')
      .flowCmd([
        'dump-types',
        '--for-tool',
        '--json',
        '--json-direct',
        '--strip-root',
        'main.js',
      ])
      .stdout(labels.write('--json-direct'), {normalize: ['json']})
      .exitCodes([0]),
    flowCmd([
      'dump-types',
      '--for-tool',
      '--json',
      '--json-direct',
      '--force-schema',
      '--strip-root',
      'main.js',
    ])
      .stdout(labels.read('--json-direct'), {normalize: ['json']})
      .exitCodes([0]),
  ]),
  test('--dedup --json-direct prints each type as JSON referring into $defs', [
    addFile('main.js')
      .flowCmd([
        'dump-types',
        '--for-tool',
        '--json',
        '--json-direct',
        '--strip-root',
        '--dedup',
        '0',
        'main.js',
      ])
      .stdout(
        `
          {
            "$defs": {
              "0": {
                "def": {
                  "kind": "NumGeneral"
                },
                "kind": "Def",
                "reason": "main.js:1:1: number"
              }
            },
            "schema": {
              "json_direct": true
            },
            "types": [
              {
                "end": 1,
                "endline": 1,
                "line": 1,
                "loc": {
                  "end": {
                    "column": 1,
                    "line": 1,
                    "offset": 1
                  },
                  "source": "main.js",
                  "start": {
                    "column": 1,
                    "line": 1,
                    "offset": 0
                  },
                  "type": "SourceFile"
                },
                "path": "main.js",
                "reasons": [],
                "start": 1,
                "type": {
                  "expression_type": {
                    "$ref": 0
                  }
                }
              }
            ]
          }
        `,
        {normalize: ['json']},
      )
      .exitCodes([0]),
  ]),
  test('--json-direct and --force-schema require --for-tool and --json', [
    addFile('main.js')
      .flowCmd(['dump-types', '--json-direct', 'main.js'])
      .stderr('dump-types: --json-direct requires --for-tool and --json')
      .exitCodes([64]),
    flowCmd(['dump-types', '--for-tool', '--json-direct', 'main.js'])
      .stderr('dump-types: --json-direct requires --json')
      .exitCodes([64]),
    flowCmd(['dump-types', '--json', '--force-schema', 'main.js'])
      .stderr('dump-types: --force-schema requires --for-tool')
      .exitCodes([64]),
  ]),
]): SuiteType);
