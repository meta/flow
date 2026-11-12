/**
 * @format
 * @flow
 */

type A = {type: 'a', a: number, x: string};
type B = {type: 'b', b: number, x: string};

declare const k: keyof (A | B);
k as 'type' | 'x'; // ok
k as 'type'; // error: 'x' is a key too
'a' as keyof (A | B); // error: 'a' is not a key of B

type Without<T, U> = T extends U ? empty : T;
'type' as Without<keyof (A | B), 'x'>; // ok
'a' as Without<keyof (A | B), 'x'>; // error

type Select<T, K extends keyof T> = {[P in K]: T[P]};
declare const p: Select<A | B, Without<keyof (A | B), 'x'>>; // ok
p.type as 'a' | 'b'; // ok
p.a; // error
