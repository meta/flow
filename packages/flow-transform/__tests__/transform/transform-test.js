/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @flow strict-local
 * @format
 */

import type {TransformVisitor} from '../../src/transform/transform';

import {transform as transformOriginal} from '../../src/transform/transform';
import * as t from '../../src/generated/node-types';
// $FlowExpectedError[cannot-resolve-module]
import prettierConfig from '../../../.prettierrc.json';

function transform(code: string, visitors: TransformVisitor) {
  return transformOriginal(code, visitors, prettierConfig);
}

describe('transform', () => {
  it('keeps a leading hashbang when the file is printed', async () => {
    const code = '#!/usr/bin/env node\n\nconst x = 1';
    const result = await transform(code, context => ({
      Identifier(node) {
        context.replaceNode(node, t.Identifier({name: 'y'}));
      },
    }));

    expect(result).toBe('#!/usr/bin/env node\n\nconst y = 1;\n');
  });

  it('should do nothing (including no formatting) if no mutations are applied', async () => {
    const code = 'const x = 1'; // no semi to ensure no formatting
    const result = await transform(code, () => ({}));

    expect(result).toBe(code);
  });

  it('should format the code as well as mutate it', async () => {
    const code = 'const x = 1';
    const result = await transform(code, context => ({
      VariableDeclaration(node) {
        context.replaceStatementWithMany(node, [
          t.VariableDeclaration({
            kind: 'const',
            declarations: [
              t.VariableDeclarator({
                id: t.Identifier({name: 'y'}),
                init: t.NullLiteral(),
              }),
            ],
          }),
        ]);
      },
    }));

    // note that it should have a semicolon
    expect(result).toBe(`\
const y = null;
`);
  });

  describe('insert', () => {
    describe('insertBeforeStatement mutation', () => {
      it('Insert before only statement', async () => {
        const code = `\
const x = 1;
`;
        const result = await transform(code, context => ({
          VariableDeclaration(node) {
            context.insertBeforeStatement(
              node,
              t.VariableDeclaration({
                kind: 'const',
                declarations: [
                  t.VariableDeclarator({
                    id: t.Identifier({
                      name: 'y',
                    }),
                    init: t.NumericLiteral({
                      value: 1,
                      raw: '1',
                    }),
                  }),
                ],
              }),
            );
          },
        }));

        expect(result).toBe(`\
const y = 1;
const x = 1;
`);
      });
      it('Insert before first statement', async () => {
        const code = `\
const x = 1;
lastStatement;
`;
        const result = await transform(code, context => ({
          VariableDeclaration(node) {
            context.insertBeforeStatement(
              node,
              t.VariableDeclaration({
                kind: 'const',
                declarations: [
                  t.VariableDeclarator({
                    id: t.Identifier({
                      name: 'y',
                    }),
                    init: t.NumericLiteral({
                      value: 1,
                      raw: '1',
                    }),
                  }),
                ],
              }),
            );
          },
        }));

        expect(result).toBe(`\
const y = 1;
const x = 1;
lastStatement;
`);
      });

      it('Insert before middle statement', async () => {
        const code = `\
firstStatement;
const x = 1;
lastStatement;
`;
        const result = await transform(code, context => ({
          VariableDeclaration(node) {
            context.insertBeforeStatement(
              node,
              t.VariableDeclaration({
                kind: 'const',
                declarations: [
                  t.VariableDeclarator({
                    id: t.Identifier({
                      name: 'y',
                    }),
                    init: t.NumericLiteral({
                      value: 1,
                      raw: '1',
                    }),
                  }),
                ],
              }),
            );
          },
        }));

        expect(result).toBe(`\
firstStatement;
const y = 1;
const x = 1;
lastStatement;
`);
      });

      it('Insert before last statement', async () => {
        const code = `\
firstStatement;
const x = 1;
`;
        const result = await transform(code, context => ({
          VariableDeclaration(node) {
            context.insertBeforeStatement(
              node,
              t.VariableDeclaration({
                kind: 'const',
                declarations: [
                  t.VariableDeclarator({
                    id: t.Identifier({
                      name: 'y',
                    }),
                    init: t.NumericLiteral({
                      value: 1,
                      raw: '1',
                    }),
                  }),
                ],
              }),
            );
          },
        }));

        expect(result).toBe(`\
firstStatement;
const y = 1;
const x = 1;
`);
      });

      it('wraps statements in a BlockStatement if they were in a bodyless parent', async () => {
        const code = 'if (condition) return true;';
        const result = await transform(code, context => ({
          ReturnStatement(node) {
            context.insertBeforeStatement(
              node,
              t.VariableDeclaration({
                kind: 'const',
                declarations: [
                  t.VariableDeclarator({
                    id: t.Identifier({
                      name: 'y',
                    }),
                    init: t.NumericLiteral({
                      value: 1,
                    }),
                  }),
                ],
              }),
            );
          },
        }));

        expect(result).toBe(`\
if (condition) {
  const y = 1;
  return true;
}
`);
      });
    });
  });
  describe('insertAfterStatement mutation', () => {
    it('Insert after only statement', async () => {
      const code = `\
const x = 1;
`;
      const result = await transform(code, context => ({
        VariableDeclaration(node) {
          context.insertAfterStatement(
            node,
            t.VariableDeclaration({
              kind: 'const',
              declarations: [
                t.VariableDeclarator({
                  id: t.Identifier({
                    name: 'y',
                  }),
                  init: t.NumericLiteral({
                    value: 1,
                    raw: '1',
                  }),
                }),
              ],
            }),
          );
        },
      }));

      expect(result).toBe(`\
const x = 1;
const y = 1;
`);
    });
    it('Insert after first statement', async () => {
      const code = `\
const x = 1;
lastStatement;
`;
      const result = await transform(code, context => ({
        VariableDeclaration(node) {
          context.insertAfterStatement(
            node,
            t.VariableDeclaration({
              kind: 'const',
              declarations: [
                t.VariableDeclarator({
                  id: t.Identifier({
                    name: 'y',
                  }),
                  init: t.NumericLiteral({
                    value: 1,
                    raw: '1',
                  }),
                }),
              ],
            }),
          );
        },
      }));

      expect(result).toBe(`\
const x = 1;
const y = 1;
lastStatement;
`);
    });

    it('Insert after middle statement', async () => {
      const code = `\
firstStatement;
const x = 1;
lastStatement;
`;
      const result = await transform(code, context => ({
        VariableDeclaration(node) {
          context.insertAfterStatement(
            node,
            t.VariableDeclaration({
              kind: 'const',
              declarations: [
                t.VariableDeclarator({
                  id: t.Identifier({
                    name: 'y',
                  }),
                  init: t.NumericLiteral({
                    value: 1,
                    raw: '1',
                  }),
                }),
              ],
            }),
          );
        },
      }));

      expect(result).toBe(`\
firstStatement;
const x = 1;
const y = 1;
lastStatement;
`);
    });

    it('Insert after last statement', async () => {
      const code = `\
firstStatement;
const x = 1;
`;
      const result = await transform(code, context => ({
        VariableDeclaration(node) {
          context.insertAfterStatement(
            node,
            t.VariableDeclaration({
              kind: 'const',
              declarations: [
                t.VariableDeclarator({
                  id: t.Identifier({
                    name: 'y',
                  }),
                  init: t.NumericLiteral({
                    value: 1,
                    raw: '1',
                  }),
                }),
              ],
            }),
          );
        },
      }));

      expect(result).toBe(`\
firstStatement;
const x = 1;
const y = 1;
`);
    });
  });

  describe('remove', () => {
    it('works with the removeStatement mutation', async () => {
      const code = 'const x = 1; console.log("I will survive");';
      const result = await transform(code, context => ({
        VariableDeclaration(node) {
          context.removeStatement(node);
        },
      }));

      expect(result).toBe(`\
console.log('I will survive');
`);
    });

    it('wraps statements in a BlockStatement if they were in a bodyless parent', async () => {
      const code = 'if (condition) return true;';
      const result = await transform(code, context => ({
        ReturnStatement(node) {
          context.removeStatement(node);
        },
      }));

      expect(result).toBe(`\
if (condition) {
}
`);
    });
  });

  describe('replace', () => {
    describe('single', () => {
      it('expression', async () => {
        const code = 'const x = 1;';
        const result = await transform(code, context => ({
          Literal(node) {
            context.replaceNode(node, t.BooleanLiteral({value: true}));
          },
        }));

        expect(result).toBe(`\
const x = true;
`);
      });

      it('statement', async () => {
        const code = 'const x = 1;';
        const result = await transform(code, context => ({
          VariableDeclaration(node) {
            context.replaceNode(
              node,
              t.VariableDeclaration({
                declarations: [
                  t.VariableDeclarator({
                    id: t.Identifier({name: 'y'}),
                    init: t.NullLiteral(),
                  }),
                ],
                kind: 'let',
              }),
            );
          },
        }));

        expect(result).toBe(`\
let y = null;
`);
      });

      it('type', async () => {
        const code = 'const x: any = 1;';
        const result = await transform(code, context => ({
          AnyTypeAnnotation(node) {
            context.replaceNode(node, t.NumberTypeAnnotation());
          },
        }));

        expect(result).toBe(`\
const x: number = 1;
`);
      });
    });

    describe('with many', () => {
      it('works with array parents', async () => {
        const code = 'const x = 1;';
        const result = await transform(code, context => ({
          VariableDeclaration(node) {
            context.replaceStatementWithMany(node, [
              t.VariableDeclaration({
                kind: 'const',
                declarations: [
                  t.VariableDeclarator({
                    id: t.Identifier({name: 'y'}),
                    init: t.NullLiteral(),
                  }),
                ],
              }),
              t.VariableDeclaration({
                kind: 'const',
                declarations: [
                  t.VariableDeclarator({
                    id: t.Identifier({name: 'z'}),
                    init: t.BooleanLiteral({value: true}),
                  }),
                ],
              }),
            ]);
          },
        }));

        expect(result).toBe(`\
const y = null;
const z = true;
`);
      });

      it('wraps statements in a BlockStatement if they were in a bodyless parent', async () => {
        const code = 'if (condition) return true;';
        const result = await transform(code, context => ({
          ReturnStatement(node) {
            context.replaceStatementWithMany(node, [
              t.VariableDeclaration({
                kind: 'const',
                declarations: [
                  t.VariableDeclarator({
                    id: t.Identifier({
                      name: 'y',
                    }),
                    init: t.NumericLiteral({
                      value: 1,
                    }),
                  }),
                ],
              }),
              t.VariableDeclaration({
                kind: 'const',
                declarations: [
                  t.VariableDeclarator({
                    id: t.Identifier({
                      name: 'z',
                    }),
                    init: t.NumericLiteral({
                      value: 2,
                    }),
                  }),
                ],
              }),
            ]);
          },
        }));

        expect(result).toBe(`\
if (condition) {
  const y = 1;
  const z = 2;
}
`);
      });
    });
  });

  describe('complex transforms', () => {
    it('should support transforms on the same subtree', async () => {
      const code = `\
class Foo {
  method(): () => () => Foo {
    function bar(): () => Foo {
      function baz(): Foo {
        return this;
      }
      return baz;
    }
    return bar;
  }
}
      `;

      // transform which replaces all function declarations with arrow functions
      const result = await transform(code, context => ({
        'FunctionDeclaration[id]'(node) {
          context.replaceNode(
            node,
            t.VariableDeclaration({
              kind: 'const',
              declarations: [
                t.VariableDeclarator({
                  id: node.id,
                  init: t.ArrowFunctionExpression({
                    async: node.async,
                    body: node.body,
                    params: node.params,
                    predicate: node.predicate,
                    returnType: node.returnType,
                    typeParameters: node.typeParameters,
                  }),
                }),
              ],
            }),
          );
        },
      }));

      expect(result).toBe(`\
class Foo {
  method(): () => () => Foo {
    const bar = (): (() => Foo) => {
      const baz = (): Foo => {
        return this;
      };
      return baz;
    };
    return bar;
  }
}
`);
    });

    it('should fail if you attempt to insert before a removed node', async () => {
      const code = `\
if (true) call();
      `;

      await expect(async () =>
        transform(code, context => ({
          ExpressionStatement(node) {
            context.replaceNode(
              node,
              t.ExpressionStatement({
                expression: t.StringLiteral({
                  value: 'removed',
                }),
              }),
            );

            context.insertBeforeStatement(
              node,
              t.ExpressionStatement({
                expression: t.StringLiteral({
                  value: 'inserted',
                }),
              }),
            );
          },
        })),
      ).rejects.toThrowErrorMatchingInlineSnapshot(
        `"Attempted to insert before a deleted ExpressionStatement node. This likely means that you attempted to mutate around the target after it was deleted/replaced."`,
      );
    });

    it('should allow insertion before removal', async () => {
      const code = `\
if (true) call();
      `;

      const result = await transform(code, context => ({
        ExpressionStatement(node) {
          context.insertBeforeStatement(
            node,
            t.ExpressionStatement({
              expression: t.StringLiteral({
                value: 'inserted',
              }),
            }),
          );

          context.replaceNode(
            node,
            t.ExpressionStatement({
              expression: t.StringLiteral({
                value: 'removed',
              }),
            }),
          );
        },
      }));

      expect(result).toBe(`\
if (true) {
  ('inserted');
  ('removed');
}
`);
    });
  });

  describe('comments', () => {
    describe('docblock', () => {
      it('should not attach to node', async () => {
        const code = `
/* @flow */
statement();
`;

        const result = await transform(code, context => ({
          Program(node) {
            expect(context.getComments(node.body[0])).toEqual([]);
            expect(node.docblock.comment.value).toBe(' @flow ');
            context.insertBeforeStatement(
              node.body[0],
              t.ExpressionStatement({
                expression: t.StringLiteral({
                  value: 'before',
                }),
              }),
            );
          },
        }));

        expect(result).toBe(`\
/* @flow */
('before');
statement();
`);
      });
    });
    describe('attachment', () => {
      it('should attach comments so they are maintained during an insertion', async () => {
        const code = `
// leading comment
statement(); // inline comment
`;

        const result = await transform(code, context => ({
          ExpressionStatement(node) {
            context.insertBeforeStatement(
              node,
              t.ExpressionStatement({
                expression: t.StringLiteral({
                  value: 'before',
                }),
              }),
            );
            context.insertAfterStatement(
              node,
              t.ExpressionStatement({
                expression: t.StringLiteral({
                  value: 'after',
                }),
              }),
            );
          },
        }));

        expect(result).toBe(`\
('before');
// leading comment
statement(); // inline comment
('after');
`);
      });

      it('should attach comments so they are removed when the associated node is removed', async () => {
        const code = `
// this should remain leading #1
const x = 1; // this should remain inline #1
// leading comment to be deleted
statement(); // inline comment to be deleted
// this should remain leading #2
const y = 1; // this should remain inline #2
`;

        const result = await transform(code, context => ({
          ExpressionStatement(node) {
            context.removeStatement(node);
          },
        }));

        expect(result).toBe(`\
// this should remain leading #1
const x = 1; // this should remain inline #1
// this should remain leading #2
const y = 1; // this should remain inline #2
`);
      });

      it('should clone comments when nodes are cloned', async () => {
        const code = `
// leading comment to be duplicated
statement(); // inline comment to be duplicated
`;

        const result = await transform(code, context => ({
          ExpressionStatement(node) {
            context.insertBeforeStatement(node, node);
          },
        }));

        expect(result).toBe(`\
// leading comment to be duplicated
statement(); // inline comment to be duplicated
// leading comment to be duplicated
statement(); // inline comment to be duplicated
`);
      });

      it('should attach comments so they are removed when the associated node is replaced (by default)', async () => {
        const code = `
// this should remain leading #1
const x = 1; // this should remain inline #1
// leading comment to be deleted
statement(); // inline comment to be deleted
// this should remain leading #2
const y = 1; // this should remain inline #2
`;

        const result = await transform(code, context => ({
          ExpressionStatement(node) {
            context.replaceNode(
              node,
              t.ExpressionStatement({
                expression: t.StringLiteral({
                  value: 'inserted',
                }),
              }),
            );
          },
        }));

        expect(result).toBe(`\
// this should remain leading #1
const x = 1; // this should remain inline #1
('inserted');
// this should remain leading #2
const y = 1; // this should remain inline #2
`);
      });

      it('should optionally attach comments so they are kept when the associated statement is replaced', async () => {
        const code = `
// this should remain leading #1
const x = 1; // this should remain inline #1
// leading comment to be deleted
statement(); // inline comment to be deleted
// this should remain leading #2
const y = 1; // this should remain inline #2
`;

        const result = await transform(code, context => ({
          ExpressionStatement(node) {
            context.replaceStatementWithMany(
              node,
              [
                t.ExpressionStatement({
                  expression: t.StringLiteral({
                    value: 'inserted1',
                  }),
                }),
                t.ExpressionStatement({
                  expression: t.StringLiteral({
                    value: 'inserted2',
                  }),
                }),
                t.ExpressionStatement({
                  expression: t.StringLiteral({
                    value: 'inserted3',
                  }),
                }),
              ],
              {keepComments: true},
            );
          },
        }));

        expect(result).toBe(`\
// this should remain leading #1
const x = 1; // this should remain inline #1
// leading comment to be deleted
('inserted1'); // inline comment to be deleted
('inserted2');
('inserted3');
// this should remain leading #2
const y = 1; // this should remain inline #2
`);
      });

      it('should optionally attach comments so they are kept when the associated node is replaced', async () => {
        const code = `
// this should remain leading #1
const x = 1; // this should remain inline #1
// leading comment to be deleted
statement(); // inline comment to be deleted
// this should remain leading #2
const y = 1; // this should remain inline #2
`;

        const result = await transform(code, context => ({
          ExpressionStatement(node) {
            context.replaceNode(
              node,
              t.ExpressionStatement({
                expression: t.StringLiteral({
                  value: 'inserted1',
                }),
              }),
              {keepComments: true},
            );
          },
        }));

        expect(result).toBe(`\
// this should remain leading #1
const x = 1; // this should remain inline #1
// leading comment to be deleted
('inserted1'); // inline comment to be deleted
// this should remain leading #2
const y = 1; // this should remain inline #2
`);
      });
    });

    describe('addition', () => {
      it('should attach leading block comments on line before node', async () => {
        const code = `\
statement();
`;

        const result = await transform(code, context => ({
          ExpressionStatement(node) {
            context.addLeadingComments(node, [
              t.BlockComment({
                value: `*
 * Line preceding comment 1
 `,
              }),
              t.BlockComment({
                value: `*
 * Line preceding comment 2
 `,
              }),
            ]);
          },
        }));

        expect(result).toBe(`\
/**
 * Line preceding comment 1
 */
/**
 * Line preceding comment 2
 */
statement();
`);
      });

      it('should attach leading inline block comments on node line', async () => {
        const code = `
statement();
`;

        const result = await transform(code, context => ({
          ExpressionStatement(node) {
            context.addLeadingInlineComments(node, [
              t.BlockComment({
                value: `Inline comment 1`,
              }),
              t.BlockComment({
                value: `Inline comment 2`,
              }),
            ]);
          },
        }));

        expect(result).toBe(`\
/*Inline comment 1*/ /*Inline comment 2*/ statement();
`);
      });

      it('should attach leading line comments on line before node', async () => {
        const code = `\
statement();
`;

        const result = await transform(code, context => ({
          ExpressionStatement(node) {
            context.addLeadingComments(node, [
              t.LineComment({
                value: `Line preceding comment 1`,
              }),
            ]);
          },
        }));

        expect(result).toBe(`\
//Line preceding comment 1
statement();
`);
      });

      it('should attach leading inline line comments on line before node', async () => {
        const code = `\
statement();
`;

        const result = await transform(code, context => ({
          ExpressionStatement(node) {
            context.addLeadingInlineComments(node, [
              t.LineComment({
                value: `Line preceding comment 1`,
              }),
            ]);
          },
        }));

        expect(result).toBe(`\
//Line preceding comment 1
statement();
`);
      });

      it('should attach trailing block comments on line after node', async () => {
        const code = `\
statement();
`;

        const result = await transform(code, context => ({
          ExpressionStatement(node) {
            context.addTrailingComments(node, [
              t.BlockComment({
                value: `Inline comment 1`,
              }),
              t.BlockComment({
                value: `Inline comment 2`,
              }),
            ]);
          },
        }));

        expect(result).toBe(`\
statement();
/*Inline comment 1*/
/*Inline comment 2*/
`);
      });

      it('should attach trailing inline block comments on node line', async () => {
        const code = `
statement();
`;

        const result = await transform(code, context => ({
          ExpressionStatement(node) {
            context.addTrailingInlineComments(node, [
              t.BlockComment({
                value: `Inline comment 1`,
              }),
              t.BlockComment({
                value: `Inline comment 2`,
              }),
            ]);
          },
        }));

        expect(result).toBe(`\
statement(); /*Inline comment 1*/ /*Inline comment 2*/
`);
      });

      it('should attach trailing line comments on line after node', async () => {
        const code = `\
statement();
`;

        const result = await transform(code, context => ({
          ExpressionStatement(node) {
            context.addTrailingComments(node, [
              t.LineComment({
                value: `Line trailing comment 1`,
              }),
            ]);
          },
        }));

        expect(result).toBe(`\
statement();
//Line trailing comment 1
`);
      });

      it('should attach trailing inline line comments on line after node', async () => {
        const code = `\
statement();
`;

        const result = await transform(code, context => ({
          ExpressionStatement(node) {
            context.addTrailingInlineComments(node, [
              t.LineComment({
                value: `Line trailing comment 1`,
              }),
            ]);
          },
        }));

        expect(result).toBe(`\
statement(); //Line trailing comment 1
`);
      });

      it('should allow leading comment addition', async () => {
        const code = `\
const x = 1;
const y = 2;`;
        const result = await transform(code, context => ({
          Identifier(node) {
            if (node.name === 'x') {
              context.addLeadingComments(node.parent.parent, [
                t.LineComment({value: 'line'}),
              ]);
            } else if (node.name === 'y') {
              context.addLeadingComments(node.parent.parent, [
                t.BlockComment({value: 'block'}),
              ]);
            }
          },
        }));

        expect(result).toBe(`\
//line
const x = 1;
/*block*/
const y = 2;
`);
      });

      it('should allow trailing comment addition', async () => {
        const code = `\
const x = 1;
const y = 2;`;
        const result = await transform(code, context => ({
          Identifier(node) {
            if (node.name === 'x') {
              context.addTrailingComments(node.parent.parent, [
                t.LineComment({value: 'line'}),
              ]);
            } else if (node.name === 'y') {
              context.addTrailingComments(node.parent.parent, [
                t.BlockComment({value: 'block'}),
              ]);
            }
          },
        }));

        expect(result).toBe(`\
const x = 1;
//line
const y = 2;
/*block*/
`);
      });

      it('should allow comment addition to new, detached nodes', async () => {
        const code = 'const x = 1;';
        const result = await transform(code, context => ({
          VariableDeclaration(node) {
            const newNode = t.ExpressionStatement({
              expression: t.StringLiteral({value: 'inserted'}),
            });
            context.insertBeforeStatement(node, newNode);

            context.addLeadingComments(newNode, [
              t.LineComment({value: 'leading line'}),
              t.BlockComment({value: 'leading block'}),
            ]);
            context.addTrailingComments(newNode, [
              t.BlockComment({value: 'trailing block 1'}),
              t.LineComment({value: 'trailing line'}),
              t.BlockComment({value: 'trailing block 2'}),
            ]);
          },
        }));

        expect(result).toBe(`\
//leading line
/*leading block*/
('inserted');
/*trailing block 1*/
//trailing line
/*trailing block 2*/
const x = 1;
`);
      });
    });

    describe('removal', () => {
      it('should allow removal of leading comments', async () => {
        const code = `\
//line
const x = 1;
/*block*/
const y = 2;`;
        const result = await transform(code, context => ({
          VariableDeclaration(node) {
            const coment = context.getComments(node)[0];
            context.removeComments(coment);
          },
        }));

        expect(result).toBe(`\
const x = 1;
const y = 2;
`);
      });

      it('should allow removal of trailing comments', async () => {
        const code = `\
const x = 1; /*block*/
const y = 2; //line`;
        const result = await transform(code, context => ({
          VariableDeclaration(node) {
            const coment = context.getComments(node)[0];
            context.removeComments(coment);
          },
        }));

        expect(result).toBe(`\
const x = 1;
const y = 2;
`);
      });
    });

    describe('clone', () => {
      it('should clone line comments to new nodes', async () => {
        const code = `\
// Leading comment
x; // EOL comment
y;`;
        const result = await transform(code, context => ({
          Program(node) {
            context.cloneCommentsTo(node.body[0], node.body[1]);
          },
        }));

        expect(result).toBe(`\
// Leading comment
x; // EOL comment
// Leading comment
y; // EOL comment
`);
      });
      it('should clone block comments to new nodes', async () => {
        const code = `\
'use strict';
/* Leading comment 1 */
/* Leading comment 2 */
x; /* EOL comment */
y;`;
        const result = await transform(code, context => ({
          Program(node) {
            context.cloneCommentsTo(node.body[1], node.body[2]);
          },
        }));

        expect(result).toBe(`\
'use strict';
/* Leading comment 1 */
/* Leading comment 2 */
x; /* EOL comment */
/* Leading comment 1 */
/* Leading comment 2 */
y; /* EOL comment */
`);
      });
      it('should clone comments to newly created nodes', async () => {
        const code = `\
x; // EOL comment`;
        const result = await transform(code, context => ({
          Program(node) {
            const x = node.body[0];
            const y = t.ExpressionStatement({
              expression: t.Identifier({name: 'y'}),
            });
            context.cloneCommentsTo(x, y);
            context.insertAfterStatement(x, y);
          },
        }));

        expect(result).toBe(`\
x; // EOL comment
y; // EOL comment
`);
      });
      it('should clone newly created comments to nodes', async () => {
        const code = `\
x;
y;`;
        const result = await transform(code, context => ({
          Program(node) {
            const x = node.body[0];
            const y = node.body[1];
            context.addTrailingInlineComments(
              x,
              t.LineComment({value: ' EOL comment'}),
            );
            context.cloneCommentsTo(x, y);
          },
        }));

        expect(result).toBe(`\
x; // EOL comment
y; // EOL comment
`);
      });
    });
  });

  it('should not crash on optional chaining', async () => {
    const code = `\
x?.y;
x?.();
foo?.[0]?.bar;
`;
    const result = await transform(code, context => ({
      Program(node) {
        context.addTrailingInlineComments(
          node.body[0],
          t.LineComment({value: 'test'}),
        );
      },
    }));
    expect(result).toBe(`\
x?.y; //test
x?.();
foo?.[0]?.bar;
`);
  });

  it('should preserve attached comments on ChainExpression nodes', async () => {
    const code = `\
foo(
  // $FlowFixMe[prop-missing]
  bar.baz,
  // $FlowFixMe[prop-missing]
  bar?.baz,
);
`;
    const result = await transform(code, context => ({
      Identifier(node) {
        context.replaceNode(
          node,
          t.Identifier({
            name: node.name,
          }),
        );
        return;
      },
    }));
    expect(result).toBe(`\
foo(
  // $FlowFixMe[prop-missing]
  bar.baz,
  // $FlowFixMe[prop-missing]
  bar?.baz,
);
`);
  });

  it('should correctly print method functions', async () => {
    const code = `\
      type A = {};`;
    const result = await transform(code, context => ({
      ObjectTypeAnnotation(node) {
        const func = t.FunctionTypeAnnotation({
          params: [],
          returnType: t.VoidTypeAnnotation(),
          rest: null,
          typeParameters: null,
          this: null,
        });
        context.modifyNodeInPlace(node, {
          properties: [
            t.ObjectTypeMethodSignature({
              key: t.Identifier({name: 'a'}),
              value: func,
            }),
            t.ObjectTypePropertySignature({
              key: t.Identifier({name: 'b'}),
              value: func,
              optional: false,
              variance: null,
            }),
            t.ObjectTypeAccessorSignature({
              key: t.Identifier({name: 'c'}),
              value: func,
              kind: 'get',
            }),
            t.ObjectTypeAccessorSignature({
              key: t.Identifier({name: 'd'}),
              // setters must have a param hence new func
              value: t.FunctionTypeAnnotation({
                params: [
                  t.FunctionTypeParam({
                    name: t.Identifier({
                      name: 'param',
                    }),
                    optional: false,
                    typeAnnotation: t.StringTypeAnnotation(),
                  }),
                ],
                returnType: t.VoidTypeAnnotation(),
                rest: null,
                typeParameters: null,
                this: null,
              }),
              kind: 'set',
            }),
          ],
        });
      },
    }));
    expect(result).toBe(`\
type A = {a(): void, b: () => void, get c(): void, set d(param: string): void};
`);
  });

  describe('New Flow Syntax Support', () => {
    it('match', async () => {
      const code = `\
const x = match (x) { _ => 1 };
`;
      const result = await transform(code, context => ({
        VariableDeclaration(node) {
          context.insertBeforeStatement(
            node,
            t.VariableDeclaration({
              kind: 'const',
              declarations: [
                t.VariableDeclarator({
                  id: t.Identifier({
                    name: 'y',
                  }),
                  init: t.NumericLiteral({
                    value: 1,
                    raw: '1',
                  }),
                }),
              ],
            }),
          );
        },
      }));

      expect(result).toBe(`\
const y = 1;
const x = match (x) {
  _ => 1,
};
`);
    });
  });
});
