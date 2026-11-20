// @flow strict

import type {T} from './import'; // Error: untyped-type-import
import type {S} from './nonstrict_export'; // Error: nonstrict-import

0 as T;

function f(x: number) {
  x = 1; // Error: cannot reassign constant parameter
}

function g(o: {a: number}) {
  o?.a; // Error: unnecessary-optional-chain
}
