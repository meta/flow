// @flow

declare function id<T>(x: T): T;
id(1);
id<string>('a');

declare function pair<A, B>(a: A, b: B): [A, B];
pair(1, 'a');

declare class C {
  m<T>(x: T): Array<T>;
}
declare const c: C;
c.m(true);

declare function f<T>(x: T): T;
declare function f(x: string, y: string): string;
f(1);
f('a', 'b');

const g = <T>(x: T): T => x;
g(1);

declare const o: {p: <T>(x: T) => Array<T>};
o.p('a');

function h(k: <T>(x: T) => T) {
  k(true);
}
