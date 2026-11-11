/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @flow strict-local
 * @format
 */

import type {DefaultExportDocPlacement} from '../src/utils/TranslationUtils';

import flowToFlowDef from '../src/flowToFlowDef';
// $FlowExpectedError[cannot-resolve-module]
import prettierConfig from '../../.prettierrc.json';
import {parse, print} from 'flow-transform';
import {trimToBeCode} from './utils/inlineCodeHelpers';

type TranslateOptions = {
  defaultExportDocPlacement?: DefaultExportDocPlacement,
  mungeUnderscores?: boolean,
};

async function translate(
  code: string,
  opts?: TranslateOptions,
): Promise<string> {
  const {ast, scopeManager} = await parse(code);

  const [flowDefAst, mutatedCode] = flowToFlowDef(ast, code, scopeManager, {
    defaultExportDocPlacement: opts?.defaultExportDocPlacement,
    recoverFromErrors: false,
    mungeUnderscores: opts?.mungeUnderscores,
  });

  return print(flowDefAst, mutatedCode, prettierConfig);
}

async function expectTranslate(
  expectCode: string,
  toBeCode: string,
  opts?: TranslateOptions,
): Promise<void> {
  const expectTranslateCode = await translate(expectCode, opts);
  expect(expectTranslateCode).toBe(trimToBeCode(toBeCode));
}
async function expectTranslateUnchanged(expectCode: string): Promise<void> {
  const expectTranslateCode = await translate(expectCode);
  expect(expectTranslateCode).toBe(trimToBeCode(expectCode));
}

