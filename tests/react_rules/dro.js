declare const a: $ReactDeepReadOnly<{x: {y: number}}>;
a.x.y = 42; // error
a.x = 42; // error

declare const x: $ReactDeepReadOnly<{x: {a: number}, y: Array<{w: {v: number}}>}>;
const y = x.y;
const z = y[0]
const w = z.w;
const v = w.v;
v as empty; //error
w.v = 0; // error
x.x.a = 42; // error
x.x.y = []; // error
x.x = 42 as any;

declare const rr: $ReactDeepReadOnly<{current: number}>;
rr.current = 42; // ok

declare const w3: $ReactDeepReadOnly<{a: {b: number}, c: number}>;
const w2 = {...w3};
w2.c = 42; // ok
w2.a.b = 42; // error

declare const v3: $ReactDeepReadOnly<Array<{a: number}>>;
const v2 = [...v3];
v2[0] = {a: 42}; // ok
v2[1].a = 42; //error

import * as React from 'react';

component Foo1(x: { y: number}) {
    x.y = 42 // error;
    return null;
}
component Foo2(...rest: {bar: number, baz: Array<string>, qux:[number, Array<string>]}) {
    rest.bar = 42; // error;
    rest.baz.push("hello"); // error
    rest.qux[0] = 42; // error
    rest.qux[1].pop(); // error
    return null;
}

component A(x: {bar: number}) { return <A x={x} />; }; // ok

export type Union = 'A' | 'B';

component UnionToElemTShortcut(
    key: Union,
    data: {
      [key: Union]: {
        prop?: number,
      },
    },
  ) {
    const obj = data[key];
    obj.prop = 0; // error
    return null;
}

component NonInterferenceWithInference(arr: Array<number>) {
  arr.sort((a, b) => { // only dro error
      return 1;
  });
  return null;
}

type MaybeUnion = 'A' | 'B' | null;

component RefineAliasMember(x: MaybeUnion | void) {
  if (x != null) {
    x as Union; // ok
  }
  return null;
}

const Enum = Object.freeze({A: '', B: 'b'});

component RefineValuesMember(x: Values<typeof Enum> | void) {
  if (x) {
    x as 'b'; // ok
  }
  return null;
}

component RefineMaybeAliasMember(x: ?MaybeUnion) {
  if (x != null) {
    x as Union; // ok
  }
  return null;
}

component RefineMaybeValuesMember(x: ?Values<typeof Enum>) {
  if (x) {
    x as 'b'; // ok
  }
  return null;
}
