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
import typeParameterBoundExactEmptyObjectToInexact from '../typeParameterBoundExactEmptyObjectToInexact';

testCodemod(
  'typeParameterBoundExactEmptyObjectToInexact',
  typeParameterBoundExactEmptyObjectToInexact,
  {
    ignored: [
      {
        description: 'Unbounded parameter',
        code: 'function foo<T>() {}',
      },
      {
        description: 'Already-inexact bound',
        code: 'function foo<T: {...}>() {}',
      },
      {
        description: 'Property-bearing bound',
        code: 'function foo<T: {x: string}>() {}',
      },
      {
        description: 'Empty-object default is not a bound',
        code: 'function foo<T = {}>() {}',
      },
      {
        description: 'Second pass leaves transformed bounds unchanged',
        code: `\
function foo<
  A: {...},
  B: string,
  C: {...},
  D: boolean,
  E: {[string]: boolean},
  F: {(): void},
>() {}
        `,
      },
    ],

    transformed: [
      {
        description: 'Exact empty object tparams bound',
        code: `\
function foo<A: {}, B: string, C: {||}, D: boolean, E: {[string]: boolean}, F: {(): void}>() {}
       `,
        output: `\
function foo<
  A: {...},
  B: string,
  C: {...},
  D: boolean,
  E: {[string]: boolean},
  F: {(): void},
>() {}
       `,
      },
    ],
  },
);
