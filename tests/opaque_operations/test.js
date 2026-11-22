{
  declare opaque type T1: {foo: string};
  declare const o: T1;
  ({...o}) as {foo: string}; // ok
  ({...o}) as empty; // error
}

{
  declare opaque type T2: component(foo: string);
  declare const foo: React.ComponentProps<T2>;
  foo as {readonly foo: string}; // ok
  foo as empty; // error
}

{
  declare opaque type T3: 'bar';
  declare const foo: {bar: string};
  declare const key: T3;
  foo[key] as string; // ok
  foo[key] as empty; // error
}

{
  declare opaque type T4: ?{readonly foo: string};
  declare const o: T4;
  o?.foo as ?string; // ok
  declare const n: NonNullable<T4>;
  n.foo as string; // ok
  n as T4; // ok
  o as NonNullable<T4>; // error
  declare const k: T4?.['foo'];
  k as ?string; // ok
}

{
  // An opaque type that is a member of its own upper bound
  declare opaque type D<X>: D<X> | null;
  declare const d: D<string>;
  d as ?D<string>; // ok
  declare opaque type A<X>: B<X> | null;
  declare opaque type B<X>: A<X> | string;
  declare const a: A<number>;
  a as ?A<number>; // ok
}

{
  // A value flows into an opaque type narrowed by an operation through its lower bound only if it
  // is in the narrowed upper bound
  declare opaque type U super null | {readonly foo: 'a'} extends ?{readonly foo: string};
  declare function f(u: NonNullable<U>): void;
  f(null); // error
  f({foo: 'a'}); // ok

  declare opaque type S super 'a' | 1 extends string | number;
  declare const s: S;
  declare function first<X>(x: number | X): X;
  let y = first(s);
  y = 'a'; // ok
  y = 1; // ok
  y = 2; // error

  // The keys of an opaque type keep its lower bound, whose values flow into them if they are keys
  declare opaque type K super 'a' | 1 extends string | number;
  declare const d: {[k: K]: boolean};
  const ks = Object.keys(d);
  ks.push('a'); // ok
  ks.push('zzz'); // error

  declare opaque type Plain super 'a' extends string;
  declare opaque type Outer super null extends Plain | null;
  const p: NonNullable<Outer> = null; // error
  declare const q: NonNullable<Outer>;
  q as string; // ok

  declare opaque type C0 super 'x' | null extends ?string;
  declare opaque type C1 super C0 | null extends C0 | ?string;
  declare opaque type C2 super C1 | null extends C1 | ?string;
  const cx: NonNullable<C2> = 'x'; // ok
  const cy: NonNullable<C2> = 'y'; // error
}

import {useMemo} from './memo';

{
  // The type argument of an opaque type that a hook returns is inferred from the return hint
  hook useFn(): void {
    const f: (x: number) => void = useMemo(() => x => {}); // ok
  }
}
