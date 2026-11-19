declare const _: any;

function test_simple() {
  declare class C<T> {}

  declare function f<T extends string>(literalValue: T): C<T>;
  declare function g<T extends string>(literalValue: C<T>): C<C<T>>;

  const x0: C<'a'> = f('a');
  const x1: C<string> = f('a');
  const x2 = g(f('a'));
  const x3: C<C<string>> = g(f('a'));
  const x4: C<C<'a'>> = g(f('a')); // okay

  const y0: C<'b'> = f('a'); // error 'a' ~> 'b'
  const y1: C<C<'b'>> = g(f('a')); // error 'a' ~> 'b'
}

function test_poly_types_1() {
  declare function f<T>(literalValue: T): {f: <V>(x: V) => {f: T}};
  declare function g<T>(literalValue: T): {f: T};

  const x1 = g(f(42));
  x1.f.f('blah').f = 1;

  const x2: {f: {f: <V>(x: V) => {f: 42}}} = g(f(42)); // okay
  const x3: {f: {f: <V>(x: V) => {f: 43}}} = g(f(42)); // error

  const x4 = g(f({f:[42]}));
  x4.f.f('blah').f.f[0] = 1; // okay

  const x5: {f: {f: <V>(x: V) => {f: {f: [42]}}}} = g(f({f:[42]})); // okay
  const x6: {f: {f: <V>(x: V) => {f: {f: [43]}}}} = g(f({f:[42]})); // error
}

function test_poly_types_2() {
  declare function f<T>(literalValue: T, callback: (value: T) => void): {f: <V>(x: V) => {f: T}};
  declare function g<T>(literalValue: T): {f: T};

  const x1 = g(f(42, (x: 42) => { x as number; })); // okay
}

function test_mapped_types() {
  declare function f<T>(literalValue: T): Readonly<{[key in keyof T]: T[key]}>;
  declare function g<T>(literalValue: T): {f: T};

  const x1 = f({f: 1, g: 2});
  type T1 = {readonly f: number, readonly g: number};
  x1 as T1; // okay
  _ as T1 as typeof x1; // okay

  const x2 = g(f({f: 1, g: 2}));
  type T2 = {f: {readonly f: number, readonly g: number}};
  x2 as T2; // okay
  _ as T2 as typeof x2; // okay
}

function test_regression() {
  declare function literal<T extends string>(literalValue: T): Wrapper<T>;
  declare function union<V>(
    ...wrappers: ReadonlyArray<Wrapper<V>>
  ): Wrapper<V>;
  declare function object<Wrappers extends {readonly [key: string]: Wrapper<unknown>}>(
    wrappers: Wrappers,
  ): Wrapper<Readonly<MapWrapperObject<Wrappers>>>;

  type Wrapper<out V> = (value: unknown) => Readonly<{value: V}>;
  type MapWrapper<C> = C extends null | void
    ? C
    : C extends Wrapper<infer T>
      ? T
      : empty;

  type MapWrapperObject<Wrappers> = {
    [K in keyof Wrappers]: MapWrapper<Wrappers[K]>,
  };

  const example0 = object({format: literal('A')});
  const example1: Wrapper<{readonly format: 'A'}> = object({format: literal('A')}); // okay

  type Params = Readonly<{format: 'A' | 'B'}>;
  const example2: Wrapper<Params> = object({ // okay
    format: union(literal('A'), literal('B')),
  });
  const example3: Wrapper<Params> = object({ // error 'C' ~> 'A' | 'B'
    format: union(literal('A'), literal('B'), literal('C')),
  });
}

// A generic call in the return of a callback passed to another generic call keeps its
// precise result, while the outer call generalizes its own result.
function test_callback_return() {
  declare const xs: Array<Array<string>>;
  declare const a: string;

  const x1 = xs.map(x => x.map(_ => 'a')); // okay
  x1 as Array<Array<string>>; // okay
  const x2 = xs.map(x => x.map(y => `${a} ${y}`)); // okay
  x2 as Array<Array<string>>; // okay
  xs.map((x, i) => x.flatMap(y => (y ? [`${i} ${y}`] : []))); // okay

  declare function useMemo<T>(create: () => T, deps: ReadonlyArray<unknown>): T;
  const x3 = useMemo(() => xs[0].map(_ => 'a'), []); // okay
  x3 as Array<string>; // okay
  useMemo(() => new Map(xs[0].map(y => [y, `label ${y}`])), []); // okay
  useMemo(() => xs[0].map(y => ({label: `label ${y}`})), []); // okay
  Array.from({length: 2}, (_, i) => ['H1', 'H2'].map(h => `${2023 + i}${h}`)); // okay

  declare function mymap<T, U>(xs: Array<T>, cb: (x: T) => U): Array<U>;
  mymap(xs, x => mymap(x, _ => 'a')); // okay

  declare class MyArr<T> {
    map<U>(cb: (x: T) => U): MyArr<U>;
  }
  declare const ys: MyArr<MyArr<string>>;
  ys.map(y => y.map(_ => 'a')); // okay

  // Wrapped callbacks
  declare function maybe_cb<U>(cb: ?() => U): U;
  maybe_cb(() => xs[0].map(_ => 'a')); // okay
  declare function obj_cb<U>(o: Readonly<{f: () => U}>): U;
  obj_cb({f: () => xs[0].map(_ => 'a')}); // okay

  // The outer result is still generalized
  declare function f<U>(cb: () => U): U;
  const x4 = f(() => 'x');
  x4 as 'y'; // error string ~> 'y'
  let x5 = f(() => 1);
  x5 = 2; // okay

  // A generic call passed directly as an argument is not a callback return
  declare function id<T>(x: T): T;
  id(xs[0].map(_ => 'a')); // error 'a' and string are not exactly the same
}

// A generic function passed as a callback pins its own tparams from both the
// param side and the return side, so routing those sides to different
// precisions would break the callback's internal coherence. Routing is
// skipped entirely for such calls.
function test_generic_callback() {
  declare function compactMap<T, K>(array: ReadonlyArray<T>, mapFn: (item: T, index: number) => ?K): Array<K>;
  declare function ident<V>(x: V): V;

  const r1 = compactMap(['lit'], ident); // okay
  r1 as Array<string>; // okay
}
