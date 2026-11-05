// An application that the argument does not fit, even with the markers open, gives no evidence.
declare class Box<T> {
  v: T;
}
declare function f<T>(x: Box<string> | Box<T>): T;
declare const n: Box<number>;

f(n) as number;
f(n) as empty; // error
