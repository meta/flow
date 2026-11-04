/*
 * @flow
 * @format
 */

import type LSPMessage from 'flow-dev-tools/src/test/lsp';
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
  }) => {
    let requestSelectionRange = function (
      uri: string,
      positions: Array<{line: number, character: number}>,
    ) {
      return lspRequestAndWaitUntilResponse('textDocument/selectionRange', {
        textDocument: {uri: uri},
        positions: positions,
      });
    };
    return [
      test('textDocument/selectionRange on jsx', [
        addFiles('jsx.js'),
        lspStartAndConnect(),
        requestSelectionRange(
          '<PLACEHOLDER_PROJECT_URL>/jsx.js',
          [{line: 7, character: 9}], // inside `Text`
        ).verifyAllLSPMessagesInStep(
          [
            {
              method: 'textDocument/selectionRange',
              result: [
                {
                  parent: {
                    parent: {
                      parent: {
                        parent: {
                          parent: {
                            range: {
                              end: {
                                character: 0,
                                line: 11,
                              },
                              start: {
                                character: 0,
                                line: 0,
                              },
                            },
                          },
                          range: {
                            end: {
                              character: 2,
                              line: 10,
                            },
                            start: {
                              character: 0,
                              line: 4,
                            },
                          },
                        },
                        range: {
                          end: {
                            character: 8,
                            line: 9,
                          },
                          start: {
                            character: 2,
                            line: 5,
                          },
                        },
                      },
                      range: {
                        end: {
                          character: 2,
                          line: 9,
                        },
                        start: {
                          character: 7,
                          line: 5,
                        },
                      },
                    },
                    range: {
                      end: {
                        character: 10,
                        line: 8,
                      },
                      start: {
                        character: 4,
                        line: 6,
                      },
                    },
                  },
                  range: {
                    end: {
                      character: 4,
                      line: 8,
                    },
                    start: {
                      character: 29,
                      line: 6,
                    },
                  },
                },
              ],
            },
          ],
          [
            'textDocument/publishDiagnostics',
            'window/showStatus',
            '$/cancelRequest',
          ],
        ),
        requestSelectionRange(
          '<PLACEHOLDER_PROJECT_URL>/jsx.js',
          [{line: 6, character: 11}], // inside attribute name
        ).verifyAllLSPMessagesInStep(
          [
            {
              method: 'textDocument/selectionRange',
              result: [
                {
                  parent: {
                    parent: {
                      parent: {
                        parent: {
                          parent: {
                            parent: {
                              parent: {
                                range: {
                                  end: {
                                    character: 0,
                                    line: 11,
                                  },
                                  start: {
                                    character: 0,
                                    line: 0,
                                  },
                                },
                              },
                              range: {
                                end: {
                                  character: 2,
                                  line: 10,
                                },
                                start: {
                                  character: 0,
                                  line: 4,
                                },
                              },
                            },
                            range: {
                              end: {
                                character: 8,
                                line: 9,
                              },
                              start: {
                                character: 2,
                                line: 5,
                              },
                            },
                          },
                          range: {
                            end: {
                              character: 2,
                              line: 9,
                            },
                            start: {
                              character: 7,
                              line: 5,
                            },
                          },
                        },
                        range: {
                          end: {
                            character: 10,
                            line: 8,
                          },
                          start: {
                            character: 4,
                            line: 6,
                          },
                        },
                      },
                      range: {
                        end: {
                          character: 29,
                          line: 6,
                        },
                        start: {
                          character: 4,
                          line: 6,
                        },
                      },
                    },
                    range: {
                      end: {
                        character: 18,
                        line: 6,
                      },
                      start: {
                        character: 9,
                        line: 6,
                      },
                    },
                  },
                  range: {
                    end: {
                      character: 12,
                      line: 6,
                    },
                    start: {
                      character: 9,
                      line: 6,
                    },
                  },
                },
              ],
            },
          ],
          [
            'textDocument/publishDiagnostics',
            'window/showStatus',
            '$/cancelRequest',
          ],
        ),
        requestSelectionRange(
          '<PLACEHOLDER_PROJECT_URL>/jsx.js',
          [{line: 6, character: 15}], // inside attribute literal value
        ).verifyAllLSPMessagesInStep(
          [
            {
              method: 'textDocument/selectionRange',
              result: [
                {
                  parent: {
                    parent: {
                      parent: {
                        parent: {
                          parent: {
                            parent: {
                              parent: {
                                range: {
                                  end: {
                                    character: 0,
                                    line: 11,
                                  },
                                  start: {
                                    character: 0,
                                    line: 0,
                                  },
                                },
                              },
                              range: {
                                end: {
                                  character: 2,
                                  line: 10,
                                },
                                start: {
                                  character: 0,
                                  line: 4,
                                },
                              },
                            },
                            range: {
                              end: {
                                character: 8,
                                line: 9,
                              },
                              start: {
                                character: 2,
                                line: 5,
                              },
                            },
                          },
                          range: {
                            end: {
                              character: 2,
                              line: 9,
                            },
                            start: {
                              character: 7,
                              line: 5,
                            },
                          },
                        },
                        range: {
                          end: {
                            character: 10,
                            line: 8,
                          },
                          start: {
                            character: 4,
                            line: 6,
                          },
                        },
                      },
                      range: {
                        end: {
                          character: 29,
                          line: 6,
                        },
                        start: {
                          character: 4,
                          line: 6,
                        },
                      },
                    },
                    range: {
                      end: {
                        character: 18,
                        line: 6,
                      },
                      start: {
                        character: 9,
                        line: 6,
                      },
                    },
                  },
                  range: {
                    end: {
                      character: 18,
                      line: 6,
                    },
                    start: {
                      character: 13,
                      line: 6,
                    },
                  },
                },
              ],
            },
          ],
          [
            'textDocument/publishDiagnostics',
            'window/showStatus',
            '$/cancelRequest',
          ],
        ),
        requestSelectionRange(
          '<PLACEHOLDER_PROJECT_URL>/jsx.js',
          [{line: 6, character: 25}], // inside attribute expression
        ).verifyAllLSPMessagesInStep(
          [
            {
              method: 'textDocument/selectionRange',
              result: [
                {
                  parent: {
                    parent: {
                      parent: {
                        parent: {
                          parent: {
                            parent: {
                              parent: {
                                parent: {
                                  range: {
                                    end: {
                                      character: 0,
                                      line: 11,
                                    },
                                    start: {
                                      character: 0,
                                      line: 0,
                                    },
                                  },
                                },
                                range: {
                                  end: {
                                    character: 2,
                                    line: 10,
                                  },
                                  start: {
                                    character: 0,
                                    line: 4,
                                  },
                                },
                              },
                              range: {
                                end: {
                                  character: 8,
                                  line: 9,
                                },
                                start: {
                                  character: 2,
                                  line: 5,
                                },
                              },
                            },
                            range: {
                              end: {
                                character: 2,
                                line: 9,
                              },
                              start: {
                                character: 7,
                                line: 5,
                              },
                            },
                          },
                          range: {
                            end: {
                              character: 10,
                              line: 8,
                            },
                            start: {
                              character: 4,
                              line: 6,
                            },
                          },
                        },
                        range: {
                          end: {
                            character: 29,
                            line: 6,
                          },
                          start: {
                            character: 4,
                            line: 6,
                          },
                        },
                      },
                      range: {
                        end: {
                          character: 28,
                          line: 6,
                        },
                        start: {
                          character: 19,
                          line: 6,
                        },
                      },
                    },
                    range: {
                      end: {
                        character: 28,
                        line: 6,
                      },
                      start: {
                        character: 23,
                        line: 6,
                      },
                    },
                  },
                  range: {
                    end: {
                      character: 27,
                      line: 6,
                    },
                    start: {
                      character: 24,
                      line: 6,
                    },
                  },
                },
              ],
            },
          ],
          [
            'textDocument/publishDiagnostics',
            'window/showStatus',
            '$/cancelRequest',
          ],
        ),
      ]),
      test('textDocument/selectionRange on array spread', [
        addFiles('array_spread.js'),
        lspStartAndConnect(),
        requestSelectionRange(
          '<PLACEHOLDER_PROJECT_URL>/array_spread.js',
          [{line: 4, character: 16}], // in baz
        ).verifyAllLSPMessagesInStep(
          [
            {
              method: 'textDocument/selectionRange',
              result: [
                {
                  parent: {
                    parent: {
                      parent: {
                        parent: {
                          range: {
                            end: {
                              character: 0,
                              line: 5,
                            },
                            start: {
                              character: 0,
                              line: 0,
                            },
                          },
                        },
                        range: {
                          end: {
                            character: 19,
                            line: 4,
                          },
                          start: {
                            character: 0,
                            line: 4,
                          },
                        },
                      },
                      range: {
                        end: {
                          character: 18,
                          line: 4,
                        },
                        start: {
                          character: 0,
                          line: 4,
                        },
                      },
                    },
                    range: {
                      end: {
                        character: 17,
                        line: 4,
                      },
                      start: {
                        character: 11,
                        line: 4,
                      },
                    },
                  },
                  range: {
                    end: {
                      character: 17,
                      line: 4,
                    },
                    start: {
                      character: 14,
                      line: 4,
                    },
                  },
                },
              ],
            },
          ],
          [
            'textDocument/publishDiagnostics',
            'window/showStatus',
            '$/cancelRequest',
          ],
        ),
      ]),
      test('textDocument/selectionRange on functions', [
        addFiles('function.js'),
        lspStartAndConnect(),

        requestSelectionRange(
          '<PLACEHOLDER_PROJECT_URL>/function.js',
          [{line: 3, character: 0}], // in declaration body
        ).verifyAllLSPMessagesInStep(
          [
            {
              method: 'textDocument/selectionRange',
              result: [
                {
                  parent: {
                    parent: {
                      range: {
                        end: {
                          character: 0,
                          line: 9,
                        },
                        start: {
                          character: 0,
                          line: 0,
                        },
                      },
                    },
                    range: {
                      end: {
                        character: 1,
                        line: 4,
                      },
                      start: {
                        character: 0,
                        line: 2,
                      },
                    },
                  },
                  range: {
                    end: {
                      character: 1,
                      line: 4,
                    },
                    start: {
                      character: 48,
                      line: 2,
                    },
                  },
                },
              ],
            },
          ],
          [
            'textDocument/publishDiagnostics',
            'window/showStatus',
            '$/cancelRequest',
          ],
        ),

        requestSelectionRange(
          '<PLACEHOLDER_PROJECT_URL>/function.js',
          [{line: 7, character: 0}], // in expression body
        ).verifyAllLSPMessagesInStep(
          [
            {
              method: 'textDocument/selectionRange',
              result: [
                {
                  parent: {
                    parent: {
                      parent: {
                        range: {
                          end: {
                            character: 0,
                            line: 9,
                          },
                          start: {
                            character: 0,
                            line: 0,
                          },
                        },
                      },
                      range: {
                        end: {
                          character: 2,
                          line: 8,
                        },
                        start: {
                          character: 0,
                          line: 6,
                        },
                      },
                    },
                    range: {
                      end: {
                        character: 1,
                        line: 8,
                      },
                      start: {
                        character: 1,
                        line: 6,
                      },
                    },
                  },
                  range: {
                    end: {
                      character: 1,
                      line: 8,
                    },
                    start: {
                      character: 30,
                      line: 6,
                    },
                  },
                },
              ],
            },
          ],
          [
            'textDocument/publishDiagnostics',
            'window/showStatus',
            '$/cancelRequest',
          ],
        ),

        requestSelectionRange(
          '<PLACEHOLDER_PROJECT_URL>/function.js',
          [{line: 2, character: 31}], // in arg2 identifier
        ).verifyAllLSPMessagesInStep(
          [
            {
              method: 'textDocument/selectionRange',
              result: [
                {
                  parent: {
                    parent: {
                      parent: {
                        parent: {
                          parent: {
                            range: {
                              end: {
                                character: 0,
                                line: 9,
                              },
                              start: {
                                character: 0,
                                line: 0,
                              },
                            },
                          },
                          range: {
                            end: {
                              character: 1,
                              line: 4,
                            },
                            start: {
                              character: 0,
                              line: 2,
                            },
                          },
                        },
                        range: {
                          end: {
                            character: 47,
                            line: 2,
                          },
                          start: {
                            character: 0,
                            line: 2,
                          },
                        },
                      },
                      range: {
                        end: {
                          character: 41,
                          line: 2,
                        },
                        start: {
                          character: 13,
                          line: 2,
                        },
                      },
                    },
                    range: {
                      end: {
                        character: 40,
                        line: 2,
                      },
                      start: {
                        character: 28,
                        line: 2,
                      },
                    },
                  },
                  range: {
                    end: {
                      character: 32,
                      line: 2,
                    },
                    start: {
                      character: 28,
                      line: 2,
                    },
                  },
                },
              ],
            },
          ],
          [
            'textDocument/publishDiagnostics',
            'window/showStatus',
            '$/cancelRequest',
          ],
        ),
      ]),
      test('textDocument/selectionRange on var', [
        addFiles('var.js'),
        lspStartAndConnect(),
        requestSelectionRange(
          '<PLACEHOLDER_PROJECT_URL>/var.js',
          [{line: 4, character: 15}], // inside 1234
        ).verifyAllLSPMessagesInStep(
          [
            {
              method: 'textDocument/selectionRange',
              result: [
                {
                  parent: {
                    parent: {
                      parent: {
                        range: {
                          end: {
                            character: 0,
                            line: 5,
                          },
                          start: {
                            character: 0,
                            line: 0,
                          },
                        },
                      },
                      range: {
                        end: {
                          character: 17,
                          line: 4,
                        },
                        start: {
                          character: 0,
                          line: 4,
                        },
                      },
                    },
                    range: {
                      end: {
                        character: 16,
                        line: 4,
                      },
                      start: {
                        character: 4,
                        line: 4,
                      },
                    },
                  },
                  range: {
                    end: {
                      character: 16,
                      line: 4,
                    },
                    start: {
                      character: 12,
                      line: 4,
                    },
                  },
                },
              ],
            },
          ],
          [
            'textDocument/publishDiagnostics',
            'window/showStatus',
            '$/cancelRequest',
          ],
        ),
        requestSelectionRange(
          '<PLACEHOLDER_PROJECT_URL>/var.js',
          [{line: 4, character: 9}], // inside T of `var x : T`
        ).verifyAllLSPMessagesInStep(
          [
            {
              method: 'textDocument/selectionRange',
              result: [
                {
                  parent: {
                    parent: {
                      parent: {
                        parent: {
                          parent: {
                            range: {
                              end: {
                                character: 0,
                                line: 5,
                              },
                              start: {
                                character: 0,
                                line: 0,
                              },
                            },
                          },
                          range: {
                            end: {
                              character: 17,
                              line: 4,
                            },
                            start: {
                              character: 0,
                              line: 4,
                            },
                          },
                        },
                        range: {
                          end: {
                            character: 16,
                            line: 4,
                          },
                          start: {
                            character: 4,
                            line: 4,
                          },
                        },
                      },
                      range: {
                        end: {
                          character: 9,
                          line: 4,
                        },
                        start: {
                          character: 4,
                          line: 4,
                        },
                      },
                    },
                    range: {
                      end: {
                        character: 9,
                        line: 4,
                      },
                      start: {
                        character: 6,
                        line: 4,
                      },
                    },
                  },
                  range: {
                    end: {
                      character: 9,
                      line: 4,
                    },
                    start: {
                      character: 8,
                      line: 4,
                    },
                  },
                },
              ],
            },
          ],
          [
            'textDocument/publishDiagnostics',
            'window/showStatus',
            '$/cancelRequest',
          ],
        ),
      ]),
      test('textDocument/selectionRange on declare export', [
        addFiles('declare_export.js'),
        lspStartAndConnect(),
        requestSelectionRange(
          '<PLACEHOLDER_PROJECT_URL>/declare_export.js',
          [{line: 2, character: 17}], // inside `var` in `declare export var ...`
        ).verifyAllLSPMessagesInStep(
          [
            {
              method: 'textDocument/selectionRange',
              result: [
                {
                  parent: {
                    range: {
                      end: {
                        character: 0,
                        line: 3,
                      },
                      start: {
                        character: 0,
                        line: 0,
                      },
                    },
                  },
                  range: {
                    end: {
                      character: 30,
                      line: 2,
                    },
                    start: {
                      character: 0,
                      line: 2,
                    },
                  },
                },
              ],
            },
          ],
          [
            'textDocument/publishDiagnostics',
            'window/showStatus',
            '$/cancelRequest',
          ],
        ),
      ]),
      test('textDocument/selectionRange on classes', [
        addFiles('class.js'),
        lspStartAndConnect(),
        requestSelectionRange(
          '<PLACEHOLDER_PROJECT_URL>/class.js',
          [{line: 6, character: 2}], // in class keyword
        ).verifyAllLSPMessagesInStep(
          [
            {
              method: 'textDocument/selectionRange',
              result: [
                {
                  parent: {
                    range: {
                      end: {
                        character: 0,
                        line: 23,
                      },
                      start: {
                        character: 0,
                        line: 0,
                      },
                    },
                  },
                  range: {
                    end: {
                      character: 1,
                      line: 22,
                    },
                    start: {
                      character: 0,
                      line: 6,
                    },
                  },
                },
              ],
            },
          ],
          [
            'textDocument/publishDiagnostics',
            'window/showStatus',
            '$/cancelRequest',
          ],
        ),
        requestSelectionRange(
          '<PLACEHOLDER_PROJECT_URL>/class.js',
          [{line: 6, character: 16}], // `A` in `class ... extends A`
        )
          // TODO: include `extends `? not in the AST but could be useful
          // to delete.
          .verifyAllLSPMessagesInStep(
            [
              {
                method: 'textDocument/selectionRange',
                result: [
                  {
                    parent: {
                      parent: {
                        range: {
                          end: {
                            character: 0,
                            line: 23,
                          },
                          start: {
                            character: 0,
                            line: 0,
                          },
                        },
                      },
                      range: {
                        end: {
                          character: 1,
                          line: 22,
                        },
                        start: {
                          character: 0,
                          line: 6,
                        },
                      },
                    },
                    range: {
                      end: {
                        character: 17,
                        line: 6,
                      },
                      start: {
                        character: 16,
                        line: 6,
                      },
                    },
                  },
                ],
              },
            ],
            [
              'textDocument/publishDiagnostics',
              'window/showStatus',
              '$/cancelRequest',
            ],
          ),
        requestSelectionRange(
          '<PLACEHOLDER_PROJECT_URL>/class.js',
          [{line: 6, character: 29}], // `I` in `class ... identifier I {`
        ).verifyAllLSPMessagesInStep(
          [
            {
              method: 'textDocument/selectionRange',
              result: [
                {
                  parent: {
                    parent: {
                      parent: {
                        range: {
                          end: {
                            character: 0,
                            line: 23,
                          },
                          start: {
                            character: 0,
                            line: 0,
                          },
                        },
                      },
                      range: {
                        end: {
                          character: 1,
                          line: 22,
                        },
                        start: {
                          character: 0,
                          line: 6,
                        },
                      },
                    },
                    range: {
                      end: {
                        character: 30,
                        line: 6,
                      },
                      start: {
                        character: 18,
                        line: 6,
                      },
                    },
                  },
                  range: {
                    end: {
                      character: 30,
                      line: 6,
                    },
                    start: {
                      character: 29,
                      line: 6,
                    },
                  },
                },
              ],
            },
          ],
          [
            'textDocument/publishDiagnostics',
            'window/showStatus',
            '$/cancelRequest',
          ],
        ),
        requestSelectionRange(
          '<PLACEHOLDER_PROJECT_URL>/class.js',
          [{line: 7, character: 11}], // in foo void return type
        ).verifyAllLSPMessagesInStep(
          [
            {
              method: 'textDocument/selectionRange',
              result: [
                {
                  parent: {
                    parent: {
                      parent: {
                        parent: {
                          parent: {
                            parent: {
                              range: {
                                end: {
                                  character: 0,
                                  line: 23,
                                },
                                start: {
                                  character: 0,
                                  line: 0,
                                },
                              },
                            },
                            range: {
                              end: {
                                character: 1,
                                line: 22,
                              },
                              start: {
                                character: 0,
                                line: 6,
                              },
                            },
                          },
                          range: {
                            end: {
                              character: 1,
                              line: 22,
                            },
                            start: {
                              character: 31,
                              line: 6,
                            },
                          },
                        },
                        range: {
                          end: {
                            character: 3,
                            line: 9,
                          },
                          start: {
                            character: 2,
                            line: 7,
                          },
                        },
                      },
                      range: {
                        end: {
                          character: 13,
                          line: 7,
                        },
                        start: {
                          character: 5,
                          line: 7,
                        },
                      },
                    },
                    range: {
                      end: {
                        character: 13,
                        line: 7,
                      },
                      start: {
                        character: 7,
                        line: 7,
                      },
                    },
                  },
                  range: {
                    end: {
                      character: 13,
                      line: 7,
                    },
                    start: {
                      character: 9,
                      line: 7,
                    },
                  },
                },
              ],
            },
          ],
          [
            'textDocument/publishDiagnostics',
            'window/showStatus',
            '$/cancelRequest',
          ],
        ),
        requestSelectionRange(
          '<PLACEHOLDER_PROJECT_URL>/class.js',
          [{line: 8, character: 0}], // in foo method body
        ).verifyAllLSPMessagesInStep(
          [
            {
              method: 'textDocument/selectionRange',
              result: [
                {
                  parent: {
                    parent: {
                      parent: {
                        parent: {
                          range: {
                            end: {
                              character: 0,
                              line: 23,
                            },
                            start: {
                              character: 0,
                              line: 0,
                            },
                          },
                        },
                        range: {
                          end: {
                            character: 1,
                            line: 22,
                          },
                          start: {
                            character: 0,
                            line: 6,
                          },
                        },
                      },
                      range: {
                        end: {
                          character: 1,
                          line: 22,
                        },
                        start: {
                          character: 31,
                          line: 6,
                        },
                      },
                    },
                    range: {
                      end: {
                        character: 3,
                        line: 9,
                      },
                      start: {
                        character: 2,
                        line: 7,
                      },
                    },
                  },
                  range: {
                    end: {
                      character: 3,
                      line: 9,
                    },
                    start: {
                      character: 14,
                      line: 7,
                    },
                  },
                },
              ],
            },
          ],
          [
            'textDocument/publishDiagnostics',
            'window/showStatus',
            '$/cancelRequest',
          ],
        ),
        requestSelectionRange(
          '<PLACEHOLDER_PROJECT_URL>/class.js',
          [{line: 11, character: 7}], // in bar identifier
        )
          // TODO: should include signature
          .verifyAllLSPMessagesInStep(
            [
              {
                method: 'textDocument/selectionRange',
                result: [
                  {
                    parent: {
                      parent: {
                        parent: {
                          parent: {
                            range: {
                              end: {
                                character: 0,
                                line: 23,
                              },
                              start: {
                                character: 0,
                                line: 0,
                              },
                            },
                          },
                          range: {
                            end: {
                              character: 1,
                              line: 22,
                            },
                            start: {
                              character: 0,
                              line: 6,
                            },
                          },
                        },
                        range: {
                          end: {
                            character: 1,
                            line: 22,
                          },
                          start: {
                            character: 31,
                            line: 6,
                          },
                        },
                      },
                      range: {
                        end: {
                          character: 3,
                          line: 13,
                        },
                        start: {
                          character: 2,
                          line: 11,
                        },
                      },
                    },
                    range: {
                      end: {
                        character: 9,
                        line: 11,
                      },
                      start: {
                        character: 6,
                        line: 11,
                      },
                    },
                  },
                ],
              },
            ],
            [
              'textDocument/publishDiagnostics',
              'window/showStatus',
              '$/cancelRequest',
            ],
          ),
        requestSelectionRange(
          '<PLACEHOLDER_PROJECT_URL>/class.js',
          [{line: 15, character: 6}], // in baz's T param
        )
          // TODO: should include signature
          .verifyAllLSPMessagesInStep(
            [
              {
                method: 'textDocument/selectionRange',
                result: [
                  {
                    parent: {
                      parent: {
                        parent: {
                          parent: {
                            parent: {
                              parent: {
                                range: {
                                  end: {
                                    character: 0,
                                    line: 23,
                                  },
                                  start: {
                                    character: 0,
                                    line: 0,
                                  },
                                },
                              },
                              range: {
                                end: {
                                  character: 1,
                                  line: 22,
                                },
                                start: {
                                  character: 0,
                                  line: 6,
                                },
                              },
                            },
                            range: {
                              end: {
                                character: 1,
                                line: 22,
                              },
                              start: {
                                character: 31,
                                line: 6,
                              },
                            },
                          },
                          range: {
                            end: {
                              character: 3,
                              line: 17,
                            },
                            start: {
                              character: 2,
                              line: 15,
                            },
                          },
                        },
                        range: {
                          end: {
                            character: 16,
                            line: 15,
                          },
                          start: {
                            character: 5,
                            line: 15,
                          },
                        },
                      },
                      range: {
                        end: {
                          character: 8,
                          line: 15,
                        },
                        start: {
                          character: 5,
                          line: 15,
                        },
                      },
                    },
                    range: {
                      end: {
                        character: 7,
                        line: 15,
                      },
                      start: {
                        character: 6,
                        line: 15,
                      },
                    },
                  },
                ],
              },
            ],
            [
              'textDocument/publishDiagnostics',
              'window/showStatus',
              '$/cancelRequest',
            ],
          ),
        requestSelectionRange(
          '<PLACEHOLDER_PROJECT_URL>/class.js',
          [{line: 19, character: 4}], // in propA key
        ).verifyAllLSPMessagesInStep(
          [
            {
              method: 'textDocument/selectionRange',
              result: [
                {
                  parent: {
                    parent: {
                      parent: {
                        parent: {
                          range: {
                            end: {
                              character: 0,
                              line: 23,
                            },
                            start: {
                              character: 0,
                              line: 0,
                            },
                          },
                        },
                        range: {
                          end: {
                            character: 1,
                            line: 22,
                          },
                          start: {
                            character: 0,
                            line: 6,
                          },
                        },
                      },
                      range: {
                        end: {
                          character: 1,
                          line: 22,
                        },
                        start: {
                          character: 31,
                          line: 6,
                        },
                      },
                    },
                    range: {
                      end: {
                        character: 16,
                        line: 19,
                      },
                      start: {
                        character: 2,
                        line: 19,
                      },
                    },
                  },
                  range: {
                    end: {
                      character: 7,
                      line: 19,
                    },
                    start: {
                      character: 2,
                      line: 19,
                    },
                  },
                },
              ],
            },
          ],
          [
            'textDocument/publishDiagnostics',
            'window/showStatus',
            '$/cancelRequest',
          ],
        ),
        requestSelectionRange(
          '<PLACEHOLDER_PROJECT_URL>/class.js',
          [{line: 19, character: 12}], // in propA annot
        ).verifyAllLSPMessagesInStep(
          [
            {
              method: 'textDocument/selectionRange',
              result: [
                {
                  parent: {
                    parent: {
                      parent: {
                        parent: {
                          parent: {
                            range: {
                              end: {
                                character: 0,
                                line: 23,
                              },
                              start: {
                                character: 0,
                                line: 0,
                              },
                            },
                          },
                          range: {
                            end: {
                              character: 1,
                              line: 22,
                            },
                            start: {
                              character: 0,
                              line: 6,
                            },
                          },
                        },
                        range: {
                          end: {
                            character: 1,
                            line: 22,
                          },
                          start: {
                            character: 31,
                            line: 6,
                          },
                        },
                      },
                      range: {
                        end: {
                          character: 16,
                          line: 19,
                        },
                        start: {
                          character: 2,
                          line: 19,
                        },
                      },
                    },
                    range: {
                      end: {
                        character: 15,
                        line: 19,
                      },
                      start: {
                        character: 7,
                        line: 19,
                      },
                    },
                  },
                  range: {
                    end: {
                      character: 15,
                      line: 19,
                    },
                    start: {
                      character: 9,
                      line: 19,
                    },
                  },
                },
              ],
            },
          ],
          [
            'textDocument/publishDiagnostics',
            'window/showStatus',
            '$/cancelRequest',
          ],
        ),
        requestSelectionRange(
          '<PLACEHOLDER_PROJECT_URL>/class.js',
          [{line: 21, character: 22}], // in propB initializer
        ).verifyAllLSPMessagesInStep(
          [
            {
              method: 'textDocument/selectionRange',
              result: [
                {
                  parent: {
                    parent: {
                      parent: {
                        parent: {
                          range: {
                            end: {
                              character: 0,
                              line: 23,
                            },
                            start: {
                              character: 0,
                              line: 0,
                            },
                          },
                        },
                        range: {
                          end: {
                            character: 1,
                            line: 22,
                          },
                          start: {
                            character: 0,
                            line: 6,
                          },
                        },
                      },
                      range: {
                        end: {
                          character: 1,
                          line: 22,
                        },
                        start: {
                          character: 31,
                          line: 6,
                        },
                      },
                    },
                    range: {
                      end: {
                        character: 25,
                        line: 21,
                      },
                      start: {
                        character: 2,
                        line: 21,
                      },
                    },
                  },
                  range: {
                    end: {
                      character: 24,
                      line: 21,
                    },
                    start: {
                      character: 19,
                      line: 21,
                    },
                  },
                },
              ],
            },
          ],
          [
            'textDocument/publishDiagnostics',
            'window/showStatus',
            '$/cancelRequest',
          ],
        ),
      ]),
      test('textDocument/selectionRange on template literals', [
        addFiles('template_literal.js'),
        lspStartAndConnect(),
        requestSelectionRange('<PLACEHOLDER_PROJECT_URL>/template_literal.js', [
          {line: 4, character: 0},
        ]).verifyAllLSPMessagesInStep(
          [
            {
              method: 'textDocument/selectionRange',
              result: [
                {
                  parent: {
                    parent: {
                      parent: {
                        range: {
                          end: {
                            character: 0,
                            line: 15,
                          },
                          start: {
                            character: 0,
                            line: 0,
                          },
                        },
                      },
                      range: {
                        end: {
                          character: 2,
                          line: 6,
                        },
                        start: {
                          character: 0,
                          line: 2,
                        },
                      },
                    },
                    range: {
                      end: {
                        character: 1,
                        line: 6,
                      },
                      start: {
                        character: 6,
                        line: 2,
                      },
                    },
                  },
                  range: {
                    end: {
                      character: 1,
                      line: 6,
                    },
                    start: {
                      character: 10,
                      line: 2,
                    },
                  },
                },
              ],
            },
          ],
          [
            'textDocument/publishDiagnostics',
            'window/showStatus',
            '$/cancelRequest',
          ],
        ),
        requestSelectionRange('<PLACEHOLDER_PROJECT_URL>/template_literal.js', [
          {line: 10, character: 12},
        ]).verifyAllLSPMessagesInStep(
          [
            {
              method: 'textDocument/selectionRange',
              result: [
                {
                  parent: {
                    parent: {
                      parent: {
                        parent: {
                          range: {
                            end: {
                              character: 0,
                              line: 15,
                            },
                            start: {
                              character: 0,
                              line: 0,
                            },
                          },
                        },
                        range: {
                          end: {
                            character: 2,
                            line: 14,
                          },
                          start: {
                            character: 0,
                            line: 10,
                          },
                        },
                      },
                      range: {
                        end: {
                          character: 1,
                          line: 14,
                        },
                        start: {
                          character: 6,
                          line: 10,
                        },
                      },
                    },
                    range: {
                      end: {
                        character: 1,
                        line: 14,
                      },
                      start: {
                        character: 10,
                        line: 10,
                      },
                    },
                  },
                  range: {
                    end: {
                      character: 13,
                      line: 10,
                    },
                    start: {
                      character: 10,
                      line: 10,
                    },
                  },
                },
              ],
            },
          ],
          [
            'textDocument/publishDiagnostics',
            'window/showStatus',
            '$/cancelRequest',
          ],
        ),
        requestSelectionRange('<PLACEHOLDER_PROJECT_URL>/template_literal.js', [
          {line: 12, character: 0},
        ]).verifyAllLSPMessagesInStep(
          [
            {
              method: 'textDocument/selectionRange',
              result: [
                {
                  parent: {
                    parent: {
                      parent: {
                        parent: {
                          range: {
                            end: {
                              character: 0,
                              line: 15,
                            },
                            start: {
                              character: 0,
                              line: 0,
                            },
                          },
                        },
                        range: {
                          end: {
                            character: 2,
                            line: 14,
                          },
                          start: {
                            character: 0,
                            line: 10,
                          },
                        },
                      },
                      range: {
                        end: {
                          character: 1,
                          line: 14,
                        },
                        start: {
                          character: 6,
                          line: 10,
                        },
                      },
                    },
                    range: {
                      end: {
                        character: 1,
                        line: 14,
                      },
                      start: {
                        character: 10,
                        line: 10,
                      },
                    },
                  },
                  range: {
                    end: {
                      character: 1,
                      line: 14,
                    },
                    start: {
                      character: 13,
                      line: 10,
                    },
                  },
                },
              ],
            },
          ],
          [
            'textDocument/publishDiagnostics',
            'window/showStatus',
            '$/cancelRequest',
          ],
        ),
      ]),
    ];
  },
) as SuiteType;
