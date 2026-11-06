// @flow

// Enum member sites frame like member references, even though members carry
// no types of their own.
enum E {A, B}
//      ^
//         ^
//   ^

const x = E.A;
//        ^

const y = E.A;
//          ^

const z: E = E.A;
//       ^

type T = Class<E>;
//   ^

import type {F} from './export.js';
//           ^
declare const f: F;
//            ^
declare const f2: F;
//                ^

import type G from './export.js';
//          ^
declare const g: G;
//            ^
declare const g2: G;
//                ^

import H from './export.js';
//     ^
declare const h: H;
//            ^
declare const h2: H;
//                ^

import type {FType} from './export.js';
//           ^
declare const i: FType;
//            ^
declare const i2: FType;
//                ^

const j = E.cast('A') ?? E.A;
//    ^

const k = true ? E : 1;
//    ^

// An enum's own methods live in the same object as its members, but are not
// members of it.
E.cast('A');
//^
