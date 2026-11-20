// The results are cast after the call, so that no hint takes part in their inference.

// `undefined` fits an optional slot, which gives `empty` to the type it is declared with.
declare function param<T>(x?: T): T;
const missing = param();
missing as empty; // ok
const undef = param(undefined);
undef as empty; // ok
declare function prop<T>(o: {readonly p?: T}): T;
const undefProp = prop({p: undefined});
undefProp as empty; // ok
declare function useRef<T>(initial?: T): {current: T};
const ref = useRef(); // ok

// A union with `undefined` meets the slot's type without it.
declare const maybeNumber: number | void;
const fromUnion = param(maybeNumber);
fromUnion as number; // ok
fromUnion as string; // error
declare const nullable: ?number;
const fromMaybe = param(nullable);
fromMaybe as number; // error: null

// Types that stand for a union with `undefined` are normalized first.
type Alias<X> = void | X;
declare const alias: Alias<string>;
const fromAlias = param(alias);
fromAlias as string; // ok

// A union of only `undefined` members: the slot takes `empty`.
declare const voids: void | void;
const fromVoids = param(voids);
fromVoids as empty; // ok
declare const aliasOfVoid: Alias<void>;
const fromAliasOfVoid = param(aliasOfVoid);
fromAliasOfVoid as empty; // ok
declare const propOfVoid: {p?: Alias<void>};
const fromPropOfVoid = prop(propOfVoid);
fromPropOfVoid as empty; // ok
declare const elementOfVoid: [a?: void | void];
const fromElementOfVoid = first(elementOfVoid);
fromElementOfVoid as empty; // ok
opaque type Local = void | number;
declare const local: Local;
const fromLocal = param(local);
fromLocal as number; // ok
// The supertype of an opaque type with a nullish supertype fits the slot, as in the flow-based
// solve.
declare opaque type Nullish: ?string;
declare const nullish: Nullish;
const fromNullish = param(nullish);
fromNullish as ?string; // ok
fromNullish as Nullish; // error
type Cond<X> = X extends string ? void | number : boolean;
declare const cond: Cond<string>;
const fromCond = param(cond);
fromCond as number; // ok
type O = {p?: number};
declare const indexed: {p: O['p']};
const fromIndexed = prop(indexed);
fromIndexed as number; // ok

// A class instance meets the slot's object type, not `undefined`.
declare class Box<out X> {
  readonly x: X;
}
declare function objectOf<T>(x?: {readonly x: T, ...}): T;
declare const box: Box<number>;
const fromInstance = objectOf(box); // error: instances are not objects
fromInstance as number; // ok

// A generic whose bound admits `undefined` meets the slot with the rest of its bound, also
// through optional tuple elements and properties.
declare function first<T>(t: Readonly<[a?: T]>): T;
declare function rw<T>(o: {p?: T}): T;
function narrowedBound<X extends number | void>(x: X, t: [a?: X], o: {p?: X}) {
  const fromParam = param(x);
  fromParam as number; // ok
  const fromElement = first(t);
  fromElement as number; // ok
  const fromProp = prop(o);
  fromProp as number; // ok
  const fromRw = rw(o);
  fromRw as number; // ok
}

// A destructor over a type parameter meets the slot as `undefined` or its type.
declare function readOnlyCallback<X extends {...}>(cb: (x: Readonly<X>) => void): X;
readOnlyCallback((x?: {a: number}) => {}) as {a: number}; // error

// A callback's optional property receives a type parameter.
declare function callback<T>(cb: (o: {p: T}) => void): T;
const fromCallback = callback((o: {readonly p?: number}) => {});
fromCallback as void | number; // ok
fromCallback as number; // error

// A recursive alias.
type Rec<T> = T | Rec<T>;
declare const rec: Rec<number>;
param(rec);

// Read-write optional properties are compared both ways.
declare const o: {p?: number};
const fromRwProp = rw(o);
fromRwProp as number; // ok
fromRwProp as string; // error

// An optional generic value passed on to an optional slot keeps its type parameter.
declare function passOn<A>(c: component(...{tier: number, ...} & A), a?: A): void;
function passOnOptional<A>(c: component(...{tier: number, ...} & A), a?: A) {
  passOn(c, a); // ok
}
function passOnUnion<A>(c: component(...{tier: number, ...} & A), a: void | A) {
  passOn(c, a); // error: the `void` of an explicit union gives `A` an `empty` bound
}

// An optional tuple element.
declare function element<T>(t: [a: string, b?: T]): T;
declare const t: [a: string, b?: number];
const fromTuple = element(t);
fromTuple as number; // ok
fromTuple as string; // error

// Optional tuple elements whose type admits `undefined`.
declare const voidable: [a?: number | void];
const fromVoidable = first(voidable);
fromVoidable as number; // ok
declare function rwTuple<T>(x: [a?: T]): T;
const fromRwTuple = rwTuple(voidable);
fromRwTuple as number; // ok
declare function tupleArray<T>(x: Array<[a?: T]>): T;
declare const voidables: Array<[a?: number | void]>;
const fromTupleArray = tupleArray(voidables);
fromTupleArray as number; // ok

// Tuples are unified element by element, each element with its own optionality.
declare function unifyRequired<T>(x: Array<[a: T]>): T;
declare const optionalElements: Array<[a?: number]>;
const required = unifyRequired(optionalElements); // error
required as empty; // error
declare function unifyOptional<T>(x: Array<[a?: T]>): T;
declare const requiredElements: Array<[a: number | void]>;
const optional = unifyOptional(requiredElements); // error
optional as empty; // error

// Hints through optional slots of a maybe type: their `undefined` says nothing about the argument.
declare class Entries<K, out V> {
  k: K;
  readonly v: V;
}
declare function mkEntries<K, V>(entries: Array<[K, V]>): Entries<K, V>;
declare function takeEntries(e?: ?Entries<string, 'a' | 'b'>): void;
takeEntries(mkEntries([['x', 'a']])); // ok
declare function takeEntriesProp(o: {e?: ?Entries<string, 'a' | 'b'>}): void;
takeEntriesProp({e: mkEntries([['x', 'a']])}); // ok

// An optional property whose type is a destructor, against a callable with a rest parameter.
type MockFn<TArgs extends ReadonlyArray<unknown>, R> = {(...args: TArgs): R, ...};
declare function mockFn<TArgs extends ReadonlyArray<any> = ReadonlyArray<any>, R = any>(): MockFn<
  TArgs,
  R,
>;
declare function withExtra<TV extends {readonly [key: string]: unknown}>(
  f: (o: {extra?: Partial<TV>}) => void,
): TV;
withExtra(mockFn()); // ok
