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
