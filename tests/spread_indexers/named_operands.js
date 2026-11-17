interface I {[key: string]: number}
type T1 = {foo: number, ...I}; // Error

declare class C {[key: string]: number}
declare const c: C;
const o = {a: 1, ...c}; // Error

function f<T extends {[key: string]: number}>(x: T) {
  const p = {a: 1, ...x}; // Error
}
