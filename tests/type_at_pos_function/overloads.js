// @flow

// Flow spells an overload as a name declared more than once. A call selects one
// signature, so hover prints that one and counts the rest; anywhere the whole
// set is in view it is listed a signature per line and nothing is counted.
declare function f(x: number): string;
//               ^
declare function f(x: string): string;
declare function f(x: boolean): string;

  f(1);
//^
  f('hi');
//^

// A reference that is not a call has the whole set in view, so it lists the
// signatures rather than counting them.
const g = f;
//        ^

declare function solo(x: number): string;
  solo(1);
//^

declare class C {
  m(x: number): string;
  m(x: string): string;
}
declare const c: C;
c.m(1);
//^
