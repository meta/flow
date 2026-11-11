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
import nameIndexerKeys from '../nameIndexerKeys';

testCodemod('nameIndexerKeys', nameIndexerKeys, {
  // IGNORED
  ignored: [
    {
      description: 'labeled indexer',
      code: `type T = {[k: string]: number};`,
    },
    {
      description: 'string and number literal keys are properties',
      code: `type T = {['a']: number, [1]: number, [-1]: number};`,
    },
    {
      description: 'keys that name a value are properties',
      code: `
import {KEY} from './m';
const Keys = {A: 'a'};
type T = {[KEY]: number, [Keys.A]: number};
interface I {
  [KEY]: number;
}`,
    },
    {
      description: 'mapped type',
      code: `type T<O: {...}> = {[K in keyof O]: O[K]};`,
    },
    {
      description: 'internal slot',
      code: `type T = {[[call]]: () => void};`,
    },
    {
      description: 'no indexer',
      code: `type T = {foo: number};`,
    },
  ],

  // TRANSFORMED
  transformed: [
    {
      description: 'unlabeled indexer',
      code: `type T = {[string]: number};`,
      output: `type T = {[key: string]: number};`,
    },
    {
      description: 'unlabeled indexer with variance',
      code: `type T = {+[string]: number};`,
      output: `type T = {+[key: string]: number};`,
    },
    {
      description: 'unlabeled indexer next to properties',
      code: `type T = {foo: boolean, [string]: number};`,
      output: `type T = {foo: boolean, [key: string]: number};`,
    },
    {
      description: 'unlabeled indexer next to a literal key',
      code: `type T = {['a']: boolean, [string]: number};`,
      output: `type T = {['a']: boolean, [key: string]: number};`,
    },
    {
      description: 'keys that name a type, a namespace member or nothing',
      code: `
import * as NS from './m';
const K = 'k';
type K = string;
type T<V> = {[K]: number, [NS.Key]: number, [Global]: number, [V]: number};`,
      output: `
import * as NS from './m';
const K = 'k';
type K = string;
type T<V> = {
  [key: K]: number,
  [key: NS.Key]: number,
  [key: Global]: number,
  [key: V]: number,
};`,
    },
    {
      description: 'exact object',
      code: `type T = {|[string]: number|};`,
      output: `type T = {|[key: string]: number|};`,
    },
    {
      description: 'nested indexers',
      code: `type T = {[string]: {[number]: boolean}};`,
      output: `type T = {[key: string]: {[key: number]: boolean}};`,
    },
    {
      description: 'indexer in a type argument',
      code: `let x: Array<{[string]: number}> = [];`,
      output: `let x: Array<{[key: string]: number}> = [];`,
    },
    {
      description: 'interface',
      code: `
interface I {
  [string]: number;
}`,
      output: `
interface I {
  [key: string]: number;
}`,
    },
    {
      description: 'declare class static indexer',
      code: `
declare class C {
  static [string]: number;
}`,
      output: `
declare class C {
  static [key: string]: number;
}`,
    },
  ],
});
