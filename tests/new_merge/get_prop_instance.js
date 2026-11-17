// @flow

declare class C {
  [key: string]: boolean;
  s: number;
}

declare const c: C;

export const instance_named = c.s; // OK
declare const s: string;
export const instance_computed = c[s]; // ERROR
