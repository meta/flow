// @flow

declare class Box<T> {
  value: T;
}
declare function wrap<T>(x: T): Box<T>;

// Generic values are instantiated in context, in argument order, each seeing
// the solution so far.
{
  declare function pipe<A, B, C>(a: A, f: (A) => B, g: (B) => C): C;
  const piped = pipe('a', wrap, wrap);
  piped.value.value as string; // ok
  piped.value.value as number; // error
}

// A generic `this` is instantiated after the arguments.
{
  const boxed = wrap.call(null, 'a');
  boxed.value as string; // ok
  boxed.value as number; // error
}

// The return hint gives evidence.
{
  declare function empty<T>(): Array<T>;
  const xs: Array<string> = empty();
  xs as Array<number>; // error
}

// A rest parameter whose type is inferred receives the arguments.
{
  declare function args<A extends ReadonlyArray<unknown>>(...args: A): A;
  const t = args(1, 'a');
  t[0] as number; // ok
  t[1] as string; // ok
}

// The parameters of a callback become the tuple a rest parameter receives.
{
  declare function debounce<Args extends ReadonlyArray<unknown>>(
    f: (...Args) => unknown,
  ): (...Args) => void;
  const d = debounce((s: string, n: number) => {});
  d('a', 1); // ok
  d(1, 'a'); // error
}

// `null` and `void` flowing to `?T` are absorbed.
{
  declare function nonnull<T>(x: ?T): T;
  declare const s: ?string;
  nonnull(s) as string; // ok
}

// `NoInfer<T>` gives no evidence.
{
  declare function first<T>(x: T, y: NoInfer<T>): T;
  first('a', 1); // error
}

// An invariant type argument is exact: the hint decides.
{
  declare function mk<K, V>(): Map<K, V>;
  const m: Map<number, string> = mk();
  m as Map<number, string>; // ok
}

// `new C(args)`: the arguments against the constructor that the class has or
// inherits, and the instance against the hint.
{
  class A<T> {
    p: T;
    constructor(p: T) {
      this.p = p;
    }
  }
  class B<T> extends A<T> {}
  new B('a').p as string; // ok
  new B('a').p as number; // error
  const b: B<string> = new B('a'); // ok
}
