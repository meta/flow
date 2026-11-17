// @flow
// Exercises the signature (cross-module) pipeline: these declarations are
// consumed in `imp.js`, so their shapes come from `type_sig`, not the
// within-file checker. Every form `within.js` checks locally has to name the
// same method here.
export interface One {
  ['m'](): number;
}
export interface Num {
  [42](): number;
}
export interface Neg {
  [-1](): number;
}
export interface Mixed {
  ['run'](): number;
  [key: string]: boolean;
}
export interface Poly {
  ['id']<T>(x: T): T;
}
export type OT = {['f'](): number};

// Value-name keys, bare and qualified, plus a bare primitive keyword read as a
// value. The signature pipeline must name them from the value's type, the same
// way the checker does.
const k = 'run';
export interface FromConst {
  [k](): number;
}
const keys = {m: 'go'} as const;
export interface FromQualified {
  [keys.m](): number;
}
declare const string: 'prim';
export interface FromPrimitiveKeyword {
  [string](): number;
}
declare const wide: string;
export interface FromWide {
  [wide](): number; // ERROR: `wide` names no one property
}

// The same overload-ordering bodies `within.js` checks locally. The signature
// pipeline only learns a value name at merge, so these are what pin the two
// pipelines to one order.
const dup = 'a';
const dup2 = 'a';
export interface LiteralFirst {
  ['a'](): 'q';
  a(): 'p';
}
export interface ValueFirst {
  [dup](): 'q';
  a(): 'p';
}
export interface ValueLast {
  a(): 'p';
  [dup](): 'q';
}
export interface TwoValues {
  [dup](): 'q';
  [dup2](): 'z';
}
export interface AccessorLast {
  [dup](): 'q';
  get a(): 'p';
}
export interface AccessorFirst {
  get a(): 'p';
  [dup](): 'q';
}

// A `declare class` carries a computed method on both sides of the boundary.
declare export class Holder {
  ['dm'](): number;
  static ['sm'](): string;
  [-1](): boolean;
}
