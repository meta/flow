// @flow

function test0() {
  declare let f: (x: string) => void;

  f = true ? ((x) => {}) : ((y) => {});
  f = true ? ((x: string | number) => {}) : ((y) => {}); // NOTE: `string => void` should be preferred as hint
}

function test1() {
  const test1 = true ? ((x: number) => {}) : ((y) => {}); // NOTE: `number => void` should be used as hint
}

function test2() {
  const test2 = true ? [] : [1];
  test2 as Array<number>;
}

function test3() {
  const test3 = true ? [x => {}] : [(x: number) => {}];
  test3 as ReadonlyArray<(x: number) => void>;
  test3 as ReadonlyArray<(x: string) => void>; // error should be number => void
}

function test4() {
  declare const x: unknown;
  declare function foo<U>(f: () => U): U;
  foo(() => (false ? [] : [x])); // okay, [] is contextually typed
  foo(() => (false ? [x] : [])); // okay, [] is contextually typed
  foo(() => (false ? [] : ['x'])); // okay, [] is contextually typed
  foo(() => (false ? ['x'] : [])); // okay, [] is contextually typed
}

function test5() {
  declare function foo(x: any): void;
  declare const value: any;

  return foo(
    foo(value) && value.id != null && value.id // okay, no definition-cycle error, but constant-condition error
  );
}
