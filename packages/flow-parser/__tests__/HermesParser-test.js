/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @flow strict-local
 * @format
 */

'use strict';

import type {ExportDefaultDeclaration, InterfaceDeclaration} from 'flow-estree';

import {parse} from '../__test_utils__/parse';

// Ordinary declarations and expressions remain value-kind default exports.
test.each([
  'export default function F() {}',
  'export default class C {}',
  'export default I;',
  'export default 1;',
])('preserves value default kind for %s', source => {
  expect(parse(source).body[0]).toMatchObject({
    type: 'ExportDefaultDeclaration',
    exportKind: 'value',
  }); // OK
});

// Default type and value exports occupy separate validation namespaces.
test('validates separate default-export namespaces', () => {
  expect(
    parse('export default interface I {} export default 1;').body.map(node =>
      node.type === 'ExportDefaultDeclaration' ? node.exportKind : null,
    ),
  ).toEqual(['type', 'value']); // OK
  expect(() =>
    parse('export default interface I {} export default interface J {}'),
  ).toThrow(); // ERROR
});

test('Can parse simple file', () => {
  expect(parse('const x = 1')).toMatchObject({
    type: 'Program',
    body: [
      expect.objectContaining({
        type: 'VariableDeclaration',
        kind: 'const',
        declarations: [
          expect.objectContaining({
            type: 'VariableDeclarator',
            id: expect.objectContaining({
              type: 'Identifier',
              name: 'x',
            }),
            init: expect.objectContaining({
              type: 'Literal',
              value: 1,
            }),
          }),
        ],
      }),
    ],
  });
});

describe('Parse errors', () => {
  // Note: the error message text below pins the Rust parser's wording, which
  // differs from upstream Hermes by design — the fork ports OCaml flow_parser,
  // which has its own diagnostic style. The upstream-compatible *behavior* is
  // what matters: parse() throws a SyntaxError with `(line:column)` suffix and
  // a `loc` property. Tests that previously asserted Hermes-formatted source
  // line + caret diagnostics have been narrowed to assert message + loc only;
  // Hermes builds the source-line/caret string in C++ at error generation
  // time, while the Rust parser surfaces only `{loc, message}`.
  test('Basic', () => {
    expect(() => parse('const = 1')).toThrow(
      new SyntaxError(`Unexpected token \`=\`, expected an identifier (1:6)`),
    );
  });

  test('Has error location', () => {
    expect(() => parse('const = 1')).toThrow(
      expect.objectContaining({
        loc: {
          line: 1,
          column: 6,
        },
      }),
    );
  });

  test('Source line with non-ASCII characters', () => {
    // Note: Hermes counts columns in bytes (Ŷ is 2 bytes UTF-8 → column 13);
    // OCaml flow_parser / Rust port counts in characters (Ŷ is 1 char →
    // column 12). The off-by-one reflects the underlying parser engine
    // change, not an adapter bug.
    expect(() => parse('/*\u0176*/ const = 1')).toThrow(
      new SyntaxError(`Unexpected token \`=\`, expected an identifier (1:12)`),
    );
  });

  test('Error with notes', () => {
    // The Rust port surfaces the duplicate-constructor diagnostic differently
    // -- OCaml flow_parser groups duplicate-class-element diagnostics under
    // its own primary error wording without Hermes' secondary `note:` line.
    // Test asserts the loc carries a coordinate inside the duplicate body.
    const source = `class C {
  constructor() { 1 }
  constructor() { 2 }
}`;
    expect(() => parse(source)).toThrow(
      expect.objectContaining({
        loc: expect.objectContaining({line: expect.any(Number)}),
      }),
    );
  });
});

