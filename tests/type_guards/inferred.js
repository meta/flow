// @flow

import * as React from 'react';

function test1() {
  declare function takesBoolReturningFunction(fn: (x: unknown) => boolean): void;

  takesBoolReturningFunction(value => typeof value === "number"); // okay
  takesBoolReturningFunction((value: unknown) => typeof value === "string"); // okay
}

function test2() {
  declare function takesGuard(fn: (x: unknown) => implies x is number): void;

  takesGuard(value => typeof value === "number"); // okay
  takesGuard((value: unknown) => typeof value === "number"); // okay

  takesGuard(value => typeof value === "string"); // error wrong guard type
  takesGuard((value: unknown) => typeof value === "string"); // error wrong guard type
}

function test3() {
  declare function takesGuardTwoParams(fn: (x: unknown, y: unknown) => implies x is number): void;

  takesGuardTwoParams(value => typeof value === "number"); // okay
  takesGuardTwoParams((value: unknown) => typeof value === "number"); // okay

  takesGuardTwoParams(
    ( // error function-predicate incompatibility
      value: unknown,
      otherValue, // error missing-local-annot
    ) => typeof value === "number");
  takesGuardTwoParams(
    (value: unknown, otherValue: unknown) => typeof value === "number" // error can only infer type guard
                                                                   // on single parameter functions
  );

  takesGuardTwoParams(value => typeof value === "string"); // error wrong guard type
  takesGuardTwoParams((value: unknown) => typeof value === "string"); // error wrong guard type
}

function test4() {
  const fn = (value: unknown) => typeof value === "number"; // infers: (value: unknown) => implies value is number
  declare const x: number | string;

  if (fn(x)) {
    x as number; // okay
    x as string; // error number ~> string
  } else {
    x as number; // error no refinement due to one-sided type guard
    x as string; // error no refinement due to one-sided type guard
  }
  fn as (value: unknown) => implies value is number; // okay
  fn as (value: unknown) => value is number; // error one-sided incompatible with two-sided
}

function test5() {
  const fn = (value: unknown): boolean => typeof value === "number"; // infers: (value: unknown) => boolean

  fn as (value: unknown) => implies value is number; // error non type guard
  fn as (value: unknown) => value is number; // error non type guard
}

function test6() {
  declare function takesSynthesizablePolyGuard<T, S extends T>(input: T, callbackfn: (value: T) => implies value is S): S;

  declare const value: ?string;
  const u = takesSynthesizablePolyGuard(value, x => x != null);
  u as string; // okay
  u as number; // error string ~> number
}

function test7() {
  declare function takesPolyGuard<T, U extends T>(guard: (x: T) => implies x is U): [T, U];

  const [t1, u1] = takesPolyGuard<number, 4>((value) => value === 4); // okay
  const [t2, u2] = takesPolyGuard((value: unknown) => value === 4);
  t2 as number;
  3 as typeof t2; // okay
  u2 as 4; // error number ~> 4: U is generalized, as in TypeScript
  u2 as 3; // error 4 ~> 3
  4 as typeof u2; // okay
  3 as typeof u2; // okay
}

function test8() {
  declare function takesOverloadedPolyGuard<T, U extends T>(value: T, guard: (x: T) => implies x is U): [T, U];
  declare function takesOverloadedPolyGuard<T>(value: T, guard: (x: T) => boolean): [T, T];

  declare const value: number;
  const [t1, u1] = takesOverloadedPolyGuard(value, x => x === 4); // picks the first overload
  t1 as number; // okay
  u1 as 4; // error number ~> 4: U is generalized, as in TypeScript
  u1 as 3; // error 4 ~> 3
  4 as typeof u1; // okay
  3 as typeof u1; // okay

  const [t2, u2] = takesOverloadedPolyGuard(value, (x): boolean => x === 4); // picks the second overload
  u2 as 4; // error number ~> 4
}

function test9() {
  // overload resolution should pick the first overload
  declare function takesOverloadedPolyGuard<T, U extends T>(guard: (x: T) => implies x is U): [T, U];
  declare function takesOverloadedPolyGuard<T>(guard: (x: T) => boolean): [T, T];

  declare const value: number;
  const [t1, u1] = takesOverloadedPolyGuard(x => x === 4); // error missing-local-annot
  const [t2, u2] = takesOverloadedPolyGuard((x: unknown) => x === 4); // okay
}

function test10() {
  let data: Array<string> = [];
  data.filter(s => s.substring(0)); // okay
}

function test11() {
  declare const arr: Array<?string>;

  const x1 = arr.filter((s) => s != null);
  x1 as Array<string>; // okay

  arr.filter((s) => s != null) as Array<string>; // okay
}

function test12() {
  class PrivateMemberRegressionTest {
    #prop(x: string): x is 'a' {
      return x === 'a';
    }
    m() {
      (x => this.#prop(x)) as (value: string) => boolean; // okay
    }
  }
}

function test13() {
  declare function takesGuard<T, U extends T>(guard: ?(x: T) => implies x is U, val: T): [T, U];

  declare const x: number;
  const [t, u] = takesGuard(
    (z) => z, // error non type guard function
    x,
  );
  t as number; // okay
  u as number; // okay
}

function test14() {
  let idx = 0;
  (_: any) => idx++; // okay, regression test for [recursive-definition]
}

function test15() {
  const fn = (x: ?number)=> x != null;

  fn as (x: ?number) => boolean;
  fn as (x: ?number) => x is number; // error inferred type guard is one-sided

  declare const fn1 : (x: ?number) => x is ?number;
  fn1 as typeof fn; // error ?number ~> number
}

function test16() {
  declare component Foo<TValue>(
    value: ReadonlyArray<TValue>,
    onChange: (value: ReadonlyArray<TValue>) => unknown,
  );

  declare const values: Array<string>;
  return <Foo value={values.filter(v => v)} onChange={x => {}} />; // okay
}

function test17() {
  declare function foo<TValue>(
    value: ReadonlyArray<TValue>,
    onChange: (value: ReadonlyArray<TValue>) => unknown,
  ): void;

  declare const values: Array<string>;
  foo(values.filter(v => v), x => {}); // okay
}

function test18() {
  const fn = (x: unknown) => typeof x === "object" && x?.hasOwnProperty("a");
  fn as (x: unknown) => implies x is unknown; // error non-type guard function
}
