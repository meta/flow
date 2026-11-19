import * as React from 'react';

declare function id<T>(x: T): T;
type Box<T> = {value: T};

// The generic argument is instantiated against what the later argument infers.
declare function apply<S>(fn: (x: S) => S, initial: S): S;
apply(id, 1) as number;
apply(id, 1) as string; // ERROR

declare function identityBox<T>(box: Box<T>): Box<T>;
const box = apply(identityBox, {value: 1});
box.value as number;
box.value as string; // ERROR

// A generic argument does not see how another is instantiated: the second `wrap` gets its bound.
declare function pipe<A, B, C>(a: A, f: (x: A) => B, g: (x: B) => C): C;
declare function wrap<T>(x: T): Box<T>;
const piped = pipe('a', wrap, wrap);
piped.value as unknown;
piped.value as string; // ERROR

// The return hint is part of what the generic arguments are instantiated against.
declare function lift<T>(f: (x: T) => T): (x: T) => T;
const lifted: (x: string) => string = lift(id);
lifted('a') as string;
const unhinted = lift(id);
unhinted('a') as string; // ERROR

// Like the hint of a call argument, a hint from an unobserved type parameter is its bound.
declare function compose<A, B, C>(f: (x: B) => C, g: (x: A) => B): (x: A) => C;
const composed = compose(id, id);
composed('a') as unknown;
composed('a') as string; // ERROR

// A generic value passed to a type parameter that nothing else infers stays generic.
declare function keep<T>(x: T, deps: Array<unknown>): T;
const kept = keep(id, []);
kept<string>('a') as string;
kept<string>('a') as number; // ERROR
declare function keepMaybe<T>(x: ?T): T;
keepMaybe(id)<number>(1) as number;

// A generic argument without evidence is instantiated with its bound.
declare function memo<T>(fn: () => T): () => T;
declare function create<T>(): Box<T>;
const unobserved = memo(create)();
unobserved.value as unknown;
unobserved.value as string; // ERROR

// Generic classes passed as values.
declare class Store<T> {
  value: T;
}
declare function makeStore<T>(store: Class<Store<T>>, value: T): Store<T>;
makeStore(Store, 1) as Store<number>;
makeStore(Store, 1).value as string; // ERROR

// Constructors.
declare class Registry<F> {
  constructor(initial: F, factory: (x: F) => F): void;
  get(): F;
}
new Registry(1, id).get() as number;
new Registry(1, id).get() as string; // ERROR

// JSX props.
declare component Mapper<T>(value: T, transform: (x: T) => T, onChange: (x: T) => void);
<Mapper
  value="a"
  transform={id}
  onChange={value => {
    value as string;
    value as number; // ERROR
  }}
/>;

// Overloaded callees.
declare function overloaded(fn: (x: string) => string): 'string';
declare function overloaded(fn: (x: number) => number): 'number';
overloaded(id) as 'string';
declare var genericOverloaded: {
  <S>(fn: (a: S) => S, x: S, y: boolean): S,
  <S>(fn: (a: S) => S, x: S): S,
};
genericOverloaded(id, 1) as number;
genericOverloaded(id, 1) as string; // ERROR

// The bound of a generic value is still checked.
type N = {n: number};
declare function needsN<T extends N>(x: T): T;
declare var notN: {s: string};
apply(needsN, notN); // ERROR
declare class NeedsN<T extends N> {
  constructor(x: T): void;
}
declare function make<S>(c: Class<NeedsN<S>>, x: S): NeedsN<S>; // error x2: S is not known to be an N
make(NeedsN, notN); // ERROR

// Generic values as `this`. A generic value met while checking another one is not instantiated:
// `applyTo` is checked against what `.call` expects, and the `wrap` it meets there leaves `U` at
// its bound.
wrap.call(null, 'a').value as string;
wrap.call(null, 'a').value as number; // ERROR
declare function applyTo<T, U>(f: (x: T) => U, x: T): U;
const applied = applyTo.call(null, wrap, 'a');
applied as unknown;
applied as Box<string>; // ERROR

// Arguments after a spread do not line up with parameters.
declare function last<S>(x: S, fn: (y: S) => S): S;
declare const single: [number];
last(...single, id) as number;
last(...single, id) as string; // ERROR

// A generic argument in an optional callback.
declare function maybeApply<T>(initial: T, fn?: ?(x: T) => T): T;
maybeApply(1, id) as number;
maybeApply(1, id) as string; // ERROR

// Conditional types over generic values keep their structure.
declare function list<T>(): Array<T>;
type Listed = ReturnType<typeof list>;
declare const listed: Listed;
listed as Array<unknown>;
listed as number; // ERROR
