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

import {parse} from '../__test_utils__/parse';
import {loc} from '../__test_utils__/loc';

describe('Locations', () => {
  test('Source locations', () => {
    // ESTree source locations include range
    expect(parse('Foo')).toMatchObject({
      type: 'Program',
      body: [
        {
          type: 'ExpressionStatement',
          loc: {
            start: {
              line: 1,
              column: 0,
            },
            end: {
              line: 1,
              column: 3,
            },
          },
          range: [0, 3],
        },
      ],
    });

    // Babel source locations include start/end properties
    expect(parse('Foo', {babel: true})).toMatchObject({
      type: 'File',
      program: {
        type: 'Program',
        body: [
          {
            type: 'ExpressionStatement',
            loc: {
              start: {
                line: 1,
                column: 0,
              },
              end: {
                line: 1,
                column: 3,
              },
            },
            start: 0,
            end: 3,
          },
        ],
      },
    });

    // Filename is included in source location if provided
    expect(parse('Foo', {sourceFilename: 'FooTest.js'})).toMatchObject({
      type: 'Program',
      body: [
        {
          type: 'ExpressionStatement',
          loc: {
            source: 'FooTest.js',
            start: {
              line: 1,
              column: 0,
            },
            end: {
              line: 1,
              column: 3,
            },
          },
        },
      ],
    });

    // Code points that will be encoded as 1, 2, 3, and 4 byte UTF-8 characters
    // within Hermes are translated back to UTF-16 code unit source locations.
    const unicode = `\
'foo1';
'foo\u00a7';
'foo\u2014';
'foo\ud83d\udea6';
`;
    expect(parse(unicode)).toMatchObject({
      type: 'Program',
      body: [
        {
          type: 'ExpressionStatement',
          expression: {
            type: 'Literal',
            loc: loc(1, 0, 1, 6),
            range: [0, 6],
            value: 'foo1',
          },
        },
        {
          type: 'ExpressionStatement',
          expression: {
            type: 'Literal',
            loc: loc(2, 0, 2, 6),
            range: [8, 14],
            value: 'foo\u00a7',
          },
        },
        {
          type: 'ExpressionStatement',
          expression: {
            type: 'Literal',
            loc: loc(3, 0, 3, 6),
            range: [16, 22],
            value: 'foo\u2014',
          },
        },
        {
          type: 'ExpressionStatement',
          expression: {
            type: 'Literal',
            loc: loc(4, 0, 4, 7),
            range: [24, 31],
            value: 'foo\ud83d\udea6',
          },
        },
      ],
    });
  });

  test('Decorator locations on class properties and methods', () => {
    // Decorators on both properties and methods should have range
    expect(
      parse(`class C {
  @d1 prop;
  @d2 method() {}
}`),
    ).toMatchObject({
      type: 'Program',
      body: [
        {
          type: 'ClassDeclaration',
          body: {
            type: 'ClassBody',
            body: [
              {
                type: 'PropertyDefinition',
                decorators: [
                  {
                    type: 'Decorator',
                    range: [12, 15],
                    expression: {
                      type: 'Identifier',
                      name: 'd1',
                      range: [13, 15],
                    },
                  },
                ],
              },
              {
                type: 'MethodDefinition',
                decorators: [
                  {
                    type: 'Decorator',
                    range: [24, 27],
                    expression: {
                      type: 'Identifier',
                      name: 'd2',
                      range: [25, 27],
                    },
                  },
                ],
              },
            ],
          },
        },
      ],
    });
  });

  test('Program source locations', () => {
    const source = `
A;
/*comment*/`;

    // ESTree programs include the full source, not just the program body.
    expect(parse(source)).toMatchObject({
      type: 'Program',
      loc: loc(1, 0, 3, 11),
      range: [0, 15],
    }); // OK

    // Babel program location starts at beginning of source and includes trailing comments
    expect(parse(source, {babel: true})).toMatchObject({
      type: 'File',
      loc: loc(1, 0, 3, 11),
      start: 0,
      end: 15,
      program: {
        type: 'Program',
        loc: loc(1, 0, 3, 11),
        start: 0,
        end: 15,
      },
    }); // OK
  });

  test.each([
    [' a ', 1, 3],
    ['', 1, 0],
    [' \t\r\n ', 2, 1],
    [' /* comment */ \n', 2, 0],
    ['#!/usr/bin/env node\n a \n', 3, 0],
    ['\n/* leading */\na;\n/* trailing */\n ', 5, 1],
    ['\uFEFFa \u2028\u2029 ', 3, 1],
    [" '\u{1F6A6}' ", 1, 6],
  ])('Program spans the full source: %p', (source, endLine, endColumn) => {
    // Both output formats use full-source, UTF-16 bounds in scripts and modules.
    for (const sourceType of ['script', 'module']) {
      const options = {sourceType, sourceFilename: 'test.js'};
      const expectedLoc = {
        ...loc(1, 0, endLine, endColumn),
        source: 'test.js',
      };
      expect(parse(source, options)).toMatchObject({
        type: 'Program',
        loc: expectedLoc,
        range: [0, source.length],
      }); // OK
      expect(parse(source, {...options, babel: true})).toMatchObject({
        type: 'File',
        start: 0,
        end: source.length,
        program: {
          type: 'Program',
          start: 0,
          end: source.length,
          loc: loc(1, 0, endLine, endColumn),
        },
      }); // OK
    }
  });

  test('Full-source program bounds preserve statement bounds', () => {
    // The issue's original example expands only the root, not its children.
    expect(parse(' a ')).toMatchObject({
      range: [0, 3],
      body: [{range: [1, 2], loc: loc(1, 1, 1, 2)}],
    }); // OK
    expect(parse(' a ', {babel: true})).toMatchObject({
      start: 0,
      end: 3,
      program: {
        body: [{start: 1, end: 2, loc: loc(1, 1, 1, 2)}],
      },
    }); // OK
  });

  test('Recovered programs still span the full source', () => {
    // Keeping a partial AST must not truncate its range at the last recovered token.
    const source = ' const = ; \n';
    const program = parse(source, {throwOnParseErrors: false});
    expect(program.errors?.length).toBeGreaterThan(0); // OK
    expect(program).toMatchObject({
      loc: loc(1, 0, 2, 0),
      range: [0, source.length],
    }); // OK
  });
});