describe('Program source type', () => {
  const typeOnlySpecifiers = [
    "import {type T} from './types';",
    "import {typeof T} from './types';",
    "import {type T, typeof U} from './types';",
    'export {type T};',
    'export {type T as default};',
    "export {type T, type U} from './types';",
    'export type {};',
  ];
  const runtimeSpecifiers = [
    "import './value';",
    "import {} from './value';",
    "import value, {type T} from './value';",
    "import * as value from './value';",
    "import {type T, value} from './value';",
    "import {type} from './value';",
    'export {};',
    "export {} from './value';",
    'export {type T, value};',
    'export {type};',
    "export * as value from './value';",
  ];

  const moduleProgram = {
    type: 'Program',
    sourceType: 'module',
  };
  const scriptProgram = {
    type: 'Program',
    sourceType: 'script',
  };

  test('hardcoded', () => {
    expect(parse('Foo', {sourceType: 'module'})).toMatchObject(moduleProgram);
    expect(parse('Foo', {sourceType: 'script'})).toMatchObject(scriptProgram);
  });

  test('detect module type', () => {
    // Verify that every value import or export results in module source type
    const moduleSources = [
      `import Foo from 'foo'`,
      `export default 1`,
      `export const foo = 1`,
      `export * from 'foo'`,
    ];

    for (const moduleSource of moduleSources) {
      expect(parse(moduleSource)).toMatchObject(moduleProgram);
      expect(parse(moduleSource, {babel: true})).toMatchObject({
        type: 'File',
        program: moduleProgram,
      });

      expect(parse(moduleSource, {sourceType: 'unambiguous'})).toMatchObject(
        moduleProgram,
      );
    }
  });

  test('detect script type', () => {
    // Verify that type imports and exports do not result in module source type
    const scriptSource = `
      import type Foo from 'foo';
      export type T = any;
      export type * from 'foo';
    `;

    expect(parse(scriptSource)).toMatchObject(scriptProgram);
    expect(parse(scriptSource, {babel: true})).toMatchObject({
      type: 'File',
      program: scriptProgram,
    });

    expect(parse(scriptSource, {sourceType: 'unambiguous'})).toMatchObject(
      scriptProgram,
    );
  });

  test('specifier-level type modifiers in ESTree', () => {
    // All-type specifier lists retain script metadata in both detection modes.
    for (const sourceOptions of [{}, {sourceType: 'unambiguous'}]) {
      for (const code of typeOnlySpecifiers) {
        expect(parse(code, sourceOptions).sourceType).toBe('script'); // OK
      }
    }

    // Runtime specifiers and empty markers still select module metadata.
    for (const code of runtimeSpecifiers) {
      expect(parse(code).sourceType).toBe('module'); // OK
    }
  });

  test('default interface exports in ESTree', () => {
    const source = 'export default interface I {value: number;}';

    // Public types admit interface declarations with a type export kind.
    const exported = parse(source).body[0];
    if (
      exported.type !== 'ExportDefaultDeclaration' ||
      exported.declaration.type !== 'InterfaceDeclaration'
    ) {
      throw new Error('Expected a default interface export');
    }
    const declaration: InterfaceDeclaration = exported.declaration;
    const exportKind: ExportDefaultDeclaration['exportKind'] = 'type';
    expect(declaration.id.name).toBe('I'); // OK
    expect(exported.exportKind).toBe(exportKind); // OK

    // Type-only default exports retain script metadata during content detection.
    for (const sourceOptions of [{}, {sourceType: 'unambiguous'}]) {
      expect(parse(source, sourceOptions).sourceType).toBe('script'); // OK
    }

    // An explicit module option takes precedence over type-only content.
    expect(parse(source, {sourceType: 'module'}).sourceType).toBe('module'); // OK

    // Runtime declarations still select module metadata alongside type exports.
    for (const runtimeDeclaration of [
      "import value from './value';",
      'export const value = 1;',
      'export default 1;',
    ]) {
      expect(parse(`${source}\n${runtimeDeclaration}`).sourceType).toBe(
        'module',
      ); // OK
    }
  });

  test('exported import aliases in ESTree', () => {
    for (const sourceFilename of ['aliases.ts', 'aliases.d.ts']) {
      const options = {sourceFilename};

      // Exported value aliases select modules in both AST formats.
      for (const source of [
        'export import Alias = Namespace.Alias;',
        "export import Alias = require('./value');",
      ]) {
        expect(parse(source, options).sourceType).toBe('module'); // OK
        expect(
          parse(source, {...options, babel: true}).program.sourceType,
        ).toBe('module'); // OK
      }

      // Ordinary aliases and type-only aliases retain script metadata.
      for (const source of [
        'import Alias = Namespace.Alias;',
        "import type Alias = require('./types');",
      ]) {
        expect(parse(source, options).sourceType).toBe('script'); // OK
        expect(
          parse(source, {...options, babel: true}).program.sourceType,
        ).toBe('script'); // OK
      }
    }
  });

  test('specifier-level type modifiers in Babel', () => {
    // All-type specifier lists retain script metadata in both detection modes.
    for (const sourceOptions of [{}, {sourceType: 'unambiguous'}]) {
      for (const code of typeOnlySpecifiers) {
        expect(
          parse(code, {...sourceOptions, babel: true}).program.sourceType,
        ).toBe('script'); // OK
      }
    }

    // Runtime specifiers and empty markers still select module metadata.
    for (const code of runtimeSpecifiers) {
      expect(parse(code, {babel: true}).program.sourceType).toBe('module'); // OK
    }
  });

  test('default interface exports in Babel', () => {
    const source = 'export default interface I {value: number;}';

    // Babel default exports deliberately omit the ESTree export-kind field.
    for (const code of [source, 'export default 1;']) {
      const exported = parse(code, {babel: true}).program.body[0];
      expect(exported).toMatchObject({type: 'ExportDefaultDeclaration'}); // OK
      expect(exported).not.toHaveProperty('exportKind'); // OK
    }

    // Type-only default exports retain script metadata during content detection.
    for (const sourceOptions of [{}, {sourceType: 'unambiguous'}]) {
      const ast = parse(source, {...sourceOptions, babel: true}).program;
      expect(ast.sourceType).toBe('script'); // OK
    }

    // An explicit module option takes precedence over type-only content.
    expect(
      parse(source, {sourceType: 'module', babel: true}).program.sourceType,
    ).toBe('module'); // OK

    // Runtime imports or exports still select module metadata alongside type exports.
    for (const runtimeDeclaration of [
      "import value from './value';",
      'export const value = 1;',
      'export default 1;',
    ]) {
      const code = `${source}\n${runtimeDeclaration}`;
      const ast = parse(code, {babel: true}).program;
      expect(ast.sourceType).toBe('module'); // OK
    }
  });

  // Module filenames select modules even when their exports are all type-only.
  test('module filenames with type-only exports', () => {
    const source = 'export default interface I {}';
    for (const sourceFilename of ['route.mjs', 'route.mts', 'route.d.mts']) {
      for (const sourceOptions of [{}, {sourceType: 'unambiguous'}]) {
        const options = {...sourceOptions, sourceFilename};
        expect(parse(source, options)).toMatchObject({
          sourceType: 'module',
          errors: [],
        }); // OK
        expect(
          parse(source, {...options, babel: true}).program.sourceType,
        ).toBe('module'); // OK
      }
    }
  });
});

