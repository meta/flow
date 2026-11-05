declare class Base<T> {}
declare class Sub<T> extends Base<T> {}
declare const s: Sub<number>;

// A member of an ancestor class before one of the same class.
declare function f<A, B>(x: Base<A> | Sub<B>): A;
const a = f(s);
a as number; // ok

// Only a member of an ancestor class.
declare function g<A>(x: Base<A> | null): A;
const b = g(s);
b as number; // ok

// The member of the same class first.
declare function h<A, B>(x: Sub<B> | Base<A>): B;
const c = h(s);
c as number; // ok
