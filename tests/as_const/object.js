// Definitions

const p = {f: 1};
declare const q: {f: number};
declare const d: {[key: string]: number};

export const obj1 = {f: 1} as const;      // {+f: 1}
export const obj2 = {p, q, d} as const;   // {+d: {[string]: number}, +p: {f: Num<1>}, +q: {f: number}}
export const obj3 = {...{f: 1}} as const; // {+f: 1}
export const obj4 = {...p} as const;      // {+f: Num<1>}
export const obj5 = {...q} as const;      // {+f: number}
export const obj6 = {...d} as const;      // {+[string]: number}
export const obj7 = {m() {}} as const;    // {m(): void}
export const obj8 = {f: {g: 1}} as const; // {+f: {+g: 1}}
export const obj9 = {...{f: {...{g: {...{h: 1}}}}}} as const; // {+f: {+g: {+h: 1}}}
export const obj10 = Object.freeze({f: 1, g: {h:1}} as const); // {+f: 1, +g: {+h: 1}}

declare const _: any;

// Tests

function test_obj1() {
  obj1 as {f: 1}; // error cast to non-readonly
  obj1 as {readonly f: 1}; // okay
  obj1 as {readonly f: 1, ...}; // okay
  obj1 as {readonly f: number}; // okay

  _ as {f: 1} as typeof obj1; // okay
  _ as {readonly f: 1} as typeof obj1; // okay
  _ as {readonly f: 1, ...} as typeof obj1; // error inexact ~> exact
  _ as {f: number} as typeof obj1; // error number ~> 1
}

function test_obj2() {
  obj2 as {readonly p: {f: number }, readonly q: {f: number}, readonly d: {[key: string]: number}};
  _ as {
    readonly p: {f: number},
    readonly q: {f: number},
    readonly d: {[key: string]: number},
  } as typeof obj2;
}

function test_obj3() {
  obj3 as {f: 1}; // error cast to non-readonly
  obj3 as {readonly f: 1}; // okay
  obj3 as {readonly f: 1, ...}; // okay
  obj3 as {readonly f: number}; // okay

  _ as {f: 1} as typeof obj3; // okay
  _ as {readonly f: 1} as typeof obj3; // okay
  _ as {readonly f: 1, ...} as typeof obj3; // error inexact ~> exact
  _ as {f: number} as typeof obj3; // error number ~> 1
}

function test_obj4() {
  obj4 as {f: number}; // error cast to non-readonly
  obj4 as {readonly f: number}; // okay
  obj4 as {readonly f: number, ...}; // okay
  obj4 as {readonly f: number}; // okay

  _ as {f: 1} as typeof obj4; // okay
  _ as {readonly f: 1} as typeof obj4; // okay
  _ as {readonly f: 1, ...} as typeof obj4; // error inexact ~> exact
  _ as {f: number} as typeof obj4; // okay number ~> Num<1>
}

function test_obj5() {
  obj5 as {f: number}; // error cast to non-readonly
  obj5 as {readonly f: number}; // okay
  obj5 as {readonly f: 1, ...}; // error number ~> 1

  _ as {f: number} as typeof obj5; // okay
  _ as {readonly f: number} as typeof obj5; // okay
  _ as {readonly f: number, ...} as typeof obj5; // error inexact ~> exact
}

function test_obj6() {
  obj6 as {[key: string]: number}; // error readonly ~> non-readonly
  obj6 as {readonly [key: string]: number}; // okay

  _ as {[key: string]: number} as typeof obj6; // okay
  _ as {readonly [key: string]: number} as typeof obj6; // okay
}

function test_obj7() {
  obj7 as {m(): void}; // okay m is readonly
  obj7 as {m: () => void}; // error m is readonly in obj7
  obj7 as {readonly m: () => void}; // okay

  _ as {m(): void} as typeof obj7; // okay
  _ as {readonly m: () => void} as typeof obj7; // okay
  _ as {m: () => void} as typeof obj7; // okay
}

function test_obj8() {
  obj8 as {readonly f: {g: 1}}; // error cast to non-readonly g
  obj8 as {readonly f: {readonly g: 1}}; // okay
  obj8 as {readonly f: {readonly g: 1, ...}}; // okay
  obj8 as {readonly f: {readonly g: number}}; // okay

  _ as {f: {g: 1}} as typeof obj8; // okay
  _ as {readonly f: {g: 1}} as typeof obj8; // okay
  _ as {readonly f: {readonly g: 1}} as typeof obj8; // okay
  _ as {readonly f: {readonly g: 1, ...}} as typeof obj8; // error inexact ~> exact
  _ as {f: {g: number}} as typeof obj8; // error number ~> 1
}

function test_obj9() {
  obj9 as {readonly f: {readonly g: {readonly h: 1}}}; // okay

  _ as {readonly f: {readonly g: {readonly h: 1}}} as typeof obj9; // okay
}

function test_obj10() {
  obj10 as {readonly f: 1, readonly g: {readonly h: 1}}; // okay
  obj10.g.h as 1; // okay

  _ as {readonly f: 1, readonly g: {readonly h: 1}} as typeof obj10; // okay
  _ as 1 as typeof obj10.g.h; // okay
  _ as 2 as typeof obj10.g.h; // error 2 ~> 1
}

function test_obj11() {
  const f = 42;
  const obj = { f: f as const }; // no error
  obj.f as 42; // okay
}
