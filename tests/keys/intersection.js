/**
 * @format
 * @flow
 */

type A = {a: number};
type B = {b: string};

declare const k: keyof (A & B);
k as 'a' | 'b'; // ok
k as 'a'; // error: 'b' is a key too
k as 'b'; // error: 'a' is a key too

type Without<T, U> = T extends U ? empty : T;
'b' as Without<keyof (A & B), 'a'>; // ok
'a' as Without<keyof (A & B), 'a'>; // error

declare const p: {[K in Without<keyof (A & B), 'a'>]: (A & B)[K]};
p.b as string; // ok
p.a; // error

// A member that is not an object has no keys.
declare const lit: keyof ({c: number} & 'lit');
lit as 'c'; // ok
function generic<T>(key: keyof (T & {c: number})): 'c' {
  return key; // ok
}
function bounded<T extends {d: number}>(key: keyof (T & {c: number})): 'c' {
  return key; // error: 'd' is a key too
}
declare const none: keyof (number & 'lit');
none as empty; // ok
declare const arr: keyof (Array<number> & {[key: string]: number});
arr as string; // ok
declare const opt: keyof ((void | {d: number}) & {c: number});
opt as 'c'; // error: 'd' is a key too

// An element of an intersection is read key by key.
declare const ab: A & B;
ab[k] as number | string; // ok
ab[k] as number; // error: `b` is a string
function read<K extends keyof (A & B)>(key: K): number | string {
  return ab[key]; // ok
}

// So are its values.
declare const values: Values<A & B>;
values as number | string; // ok
values as number; // error: `b` is a string
