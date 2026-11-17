declare var wrap: <T>() => (x: T) => T;

declare function pipe<A, B>(a: A, f: (b: A) => B): B;
pipe('a', wrap()); // OK: return hint is `(string) => <placeholder>`
pipe('a', x => x); // OK

const x: (y: number) => unknown = wrap(); // OK

declare function g<A>(cb: (x: A) => string): A;
g(wrap()); // OK: return hint is `(<placeholder>) => string`

declare function pool<T>(fs: Array<() => Promise<T>>): Promise<Array<T>>;
declare var arr: Array<number>;
pool(arr.map(x => () => Promise.resolve(x))); // OK: the hint would resolve `U` to a type with a placeholder, so it is dropped

declare class Box<V> {
  map<N>(fn: (value: V) => Box<N> | N): Box<N>;
}
declare function filterNulls<T>(items: ReadonlyArray<?T>): Array<T>;
declare function all<V>(boxes: ReadonlyArray<Box<V>>): Box<Array<V>>;
declare var boxes: Box<Array<Box<?number>>>;
boxes.map(bs => all(bs).map(filterNulls)); // OK: the hint would leak a placeholder into the bounds of `N`, so it is dropped

declare var wrapNum: <T extends number>() => (x: T) => T;
pipe('a', wrapNum()); // ERROR: string is incompatible with number

declare function pipe2<A, B>(a: A, f: (x: B) => B): B;
pipe2('a', wrap()); // ERROR: T is underconstrained

declare function pipe3<A, B, C>(a: A, f: (x: B) => B, g: (x: C) => C): C;
pipe3('a', wrap(), wrap()); // ERROR: T is underconstrained in the second call

declare function useState<S>(x: S | () => S): [S, (value: S) => void];
useState(new Set()); // ERROR: T is underconstrained