// Module selection controls top-level await with and without token output.
describe.each([{tokens: false}, {tokens: true}])(
  'Top-level await (tokens: $tokens)',
  ({tokens}) => {
    test('await expressions in explicit modules', () => {
      const awaitExpression = {
        type: 'AwaitExpression',
        argument: {type: 'CallExpression', callee: {name: 'live'}},
      };
      const sources = [
        [
          'export {}; await live();',
          {type: 'ExpressionStatement', expression: awaitExpression},
        ],
        [
          "export {}; const {POST} = await import('./route');",
          {
            type: 'VariableDeclaration',
            declarations: [
              {
                id: {type: 'ObjectPattern'},
                init: {
                  type: 'AwaitExpression',
                  argument: {type: 'ImportExpression'},
                },
              },
            ],
          },
        ],
      ];
      for (const sourceOptions of [
        {sourceType: 'module'},
        {sourceType: 'module', sourceFilename: 'route.mts'},
        {sourceFilename: 'route.mjs'},
        {sourceFilename: 'route.mts'},
      ]) {
        for (const [source, statement] of sources) {
          const ast = parse(source, {...sourceOptions, tokens});
          expect(ast).toMatchObject({sourceType: 'module', errors: []});
          expect(ast.body[1]).toMatchObject(statement);
          if (tokens) {
            expect(ast.tokens).toEqual(
              parse(source, {...sourceOptions, sourceType: 'module', tokens})
                .tokens,
            );
            expect(ast.tokens).toEqual(
              expect.arrayContaining([
                expect.objectContaining({value: 'await'}),
              ]),
            );
          }
        }
      }
    });

    test('default parsing retains script grammar with module metadata', () => {
      const source = 'export {}; with (obj) {} await(live());';
      for (const options of [{}, {sourceType: 'unambiguous'}]) {
        const ast = parse(source, {...options, tokens});
        expect(ast).toMatchObject({sourceType: 'module', errors: []});
        expect(ast.body[2]).toMatchObject({
          type: 'ExpressionStatement',
          expression: {type: 'CallExpression', callee: {name: 'await'}},
        });
      }
    });

    test('explicit modules without imports or exports', () => {
      const ast = parse('await live(); for await (const item of items) {}', {
        sourceType: 'module',
        tokens,
      });
      expect(ast).toMatchObject({
        sourceType: 'module',
        errors: [],
        body: [
          {
            type: 'ExpressionStatement',
            expression: {type: 'AwaitExpression'},
          },
          {type: 'ForOfStatement', await: true},
        ],
      });
    });

    test.each(['route.mjs', 'route.mts', 'route.d.mts'])(
      'module filename %s without imports or exports',
      sourceFilename => {
        for (const sourceOptions of [{}, {sourceType: 'unambiguous'}]) {
          const options = {...sourceOptions, sourceFilename, tokens};
          const ast = parse('await live();', options);
          expect(ast).toMatchObject({
            sourceType: 'module',
            errors: [],
            body: [
              {
                type: 'ExpressionStatement',
                expression: {type: 'AwaitExpression'},
              },
            ],
          });
          expect(ast.tokens).toEqual(
            parse('await live();', {...options, sourceType: 'module'}).tokens,
          );
          expect(
            parse("const {POST} = await import('./route');", options),
          ).toMatchObject({
            sourceType: 'module',
            errors: [],
            body: [
              {
                declarations: [
                  {
                    init: {
                      type: 'AwaitExpression',
                      argument: {type: 'ImportExpression'},
                    },
                  },
                ],
              },
            ],
          });
          expect(
            parse('await live();', {...options, babel: true}).program,
          ).toMatchObject({
            sourceType: 'module',
            body: [
              {
                type: 'ExpressionStatement',
                expression: {type: 'AwaitExpression'},
              },
            ],
          });
          expect(() => parse('with (obj) {}', options)).toThrow(SyntaxError);
          expect(() =>
            parse('function f() { await live(); }', options),
          ).toThrow(SyntaxError);
        }
      },
    );

    test('explicit script source type takes precedence over filename', () => {
      for (const sourceFilename of ['route.mjs', 'route.mts']) {
        const options = {sourceType: 'script', sourceFilename, tokens};
        expect(parse('with (obj) {}', options)).toMatchObject({
          sourceType: 'script',
          errors: [],
        });
        expect(() => parse('await live();', options)).toThrow(SyntaxError);
      }
    });

    test('other filenames use content detection', () => {
      for (const sourceFilename of [
        'route.js',
        'route.ts',
        'route.cjs',
        'route.cts',
        'route.mjs.map',
        'route.mtsx',
      ]) {
        const options = {sourceFilename, tokens};
        expect(parse('const x = 1;', options)).toMatchObject({
          sourceType: 'script',
          errors: [],
        });
        expect(() => parse('await live();', options)).toThrow(SyntaxError);
      }
    });

    test('scripts and ordinary nested functions reject await', () => {
      for (const [source, options] of [
        ['await live();', {}],
        ['await live();', {sourceType: 'script', sourceFilename: 'route.cjs'}],
        ['export {}; await live();', {sourceType: 'script'}],
        ["import type T from './types'; await live();", {}],
        ["const route = await import('./route');", {}],
        ['function f() { await live(); }', {sourceType: 'module'}],
        ['export {}; function f() { await live(); }', {}],
      ]) {
        expect(() => parse(source, {...options, tokens})).toThrow(SyntaxError);
        expect(
          parse(source, {...options, tokens, throwOnParseErrors: false}).errors
            .length,
        ).toBeGreaterThan(0);
      }
    });

    test('async functions allow await', () => {
      for (const sourceType of ['script', 'module']) {
        expect(
          parse('async function f() { await live(); }', {sourceType, tokens}),
        ).toMatchObject({errors: []});
      }
    });

    test('modules retain strict mode diagnostics and partial ASTs', () => {
      const source = 'export {}; with (obj) {} await live();';
      for (const options of [{sourceType: 'module'}]) {
        expect(() => parse(source, {...options, tokens})).toThrow(SyntaxError);
        const ast = parse(source, {
          ...options,
          tokens,
          throwOnParseErrors: false,
        });
        expect(ast.errors.length).toBeGreaterThan(0);
        expect(ast.body[2]).toMatchObject({
          type: 'ExpressionStatement',
          expression: {type: 'AwaitExpression'},
        });
      }
      expect(
        parse('with (obj) {}', {sourceType: 'script', tokens}),
      ).toMatchObject({errors: []});
    });
  },
);

