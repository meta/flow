import * as React from 'react';
import { PolyComponent } from "./poly_react_component_export";

function test1() {
  declare function f<T>(f: (s: string) => T): T;
  // We should be able to infer that c is string.
  const c = f((a) => a);
  c as string;
  const d = f(function (a) { return a });
  d as string;

  let f2: (s: string) => string = (a) => a as string; // ok
  f2 = (b) => b as string; // ok

  const reasonTest: (s: string) => unknown = (a) => a as empty; // error: a is contextually typed as string, not empty
}

function test2() {
  declare class Foo {
    bar<T>(f: (s: string) => T): T
  }
  declare const foo: Foo;

  const c = foo.bar((a) => a);
  c as string;
  const d = foo?.bar((a) => a);
  d as string;

  class Bar {
    #baz<T>(f: (s: string) => T): T {
      return f("");
    }

    test() {
      const c = this.#baz((a) => a);
      c as string;
    }
  }
}

function test3() {
  declare function magic<T>(): T;
  declare class A<T> {}

  const magicResult: string = magic(); // OK
  const map: A<string> = new A(); // OK
}

function test4() {
  declare function foo<T>(x: T): Array<T>;
  declare const n: number;
  const x: Array<?number> = foo(n); // OK

  declare function bar<T>(x?: T): Array<T>;
  const y: Array<number> = bar(); // OK

  const z1: Array<empty> = Array.of(); // ok
  const z2: Array<unknown> = Array.of(); // ok
  const z3: Array<string> = Array.of(); // ok
  const z4: Array<string | number> = Array.of(1); // ok
  const z5: Array<string> = z3.concat(1); // Only one error on 1 ~> string
}

function test5() {
  declare function id<T>(x: T): T;
  const f1: (s: string) => string = id(s => s as string); // ok

  declare class A<T> {
    constructor(x: T): void
  }
  const a: A<(s: string)=>string> = new A(x => x); // OK

  declare class B<T> {
    constructor<V>(x: T): void
  }
  const b: B<(s: string)=>string> = new B(x => x); // OK

  declare class C<T> {
    constructor(x: T, n: number): void;
    constructor(x: T): void;
  }
  const c1: C<(s: string)=>string> = new C(x => x); // OK
  const c2: C<(s: string)=>string> = new C(x => x, 1); // OK

  declare function Component<T>(x: {v: T, f: (x: T) => void}): void;
  <Component v="1" f={(v) => {v as string}} />; // OK
  <PolyComponent v="1" f={(v) => {v as string}} />; // OK
}

function test6() {
  declare function call<A>(v: A, f: (x: A) => void): void
  call("ss", s => {}); // ok

  declare function pipe<A, B, C>(a: A, f1: (x: A)=>B, f2: (x: B)=>C): C;
  const _: string = pipe("s", s => 1, n => "");

  declare class Pipe<A, B> {
    constructor(a: A, f1: (x: A)=>B, f2: (x: B)=>void): void
  }
  new Pipe("s", s => 1, n => {}); // OK
}

function test7() {
  declare function f<A>(a: A, b: (x: A)=>void, c: number): void;
  f(3, (n) => {}, ""); // Error on third argument, but n can still be contextually typed.
}

function test8() {
  declare function f<T=string>(x: (y: T) => string): T;
  f(s => s as string); // ok
}

function test9() {
  // This overload selection problem will trigger a nested implicit instantiation.
  // Correctly selecting the overload requires us to flow targ to bound, which is
  // disabled during implicit instantiation.
  type M<O> = {[K in keyof O]: O[K] extends string ? string : boolean};
  declare function id<T>(x: T): T;
  // A regression test for an earlier bug where the overload is incorrectly selected, and causes
  // an incompatibility.
  const _: M<{ num: number }> = {num: id(true)}; // ok
}

function test10() {
  type V = {};
  type Variables = {readonly [key: string]: any};
  declare opaque type Query<in TVariables extends Variables>;
  declare const q: Query<V>;
  declare function poly<TVariables extends Variables>(q: Query<TVariables>): void;
  poly(q); // ok
}

function test11() {
  declare const jest: {
    fn<TArguments extends ReadonlyArray<any> = ReadonlyArray<any>>(): (...args: TArguments)=>void
  }
  declare const C1: React.ComponentType<{readonly f: () => void}>;
  <C1 f={jest.fn()} />; // ok

  declare const C2: React.ComponentType<{f1: () => void, f2: () => void}>
  <C2 f1={jest.fn()} f2={jest.fn()} />; // ok
}

function test12(foo: ?Array<string>) {
  const a = new Set(foo || []);
  const b = new Set(foo ?? []);
  a as Set<empty>; // error Set<string> ~> Set<empty>
  b as Set<empty>; // error Set<string> ~> Set<empty>
}

function test13() {
  declare function foo<V>(v: V, f: (x: V) => void): void;

  foo(
    { elements: [] },
    (value: { elements: Array<string> }) => { value as empty; }, // error
  )
}

function test14() {
  declare function magic<T>(): T;
  const m: unknown = { foo: magic() }; // ok
  const interfaceVar: interface {[key: string]: unknown} = { foo: magic() }; // ok
}

function test15() {
  const map: Map<string, string> = [""].reduce((acc, v) => {
    // TODO: This is an unfortunate error when we wrap the hint on logical expression with MaybeT
    //       for LHS. It makes acc to have type `?Map<string, string>`
    acc.set("", ""); // error
    return acc;
  }, new Map()) ?? new Map();
}

function test16() {
  const safeParse = <T>(json: ?string): ?T => null;
  const foo: Array<string> = safeParse('') ?? []; // ok
}

function test17() {
  declare function magic<T>(x: null): T;
  declare function magic<T>(): T;
  const _: interface {[key: string]: any} = { foo: magic() }; // ok
}

function test18() {
  declare function foo(x: {}): void;
  foo({bar: new Set([''])}); // error: there should be only one prop-missing error
}

function test19() {
  declare const xs: Array<any>;

  async function foo() {
    const imageHashes: Array<string> = await Promise.all(xs); // ok
  }
}

function test20() {
  type Record<T extends {...}> = { ...T, ... };

  declare const fn: <TArguments, TReturn>(
    implementation: (...args: TArguments) => TReturn,
  ) => { (...args: TArguments): TReturn };

  const result: (
    value: unknown,
    records: Record<{...}>,
  ) => any = fn((value, records) => 42); // okay
}

function test21() {
  declare const a: Array<number>;

  function F(): React.MixedElement {
    return (
      <>
        { a.map(_ => 3 /* okay */) }
      </>
    );
  }
}

// Iterable
{
  declare const f: (x: Iterable<(s: string) => unknown>) => void;
  f([x => 1]); // OK
}
