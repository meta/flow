// `undefined` and `void` arguments meet a `void` union member, as subtyping
// matches them, so they give the type parameter no evidence.
declare const v: void;

declare function f<T>(x: T | void): T;
f(undefined); // ERROR: underconstrained
f(v); // ERROR: underconstrained

declare function p<T>(x: T | void, y: T): T;
p(undefined, 1) as number; // OK
p(v, 1) as number; // OK

declare function q<T>(x: {f: T} | void): T;
q({f: 1}) as number; // OK

// A `void` argument infers `void`, not `undefined`.
declare function g<T>(x: T): T;
g(v) as void; // OK
g(v) as undefined; // ERROR

declare function k<T>(x: T, y: T): T;
k(v, undefined) as void; // OK
k(v, undefined) as undefined; // ERROR