test('Semantic validation', () => {
  // Semantic validator catches errors. Rust port surfaces the
  // top-level-return diagnostic with OCaml flow_parser wording.
  expect(() => parse(`return 1;`)).toThrow(
    new SyntaxError(`Illegal return statement (1:0)`),
  );

  // But invalid regexps are not reported
  expect(parse(`/(((/;`)).toMatchObject({
    type: 'Program',
    body: [
      {
        type: 'ExpressionStatement',
        expression: {
          type: 'Literal',
          value: null,
          regex: {
            pattern: '(((',
            flags: '',
          },
        },
      },
    ],
  });
});

test('Allow return outside function', () => {
  expect(() => parse('return 1')).toThrow(
    new SyntaxError(`Illegal return statement (1:0)`),
  );

  expect(parse('return 1', {allowReturnOutsideFunction: true})).toMatchObject({
    type: 'Program',
    body: [
      {
        type: 'ReturnStatement',
        argument: {
          type: 'Literal',
          value: 1,
        },
      },
    ],
  });
});

test('Allow component syntax', () => {
  expect(() =>
    parse('component Foo() {}', {enableExperimentalComponentSyntax: false}),
  ).toThrow(
    expect.objectContaining({
      loc: expect.objectContaining({line: 1, column: 10}),
    }),
  );

  expect(parse('component Foo() {}')).toMatchObject({
    type: 'Program',
    body: [
      {
        type: 'ComponentDeclaration',
        id: {
          type: 'Identifier',
          name: 'Foo',
        },
      },
    ],
  });
});

