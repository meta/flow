// @flow

// A bound may refer to its own type parameter, or to a sibling that refers back to it, wherever a
// type alias may refer to itself: under type arguments, object members, functions, arrays and
// tuples.

function selfToArray<T extends Array<T>>(x: T): Array<T> {
  return x; // OK
}
function arrayToSelf<T extends Array<T>>(x: Array<T>): T {
  return x; // ERROR
}

function mutual<A extends ReadonlyArray<B>, B extends ReadonlyArray<A>>(
  a: A,
): ReadonlyArray<ReadonlyArray<A>> {
  return a; // OK
}
function mutualBad<A extends ReadonlyArray<B>, B extends ReadonlyArray<A>>(a: A): B {
  return a; // ERROR
}

interface Comparable<T> {
  compareTo(other: T): number;
}
declare function max<T extends Comparable<T>>(a: T, b: T): T;
declare class Num implements Comparable<Num> {
  compareTo(other: Num): number;
}
declare class Bad {
  compareTo(other: string): number;
}
max(new Num(), new Num()) as Num; // OK
max(new Bad(), new Bad()); // ERROR

type RecordOf<K extends string | number | symbol, V> = {[P in K]: V};
declare function strings<T extends RecordOf<keyof T, string>>(x: T): T;
strings({a: 'x'}); // OK
strings({a: 1}); // ERROR

function chain<
  A extends ReadonlyArray<B>,
  B extends ReadonlyArray<C>,
  C extends ReadonlyArray<D>,
  D extends ReadonlyArray<A>,
>(a: A, d: D): void {
  a as ReadonlyArray<ReadonlyArray<ReadonlyArray<ReadonlyArray<A>>>>; // OK
  a as ReadonlyArray<ReadonlyArray<ReadonlyArray<ReadonlyArray<B>>>>; // ERROR
  d as ReadonlyArray<ReadonlyArray<B>>; // OK
}
function deep<T extends ReadonlyArray<ReadonlyArray<ReadonlyArray<T>>>>(x: T): void {
  x[0][0][0][0][0][0] as T; // OK
  x[0][0][0][0][0] as T; // ERROR
}

declare class TreeNode<T extends TreeNode<T>> {
  children: Array<T>;
}
declare class Leaf extends TreeNode<Leaf> {}
new Leaf().children[0] as Leaf; // OK
declare var badTree: TreeNode<number>; // ERROR

declare class Registry<T extends Registry<T>> {
  static count: number;
  static reset(): void;
}
Registry.count as number; // OK: the bounds that stand in for the type arguments are not checked
Registry.reset(); // OK

type Checkable<N> = string extends keyof N ? empty : unknown;
declare function nodeSchema<N extends Checkable<N>>(): void;
nodeSchema<{a: number}>(); // OK
nodeSchema<{[key: string]: number}>(); // ERROR

declare var poly: <T extends ReadonlyArray<T>>(x: T) => T;
const samePoly: <U extends ReadonlyArray<U>>(x: U) => U = poly; // OK

// A bound that needs itself to be evaluated, or that is its own parameter, is still trivially
// recursive, as for a type alias.
function union<T extends T | string>(): void {} // ERROR
function indexed<T extends T[keyof T]>(): void {} // ERROR
function spread<T extends {...T}>(): void {} // ERROR
type Id<X> = X;
function aliasedSelf<T extends Id<T>>(x: T): void { // ERROR
  x.foo; // OK: the bound is recovered as any
}

// As in TypeScript, the keys of a type parameter whose bound refers to it are a type of their
// own, not the keys of the bound.
function keys<T extends keyof T>(): void {} // OK
// Unlike TypeScript, a homomorphic mapped type over `T` looks into the bound of `T`.
function mapped<T extends {[K in keyof T]: string}>(): void {} // ERROR
type Keys<X> = keyof X;
function aliasedKeys<T extends Keys<T>>(x: T): void {
  x as string | number | symbol; // OK
}
function indexByKeys<T extends {next: T}>(t: T, k: keyof T): unknown {
  return t[k]; // ERROR, which TypeScript accepts, as for a type parameter without a bound
}
function keyOfBound<T extends {next: T}>(): keyof T {
  return 'next'; // ERROR, which TypeScript accepts, as for a type parameter without a bound
}
