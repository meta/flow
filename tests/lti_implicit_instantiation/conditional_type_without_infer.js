// A conditional type over `T` is decided as in TypeScript, so `T` is inferred from the argument.
type Id<P> = P extends unknown ? P : empty;
declare function f<T>(x: Id<{v: T}>): T;
const r = f({v: 1});
r as number; // ok
r as empty; // error: number ~> empty

// `T` is inferred as `number | string`, so the distributive parameter is
// `Array<number> | Array<string>`.
declare function g<T>(x: T extends unknown ? Array<T> : empty): T;
declare const xs: Array<number | string>;
g(xs); // error