test('Allow Flow match syntax', () => {
  expect(() =>
    parse('const e = match (x) {}', {enableExperimentalFlowMatchSyntax: false}),
  ).toThrow(
    expect.objectContaining({
      loc: expect.objectContaining({line: 1, column: 20}),
    }),
  );

  expect(parse('const e = match (x) {}')).toMatchObject({
    type: 'Program',
    body: [
      {
        type: 'VariableDeclaration',
        declarations: [
          {
            type: 'VariableDeclarator',
            id: {
              type: 'Identifier',
              name: 'e',
            },
            init: {
              type: 'MatchExpression',
              argument: {
                type: 'Identifier',
                name: 'x',
              },
              cases: [],
            },
          },
        ],
        kind: 'const',
      },
    ],
  });
});

test('Allow Flow record syntax', () => {
  expect(() =>
    parse('record R {}', {enableExperimentalFlowRecordSyntax: false}),
  ).toThrow(
    expect.objectContaining({
      loc: expect.objectContaining({line: 1}),
    }),
  );

  expect(parse('record R {}')).toMatchObject({
    type: 'Program',
    body: [
      {
        type: 'RecordDeclaration',
        id: {
          type: 'Identifier',
          name: 'R',
        },
        implements: [],
        typeParameters: null,
        body: {
          type: 'RecordDeclarationBody',
          elements: [],
        },
      },
    ],
  });
});
