/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @format
 * @flow
 */

import {testCodemod} from '../../../testUtils/codemodTestUtils';
import nameFunctionTypeParams from '../nameFunctionTypeParams';

testCodemod('nameFunctionTypeParams', nameFunctionTypeParams, {
  // IGNORED
  ignored: [
    {
      description: 'all params already named',
      code: `type T = (x: number, y: string) => void;`,
    },
    {
      description: 'named rest param',
      code: `type T = (x: number, ...args: Array<string>) => void;`,
    },
    {
      description: 'no params',
      code: `type T = () => void;`,
    },
    {
      description: 'declare function with named params',
      code: `declare function foo(x: number, ...rest: Array<string>): void;`,
    },
    {
      description: 'regular function',
      code: `function foo(x: number): void {}`,
    },
  ],

  // TRANSFORMED
  transformed: [
    {
      description: 'single unnamed param',
      code: `type T = (number) => void;`,
      output: `type T = (v: number) => void;`,
    },
    {
      description: 'single unnamed param without parens',
      code: `type T = number => void;`,
      output: `type T = (v: number) => void;`,
    },
    {
      description: 'multiple unnamed params',
      code: `type T = (number, string, boolean) => void;`,
      output: `type T = (v0: number, v1: string, v2: boolean) => void;`,
    },
    {
      description: 'mixed named and unnamed params',
      code: `type T = (x: number, string, y: boolean) => void;`,
      output: `type T = (x: number, v1: string, y: boolean) => void;`,
    },
    {
      description: 'unnamed rest param',
      code: `type T = (...Array<number>) => void;`,
      output: `type T = (...rest: Array<number>) => void;`,
    },
    {
      description: 'unnamed params with unnamed rest',
      code: `type T = (number, string, ...Array<boolean>) => void;`,
      output: `type T = (v0: number, v1: string, ...rest: Array<boolean>) => void;`,
    },
    {
      description: 'nullable type unnamed param',
      code: `type T = (number, ?string) => void;`,
      output: `type T = (v0: number, v1: ?string) => void;`,
    },
    {
      description: 'generic function type',
      code: `type T = <U>(U) => U;`,
      output: `type T = <U>(v: U) => U;`,
    },
    {
      description: 'this param is kept and not counted',
      code: `type T = (this: Foo, string) => void;`,
      output: `type T = (this: Foo, v: string) => void;`,
    },
    {
      description: 'nested function types',
      code: `type T = ((string) => void) => void;`,
      output: `type T = (v: (v: string) => void) => void;`,
    },
    {
      description: 'generated name does not collide with an existing name',
      code: `type T = (v1: number, string) => void;`,
      output: `type T = (v1: number, v1_: string) => void;`,
    },
    {
      description: 'variable annotation',
      code: `let f: (string) => void;`,
      output: `let f: (v: string) => void;`,
    },
    {
      description: 'object type method',
      code: `type T = {m(string): void};`,
      output: `type T = {m(v: string): void};`,
    },
    {
      description: 'object type call property',
      code: `type T = {(string): void};`,
      output: `type T = {(v: string): void};`,
    },
    {
      description: 'declare class method',
      code: `
declare class Foo {
  bar(string, number): void;
}`,
      output: `
declare class Foo {
  bar(v0: string, v1: number): void;
}`,
    },
    {
      description: 'declare function',
      code: `declare function foo(number): void;`,
      output: `declare function foo(v: number): void;`,
    },
    {
      description: 'declare function with mixed params and unnamed rest',
      code: `declare function foo(x: number, string, ...Array<boolean>): void;`,
      output: `declare function foo(x: number, v1: string, ...rest: Array<boolean>): void;`,
    },
    {
      description: 'declare export function',
      code: `declare export function foo(number, string): void;`,
      output: `declare export function foo(v0: number, v1: string): void;`,
    },
    {
      description: 'function type nested in a declare function signature',
      code: `declare function foo((string) => void): void;`,
      output: `declare function foo(v: (v: string) => void): void;`,
    },
    {
      description: 'hook type',
      code: `type T = hook (string) => void;`,
      output: `type T = hook (v: string) => void;`,
    },
    {
      description: 'declare hook',
      code: `declare hook useFoo(string): void;`,
      output: `declare hook useFoo(v: string): void;`,
    },
  ],
});