describe('flowToFlowDef', () => {
  describe('Comments', () => {
    it('maintain docblock', async () => {
      await expectTranslateUnchanged(
        `/**
          * @flow
          */
         export type Bar = string;`,
      );
    });
    it('maintain toplevel statement comments', async () => {
      await expectTranslate(
        `/**
          * @flow
          */

         // Comment 1
         export type Bar = string;

         // Comment 2
         export function bar(): void {}`,
        `/**
          * @flow
          */

         // Comment 1
         export type Bar = string;

         // Comment 2
         declare export function bar(): void;`,
      );
    });
  });
  describe('dependency walking', () => {
    it('strip unused function', async () => {
      await expectTranslate(
        `function foo(): void {}
         export function bar(): void { foo(); }`,
        `declare export function bar(): void;`,
      );
    });
    it('keep used TypeAlias', async () => {
      await expectTranslateUnchanged(
        `type Foo = string;
         export type Bar = Foo;`,
      );
    });
    it('keep chain of used TypeAliases', async () => {
      await expectTranslateUnchanged(
        `type Foo = string;
         type Bar = Foo;
         type Baz = Bar;
         export type Boo = Baz;`,
      );
    });
    it('keep used TypeAlias with many references', async () => {
      await expectTranslateUnchanged(
        `type Foo = string;
         export type Bar = Foo;
         export type Baz = [Bar, Foo];`,
      );
    });
    it('strip unused but shadowed deps', async () => {
      // These should never be referenced by anything
      const neverReferenced = `
        type Foo = string;`;
      // Shadowing "Foo", this shouldn't create a dep on the outer "Foo"
      const expectedOutput = `
        type Bar = string;
        declare export class BazC<Foo> {
          prop1: Foo;
          prop2: Bar;
        }`;
      await expectTranslate(neverReferenced + expectedOutput, expectedOutput);
    });
    it('strip unused but shadowed deps (complex)', async () => {
      // These should never be referenced by anything
      const neverReferenced = `
        type T = 1;`;
      // These should all have a reference
      const expectedOutput = `
        declare class OuterClass<T1> {}
        declare class OuterMixin<T1> {}
        declare interface OuterInterface<T1> {}
        type Outer1 = number;
        type Outer2 = number;
        type Outer3 = number;
        type Outer4 = number;
        type Outer5 = number;
        type Outer6 = number;

        declare export class Foo<
          // shouldn't create a reference on the outer T
          T: Outer1 = Outer2,
          // shouldn't create a reference on the outer T
          T2: T = T,
        >
          // shouldn't create a reference on the outer T
          extends OuterClass<T>
          // shouldn't create a reference on the outer T
          mixins OuterMixin<T>
          // shouldn't create a reference on the outer T
          implements OuterInterface<T>
        {
          // shouldn't create a reference on the outer T
          prop1: T;
          // shouldn't create a reference on the outer T
          method1(): T;
          // shouldn't create a reference on the outer T
          [T]: string;

          prop2: Outer3;
          method2(): Outer4;
          [Outer5]: Outer6;

          // this should create a (circular) reference to the containing Foo
          constructor(): Foo;
        }`;
      await expectTranslate(neverReferenced + expectedOutput, expectedOutput);
    });
  });
  describe('optimization pass', () => {
    it('strip unused import defs', async () => {
      await expectTranslate(
        `import type {Foo, Bar} from 'Foo';
         export type Baz = Foo;`,
        `import type {Foo} from 'Foo';
         export type Baz = Foo;`,
      );
    });
  });
  describe('ExportNamedDeclaration', () => {
    it('type specifier', async () => {
      await expectTranslateUnchanged(
        `type Bar = string;
         export type {Foo, Bar as Baz};`,
      );
    });
    it('type specifier with source', async () => {
      await expectTranslateUnchanged(
        `export type {Foo, Bar as Baz} from 'Baz';`,
      );
    });
    it('value specifier with source', async () => {
      await expectTranslateUnchanged(`export {Foo, Bar as Baz} from 'Baz';`);
    });
    it('all with source', async () => {
      await expectTranslateUnchanged(`export * as Foo from 'Foo';`);
    });
  });
  describe('ExportDefaultDeclaration', () => {
    // Valid default interfaces get a support diagnostic, not an invalid-AST error.
    it('reports unsupported default interface translation', async () => {
      await expect(translate('export default interface I {}')).rejects.toThrow(
        'Default interface exports are not supported',
      ); // ERROR
    });

    it('export default function', async () => {
      await expectTranslate(
        `export default function Foo() {}`,
        `declare export default function Foo(): void;`,
      );
    });
    it('export default class', async () => {
      await expectTranslate(
        `export default class Foo {}`,
        `declare export default class Foo {}`,
      );
    });
    it('export default expression', async () => {
      await expectTranslate(
        `export default (1: number);`,
        `declare export default number;`,
      );
    });
    it('export default var', async () => {
      await expectTranslate(
        `function foo() {}
         export default foo;`,
        `declare function foo(): void;
         declare export default typeof foo;`,
      );
    });
    describe('doc comments', () => {
      const source = `'use strict';
        /** Foo documentation */
        function Foo(): void {}
        export default Foo;`;
      const namedExportSource = `'use strict';
        /** Foo documentation */
        export function Foo(): void {}
        export default Foo;`;

      it('leaves the declaration comment unchanged by default', async () => {
        const expected = trimToBeCode(`/** Foo documentation */
          declare function Foo(): void;
          declare export default typeof Foo;`);

        expect(await translate(source)).toBe(expected);
        expect(
          await translate(source, {defaultExportDocPlacement: 'declaration'}),
        ).toBe(expected);
      });

      it('moves the declaration comment to the default export', async () => {
        await expectTranslate(
          source,
          `declare function Foo(): void;
           /** Foo documentation */
           declare export default typeof Foo;`,
          {defaultExportDocPlacement: 'export'},
        );
      });

      it('clones the declaration comment onto the default export', async () => {
        await expectTranslate(
          source,
          `/** Foo documentation */
           declare function Foo(): void;
           /** Foo documentation */
           declare export default typeof Foo;`,
          {defaultExportDocPlacement: 'both'},
        );
      });

      it('leaves a named export comment unchanged by default', async () => {
        const expected = trimToBeCode(`/** Foo documentation */
          declare export function Foo(): void;
          declare export default typeof Foo;`);

        expect(await translate(namedExportSource)).toBe(expected);
        expect(
          await translate(namedExportSource, {
            defaultExportDocPlacement: 'declaration',
          }),
        ).toBe(expected);
      });

      // Moving leaves the named export itself undocumented, which is what
      // `export` means; `both` is the placement that documents each symbol.
      it('moves a named export comment to the default export', async () => {
        await expectTranslate(
          namedExportSource,
          `declare export function Foo(): void;
           /** Foo documentation */
           declare export default typeof Foo;`,
          {defaultExportDocPlacement: 'export'},
        );
      });

      it('preserves a named export comment when cloning', async () => {
        await expectTranslate(
          namedExportSource,
          `/** Foo documentation */
           declare export function Foo(): void;
           /** Foo documentation */
           declare export default typeof Foo;`,
          {defaultExportDocPlacement: 'both'},
        );
      });

      it('does not override a comment already on the default export', async () => {
        await expectTranslate(
          `'use strict';
           /** Declaration documentation */
           function Foo(): void {}
           /** Export documentation */
           export default Foo;`,
          `/** Declaration documentation */
           declare function Foo(): void;
           /** Export documentation */
           declare export default typeof Foo;`,
          {defaultExportDocPlacement: 'export'},
        );
      });

      it('leaves a directly exported class comment on the class', async () => {
        await expectTranslate(
          `'use strict';
           /** Foo documentation */
           class Foo {}
           export default Foo;`,
          `/** Foo documentation */
           declare class Foo {}
           declare export default typeof Foo;`,
          {defaultExportDocPlacement: 'export'},
        );
      });

      it('uses a wrapper display name to find the documented declaration', async () => {
        const result = await translate(
          `'use strict';
           /** Foo documentation */
           function Foo(): void {}
           const MemoedFoo: typeof Foo = Foo;
           MemoedFoo.displayName = 'Foo';
           export default MemoedFoo;`,
          {defaultExportDocPlacement: 'export'},
        );

        expect(result.split('Foo documentation')).toHaveLength(2);
        expect(result).toMatch(
          /declare const MemoedFoo: typeof Foo;\s+\/\*\* Foo documentation \*\/\s+declare export default typeof MemoedFoo;/,
        );
      });
    });
    it('export default object expression', async () => {
      await expectTranslate(
        `const add = (a: number, b: number) => a + b;
         export default {add};`,
        `declare const add: (a: number, b: number) => void;
         declare export default {add: typeof add};`,
      );
    });
    it('export default member expression', async () => {
      await expectTranslate(
        `import {foo} from 'foo';
         export default foo.bar;`,
        `import {foo} from 'foo';
         declare export default typeof foo.bar;`,
      );
    });
    it('export default object with member expression', async () => {
      await expectTranslate(
        `import {foo} from 'foo';
         export default {bar: foo.bar};`,
        `import {foo} from 'foo';
         declare export default {bar: typeof foo.bar};`,
      );
    });
  });
  describe('ExportAllDeclaration', () => {
    it('export basic', async () => {
      await expectTranslateUnchanged(`export * from 'Foo';`);
    });
  });
  describe('module.exports', () => {
    it('export basic', async () => {
      await expectTranslate(
        `module.exports = 1;`,
        `declare module.exports: 1;`,
      );
    });
  });
  describe('exports.*', () => {
    it('export basic', async () => {
      await expect(async () => translate(`exports.A = 1;`)).rejects
        .toThrowErrorMatchingInlineSnapshot(`
        "
        > 1 | exports.A = 1;
            | ^^^^^^^^^^^^^^ convertExport: Named CommonJS exports not supported. Use either \`module.exports = {...}\` or ES6 exports."
      `);
      await expect(async () => translate(`module.exports.A = 1;`)).rejects
        .toThrowErrorMatchingInlineSnapshot(`
        "
        > 1 | module.exports.A = 1;
            | ^^^^^^^^^^^^^^^^^^^^^ convertExport: Named CommonJS exports not supported. Use either \`module.exports = {...}\` or ES6 exports."
      `);
    });
  });
  describe('FunctionDeclaration', () => {
    it('basic', async () => {
      await expectTranslate(
        `export function foo(): void {}`,
        `declare export function foo(): void;`,
      );
    });
    it('sync without return type', async () => {
      await expectTranslate(
        `export function foo() {}`,
        `declare export function foo(): void;`,
      );
    });
    it('async without return type', async () => {
      await expectTranslate(
        `export async function foo() {}`,
        `declare export function foo(): Promise<void>;`,
      );
    });
    it('with type params', async () => {
      await expectTranslate(
        `export function foo<T>(): T {}`,
        `declare export function foo<T>(): T;`,
      );
    });
    it('with params', async () => {
      await expectTranslate(
        `export function foo(bar: string, baz: number): void {}`,
        `declare export function foo(bar: string, baz: number): void;`,
      );
    });
    it('with rest params', async () => {
      await expectTranslate(
        `export function foo(bar: string, ...baz: Array<number>): void {}`,
        `declare export function foo(bar: string, ...baz: Array<number>): void;`,
      );
    });
    it('with destructured rest params', async () => {
      await expectTranslate(
        `export function foo(...{bar}: {bar: string}): void {}`,
        `declare export function foo(...$$PARAM_0$$: {bar: string}): void;`,
      );
    });
    it('with default params', async () => {
      await expectTranslate(
        `export function foo(bar: string = 'hello'): void {}`,
        `declare export function foo(bar?: string): void;`,
      );
    });
    it('without identifier', async () => {
      await expectTranslate(
        `export function foo({foo = 'foo'}: Foo): void {}`,
        `declare export function foo($$PARAM_0$$: Foo): void;`,
      );
    });
    it('without identifier with default param', async () => {
      await expectTranslate(
        `export function foo({foo = 'foo'}: Foo = {}): void {}`,
        `declare export function foo($$PARAM_0$$?: Foo): void;`,
      );
    });
    it('with predicates', async () => {
      await expectTranslate(
        `function bar(baz: string): boolean %checks {
          return baz === '';
         }
         export function foo(): boolean %checks {
           return bar('');
         }`,
        `declare function bar(baz: string): boolean %checks(baz === '');
         declare export function foo(): boolean %checks(bar(''));`,
      );
    });
  });
  describe('TypeAlias', () => {
    it('basic', async () => {
      await expectTranslateUnchanged(`export type Foo = string;`);
    });
    it('with type params', async () => {
      await expectTranslateUnchanged(
        `export type Foo<Bar: Baz, Boo> = string;`,
      );
    });
  });
  describe('OpaqueType', () => {
    it('basic', async () => {
      await expectTranslate(
        `export opaque type Foo = string;`,
        `declare export opaque type Foo;`,
      );
    });
    it('basic local', async () => {
      await expectTranslate(
        `type Foo = string;
         opaque type Bar = Foo;
         export type Baz = Bar;`,
        `declare opaque type Bar;
         export type Baz = Bar;`,
      );
    });
    it('with type params', async () => {
      await expectTranslate(
        `export opaque type Foo<Bar: Baz, Boo> = string;`,
        `declare export opaque type Foo<Bar: Baz, Boo>;`,
      );
    });
    it('with super type', async () => {
      await expectTranslate(
        `export opaque type Foo: Bar = string;`,
        `declare export opaque type Foo: Bar;`,
      );
    });
    it('with super type and type params', async () => {
      await expectTranslate(
        `export opaque type Foo<Bar: Baz, Boo>: Boa = string;`,
        `declare export opaque type Foo<Bar: Baz, Boo>: Boa;`,
      );
    });
  });
  describe('ImportDeclaration', () => {
    it('basic', async () => {
      await expectTranslateUnchanged(
        `import type {Foo} from 'foo';
         export type {Foo};`,
      );
    });
    it('type specifiers', async () => {
      await expectTranslateUnchanged(
        `import {type Foo} from 'foo';
         export type {Foo};`,
      );
    });
  });
  describe('ClassDeclaration', () => {
    it('property', async () => {
      await expectTranslate(
        `export class A {
           /** foo documentation */
           foo: string = '';
         }`,
        `declare export class A {
           /** foo documentation */
           foo: string;
         }`,
      );
      await expectTranslate(
        `export class A {
           'foo': string = '';
         }`,
        `declare export class A {
           foo: string;
         }`,
      );
      await expectTranslate(
        `export class A {
           1: string = '';
         }`,
        `declare export class A {
           1: string;
         }`,
      );
      await expectTranslate(
        `export class A {
           foo = () => {};
           static foo = () => {};
         }`,
        `declare export class A {
           foo: () => void;
           static foo: () => void;
         }`,
      );
      await expectTranslate(
        `export class A {
           foo = async () => {};
           static foo = async () => {};
         }`,
        `declare export class A {
           foo: () => Promise<void>;
           static foo: () => Promise<void>;
         }`,
      );
      await expectTranslate(
        `export class A {
           foo: (val: string) => number = (val: string) => { return 1 };
           static foo: (val: string) => number = (val: string) => { return 1 };
         }`,
        `declare export class A {
           foo: (val: string) => number;
           static foo: (val: string) => number;
         }`,
      );
      await expectTranslate(
        `export class A {
           foo = (val: string): number => { return 1 };
           static foo = (val: string): number => { return 1 };
         }`,
        `declare export class A {
           foo: (val: string) => number;
           static foo: (val: string) => number;
         }`,
      );
    });
    it('method', async () => {
      await expectTranslate(
        `export class A {
           /** foo documentation */
           foo() {}
           /** static bar documentation */
           static bar() {}
         }`,
        `declare export class A {
           /** foo documentation */
           foo(): void;
           /** static bar documentation */
           static bar(): void;
         }`,
      );
      await expectTranslate(
        `export class A {
           async foo() {}
           static async bar() {}
         }`,
        `declare export class A {
           foo(): Promise<void>;
           static bar(): Promise<void>;
         }`,
      );
      await expectTranslate(
        `export class A {
           'foo'() {}
           static 'bar'() {}
         }`,
        `declare export class A {
           foo(): void;
           static bar(): void;
         }`,
      );
      await expectTranslate(
        `export class A {
           1() {}
           static 2() {}
         }`,
        `declare export class A {
           1(): void;
           static 2(): void;
         }`,
      );
      await expectTranslate(
        `export class A {
           [Symbol.iterator]() {}
           static get [Symbol.asyncIterator]() {}
         }`,
        `declare export class A {
           @@iterator(): void;
           static get @@asyncIterator(): void;
         }`,
      );
    });
    it('strips munged underscore members', async () => {
      await expectTranslate(
        `export class A {
           _private: string = '';
           public: string = '';
           _alsoPrivate(): void {}
           alsoPublic(): void {}
         }`,
        `declare export class A {
           public: string;
           alsoPublic(): void;
         }`,
      );
    });
    it('keeps double-underscore members', async () => {
      await expectTranslate(
        `export class A {
           __notPrivate: string = '';
           _private: string = '';
         }`,
        `declare export class A {
           __notPrivate: string;
         }`,
      );
    });
    it('keeps single-char underscore member', async () => {
      await expectTranslate(
        `export class A {
           _: string = '';
         }`,
        `declare export class A {
           _: string;
         }`,
      );
    });
    it('strips static munged members', async () => {
      await expectTranslate(
        `export class A {
           static _privateStatic: number = 1;
           static publicStatic: number = 1;
           static _privateMethod(): void {}
           static publicMethod(): void {}
         }`,
        `declare export class A {
           static publicStatic: number;
           static publicMethod(): void;
         }`,
      );
    });
    it('strips munged members and their unused imports', async () => {
      await expectTranslate(
        `import PrivateDep from 'PrivateDep';
         import PublicDep from 'PublicDep';
         export class A {
           _private: PrivateDep;
           public: PublicDep;
         }`,
        `import PublicDep from 'PublicDep';
         declare export class A {
           public: PublicDep;
         }`,
      );
    });
    it('keeps munged members when mungeUnderscores is false', async () => {
      const code = `export class A {
           _private: string = '';
           public: string = '';
         }`;
      const result = await translate(code, {mungeUnderscores: false});
      expect(result).toBe(
        trimToBeCode(
          `declare export class A {
             _private: string;
             public: string;
           }`,
        ),
      );
    });
    it('definite assignment assertion', async () => {
      await expectTranslate(
        `export class A {
           foo!: string;
           #bar!: number;
         }`,
        `declare export class A {
           foo: string;
         }`,
      );
    });
  });
  describe('InterfaceDeclaration', () => {
    it('property', async () => {
      await expectTranslate(
        `export interface A {
           foo: string;
         }`,
        `export interface A {
           foo: string;
         }`,
      );
    });
    it('method', async () => {
      await expectTranslate(
        `export interface A {
           foo(): void;
         }`,
        `export interface A {
           foo(): void;
         }`,
      );
    });
    it('local', async () => {
      await expectTranslate(
        `interface Foo {}
         export type Bar = Foo;`,
        `interface Foo {}
         export type Bar = Foo;`,
      );
    });
  });
  describe('VariableDeclaration', () => {
    it('default require of class used as type', async () => {
      await expectTranslate(
        `const MyClass = require('MyClass');
         declare export class Wrapper {
           getInner(): MyClass<string>;
         }`,
        `import MyClass from 'MyClass';
         declare export class Wrapper {
           getInner(): MyClass<string>;
         }`,
      );
    });
    it('default require of function used with typeof', async () => {
      await expectTranslate(
        `const processData = require('processData');
         export type Pipeline = {
           transform: typeof processData,
           label: string,
         };`,
        `import processData from 'processData';
         export type Pipeline = {
           transform: typeof processData,
           label: string,
         };`,
      );
    });
    it('default require transitive dep', async () => {
      await expectTranslate(
        `const Foo = require('foo');
         const Bar = Foo;
         export type Baz = typeof Bar;`,
        `import Foo from 'foo';
         declare const Bar: typeof Foo;
         export type Baz = typeof Bar;`,
      );
    });
    it('default require member access', async () => {
      await expectTranslate(
        `const Utils = require('Utils');
         const normalize = Utils.normalize;
         export type Transformer = typeof normalize;`,
        `import Utils from 'Utils';
         declare const normalize: typeof Utils.normalize;
         export type Transformer = typeof normalize;`,
      );
    });
    it('destructured require all specifiers used', async () => {
      await expectTranslate(
        `const {normalize, format, DEFAULT_TIMEOUT} = require('Utils');
         export type Config = {
           transformer: typeof normalize,
           formatter: typeof format,
           timeout: typeof DEFAULT_TIMEOUT,
         };`,
        `import {normalize, format, DEFAULT_TIMEOUT} from 'Utils';
         export type Config = {
           transformer: typeof normalize,
           formatter: typeof format,
           timeout: typeof DEFAULT_TIMEOUT,
         };`,
      );
    });
    it('destructured require strips unused specifiers', async () => {
      await expectTranslate(
        `const {normalize, format} = require('Utils');
         export type Formatter = typeof format;`,
        `import {format} from 'Utils';
         export type Formatter = typeof format;`,
      );
    });
    it('destructured require with rename', async () => {
      await expectTranslate(
        `const {foo: bar} = require('source');
         export type Baz = typeof bar;`,
        `import {foo as bar} from 'source';
         export type Baz = typeof bar;`,
      );
    });
    it('unused default require is stripped', async () => {
      await expectTranslate(
        `const Foo = require('foo');
         export type Bar = string;`,
        `export type Bar = string;`,
      );
    });
    it('unused destructured require is stripped', async () => {
      await expectTranslate(
        `const {Foo} = require('foo');
         export type Bar = string;`,
        `export type Bar = string;`,
      );
    });
    it('mixed default and destructured requires', async () => {
      await expectTranslate(
        `const MyClass = require('MyClass');
         const {normalize} = require('Utils');
         declare export class Processor {
           getInstance(): MyClass<string>;
         }`,
        `import MyClass from 'MyClass';
         declare export class Processor {
           getInstance(): MyClass<string>;
         }`,
      );
    });
    it('basic type parameter', async () => {
      await expectTranslate(
        `export const foo: number = 1;`,
        `declare export const foo: number;`,
      );
    });
    it('definite assignment assertion', async () => {
      await expectTranslate(
        `export let foo!: number;`,
        `declare export let foo: number;`,
      );
    });
    it('basic typecast', async () => {
      await expectTranslate(
        `export const foo = (1: number);`,
        `declare export const foo: number;`,
      );
    });
    it('prefer type parameter', async () => {
      await expectTranslate(
        `export const foo: number = (1: any);`,
        `declare export const foo: number;`,
      );
    });
    it('with dependency', async () => {
      await expectTranslate(
        `const foo: number = 1;
         export const bar: typeof foo = 1;`,
        `declare const foo: number;
         declare export const bar: typeof foo;`,
      );
    });
    it('with imported value', async () => {
      await expectTranslate(
        `import {foo} from 'foo';
         export const bar = foo;`,
        `import {foo} from 'foo';
         declare export const bar: typeof foo;`,
      );
    });
    it('with object type', async () => {
      await expectTranslate(
        `export const foo = {
           /** Example documentation */
           bar: 1,
           /** This is useful */
           baz(): number { return 123; }
         };`,
        `declare export const foo: {
           /** Example documentation */
           bar: 1,
           /** This is useful */
           baz(): number,
         };`,
      );
    });
  });
  describe('EnumDeclaration', () => {
    it('basic', async () => {
      await expectTranslateUnchanged(`export enum Foo {}`);
    });
    it('local', async () => {
      await expectTranslateUnchanged(
        `enum Foo {}
         declare export const bar: Foo;`,
      );
    });
  });
  describe('DeclareClass', () => {
    it('basic', async () => {
      await expectTranslateUnchanged(
        `declare class Foo {}
         declare export const bar: Foo;`,
      );
    });
    it('complex', async () => {
      await expectTranslateUnchanged(
        `declare export class Foo<T>
           extends TClass<T>
           mixins TMixin<T>
           implements TInterface<T>
         {
           prop1: T;
           method1(): T;
           [T]: string;
           constructor(): Foo;
         }`,
      );
    });
    it('extends member expression', async () => {
      await expectTranslateUnchanged(
        `declare export class Foo<T> extends Bar.TClass<T> {}`,
      );
    });
    it('extends type cast expression', async () => {
      await expectTranslate(
        `export class Foo<T> extends (Bar: X) {}`,
        `declare export class Foo<T> extends X {}`,
      );
    });
    it('extends as cast expression', async () => {
      await expectTranslate(
        `export class Foo<T> extends (Bar as X) {}`,
        `declare export class Foo<T> extends X {}`,
      );
    });
    it('extends type cast typeof expression', async () => {
      await expectTranslate(
        `export class Foo<T> extends (Bar: typeof X) {}`,
        `declare export class Foo<T> extends X {}`,
      );
    });
    it('rejects an import type cast expression', async () => {
      await expect(
        translate(`export class Foo extends (Bar as import('module')) {}`),
      ).rejects.toThrow('SuperClass: Import type not supported');
    });
  });
  describe('Expression', () => {
    async function expectTranslateExpression(
      expectExprCode: string,
      toBeExprCode: string,
    ): Promise<void> {
      await expectTranslate(
        `export const expr = ${expectExprCode};`,
        `declare export const expr: ${toBeExprCode};`,
      );
    }
    describe('Identifier', () => {
      it('basic', async () => {
        await expectTranslateExpression(`foo`, `typeof foo`);
      });
    });
    describe('ObjectExpression', () => {
      it('empty', async () => {
        await expectTranslateExpression(`{}`, `{}`);
      });
      it('methods', async () => {
        await expectTranslateExpression(`{foo() {}}`, `{foo(): void}`);
        await expectTranslateExpression(
          `{async foo() {}}`,
          `{foo(): Promise<void>}`,
        );
        await expectTranslateExpression(`{1() {}}`, `{1(): void}`);
        await expectTranslateExpression(`{'foo'() {}}`, `{foo(): void}`);
        await expectTranslateExpression(`{get foo() {}}`, `{get foo(): void}`);
        await expectTranslateExpression(`{get 1() {}}`, `{get 1(): void}`);
        await expectTranslateExpression(
          `{get 'foo'() {}}`,
          `{get foo(): void}`,
        );
        await expectTranslateExpression(
          `{set foo(bar: string) {}}`,
          `{set foo(bar: string): void}`,
        );
        await expectTranslateExpression(
          `{set 1(bar: string) {}}`,
          `{set 1(bar: string): void}`,
        );
        await expectTranslateExpression(
          `{set 'foo'(bar: string) {}}`,
          `{set foo(bar: string): void}`,
        );
      });
      it('properties', async () => {
        await expectTranslateExpression(`{FOO: 1}`, `{FOO: 1}`);
        await expectTranslateExpression(`{'foo-bar': 1}`, `{'foo-bar': 1}`);
        await expectTranslateExpression(`{1: 1}`, `{1: 1}`);
      });
      it('spread', async () => {
        await expectTranslateExpression(`{...a}`, `{...typeof a}`);
      });
    });
    describe('Literals', () => {
      it('number', async () => {
        await expectTranslateExpression(`1`, `1`);
        await expectTranslateExpression(`1.99`, `1.99`);
      });
      it('string', async () => {
        await expectTranslateExpression(`'s'`, `'s'`);
      });
      it('boolean', async () => {
        await expectTranslateExpression(`true`, `true`);
      });
      it('regex', async () => {
        await expectTranslateExpression(`/a/`, `RegExp`);
      });
      it('null', async () => {
        await expectTranslateExpression(`null`, `null`);
      });
    });
    describe('TypeCastExpression', () => {
      it('basic', async () => {
        await expectTranslateExpression(`(1: number)`, `number`);
      });
    });
    describe('AsExpression', () => {
      it('basic', async () => {
        await expectTranslateExpression(`1 as number`, `number`);
      });
    });
    describe('FunctionExpression', () => {
      it('basic', async () => {
        await expectTranslateExpression(`function foo() {}`, `() => void`);
        await expectTranslateExpression(
          `async function foo() {}`,
          `() => Promise<void>`,
        );
        await expectTranslateExpression(
          `function foo<T>(baz: T, bar: string) {}`,
          `<T>(baz: T, bar: string) => void`,
        );
      });
    });
    describe('ArrowFunctionExpression', () => {
      it('basic', async () => {
        await expectTranslateExpression(`() => {}`, `() => void`);
        await expectTranslateExpression(
          `async () => {}`,
          `() => Promise<void>`,
        );
        await expectTranslateExpression(
          `<T>(baz: T, bar: string) => {}`,
          `<T>(baz: T, bar: string) => void`,
        );
      });
    });
  });
  describe('ComponentDeclaration', () => {
    it('export', async () => {
      await expectTranslate(
        `export component Foo() {}`,
        `declare export component Foo();`,
      );
    });
    it('export default', async () => {
      await expectTranslate(
        `export default component Foo() {}`,
        `declare export default component Foo();`,
      );
    });
    it('params', async () => {
      await expectTranslate(
        `export component Foo(foo: string, 'bar' as BAR?: string) {}`,
        `declare export component Foo(foo: string, bar?: string);`,
      );
    });
    it('params with identifier-safe string literal names use individual params', async () => {
      await expectTranslate(
        `export component Foo(foo: string, 'bar' as bar?: string, 'baz' as qux: number) {}`,
        `declare export component Foo(foo: string, bar?: string, baz: number);`,
      );
    });
    it('params with non-identifier string literal names use props object', async () => {
      await expectTranslate(
        `export component Foo(foo: string, 'data-bar' as dataBar?: number) {}`,
        `declare export component Foo(...props: {foo: string, 'data-bar'?: number});`,
      );
    });
    it('params with comments', async () => {
      await expectTranslate(
        `export component Foo(
          /** The foo param */
          foo: string,
          /** The bar param */
          bar?: number,
        ) {}`,
        `declare export component Foo(
          /** The foo param */
          foo: string,
          /** The bar param */
          bar?: number,
        );`,
      );
    });
    it('default params', async () => {
      await expectTranslate(
        `export component Foo(foo: string = '') {}`,
        `declare export component Foo(foo?: string);`,
      );
    });
    it('rest param', async () => {
      await expectTranslate(
        `export component Foo(...foo: {...}) {}`,
        `declare export component Foo(...foo: {...});`,
      );
    });
    it('params with rest param', async () => {
      await expectTranslate(
        `export component Foo(foo: string, ...rest: {bar: number}) {}`,
        `declare export component Foo(foo: string, ...rest: {bar: number});`,
      );
    });
    it('non-identifier string literal params with rest param', async () => {
      await expectTranslate(
        `export component Foo('data-x' as dataX: string, ...rest: {bar: number}) {}`,
        `declare export component Foo(...props: {'data-x': string, ...{bar: number}});`,
      );
    });
    it('non-identifier string literal params with typed rest param', async () => {
      await expectTranslate(
        `type Props = {bar: number};
         export component Foo('data-x' as dataX: string, ...rest: Props) {}`,
        `type Props = {bar: number};
         declare export component Foo(...props: {'data-x': string, ...Props});`,
      );
    });
    it('destructured rest param', async () => {
      await expectTranslate(
        `export component Foo(...{foo}: {...}) {}`,
        `declare export component Foo(...rest: {...});`,
      );
    });
    it('renders type', async () => {
      await expectTranslate(
        `type T = Bar;
         export component Foo() renders T {}`,
        `type T = Bar;
         declare export component Foo() renders T;`,
      );
    });
  });
  describe('HookDeclaration', () => {
    it('basic', async () => {
      await expectTranslate(
        `export hook useFoo(): string { return ''; }`,
        `declare export hook useFoo(): string;`,
      );
    });
    it('with params', async () => {
      await expectTranslate(
        `export hook useFoo(x: number, y?: string): boolean { return true; }`,
        `declare export hook useFoo(x: number, y?: string): boolean;`,
      );
    });
    it('with type params', async () => {
      await expectTranslate(
        `export hook useFoo<T>(x: T): T { return x; }`,
        `declare export hook useFoo<T>(x: T): T;`,
      );
    });
    it('without return type', async () => {
      await expectTranslate(
        `export hook useFoo() {}`,
        `declare export hook useFoo(): void;`,
      );
    });
  });
});
