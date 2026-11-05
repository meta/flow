import type {Box, Cell} from 'ts_box';

declare function f<T>(x: Box<T>, y: T): T;
declare const b: Box<number>;

// A type parameter of a TypeScript declaration without a variance is covariant.
f(b, 'x') as number | string; // ok

declare function g<T>(x: Cell<T>, y: T): T;
declare const c: Cell<number>;

g(c, 'x') as number | string; // ok
