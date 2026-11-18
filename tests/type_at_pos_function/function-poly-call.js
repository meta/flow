// @flow

declare function id<T>(x: T): T;
// A call to a polymorphic function prints the signature as declared, and the
// type arguments of its instantiation apart from it.
  id(1);
//^
  id<string>('a');
//^

declare function pair<A, B>(a: A, b: B): [A, B];
  pair(1, 'a');
//^

declare class C {
  m<T>(x: T): Array<T>;
}
declare const c: C;
c.m(true);
//^

declare function f<T>(x: T): T;
declare function f(x: string, y: string): string;
  f(1);
//^
  f('a', 'b');
//^

const g = <T>(x: T): T => x;
// A callee bound by `const`, held in a property or passed as a parameter prints
// as an arrow type.
  g(1);
//^

declare const o: {p: <T>(x: T) => Array<T>};
o.p('a');
//^

function h(k: <T>(x: T) => T) {
  k(true);
//^
}

declare class Box<+T> {}
declare function unwrap<T>(x: T): T extends Box<infer V> ? V : T;
declare const box: Box<number>;
// A type the signature computes from its type parameters prints as this call
// evaluates it.
  unwrap(box);
//^
  unwrap('a');
//^

declare function kind<T>(x: T): T extends string ? 's' : 'n';
declare const ns: number | string;
  kind(ns);
//^

type Obj = {a: number, b: string};
declare function get<K extends keyof Obj>(k: K): Obj[K];
  get('b');
//^

declare function raw<R = void>(r?: NonNullable<R>): R;
  raw();
//^

declare function useS<S>(init: (() => S) | S): S;
declare const err: ?Box<string>;
// The members of the type argument merge into the union, which stays as declared.
  useS(err);
//^

declare function shadow<T>(x: T, cb: <T>(y: T) => NonNullable<T>): void;
declare const cb: <T>(y: T) => NonNullable<T>;
// The type parameter of a nested generic type shadows the instantiated one.
  shadow(null, cb);
//^
