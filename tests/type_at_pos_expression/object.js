// @flow

declare const obj: {a?: {b: ?{c: null | {d: number, ...}, ...}, ...}, ...};
//              ^
//                  ^
//                       ^
//                            ^
//                                       ^

let _ =
  // TODO can we do better with duplication?
  obj.a ?
//  ^
//    ^
    (obj.a.b ?
//       ^
//         ^
      (obj.a.b.c ?
//           ^
//             ^
        (obj.a.b.c.d ? obj.a.b.c.d : null) :
//               ^
//                               ^
        null) :
      null) :
    null;

let myobj = {
  m(n: string) { return n; },
//^
  n: (k: number) => k
//^
};

myobj.m;
//^
//    ^
myobj.m("a");
//    ^
myobj.n;
//    ^
myobj.n(1);
//    ^

declare const litobj: {
  f: "ff";
  g: "g\"g";
  h: 'h"h';
  i: "i'i";
  j: '\'';
  d: 11;
  x: 0x111;
  b: 0b111;
 ...}

  litobj;
//^

const frozenObject1 = Object.freeze({ A: "a", B: "b" });
//    ^

const frozenObject2 = Object.freeze({ A: "a", B: { C: "c"} });
//    ^

obj.constructor = () => {};
//  ^

obj.constructor = <T>(x: T): T => x;
//  ^
