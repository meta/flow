/*
 * @flow
 */

import type {SuiteType} from "../Tester";
const {suite, test} = require('../Tester');

// A suite is a function. It takes an empty step and returns a list of tests.
// For convenience, it's nice to destructure the empty test, since you almost
// always want to perform some action first.
module.exports = (suite(({addFile, addFiles, addCode}) => [
  // A test is a list of steps.
  //
  // Each test starts off with an empty tmp directory with a .flowconfig and a
  // test.js, that is empty except for the @flow comment.
  test('addCode example', [
    // Each step adds code to test.js, adds files to the directory, runs flow
    // commands, etc. After performing 0 or more actions, each step can perform
    // 0 or more assertions.

    // Here is a step that adds two lines of code and then asserts that this
    // step introduced 0 flow errors
    addCode('var myNum = 123;')
      .addCode('var myStr = "hello";')
      .noNewErrors(),

    // Here we introduce a flow error but no assertion. This flow error is
    // ignored and won't affect the other assertions.
    addCode('var ignored = myNum * myStr;'),

    // And here is how you can assert that a step introduces new flow errors. I
    // often write newErrors('todo') and then run ./tool record myTest, but you
    // can copy paste errors in by hand. The test runner should be fairly
    // tolerant of whitespace changes. Below I added newlines to the beginning
    // and end of the error and indented it a bunch
    addCode('var boom = myNum * myStr;')
      .newErrors(
        `
          test.js:9
            9: var boom = myNum * myStr;
                          ^^^^^^^^^^^^^ Cannot use operator \`*\` with operands \`number\` [1] and \`string\` [2] [unsafe-arithmetic]
            References:
              3: var myNum = 123;
                             ^^^ [1]
              5: var myStr = "hello";
                             ^^^^^^^ [2]
        `,
      )
  ]),

  test('addFile(s) example', [
    // You can add single files to the test directory
    addFile('A.js')
      .addCode('import A from "./A"')
      .noNewErrors(),

    // You can add multiple files to the test directory
    addFiles('B.js', 'C.js')
      .addCode('import B from "./B"')
      .addCode('import C from "./C"')
      .noNewErrors(),

    // You can rename files when you add them
    addFile('A.js', 'D.js')
      .addCode('import D from "./D"')
      .noNewErrors(),

    // Directories are automatically created when you add files to them
    addFile('B.js', 'some/dir/E.js')
      .addCode('import E from "./some/dir/E"')
      .noNewErrors(),
  ]),

  // By default, the test runner will use the _flowconfig file as the
  // .flowconfig for each test in the suite. You can override this behavior and
  // use another file as the flowconfig.
  test('flowconfig', [
    addCode(`
      // $FlowFixMe[unclear-type] - This lint is enabled in other_flowconfig
      const x: any = {};
    `).noNewErrors(),
  ]).flowConfig('other_flowconfig'),

  // Sometimes we want the test to call the flow cli directly. This is possible
  test('flowCmd', [
    // This step adds A.js and then runs `flow ast --tokens < A.js`
    // tool record will pretty print json.
    addFile('A.js')
      .flowCmd(['ast', '--tokens'], 'A.js')
      .stdout(
        `
          {
            "body": [
              {
                "declaration": {
                  "loc": {
                    "end": {
                      "column": 18,
                      "line": 1
                    },
                    "source": null,
                    "start": {
                      "column": 15,
                      "line": 1
                    }
                  },
                  "range": [
                    15,
                    18
                  ],
                  "raw": "123",
                  "type": "Literal",
                  "value": 123
                },
                "exportKind": "value",
                "loc": {
                  "end": {
                    "column": 19,
                    "line": 1
                  },
                  "source": null,
                  "start": {
                    "column": 0,
                    "line": 1
                  }
                },
                "range": [
                  0,
                  19
                ],
                "type": "ExportDefaultDeclaration"
              }
            ],
            "comments": [],
            "errors": [],
            "loc": {
              "end": {
                "column": 0,
                "line": 2
              },
              "source": null,
              "start": {
                "column": 0,
                "line": 1
              }
            },
            "range": [
              0,
              20
            ],
            "tokens": [
              {
                "context": "normal",
                "loc": {
                  "end": {
                    "column": 6,
                    "line": 1
                  },
                  "start": {
                    "column": 0,
                    "line": 1
                  }
                },
                "range": [
                  0,
                  6
                ],
                "type": "T_EXPORT",
                "value": "export"
              },
              {
                "context": "normal",
                "loc": {
                  "end": {
                    "column": 14,
                    "line": 1
                  },
                  "start": {
                    "column": 7,
                    "line": 1
                  }
                },
                "range": [
                  7,
                  14
                ],
                "type": "T_DEFAULT",
                "value": "default"
              },
              {
                "context": "normal",
                "loc": {
                  "end": {
                    "column": 18,
                    "line": 1
                  },
                  "start": {
                    "column": 15,
                    "line": 1
                  }
                },
                "range": [
                  15,
                  18
                ],
                "type": "T_NUMBER",
                "value": "123"
              },
              {
                "context": "normal",
                "loc": {
                  "end": {
                    "column": 19,
                    "line": 1
                  },
                  "start": {
                    "column": 18,
                    "line": 1
                  }
                },
                "range": [
                  18,
                  19
                ],
                "type": "T_SEMICOLON",
                "value": ";"
              }
            ],
            "type": "Program"
          }
        `,
      ),
  ]),
]) as SuiteType);
