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
  // A narrowed upper bound does not keep the lower bound
  declare opaque type U super null extends ?{readonly foo: string};
  declare function f(u: NonNullable<U>): void;
  f(null); // error
}

import {useMemo} from './memo';

{
  // The type argument of an opaque type that a hook returns is inferred from the return hint
  hook useFn(): void {
    const f: (x: number) => void = useMemo(() => x => {}); // ok
  }
}
