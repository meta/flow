/*
 * @flow
 * @format
 */

import type {SuiteType} from '../../Tester';
const {suite, test} = require('../../Tester');

module.exports = suite(
  ({
    addFile,
    addFiles,
    addCode,
    lspStartAndConnect,
    lspRequestAndWaitUntilResponse,
    lspIgnoreStatusAndCancellation,
  }) => [
    test('textDocument/signatureHelp', [
      addFiles(
        'nestedClasses.js',
        'nestedFunctions.js',
        'paramDocumentation.js',
        'tupleRestParam.js',
        'member_callee.js',
        'constructor.js',
        'typeAliasCall.js',
        'calls_monomorphic.js',
        'calls_overloaded.js',
        'calls_generic.js',
        'jsx_attr.js',
        'jsx_attr_docs.js',
      ),
      lspStartAndConnect(),
      lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
        textDocument: {uri: '<PLACEHOLDER_PROJECT_URL>/nestedClasses.js'},
        position: {line: 4, character: 2}, // `f(|class {})`
      }).verifyAllLSPMessagesInStep(
        [
          {
            method: 'textDocument/signatureHelp',
            result: {
              activeParameter: 0,
              activeSignature: 0,
              signatures: [
                {
                  label: '(x: unknown): void',
                  parameters: [
                    {
                      label: 'x: unknown',
                    },
                  ],
                },
              ],
            },
          },
        ],
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),
      lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
        textDocument: {uri: '<PLACEHOLDER_PROJECT_URL>/nestedClasses.js'},
        position: {line: 4, character: 7}, // `f(class| {})`
      }).verifyAllLSPMessagesInStep(
        [
          {
            method: 'textDocument/signatureHelp',
            result: {
              activeParameter: 0,
              activeSignature: 0,
              signatures: [
                {
                  label: '(x: unknown): void',
                  parameters: [
                    {
                      label: 'x: unknown',
                    },
                  ],
                },
              ],
            },
          },
        ],
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),
      lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
        textDocument: {uri: '<PLACEHOLDER_PROJECT_URL>/nestedClasses.js'},
        position: {line: 4, character: 9}, // `f(class {|})`
      }).verifyAllLSPMessagesInStep(
        [
          {
            method: 'textDocument/signatureHelp',
            result: null,
          },
        ],
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),
      lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
        textDocument: {uri: '<PLACEHOLDER_PROJECT_URL>/nestedFunctions.js'},
        position: {line: 6, character: 2},
      }).verifyAllLSPMessagesInStep(
        [
          {
            method: 'textDocument/signatureHelp',
            result: {
              activeParameter: 0,
              activeSignature: 0,
              signatures: [
                {
                  label: '(a: string, f: F, b: number): void',
                  parameters: [
                    {
                      label: 'a: string',
                    },
                    {
                      label: 'f: F',
                    },
                    {
                      label: 'b: number',
                    },
                  ],
                },
              ],
            },
          },
        ],
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),
      lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
        textDocument: {uri: '<PLACEHOLDER_PROJECT_URL>/nestedFunctions.js'},
        position: {line: 8, character: 2}, // inside nested function body
      }).verifyAllLSPMessagesInStep(
        [
          {
            method: 'textDocument/signatureHelp',
            result: null,
          },
        ],
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),
      lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
        textDocument: {uri: '<PLACEHOLDER_PROJECT_URL>/tupleRestParam.js'},
        position: {line: 6, character: 1},
      }).verifyAllLSPMessagesInStep(
        [
          {
            method: 'textDocument/signatureHelp',
            result: {
              activeParameter: 0,
              activeSignature: 0,
              signatures: [
                {
                  label: '(arg[0]: string, arg[1]: number): void',
                  parameters: [
                    {
                      label: 'arg[0]: string',
                    },
                    {
                      label: 'arg[1]: number',
                    },
                  ],
                },
              ],
            },
          },
        ],
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),
      lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
        textDocument: {uri: '<PLACEHOLDER_PROJECT_URL>/tupleRestParam.js'},
        position: {line: 12, character: 1},
      }).verifyAllLSPMessagesInStep(
        [
          {
            method: 'textDocument/signatureHelp',
            result: {
              activeParameter: 0,
              activeSignature: 0,
              signatures: [
                {
                  label: '(params[0]: string, params[1]: number): void',
                  parameters: [
                    {
                      label: 'params[0]: string',
                    },
                    {
                      label: 'params[1]: number',
                    },
                  ],
                },
              ],
            },
          },
        ],
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),
      lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
        textDocument: {uri: '<PLACEHOLDER_PROJECT_URL>/tupleRestParam.js'},
        position: {line: 18, character: 1},
      }).verifyAllLSPMessagesInStep(
        [
          {
            method: 'textDocument/signatureHelp',
            result: {
              activeParameter: 0,
              activeSignature: 0,
              signatures: [
                {
                  label: '(foo: string, bar: number): void',
                  parameters: [
                    {
                      label: 'foo: string',
                    },
                    {
                      label: 'bar: number',
                    },
                  ],
                },
              ],
            },
          },
        ],
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),
      lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
        textDocument: {uri: '<PLACEHOLDER_PROJECT_URL>/tupleRestParam.js'},
        position: {line: 24, character: 1},
      }).verifyAllLSPMessagesInStep(
        [
          {
            method: 'textDocument/signatureHelp',
            result: {
              activeParameter: 0,
              activeSignature: 0,
              signatures: [
                {
                  label: '(foo: string, bar: number): void',
                  parameters: [
                    {
                      label: 'foo: string',
                    },
                    {
                      label: 'bar: number',
                    },
                  ],
                },
              ],
            },
          },
        ],
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),
      lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
        textDocument: {uri: '<PLACEHOLDER_PROJECT_URL>/tupleRestParam.js'},
        position: {line: 30, character: 1},
      }).verifyAllLSPMessagesInStep(
        [
          {
            method: 'textDocument/signatureHelp',
            result: {
              activeParameter: 0,
              activeSignature: 0,
              signatures: [
                {
                  label: '(foo: string, arg[1]: number): void',
                  parameters: [
                    {
                      label: 'foo: string',
                    },
                    {
                      label: 'arg[1]: number',
                    },
                  ],
                },
              ],
            },
          },
        ],
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),
      lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
        textDocument: {uri: '<PLACEHOLDER_PROJECT_URL>/tupleRestParam.js'},
        position: {line: 36, character: 1},
      }).verifyAllLSPMessagesInStep(
        [
          {
            method: 'textDocument/signatureHelp',
            result: {
              activeParameter: 0,
              activeSignature: 0,
              signatures: [
                {
                  label: '(foo: string, params[1]: number): void',
                  parameters: [
                    {
                      label: 'foo: string',
                    },
                    {
                      label: 'params[1]: number',
                    },
                  ],
                },
              ],
            },
          },
        ],
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),
      lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/paramDocumentation.js',
        },
        position: {line: 12, character: 5},
      }).verifyAllLSPMessagesInStep(
        [
          {
            method: 'textDocument/signatureHelp',
            result: {
              activeParameter: 0,
              activeSignature: 0,
              signatures: [
                {
                  documentation: {
                    kind: 'markdown',
                    value:
                      '\n foo\n\n**@unrecognized**  this tag is unrecognized',
                  },
                  label: '(bar: string, baz: number): void',
                  parameters: [
                    {
                      documentation: {
                        kind: 'markdown',
                        value: 'bar -  the first summand',
                      },
                      label: 'bar: string',
                    },
                    {
                      documentation: {
                        kind: 'markdown',
                        value:
                          'baz -  the second and third summands\nbaz.x (optional)  -  the second summand\nbaz.y (optional, defaults to 0)  -  the third summand',
                      },
                      label: 'baz: number',
                    },
                  ],
                },
              ],
            },
          },
        ],
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),
      lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/member_callee.js',
        },
        position: {line: 17, character: 3},
      }).verifyAllLSPMessagesInStep(
        [
          {
            method: 'textDocument/signatureHelp',
            result: {
              activeParameter: 0,
              activeSignature: 0,
              signatures: [
                {
                  documentation: {
                    kind: 'markdown',
                    value: '\n foo',
                  },
                  label: '(x: string): void',
                  parameters: [
                    {
                      documentation: {
                        kind: 'markdown',
                        value: 'x - a string',
                      },
                      label: 'x: string',
                    },
                  ],
                },
              ],
            },
          },
        ],
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),
      lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/member_callee.js',
        },
        position: {line: 21, character: 3},
      }).verifyAllLSPMessagesInStep(
        [
          {
            method: 'textDocument/signatureHelp',
            result: {
              activeParameter: 0,
              activeSignature: 0,
              signatures: [
                {
                  documentation: {
                    kind: 'markdown',
                    value: '\n bar',
                  },
                  label: '(y: number): void',
                  parameters: [
                    {
                      documentation: {
                        kind: 'markdown',
                        value: 'y - a number',
                      },
                      label: 'y: number',
                    },
                  ],
                },
              ],
            },
          },
        ],
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),
      lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/member_callee.js',
        },
        position: {line: 33, character: 3},
      }).verifyAllLSPMessagesInStep(
        [
          {
            method: 'textDocument/signatureHelp',
            result: {
              activeParameter: 0,
              activeSignature: 0,
              signatures: [
                {
                  documentation: {
                    kind: 'markdown',
                    value: '\n baz',
                  },
                  label: '(z: bigint): void',
                  parameters: [
                    {
                      documentation: {
                        kind: 'markdown',
                        value: 'z - a bigint',
                      },
                      label: 'z: bigint',
                    },
                  ],
                },
              ],
            },
          },
        ],
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),
      lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/constructor.js',
        },
        position: {line: 12, character: 3},
      }).verifyAllLSPMessagesInStep(
        [
          {
            method: 'textDocument/signatureHelp',
            result: {
              activeParameter: 0,
              activeSignature: 0,
              signatures: [
                {
                  documentation: {
                    kind: 'markdown',
                    value: '\n constructor docs',
                  },
                  label: '(x: string, y: number): void',
                  parameters: [
                    {
                      documentation: {
                        kind: 'markdown',
                        value: 'x - is an x',
                      },
                      label: 'x: string',
                    },
                    {
                      documentation: {
                        kind: 'markdown',
                        value: 'y - does a y',
                      },
                      label: 'y: number',
                    },
                  ],
                },
              ],
            },
          },
        ],
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),
      lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/typeAliasCall.js',
        },
        position: {line: 14, character: 3},
      }).verifyAllLSPMessagesInStep(
        [
          {
            method: 'textDocument/signatureHelp',
            result: {
              activeParameter: 0,
              activeSignature: 0,
              signatures: [
                {
                  documentation: {
                    kind: 'markdown',
                    value: '\n variable declaration',
                  },
                  label: '(x: string): void',
                  parameters: [
                    {
                      documentation: {
                        kind: 'markdown',
                        value: 'x - is a string',
                      },
                      label: 'x: string',
                    },
                  ],
                },
              ],
            },
          },
        ],
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),
      lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/calls_monomorphic.js',
        },
        position: {line: 5, character: 4},
      }).verifyAllLSPMessagesInStep(
        [
          {
            method: 'textDocument/signatureHelp',
            result: {
              activeParameter: 0,
              activeSignature: 0,
              signatures: [
                {
                  label: '(x: string): void',
                  parameters: [
                    {
                      label: 'x: string',
                    },
                  ],
                },
              ],
            },
          },
        ],
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),
      lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/calls_monomorphic.js',
        },
        position: {line: 9, character: 6},
      }).verifyAllLSPMessagesInStep(
        [
          {
            method: 'textDocument/signatureHelp',
            result: {
              activeParameter: 0,
              activeSignature: 0,
              signatures: [
                {
                  label: '(x: string): void',
                  parameters: [
                    {
                      label: 'x: string',
                    },
                  ],
                },
              ],
            },
          },
        ],
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),
      lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/calls_monomorphic.js',
        },
        position: {line: 17, character: 13},
      }).verifyAllLSPMessagesInStep(
        [
          {
            method: 'textDocument/signatureHelp',
            result: {
              activeParameter: 0,
              activeSignature: 0,
              signatures: [
                {
                  label: '(x: string): void',
                  parameters: [
                    {
                      label: 'x: string',
                    },
                  ],
                },
              ],
            },
          },
        ],
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),
      lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/calls_monomorphic.js',
        },
        position: {line: 21, character: 9},
      }).verifyAllLSPMessagesInStep(
        [
          {
            method: 'textDocument/signatureHelp',
            result: {
              activeParameter: 0,
              activeSignature: 0,
              signatures: [
                {
                  label: '(x: string): void',
                  parameters: [
                    {
                      label: 'x: string',
                    },
                  ],
                },
              ],
            },
          },
        ],
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),
      lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/calls_monomorphic.js',
        },
        position: {line: 25, character: 8},
      }).verifyAllLSPMessagesInStep(
        [
          {
            method: 'textDocument/signatureHelp',
            result: {
              activeParameter: 0,
              activeSignature: 0,
              signatures: [
                {
                  label: '(x: string): void',
                  parameters: [
                    {
                      label: 'x: string',
                    },
                  ],
                },
              ],
            },
          },
        ],
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),
      lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/calls_monomorphic.js',
        },
        position: {line: 30, character: 8},
      }).verifyAllLSPMessagesInStep(
        [
          {
            method: 'textDocument/signatureHelp',
            result: {
              activeParameter: 0,
              activeSignature: 0,
              signatures: [
                {
                  label: '(x: string): void',
                  parameters: [
                    {
                      label: 'x: string',
                    },
                  ],
                },
              ],
            },
          },
        ],
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),
      lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/calls_monomorphic.js',
        },
        position: {line: 41, character: 6},
      }).verifyAllLSPMessagesInStep(
        [
          {
            method: 'textDocument/signatureHelp',
            result: {
              activeParameter: 0,
              activeSignature: 0,
              signatures: [
                {
                  label: '(x: 1, y: 2): 1',
                  parameters: [
                    {
                      documentation: {
                        kind: 'markdown',
                        value: 'x -  1st param',
                      },
                      label: 'x: 1',
                    },
                    {
                      documentation: {
                        kind: 'markdown',
                        value: 'y -  2nd param',
                      },
                      label: 'y: 2',
                    },
                  ],
                },
              ],
            },
          },
        ],
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),
      lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/calls_monomorphic.js',
        },
        position: {line: 42, character: 8},
      }).verifyAllLSPMessagesInStep(
        [
          {
            method: 'textDocument/signatureHelp',
            result: {
              activeParameter: 1,
              activeSignature: 0,
              signatures: [
                {
                  label: '(x: 1, y: 2): 1',
                  parameters: [
                    {
                      documentation: {
                        kind: 'markdown',
                        value: 'x -  1st param',
                      },
                      label: 'x: 1',
                    },
                    {
                      documentation: {
                        kind: 'markdown',
                        value: 'y -  2nd param',
                      },
                      label: 'y: 2',
                    },
                  ],
                },
              ],
            },
          },
        ],
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),
      lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/calls_overloaded.js',
        },
        position: {line: 8, character: 4},
      }).verifyAllLSPMessagesInStep(
        [
          {
            method: 'textDocument/signatureHelp',
            result: {
              activeParameter: 0,
              activeSignature: 0,
              signatures: [
                {
                  label: '(): void',
                  parameters: [],
                },
                {
                  label: '(x: string): void',
                  parameters: [
                    {
                      label: 'x: string',
                    },
                  ],
                },
                {
                  label: '(x: A, y: A): void',
                  parameters: [
                    {
                      label: 'x: A',
                    },
                    {
                      label: 'y: A',
                    },
                  ],
                },
              ],
            },
          },
        ],
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),
      lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/calls_overloaded.js',
        },
        position: {line: 12, character: 6},
      }).verifyAllLSPMessagesInStep(
        [
          {
            method: 'textDocument/signatureHelp',
            result: {
              activeParameter: 0,
              activeSignature: 0,
              signatures: [
                {
                  label: '(): void',
                  parameters: [],
                },
                {
                  label: '(x: string): void',
                  parameters: [
                    {
                      label: 'x: string',
                    },
                  ],
                },
                {
                  label: '(x: A, y: A): void',
                  parameters: [
                    {
                      label: 'x: A',
                    },
                    {
                      label: 'y: A',
                    },
                  ],
                },
              ],
            },
          },
        ],
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),
      lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/calls_overloaded.js',
        },
        position: {line: 16, character: 7},
      }).verifyAllLSPMessagesInStep(
        [
          {
            method: 'textDocument/signatureHelp',
            result: {
              activeParameter: 0,
              activeSignature: 0,
              signatures: [
                {
                  label: '(): void',
                  parameters: [],
                },
                {
                  label: '(x: string): void',
                  parameters: [
                    {
                      label: 'x: string',
                    },
                  ],
                },
                {
                  label: '(x: A, y: A): void',
                  parameters: [
                    {
                      label: 'x: A',
                    },
                    {
                      label: 'y: A',
                    },
                  ],
                },
              ],
            },
          },
        ],
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),
      lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/calls_overloaded.js',
        },
        position: {line: 20, character: 13},
      }).verifyAllLSPMessagesInStep(
        [
          {
            method: 'textDocument/signatureHelp',
            result: {
              activeParameter: 0,
              activeSignature: 0,
              signatures: [
                {
                  label: '(): void',
                  parameters: [],
                },
                {
                  label: '(x: string): void',
                  parameters: [
                    {
                      label: 'x: string',
                    },
                  ],
                },
                {
                  label: '(x: A, y: A): void',
                  parameters: [
                    {
                      label: 'x: A',
                    },
                    {
                      label: 'y: A',
                    },
                  ],
                },
              ],
            },
          },
        ],
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),
      lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/calls_overloaded.js',
        },
        position: {line: 24, character: 9},
      }).verifyAllLSPMessagesInStep(
        [
          {
            method: 'textDocument/signatureHelp',
            result: {
              activeParameter: 0,
              activeSignature: 0,
              signatures: [
                {
                  label: '(): void',
                  parameters: [],
                },
                {
                  label: '(x: string): void',
                  parameters: [
                    {
                      label: 'x: string',
                    },
                  ],
                },
                {
                  label: '(x: A, y: A): void',
                  parameters: [
                    {
                      label: 'x: A',
                    },
                    {
                      label: 'y: A',
                    },
                  ],
                },
              ],
            },
          },
        ],
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),
      lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/calls_overloaded.js',
        },
        position: {line: 28, character: 8},
      }).verifyAllLSPMessagesInStep(
        [
          {
            method: 'textDocument/signatureHelp',
            result: {
              activeParameter: 0,
              activeSignature: 0,
              signatures: [
                {
                  label: '(): void',
                  parameters: [],
                },
                {
                  label: '(x: string): void',
                  parameters: [
                    {
                      label: 'x: string',
                    },
                  ],
                },
                {
                  label: '(x: A, y: A): void',
                  parameters: [
                    {
                      label: 'x: A',
                    },
                    {
                      label: 'y: A',
                    },
                  ],
                },
              ],
            },
          },
        ],
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),
      lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/calls_overloaded.js',
        },
        position: {line: 33, character: 8},
      }).verifyAllLSPMessagesInStep(
        [
          {
            method: 'textDocument/signatureHelp',
            result: {
              activeParameter: 0,
              activeSignature: 0,
              signatures: [
                {
                  label: '(): void',
                  parameters: [],
                },
                {
                  label: '(x: string): void',
                  parameters: [
                    {
                      label: 'x: string',
                    },
                  ],
                },
                {
                  label: '(x: A, y: A): void',
                  parameters: [
                    {
                      label: 'x: A',
                    },
                    {
                      label: 'y: A',
                    },
                  ],
                },
              ],
            },
          },
        ],
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),
      lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/calls_overloaded.js',
        },
        position: {line: 48, character: 4},
      }).verifyAllLSPMessagesInStep(
        [
          {
            method: 'textDocument/signatureHelp',
            result: {
              activeParameter: 0,
              activeSignature: 0,
              signatures: [
                {
                  label: '(): void',
                  parameters: [],
                },
                {
                  label: '(x: string): void',
                  parameters: [
                    {
                      label: 'x: string',
                    },
                  ],
                },
                {
                  label: '(x: A, y: A): void',
                  parameters: [
                    {
                      label: 'x: A',
                    },
                    {
                      label: 'y: A',
                    },
                  ],
                },
              ],
            },
          },
        ],
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),
      lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/calls_overloaded.js',
        },
        position: {line: 52, character: 6},
      }).verifyAllLSPMessagesInStep(
        [
          {
            method: 'textDocument/signatureHelp',
            result: {
              activeParameter: 0,
              activeSignature: 0,
              signatures: [
                {
                  label: '(): void',
                  parameters: [],
                },
                {
                  label: '(x: string): void',
                  parameters: [
                    {
                      label: 'x: string',
                    },
                  ],
                },
                {
                  label: '(x: A, y: A): void',
                  parameters: [
                    {
                      label: 'x: A',
                    },
                    {
                      label: 'y: A',
                    },
                  ],
                },
              ],
            },
          },
        ],
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),
      lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/calls_overloaded.js',
        },
        position: {line: 56, character: 7},
      }).verifyAllLSPMessagesInStep(
        [
          {
            method: 'textDocument/signatureHelp',
            result: {
              activeParameter: 0,
              activeSignature: 0,
              signatures: [
                {
                  label: '(): void',
                  parameters: [],
                },
                {
                  label: '(x: string): void',
                  parameters: [
                    {
                      label: 'x: string',
                    },
                  ],
                },
                {
                  label: '(x: A, y: A): void',
                  parameters: [
                    {
                      label: 'x: A',
                    },
                    {
                      label: 'y: A',
                    },
                  ],
                },
              ],
            },
          },
        ],
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),
      lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/calls_overloaded.js',
        },
        position: {line: 60, character: 13},
      }).verifyAllLSPMessagesInStep(
        [
          {
            method: 'textDocument/signatureHelp',
            result: {
              activeParameter: 0,
              activeSignature: 0,
              signatures: [
                {
                  label: '(): void',
                  parameters: [],
                },
                {
                  label: '(x: string): void',
                  parameters: [
                    {
                      label: 'x: string',
                    },
                  ],
                },
                {
                  label: '(x: A, y: A): void',
                  parameters: [
                    {
                      label: 'x: A',
                    },
                    {
                      label: 'y: A',
                    },
                  ],
                },
              ],
            },
          },
        ],
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),
      lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/calls_overloaded.js',
        },
        position: {line: 64, character: 9},
      }).verifyAllLSPMessagesInStep(
        [
          {
            method: 'textDocument/signatureHelp',
            result: {
              activeParameter: 0,
              activeSignature: 0,
              signatures: [
                {
                  label: '(): void',
                  parameters: [],
                },
                {
                  label: '(x: string): void',
                  parameters: [
                    {
                      label: 'x: string',
                    },
                  ],
                },
                {
                  label: '(x: A, y: A): void',
                  parameters: [
                    {
                      label: 'x: A',
                    },
                    {
                      label: 'y: A',
                    },
                  ],
                },
              ],
            },
          },
        ],
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),
      lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/calls_overloaded.js',
        },
        position: {line: 68, character: 8},
      }).verifyAllLSPMessagesInStep(
        [
          {
            method: 'textDocument/signatureHelp',
            result: {
              activeParameter: 0,
              activeSignature: 0,
              signatures: [
                {
                  label: '(): void',
                  parameters: [],
                },
                {
                  label: '(x: string): void',
                  parameters: [
                    {
                      label: 'x: string',
                    },
                  ],
                },
                {
                  label: '(x: A, y: A): void',
                  parameters: [
                    {
                      label: 'x: A',
                    },
                    {
                      label: 'y: A',
                    },
                  ],
                },
              ],
            },
          },
        ],
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),
      lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/calls_overloaded.js',
        },
        position: {line: 73, character: 8},
      }).verifyAllLSPMessagesInStep(
        [
          {
            method: 'textDocument/signatureHelp',
            result: {
              activeParameter: 0,
              activeSignature: 0,
              signatures: [
                {
                  label: '(): void',
                  parameters: [],
                },
                {
                  label: '(x: string): void',
                  parameters: [
                    {
                      label: 'x: string',
                    },
                  ],
                },
                {
                  label: '(x: A, y: A): void',
                  parameters: [
                    {
                      label: 'x: A',
                    },
                    {
                      label: 'y: A',
                    },
                  ],
                },
              ],
            },
          },
        ],
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),
      lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/calls_overloaded.js',
        },
        position: {line: 80, character: 4},
      }).verifyAllLSPMessagesInStep(
        [
          {
            method: 'textDocument/signatureHelp',
            result: {
              activeParameter: 0,
              activeSignature: 0,
              signatures: [
                {
                  label: '(): void',
                  parameters: [],
                },
                {
                  label: '(x: string): void',
                  parameters: [
                    {
                      label: 'x: string',
                    },
                  ],
                },
                {
                  label: '(x: A, y: A): void',
                  parameters: [
                    {
                      label: 'x: A',
                    },
                    {
                      label: 'y: A',
                    },
                  ],
                },
              ],
            },
          },
        ],
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),
      lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/calls_overloaded.js',
        },
        position: {line: 81, character: 4},
      }).verifyAllLSPMessagesInStep(
        [
          {
            method: 'textDocument/signatureHelp',
            result: {
              activeParameter: 0,
              activeSignature: 0,
              signatures: [
                {
                  label: '(): void',
                  parameters: [],
                },
                {
                  label: '(x: string): void',
                  parameters: [
                    {
                      label: 'x: string',
                    },
                  ],
                },
                {
                  label: '(x: A, y: A): void',
                  parameters: [
                    {
                      label: 'x: A',
                    },
                    {
                      label: 'y: A',
                    },
                  ],
                },
              ],
            },
          },
        ],
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),
      lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/calls_overloaded.js',
        },
        position: {line: 89, character: 4},
      }).verifyAllLSPMessagesInStep(
        [
          {
            method: 'textDocument/signatureHelp',
            result: {
              activeParameter: 0,
              activeSignature: 0,
              signatures: [
                {
                  label: '(x: 1): 1',
                  parameters: [
                    {
                      label: 'x: 1',
                    },
                  ],
                },
                {
                  label: '(x: 2): 2',
                  parameters: [
                    {
                      label: 'x: 2',
                    },
                  ],
                },
              ],
            },
          },
        ],
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),
      lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/calls_overloaded.js',
        },
        position: {line: 99, character: 7},
      }).verifyAllLSPMessagesInStep(
        [
          {
            method: 'textDocument/signatureHelp',
            result: {
              activeParameter: 0,
              activeSignature: 0,
              signatures: [
                {
                  label: '(x: 1): 1',
                  parameters: [
                    {
                      label: 'x: 1',
                    },
                  ],
                },
                {
                  label: '(x: 2): 2',
                  parameters: [
                    {
                      label: 'x: 2',
                    },
                  ],
                },
              ],
            },
          },
        ],
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),
      lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/calls_overloaded.js',
        },
        position: {line: 103, character: 7},
      }).verifyAllLSPMessagesInStep(
        [
          {
            method: 'textDocument/signatureHelp',
            result: {
              activeParameter: 0,
              activeSignature: 0,
              signatures: [
                {
                  label: '(x: 1): 1',
                  parameters: [
                    {
                      label: 'x: 1',
                    },
                  ],
                },
                {
                  label: '(x: 2): 2',
                  parameters: [
                    {
                      label: 'x: 2',
                    },
                  ],
                },
              ],
            },
          },
        ],
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),
      lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/calls_overloaded.js',
        },
        position: {line: 114, character: 7},
      }).verifyAllLSPMessagesInStep(
        [
          {
            method: 'textDocument/signatureHelp',
            result: {
              activeParameter: 0,
              activeSignature: 0,
              signatures: [
                {
                  label: '(x: 1): void',
                  parameters: [
                    {
                      label: 'x: 1',
                    },
                  ],
                },
                {
                  label: '(x: 2): void',
                  parameters: [
                    {
                      label: 'x: 2',
                    },
                  ],
                },
              ],
            },
          },
        ],
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),
      lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/calls_overloaded.js',
        },
        position: {line: 123, character: 7},
      }).verifyAllLSPMessagesInStep(
        [
          {
            method: 'textDocument/signatureHelp',
            result: {
              activeParameter: 0,
              activeSignature: 0,
              signatures: [
                {
                  label: '(x: X): void',
                  parameters: [
                    {
                      label: 'x: X',
                    },
                  ],
                },
                {
                  label: '(x: X): void',
                  parameters: [
                    {
                      label: 'x: X',
                    },
                  ],
                },
              ],
            },
          },
        ],
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),
      lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/calls_overloaded.js',
        },
        position: {line: 132, character: 7},
      }).verifyAllLSPMessagesInStep(
        [
          {
            method: 'textDocument/signatureHelp',
            result: {
              activeParameter: 0,
              activeSignature: 0,
              signatures: [
                {
                  label: '(x: 1): void',
                  parameters: [
                    {
                      label: 'x: 1',
                    },
                  ],
                },
                {
                  label: '(x: 2): void',
                  parameters: [
                    {
                      label: 'x: 2',
                    },
                  ],
                },
              ],
            },
          },
        ],
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),
      lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/calls_overloaded.js',
        },
        position: {line: 152, character: 7},
      }).verifyAllLSPMessagesInStep(
        [
          {
            method: 'textDocument/signatureHelp',
            result: {
              activeParameter: 0,
              activeSignature: 0,
              signatures: [
                {
                  documentation: {
                    kind: 'markdown',
                    value: '\n first overload',
                  },
                  label: '(x: 10, y: 11): 1',
                  parameters: [
                    {
                      documentation: {
                        kind: 'markdown',
                        value: 'x -  1st param of first overload',
                      },
                      label: 'x: 10',
                    },
                    {
                      documentation: {
                        kind: 'markdown',
                        value: 'y -  2nd param of first overload',
                      },
                      label: 'y: 11',
                    },
                  ],
                },
                {
                  documentation: {
                    kind: 'markdown',
                    value: '\n second overload',
                  },
                  label: '(x: 20, y: 21): 2',
                  parameters: [
                    {
                      documentation: {
                        kind: 'markdown',
                        value: 'x -  1st param of second overload',
                      },
                      label: 'x: 20',
                    },
                    {
                      documentation: {
                        kind: 'markdown',
                        value: 'y -  2nd param of second overload',
                      },
                      label: 'y: 21',
                    },
                  ],
                },
              ],
            },
          },
        ],
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),
      lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/calls_overloaded.js',
        },
        position: {line: 153, character: 9},
      }).verifyAllLSPMessagesInStep(
        [
          {
            method: 'textDocument/signatureHelp',
            result: {
              activeParameter: 1,
              activeSignature: 0,
              signatures: [
                {
                  documentation: {
                    kind: 'markdown',
                    value: '\n first overload',
                  },
                  label: '(x: 10, y: 11): 1',
                  parameters: [
                    {
                      documentation: {
                        kind: 'markdown',
                        value: 'x -  1st param of first overload',
                      },
                      label: 'x: 10',
                    },
                    {
                      documentation: {
                        kind: 'markdown',
                        value: 'y -  2nd param of first overload',
                      },
                      label: 'y: 11',
                    },
                  ],
                },
                {
                  documentation: {
                    kind: 'markdown',
                    value: '\n second overload',
                  },
                  label: '(x: 20, y: 21): 2',
                  parameters: [
                    {
                      documentation: {
                        kind: 'markdown',
                        value: 'x -  1st param of second overload',
                      },
                      label: 'x: 20',
                    },
                    {
                      documentation: {
                        kind: 'markdown',
                        value: 'y -  2nd param of second overload',
                      },
                      label: 'y: 21',
                    },
                  ],
                },
              ],
            },
          },
        ],
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),
      lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/calls_overloaded.js',
        },
        position: {line: 154, character: 9},
      }).verifyAllLSPMessagesInStep(
        [
          {
            method: 'textDocument/signatureHelp',
            result: {
              activeParameter: 1,
              activeSignature: 0,
              signatures: [
                {
                  documentation: {
                    kind: 'markdown',
                    value: '\n first overload',
                  },
                  label: '(x: 10, y: 11): 1',
                  parameters: [
                    {
                      documentation: {
                        kind: 'markdown',
                        value: 'x -  1st param of first overload',
                      },
                      label: 'x: 10',
                    },
                    {
                      documentation: {
                        kind: 'markdown',
                        value: 'y -  2nd param of first overload',
                      },
                      label: 'y: 11',
                    },
                  ],
                },
                {
                  documentation: {
                    kind: 'markdown',
                    value: '\n second overload',
                  },
                  label: '(x: 20, y: 21): 2',
                  parameters: [
                    {
                      documentation: {
                        kind: 'markdown',
                        value: 'x -  1st param of second overload',
                      },
                      label: 'x: 20',
                    },
                    {
                      documentation: {
                        kind: 'markdown',
                        value: 'y -  2nd param of second overload',
                      },
                      label: 'y: 21',
                    },
                  ],
                },
              ],
            },
          },
        ],
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),
      lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/calls_generic.js',
        },
        position: {line: 5, character: 4},
      }).verifyAllLSPMessagesInStep(
        [
          {
            method: 'textDocument/signatureHelp',
            result: {
              activeParameter: 0,
              activeSignature: 0,
              signatures: [
                {
                  label: '(x: void, y: void, z: Array<void>): void',
                  parameters: [
                    {
                      label: 'x: void',
                    },
                    {
                      label: 'y: void',
                    },
                    {
                      label: 'z: Array<void>',
                    },
                  ],
                },
              ],
            },
          },
        ],
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),
      lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/calls_generic.js',
        },
        position: {line: 6, character: 7},
      }).verifyAllLSPMessagesInStep(
        [
          {
            method: 'textDocument/signatureHelp',
            result: {
              activeParameter: 1,
              activeSignature: 0,
              signatures: [
                {
                  label:
                    '(x: void | number, y: void | number, z: Array<void | number>): void',
                  parameters: [
                    {
                      label: 'x: void | number',
                    },
                    {
                      label: 'y: void | number',
                    },
                    {
                      label: 'z: Array<void | number>',
                    },
                  ],
                },
              ],
            },
          },
        ],
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),
      lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/calls_generic.js',
        },
        position: {line: 7, character: 11},
      }).verifyAllLSPMessagesInStep(
        [
          {
            method: 'textDocument/signatureHelp',
            result: {
              activeParameter: 2,
              activeSignature: 0,
              signatures: [
                {
                  label:
                    '(x: string | number, y: string | number, z: Array<string | number>): void',
                  parameters: [
                    {
                      label: 'x: string | number',
                    },
                    {
                      label: 'y: string | number',
                    },
                    {
                      label: 'z: Array<string | number>',
                    },
                  ],
                },
              ],
            },
          },
        ],
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),
      lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/calls_generic.js',
        },
        position: {line: 11, character: 6},
      }).verifyAllLSPMessagesInStep(
        [
          {
            method: 'textDocument/signatureHelp',
            result: {
              activeParameter: 0,
              activeSignature: 0,
              signatures: [
                {
                  label: '(x: void, y: void, z: Array<void>): void',
                  parameters: [
                    {
                      label: 'x: void',
                    },
                    {
                      label: 'y: void',
                    },
                    {
                      label: 'z: Array<void>',
                    },
                  ],
                },
              ],
            },
          },
        ],
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),
      lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/calls_generic.js',
        },
        position: {line: 12, character: 9},
      }).verifyAllLSPMessagesInStep(
        [
          {
            method: 'textDocument/signatureHelp',
            result: {
              activeParameter: 1,
              activeSignature: 0,
              signatures: [
                {
                  label:
                    '(x: void | number, y: void | number, z: Array<void | number>): void',
                  parameters: [
                    {
                      label: 'x: void | number',
                    },
                    {
                      label: 'y: void | number',
                    },
                    {
                      label: 'z: Array<void | number>',
                    },
                  ],
                },
              ],
            },
          },
        ],
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),
      lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/calls_generic.js',
        },
        position: {line: 13, character: 13},
      }).verifyAllLSPMessagesInStep(
        [
          {
            method: 'textDocument/signatureHelp',
            result: {
              activeParameter: 2,
              activeSignature: 0,
              signatures: [
                {
                  label:
                    '(x: string | number, y: string | number, z: Array<string | number>): void',
                  parameters: [
                    {
                      label: 'x: string | number',
                    },
                    {
                      label: 'y: string | number',
                    },
                    {
                      label: 'z: Array<string | number>',
                    },
                  ],
                },
              ],
            },
          },
        ],
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),

      lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/calls_generic.js',
        },
        position: {line: 17, character: 7},
      }).verifyAllLSPMessagesInStep(
        [
          {
            method: 'textDocument/signatureHelp',
            result: {
              activeParameter: 0,
              activeSignature: 0,
              signatures: [
                {
                  label: '(x: void, y: void, z: Array<void>): void',
                  parameters: [
                    {
                      label: 'x: void',
                    },
                    {
                      label: 'y: void',
                    },
                    {
                      label: 'z: Array<void>',
                    },
                  ],
                },
              ],
            },
          },
        ],
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),
      lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/calls_generic.js',
        },
        position: {line: 18, character: 10},
      }).verifyAllLSPMessagesInStep(
        [
          {
            method: 'textDocument/signatureHelp',
            result: {
              activeParameter: 1,
              activeSignature: 0,
              signatures: [
                {
                  label:
                    '(x: void | number, y: void | number, z: Array<void | number>): void',
                  parameters: [
                    {
                      label: 'x: void | number',
                    },
                    {
                      label: 'y: void | number',
                    },
                    {
                      label: 'z: Array<void | number>',
                    },
                  ],
                },
              ],
            },
          },
        ],
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),
      lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/calls_generic.js',
        },
        position: {line: 19, character: 14},
      }).verifyAllLSPMessagesInStep(
        [
          {
            method: 'textDocument/signatureHelp',
            result: {
              activeParameter: 2,
              activeSignature: 0,
              signatures: [
                {
                  label:
                    '(x: string | number, y: string | number, z: Array<string | number>): void',
                  parameters: [
                    {
                      label: 'x: string | number',
                    },
                    {
                      label: 'y: string | number',
                    },
                    {
                      label: 'z: Array<string | number>',
                    },
                  ],
                },
              ],
            },
          },
        ],
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),

      lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/calls_generic.js',
        },
        position: {line: 23, character: 13},
      }).verifyAllLSPMessagesInStep(
        [
          {
            method: 'textDocument/signatureHelp',
            result: {
              activeParameter: 0,
              activeSignature: 0,
              signatures: [
                {
                  label: '(x: void, y: void, z: Array<void>): void',
                  parameters: [
                    {
                      label: 'x: void',
                    },
                    {
                      label: 'y: void',
                    },
                    {
                      label: 'z: Array<void>',
                    },
                  ],
                },
              ],
            },
          },
        ],
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),
      lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/calls_generic.js',
        },
        position: {line: 24, character: 16},
      }).verifyAllLSPMessagesInStep(
        [
          {
            method: 'textDocument/signatureHelp',
            result: {
              activeParameter: 1,
              activeSignature: 0,
              signatures: [
                {
                  label:
                    '(x: void | number, y: void | number, z: Array<void | number>): void',
                  parameters: [
                    {
                      label: 'x: void | number',
                    },
                    {
                      label: 'y: void | number',
                    },
                    {
                      label: 'z: Array<void | number>',
                    },
                  ],
                },
              ],
            },
          },
        ],
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),
      lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/calls_generic.js',
        },
        position: {line: 25, character: 20},
      }).verifyAllLSPMessagesInStep(
        [
          {
            method: 'textDocument/signatureHelp',
            result: {
              activeParameter: 2,
              activeSignature: 0,
              signatures: [
                {
                  label:
                    '(x: string | number, y: string | number, z: Array<string | number>): void',
                  parameters: [
                    {
                      label: 'x: string | number',
                    },
                    {
                      label: 'y: string | number',
                    },
                    {
                      label: 'z: Array<string | number>',
                    },
                  ],
                },
              ],
            },
          },
        ],
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),

      lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/calls_generic.js',
        },
        position: {line: 29, character: 9},
      }).verifyAllLSPMessagesInStep(
        [
          {
            method: 'textDocument/signatureHelp',
            result: {
              activeParameter: 0,
              activeSignature: 0,
              signatures: [
                {
                  label: '(x: void, y: void, z: Array<void>): void',
                  parameters: [
                    {
                      label: 'x: void',
                    },
                    {
                      label: 'y: void',
                    },
                    {
                      label: 'z: Array<void>',
                    },
                  ],
                },
              ],
            },
          },
        ],
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),
      lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/calls_generic.js',
        },
        position: {line: 30, character: 12},
      }).verifyAllLSPMessagesInStep(
        [
          {
            method: 'textDocument/signatureHelp',
            result: {
              activeParameter: 1,
              activeSignature: 0,
              signatures: [
                {
                  label:
                    '(x: void | number, y: void | number, z: Array<void | number>): void',
                  parameters: [
                    {
                      label: 'x: void | number',
                    },
                    {
                      label: 'y: void | number',
                    },
                    {
                      label: 'z: Array<void | number>',
                    },
                  ],
                },
              ],
            },
          },
        ],
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),
      lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/calls_generic.js',
        },
        position: {line: 31, character: 16},
      }).verifyAllLSPMessagesInStep(
        [
          {
            method: 'textDocument/signatureHelp',
            result: {
              activeParameter: 2,
              activeSignature: 0,
              signatures: [
                {
                  label:
                    '(x: string | number, y: string | number, z: Array<string | number>): void',
                  parameters: [
                    {
                      label: 'x: string | number',
                    },
                    {
                      label: 'y: string | number',
                    },
                    {
                      label: 'z: Array<string | number>',
                    },
                  ],
                },
              ],
            },
          },
        ],
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),

      lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/calls_generic.js',
        },
        position: {line: 35, character: 8},
      }).verifyAllLSPMessagesInStep(
        [
          {
            method: 'textDocument/signatureHelp',
            result: {
              activeParameter: 0,
              activeSignature: 0,
              signatures: [
                {
                  label: '(x: void, y: void, z: Array<void>): void',
                  parameters: [
                    {
                      label: 'x: void',
                    },
                    {
                      label: 'y: void',
                    },
                    {
                      label: 'z: Array<void>',
                    },
                  ],
                },
              ],
            },
          },
        ],
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),
      lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/calls_generic.js',
        },
        position: {line: 36, character: 11},
      }).verifyAllLSPMessagesInStep(
        [
          {
            method: 'textDocument/signatureHelp',
            result: {
              activeParameter: 1,
              activeSignature: 0,
              signatures: [
                {
                  label:
                    '(x: void | number, y: void | number, z: Array<void | number>): void',
                  parameters: [
                    {
                      label: 'x: void | number',
                    },
                    {
                      label: 'y: void | number',
                    },
                    {
                      label: 'z: Array<void | number>',
                    },
                  ],
                },
              ],
            },
          },
        ],
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),
      lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/calls_generic.js',
        },
        position: {line: 37, character: 15},
      }).verifyAllLSPMessagesInStep(
        [
          {
            method: 'textDocument/signatureHelp',
            result: {
              activeParameter: 2,
              activeSignature: 0,
              signatures: [
                {
                  label:
                    '(x: string | number, y: string | number, z: Array<string | number>): void',
                  parameters: [
                    {
                      label: 'x: string | number',
                    },
                    {
                      label: 'y: string | number',
                    },
                    {
                      label: 'z: Array<string | number>',
                    },
                  ],
                },
              ],
            },
          },
        ],
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),

      lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/calls_generic.js',
        },
        position: {line: 42, character: 8},
      }).verifyAllLSPMessagesInStep(
        [
          {
            method: 'textDocument/signatureHelp',
            result: {
              activeParameter: 0,
              activeSignature: 0,
              signatures: [
                {
                  label: '(x: void, y: void, z: Array<void>): void',
                  parameters: [
                    {
                      label: 'x: void',
                    },
                    {
                      label: 'y: void',
                    },
                    {
                      label: 'z: Array<void>',
                    },
                  ],
                },
              ],
            },
          },
        ],
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),
      lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/calls_generic.js',
        },
        position: {line: 43, character: 11},
      }).verifyAllLSPMessagesInStep(
        [
          {
            method: 'textDocument/signatureHelp',
            result: {
              activeParameter: 1,
              activeSignature: 0,
              signatures: [
                {
                  label:
                    '(x: void | number, y: void | number, z: Array<void | number>): void',
                  parameters: [
                    {
                      label: 'x: void | number',
                    },
                    {
                      label: 'y: void | number',
                    },
                    {
                      label: 'z: Array<void | number>',
                    },
                  ],
                },
              ],
            },
          },
        ],
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),
      lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/calls_generic.js',
        },
        position: {line: 44, character: 15},
      }).verifyAllLSPMessagesInStep(
        [
          {
            method: 'textDocument/signatureHelp',
            result: {
              activeParameter: 2,
              activeSignature: 0,
              signatures: [
                {
                  label:
                    '(x: string | number, y: string | number, z: Array<string | number>): void',
                  parameters: [
                    {
                      label: 'x: string | number',
                    },
                    {
                      label: 'y: string | number',
                    },
                    {
                      label: 'z: Array<string | number>',
                    },
                  ],
                },
              ],
            },
          },
        ],
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),
      lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/jsx_attr.js',
        },
        position: {line: 16, character: 22},
      }).verifyAllLSPMessagesInStep(
        [
          {
            method: 'textDocument/signatureHelp',
            result: {
              activeParameter: 0,
              activeSignature: 0,
              signatures: [
                {
                  label: 'foo: number',
                  parameters: [
                    {
                      label: 'foo: number',
                    },
                  ],
                },
              ],
            },
          },
        ],
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),

      lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/jsx_attr.js',
        },
        position: {line: 20, character: 22},
      }).verifyAllLSPMessagesInStep(
        [
          {
            method: 'textDocument/signatureHelp',
            result: {
              activeParameter: 0,
              activeSignature: 0,
              signatures: [
                {
                  label: 'foo: number',
                  parameters: [
                    {
                      label: 'foo: number',
                    },
                  ],
                },
              ],
            },
          },
        ],
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),

      lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/jsx_attr.js',
        },
        position: {line: 24, character: 22},
      }).verifyAllLSPMessagesInStep(
        [
          {
            method: 'textDocument/signatureHelp',
            result: {
              activeParameter: 0,
              activeSignature: 0,
              signatures: [
                {
                  label: 'foo: number',
                  parameters: [
                    {
                      label: 'foo: number',
                    },
                  ],
                },
              ],
            },
          },
        ],
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),

      lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/jsx_attr.js',
        },
        position: {line: 29, character: 22},
      }).verifyAllLSPMessagesInStep(
        [
          {
            method: 'textDocument/signatureHelp',
            result: {
              activeParameter: 0,
              activeSignature: 0,
              signatures: [
                {
                  label: 'foo: number',
                  parameters: [
                    {
                      label: 'foo: number',
                    },
                  ],
                },
              ],
            },
          },
        ],
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),

      lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/jsx_attr.js',
        },
        position: {line: 33, character: 22},
      }).verifyAllLSPMessagesInStep(
        [
          {
            method: 'textDocument/signatureHelp',
            result: {
              activeParameter: 0,
              activeSignature: 0,
              signatures: [
                {
                  label: 'foo: number',
                  parameters: [
                    {
                      label: 'foo: number',
                    },
                  ],
                },
              ],
            },
          },
        ],
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),

      lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/jsx_attr.js',
        },
        position: {line: 37, character: 22},
      }).verifyAllLSPMessagesInStep(
        [
          {
            method: 'textDocument/signatureHelp',
            result: {
              activeParameter: 0,
              activeSignature: 0,
              signatures: [
                {
                  label: 'foo: P',
                  parameters: [
                    {
                      label: 'foo: P',
                    },
                  ],
                },
              ],
            },
          },
        ],
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),

      lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/jsx_attr.js',
        },
        position: {line: 41, character: 22},
      }).verifyAllLSPMessagesInStep(
        [
          {
            method: 'textDocument/signatureHelp',
            result: {
              activeParameter: 0,
              activeSignature: 0,
              signatures: [
                {
                  label: 'foo: P',
                  parameters: [
                    {
                      label: 'foo: P',
                    },
                  ],
                },
              ],
            },
          },
        ],
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),

      lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/jsx_attr.js',
        },
        position: {line: 45, character: 22},
      }).verifyAllLSPMessagesInStep(
        [
          {
            method: 'textDocument/signatureHelp',
            result: {
              activeParameter: 0,
              activeSignature: 0,
              signatures: [
                {
                  label: 'foo: number',
                  parameters: [
                    {
                      label: 'foo: number',
                    },
                  ],
                },
              ],
            },
          },
        ],
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),

      lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/jsx_attr.js',
        },
        position: {line: 49, character: 22},
      }).verifyAllLSPMessagesInStep(
        [
          {
            method: 'textDocument/signatureHelp',
            result: {
              activeParameter: 0,
              activeSignature: 0,
              signatures: [
                {
                  label: 'foo: number',
                  parameters: [
                    {
                      label: 'foo: number',
                    },
                  ],
                },
              ],
            },
          },
        ],
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),

      lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/jsx_attr.js',
        },
        position: {line: 53, character: 22},
      }).verifyAllLSPMessagesInStep(
        [
          {
            method: 'textDocument/signatureHelp',
            result: {
              activeParameter: 0,
              activeSignature: 0,
              signatures: [
                {
                  label: 'foo: number',
                  parameters: [
                    {
                      label: 'foo: number',
                    },
                  ],
                },
                {
                  label: 'foo: number',
                  parameters: [
                    {
                      label: 'foo: number',
                    },
                  ],
                },
              ],
            },
          },
        ],
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),

      lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/jsx_attr.js',
        },
        position: {line: 57, character: 22},
      }).verifyAllLSPMessagesInStep(
        [
          {
            method: 'textDocument/signatureHelp',
            result: {
              activeParameter: 0,
              activeSignature: 0,
              signatures: [
                {
                  label: 'foo: number',
                  parameters: [
                    {
                      label: 'foo: number',
                    },
                  ],
                },
                {
                  label: 'foo: boolean',
                  parameters: [
                    {
                      label: 'foo: boolean',
                    },
                  ],
                },
              ],
            },
          },
        ],
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),

      lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/jsx_attr.js',
        },
        position: {line: 61, character: 22},
      }).verifyAllLSPMessagesInStep(
        [
          {
            method: 'textDocument/signatureHelp',
            result: {
              activeParameter: 0,
              activeSignature: 0,
              signatures: [
                {
                  label: 'foo: number',
                  parameters: [
                    {
                      label: 'foo: number',
                    },
                  ],
                },
                {
                  label: 'foo?: string',
                  parameters: [
                    {
                      label: 'foo?: string',
                    },
                  ],
                },
              ],
            },
          },
        ],
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),

      lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/jsx_attr.js',
        },
        position: {line: 65, character: 22},
      }).verifyAllLSPMessagesInStep(
        [
          {
            method: 'textDocument/signatureHelp',
            result: {
              activeParameter: 0,
              activeSignature: 0,
              signatures: [
                {
                  label: 'foo: number & boolean',
                  parameters: [
                    {
                      label: 'foo: number & boolean',
                    },
                  ],
                },
              ],
            },
          },
        ],
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),

      lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/jsx_attr.js',
        },
        position: {line: 69, character: 22},
      }).verifyAllLSPMessagesInStep(
        [
          {
            method: 'textDocument/signatureHelp',
            result: {
              activeParameter: 0,
              activeSignature: 0,
              signatures: [
                {
                  label: 'foo: boolean & string',
                  parameters: [
                    {
                      label: 'foo: boolean & string',
                    },
                  ],
                },
              ],
            },
          },
        ],
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),

      lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/jsx_attr.js',
        },
        position: {line: 73, character: 22},
      }).verifyAllLSPMessagesInStep(
        [
          {
            method: 'textDocument/signatureHelp',
            result: {
              activeParameter: 0,
              activeSignature: 0,
              signatures: [
                {
                  label: 'foo: number',
                  parameters: [
                    {
                      label: 'foo: number',
                    },
                  ],
                },
                {
                  label: 'foo: boolean & string',
                  parameters: [
                    {
                      label: 'foo: boolean & string',
                    },
                  ],
                },
              ],
            },
          },
        ],
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),

      lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/jsx_attr.js',
        },
        position: {line: 77, character: 22},
      }).verifyAllLSPMessagesInStep(
        [
          {
            method: 'textDocument/signatureHelp',
            result: {
              activeParameter: 0,
              activeSignature: 0,
              signatures: [
                {
                  label: 'foo: boolean',
                  parameters: [
                    {
                      label: 'foo: boolean',
                    },
                  ],
                },
                {
                  label: 'foo?: string',
                  parameters: [
                    {
                      label: 'foo?: string',
                    },
                  ],
                },
              ],
            },
          },
        ],
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),

      lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/jsx_attr.js',
        },
        position: {line: 81, character: 22},
      }).verifyAllLSPMessagesInStep(
        [
          {
            method: 'textDocument/signatureHelp',
            result: {
              activeParameter: 0,
              activeSignature: 0,
              signatures: [
                {
                  label: 'foo: number',
                  parameters: [
                    {
                      label: 'foo: number',
                    },
                  ],
                },
              ],
            },
          },
        ],
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),

      lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/jsx_attr.js',
        },
        position: {line: 85, character: 22},
      }).verifyAllLSPMessagesInStep(
        [
          {
            method: 'textDocument/signatureHelp',
            result: {
              activeParameter: 0,
              activeSignature: 0,
              signatures: [
                {
                  label: 'foo: () => PropsA["foo"]',
                  parameters: [
                    {
                      label: 'foo: () => PropsA["foo"]',
                    },
                  ],
                },
              ],
            },
          },
        ],
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),

      lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/jsx_attr.js',
        },
        position: {line: 89, character: 22},
      }).verifyAllLSPMessagesInStep(
        [
          {
            method: 'textDocument/signatureHelp',
            result: {
              activeParameter: 0,
              activeSignature: 0,
              signatures: [
                {
                  label: 'foo: number | string',
                  parameters: [
                    {
                      label: 'foo: number | string',
                    },
                  ],
                },
              ],
            },
          },
        ],
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),

      ...[
        {line: 94, character: 36, label: 'onChange: (number) => void'},
        {line: 101, character: 30, label: 'bar?: string | void'},
      ].map(({line, character, label}) =>
        lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
          textDocument: {uri: '<PLACEHOLDER_PROJECT_URL>/jsx_attr.js'},
          position: {line, character},
        }).verifyAllLSPMessagesInStep(
          [
            {
              method: 'textDocument/signatureHelp',
              result: {
                activeParameter: 0,
                activeSignature: 0,
                signatures: [{label, parameters: [{label}]}],
              },
            },
          ],
          [
            'textDocument/publishDiagnostics',
            ...lspIgnoreStatusAndCancellation,
          ],
        ),
      ),

      lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/jsx_attr_docs.js',
        },
        position: {line: 27, character: 22},
      }).verifyAllLSPMessagesInStep(
        [
          {
            method: 'textDocument/signatureHelp',
            result: {
              activeParameter: 0,
              activeSignature: 0,
              signatures: [
                {
                  label: 'foo: number',
                  parameters: [
                    {
                      documentation: {
                        kind: 'markdown',
                        value:
                          '\n This is documentation for Props1.foo property',
                      },
                      label: 'foo: number',
                    },
                  ],
                },
              ],
            },
          },
        ],
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),
      lspRequestAndWaitUntilResponse('textDocument/signatureHelp', {
        textDocument: {
          uri: '<PLACEHOLDER_PROJECT_URL>/jsx_attr_docs.js',
        },
        position: {line: 31, character: 22},
      }).verifyAllLSPMessagesInStep(
        [
          {
            method: 'textDocument/signatureHelp',
            result: {
              activeParameter: 0,
              activeSignature: 0,
              signatures: [
                {
                  label: 'foo: number',
                  parameters: [
                    {
                      documentation: {
                        kind: 'markdown',
                        value:
                          '\n This is documentation for Props1.foo property',
                      },
                      label: 'foo: number',
                    },
                  ],
                },
                {
                  label: 'foo: string',
                  parameters: [
                    {
                      documentation: {
                        kind: 'markdown',
                        value:
                          '\n This is documentation for Props2.foo property',
                      },
                      label: 'foo: string',
                    },
                  ],
                },
              ],
            },
          },
        ],
        [
          'textDocument/publishDiagnostics',
          'window/showStatus',
          '$/cancelRequest',
        ],
      ),
    ]),
  ],
) as SuiteType;
