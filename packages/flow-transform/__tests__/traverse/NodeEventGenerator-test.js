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

import type {ESNode, Program} from 'flow-estree';

import {parseForESLint} from 'flow-eslint';
import {SimpleTraverser} from 'flow-parser';
import {SafeEmitter} from '../../src/traverse/SafeEmitter';
import {NodeEventGenerator} from '../../src/traverse/NodeEventGenerator';

describe('NodeEventGenerator', () => {
  describe('selector handling', () => {
    it('parses valid selectors', () => {
      for (const selector of [
        'ExpressionStatement',
        'Identifier',
        'AnyTypeAnnotation',
        'EnumBody',
        'ExpressionStatement > Identifier',
        'ExpressionStatement:exit',
        'Identifier[name="foo"].key',
        '*',
      ]) {
        const emitter = new SafeEmitter();
        emitter.on(selector, () => {});
        expect(() => new NodeEventGenerator(emitter)).not.toThrow();
      }
    });

    it('throws on invalid selectors', () => {
      for (const selector of [
        'Foo',
        'Bar',
        'Foo > Bar',
        'Identifier > Foo',
        'Identifier:notexit',
        'EnumSymbolBody',
      ]) {
        const emitter = new SafeEmitter();
        emitter.on(selector, () => {});
        expect(() => new NodeEventGenerator(emitter)).toThrow();
      }
    });

    it('throws a useful error', () => {
      const emitter = new SafeEmitter();

      emitter.on('Foo >', () => {});
      expect(() => new NodeEventGenerator(emitter)).toThrow(
        /Syntax error in selector "Foo >" at position 5: Expected " ", "!", .*/u,
      );
    });
  });

  describe('entering a single AST node', () => {
    let emitter;
    let emitSpy;
    let generator;

    beforeEach(() => {
      emitter = new SafeEmitter();
      emitSpy = jest.spyOn(emitter, 'emit');

      for (const selector of [
        'ExpressionStatement',
        'Identifier',
        'ExpressionStatement > Identifier',
        'ExpressionStatement:exit',
      ]) {
        emitter.on(selector, () => {});
      }
      generator = new NodeEventGenerator(emitter);
    });

    it('should generate events for entering AST node.', () => {
      const dummyNode: $FlowFixMe = {type: 'ExpressionStatement', value: 1};

      generator.enterNode(dummyNode);

      expect(emitSpy).toHaveBeenCalledTimes(1);
      expect(emitSpy).toHaveBeenCalledWith('ExpressionStatement', dummyNode);
    });

    it('should generate events for exitting AST node.', () => {
      const dummyNode: $FlowFixMe = {type: 'ExpressionStatement', value: 1};

      generator.leaveNode(dummyNode);

      expect(emitSpy).toHaveBeenCalledTimes(1);
      expect(emitSpy).toHaveBeenCalledWith(
        'ExpressionStatement:exit',
        dummyNode,
      );
    });

    it('should generate events for AST queries', () => {
      const dummyNode: $FlowFixMe = {
        type: 'Identifier',
        parent: {type: 'ExpressionStatement'},
      };

      // make sure the generator has the ancestry recorded
      generator.enterNode(dummyNode.parent);
      emitSpy.mockClear();

      generator.enterNode(dummyNode);

      expect(emitSpy).toHaveBeenCalledTimes(2);
      expect(emitSpy).toHaveBeenCalledWith(
        'ExpressionStatement > Identifier',
        dummyNode,
      );
    });
  });

  describe('traversing the entire AST', () => {
    type Emissions = ReadonlyArray<[string, ESNode]>;
    /**
     * Gets a list of emitted types/selectors from the generator, in emission order
     * @param ast The AST to traverse
     * @param possibleQueries Selectors to detect
     * @returns A list of emissions, in the order that they were emitted. Each emission is a two-element
     * array where the first element is a string, and the second element is the emitted AST node.
     */
    function getEmissions(
      ast: Program,
      possibleQueries: ReadonlyArray<string>,
    ): Emissions {
      const emissions: Array<Emissions[number]> = [];
      const emitter = new SafeEmitter();
      jest
        .spyOn(emitter, 'emit')
        .mockImplementation((selector: string, node: ESNode) =>
          emissions.push([selector, node]),
        );

      possibleQueries.forEach(query => emitter.on(query, () => {}));
      const generator = new NodeEventGenerator(emitter);

      SimpleTraverser.traverse(ast, {
        enter(node) {
          generator.enterNode(node);
        },
        leave(node) {
          generator.leaveNode(node);
        },
      });

      return emissions;
    }

    /**
     * Creates a test case that asserts a particular sequence of generator emissions
     * @param sourceText The source text that should be parsed and traversed
     * @param possibleQueries A collection of selectors that rules are listening for
     * @param expectedEmissions A function that accepts the AST and returns a list of the emissions that the
     * generator is expected to produce, in order.
     * Each element of this list is an array where the first element is a selector (string), and the second is an AST node
     * This should only include emissions that appear in possibleQueries.
     */
    function assertEmissions(
      sourceText: string,
      possibleQueries: ReadonlyArray<string>,
      // these tests do simple access into the AST, so it's much easier to just have the access untyped
      expectedEmissions: (v: $FlowFixMe) => Emissions,
    ): void {
      it(`${sourceText} - ${possibleQueries.join('; ')}`, () => {
        const {ast} = parseForESLint(sourceText);
        const emissions = getEmissions(ast, possibleQueries).filter(
          emission => possibleQueries.indexOf(emission[0]) !== -1,
        );

        expect(emissions).toEqual(expectedEmissions(ast));
      });
    }

    assertEmissions(
      'enum E { A = 1 }',
      ['EnumBody', 'EnumBody > EnumNumberMember', 'EnumNumberMember:exit'],
      ast => [
        ['EnumBody', ast.body[0].body],
        ['EnumBody > EnumNumberMember', ast.body[0].body.members[0]],
        ['EnumNumberMember:exit', ast.body[0].body.members[0]],
      ],
    );

    assertEmissions(
      'foo + bar;',
      [
        'Program',
        'Program:exit',
        'ExpressionStatement',
        'ExpressionStatement:exit',
        'BinaryExpression',
        'BinaryExpression:exit',
        'Identifier',
        'Identifier:exit',
      ],
      ast => [
        ['Program', ast], // entering program
        ['ExpressionStatement', ast.body[0]], // entering 'foo + bar;'
        ['BinaryExpression', ast.body[0].expression], // entering 'foo + bar'
        ['Identifier', ast.body[0].expression.left], // entering 'foo'
        ['Identifier:exit', ast.body[0].expression.left], // exiting 'foo'
        ['Identifier', ast.body[0].expression.right], // entering 'bar'
        ['Identifier:exit', ast.body[0].expression.right], // exiting 'bar'
        ['BinaryExpression:exit', ast.body[0].expression], // exiting 'foo + bar'
        ['ExpressionStatement:exit', ast.body[0]], // exiting 'foo + bar;'
        ['Program:exit', ast], // exiting program
      ],
    );

    assertEmissions(
      'foo + 5',
      [
        'BinaryExpression > Identifier',
        'BinaryExpression',
        'BinaryExpression Literal:exit',
        'BinaryExpression > Identifier:exit',
        'BinaryExpression:exit',
      ],
      ast => [
        ['BinaryExpression', ast.body[0].expression], // foo + 5
        ['BinaryExpression > Identifier', ast.body[0].expression.left], // foo
        ['BinaryExpression > Identifier:exit', ast.body[0].expression.left], // exiting foo
        ['BinaryExpression Literal:exit', ast.body[0].expression.right], // exiting 5
        ['BinaryExpression:exit', ast.body[0].expression], // exiting foo + 5
      ],
    );

    assertEmissions(
      'foo + 5',
      ["BinaryExpression > *[name='foo']"],
      ast => [
        ["BinaryExpression > *[name='foo']", ast.body[0].expression.left],
      ], // entering foo
    );

    assertEmissions('foo', ['*'], ast => [
      ['*', ast], // Program
      ['*', ast.body[0]], // ExpressionStatement
      ['*', ast.body[0].expression], // Identifier
    ]);

    assertEmissions('foo', ['*:not(ExpressionStatement)'], ast => [
      ['*:not(ExpressionStatement)', ast], // Program
      ['*:not(ExpressionStatement)', ast.body[0].expression], // Identifier
    ]);

    assertEmissions(
      'foo()',
      ["CallExpression[callee.name='foo']"],
      ast => [["CallExpression[callee.name='foo']", ast.body[0].expression]], // foo()
    );

    assertEmissions(
      'foo()',
      ["CallExpression[callee.name='bar']"],
      () => [], // (nothing emitted)
    );

    assertEmissions(
      'foo + bar + baz',
      [':not(*)'],
      () => [], // (nothing emitted)
    );

    assertEmissions(
      'foo + bar + baz',
      [
        ":matches(Identifier[name='foo'], Identifier[name='bar'], Identifier[name='baz'])",
      ],
      ast => [
        [
          ":matches(Identifier[name='foo'], Identifier[name='bar'], Identifier[name='baz'])",
          ast.body[0].expression.left.left,
        ], // foo
        [
          ":matches(Identifier[name='foo'], Identifier[name='bar'], Identifier[name='baz'])",
          ast.body[0].expression.left.right,
        ], // bar
        [
          ":matches(Identifier[name='foo'], Identifier[name='bar'], Identifier[name='baz'])",
          ast.body[0].expression.right,
        ], // baz
      ],
    );

    assertEmissions('foo + 5 + 6', ['Identifier, Literal[value=5]'], ast => [
      ['Identifier, Literal[value=5]', ast.body[0].expression.left.left], // foo
      ['Identifier, Literal[value=5]', ast.body[0].expression.left.right], // 5
    ]);

    assertEmissions(
      '[foo, 5, foo]',
      ['Identifier + Literal'],
      ast => [['Identifier + Literal', ast.body[0].expression.elements[1]]], // 5
    );

    assertEmissions(
      '[foo, {}, 5]',
      ['Identifier + Literal', 'Identifier ~ Literal'],
      ast => [['Identifier ~ Literal', ast.body[0].expression.elements[2]]], // 5
    );

    assertEmissions(
      'foo; bar + baz; qux()',
      [':expression', ':statement'],
      ast => [
        [':statement', ast.body[0]],
        [':expression', ast.body[0].expression],
        [':statement', ast.body[1]],
        [':expression', ast.body[1].expression],
        [':expression', ast.body[1].expression.left],
        [':expression', ast.body[1].expression.right],
        [':statement', ast.body[2]],
        [':expression', ast.body[2].expression],
        [':expression', ast.body[2].expression.callee],
      ],
    );

    assertEmissions(
      'function foo(){} var x; (function (p){}); () => {};',
      [
        ':function',
        'ExpressionStatement > :function',
        'VariableDeclaration, :function[params.length=1]',
      ],
      ast => [
        [':function', ast.body[0]], // function foo(){}
        ['VariableDeclaration, :function[params.length=1]', ast.body[1]], // var x;
        [':function', ast.body[2].expression], // function (p){}
        ['ExpressionStatement > :function', ast.body[2].expression], // function (p){}
        [
          'VariableDeclaration, :function[params.length=1]',
          ast.body[2].expression,
        ], // function (p){}
        [':function', ast.body[3].expression], // () => {}
        ['ExpressionStatement > :function', ast.body[3].expression], // () => {}
      ],
    );

    assertEmissions(
      'foo;',
      [
        '*',
        ':not(*)',
        'Identifier',
        'ExpressionStatement > *',
        'ExpressionStatement > Identifier',
        "ExpressionStatement > [name='foo']",
        'Identifier, ReturnStatement',
        "[name = 'foo']",
        "[name='foo']",
        "[name ='foo']",
        "Identifier[name='foo']",
        "[name='foo'][name.length=3]",
        ':not(Program, ExpressionStatement)',
        ':not(Program, Identifier) > [name.length=3]',
      ],
      ast => [
        ['*', ast], // Program
        ['*', ast.body[0]], // ExpressionStatement

        // selectors for the 'foo' identifier, in order of increasing specificity
        ['*', ast.body[0].expression], // 0 identifiers, 0 pseudoclasses
        ['ExpressionStatement > *', ast.body[0].expression], // 0 pseudoclasses, 1 identifier
        ['Identifier', ast.body[0].expression], // 0 pseudoclasses, 1 identifier
        [':not(Program, ExpressionStatement)', ast.body[0].expression], // 0 pseudoclasses, 2 identifiers
        ['ExpressionStatement > Identifier', ast.body[0].expression], // 0 pseudoclasses, 2 identifiers
        ['Identifier, ReturnStatement', ast.body[0].expression], // 0 pseudoclasses, 2 identifiers
        ["[name = 'foo']", ast.body[0].expression], // 1 pseudoclass, 0 identifiers
        ["[name ='foo']", ast.body[0].expression], // 1 pseudoclass, 0 identifiers
        ["[name='foo']", ast.body[0].expression], // 1 pseudoclass, 0 identifiers
        ["ExpressionStatement > [name='foo']", ast.body[0].expression], // 1 attribute, 1 identifier
        ["Identifier[name='foo']", ast.body[0].expression], // 1 attribute, 1 identifier
        [':not(Program, Identifier) > [name.length=3]', ast.body[0].expression], // 1 attribute, 2 identifiers
        ["[name='foo'][name.length=3]", ast.body[0].expression], // 2 attributes, 0 identifiers
      ],
    );

    assertEmissions(
      'foo(); bar; baz;',
      ["CallExpression, [name='bar']"],
      ast => [
        ["CallExpression, [name='bar']", ast.body[0].expression],
        ["CallExpression, [name='bar']", ast.body[1].expression],
      ],
    );

    assertEmissions('foo; bar;', ['[name.length=3]:exit'], ast => [
      ['[name.length=3]:exit', ast.body[0].expression],
      ['[name.length=3]:exit', ast.body[1].expression],
    ]);

    // https://github.com/eslint/eslint/issues/14799
    assertEmissions('const {a = 1} = b;', ['Property > .key'], ast => [
      ['Property > .key', ast.body[0].declarations[0].id.properties[0].key],
    ]);
  });
});
