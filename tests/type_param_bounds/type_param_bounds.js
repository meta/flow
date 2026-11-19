type X<T extends number> = boolean;
true as X<string>;

// Type arguments are checked where they are written, including in generic definitions, and not
// again where a definition is instantiated.
type Y<T> = X<T>; // error: T is unknown
true as Y<string>; // ok
declare function f<T>(x: X<T>): void; // error
class Base<T extends number> {}
class Derived<T> extends Base<T> {} // error
type Z<T> = X<T, T>; // error: too many type arguments
true as Z<number>; // ok

// The key of a mapped type is bounded by the keys that it maps.
type Get<O, K extends keyof O> = O[K];
type PropValues<O> = {[K in keyof O]: Get<O, K>}; // ok
type Return<F extends () => unknown> = F extends () => infer R ? R : empty;
type Returns<T extends ReadonlyArray<() => unknown>> = {[K in keyof T]: Return<T[K]>}; // ok

// An `infer` type that is a type argument has the bound of the type parameter it is passed to.
import type {NumOf} from './infer_bound';
type NeedsNum<N extends number> = N;
type BoxedNum<B> = B extends {v: infer X} ? NeedsNum<X> : empty; // error: X is unknown
declare const notNum: NumOf<{v: 'a'}>;
notNum as 'no'; // ok: X must be a number
type Pair<A, B extends ReadonlyArray<A>> = [A, B];
type Second<T> = T extends Pair<infer A, infer B> ? B : 'no';
declare const second: Second<[number, Array<number>]>;
second as Array<number>; // ok: B is checked against the bound with A inferred
type Boxes<T> = T extends [{v: infer X}, Pair<number, infer X>] ? NeedsNum<X[0]> : empty; // ok
type AorB<T extends 'a' | 'b'> = T;
type Str<T extends string> = T;
type Both<T> = T extends [AorB<infer X>, Str<infer X>] ? X : empty; // ok
