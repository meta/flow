// Exercises the signature (cross-module) pipeline: these declarations are
// consumed in `imp.js`, so their shapes come from `type_sig`, not the
// within-file checker.
export interface One {
  ['a']: number,
}
export interface Multi {
  ['a']: number,
  ['b']: string,
}
export interface Num {
  [42]: boolean,
}
export interface Mixed {
  foo: string,
  ['bar']: boolean,
  [key: string]: number,
}
export interface Opt {
  ['a']?: number,
}
export interface Labeled {
  [label: 'a']: number,
}

// Value-name keys, both bare and qualified. The signature pipeline must read
// them as the value's type, the same way the checker does.
const key = 'foo';
export interface FromConst {
  [key]: number,
}
const keys = {foo: 'a'} as const;
export interface FromQualified {
  [keys.foo]: number,
}
declare const wide: string;
export interface FromWide {
  [wide]: number, // ERROR: `wide` names no one property
}

// A type-name key stays an index signature across the boundary too.
type AliasA = 'a';
export interface OneAlias {
  [AliasA]: number,
}

// Same-name collisions must resolve by source order, last winning, so the
// imported shape agrees with the local one. A value key collides the same way,
// even though the signature only learns its name at merge, and it collides
// with a method as well as with a property.
export interface PlainWins {
  ['a']: number,
  a: string,
}
export interface LiteralWins {
  a: string,
  ['a']: number,
}
const dup = 'a';
export interface ValueThenPlain {
  [dup]: number,
  a: string,
}
export interface PlainThenValue {
  a: string,
  [dup]: number,
}
export interface ValueThenMethod {
  [dup]: number,
  a(): void,
}
export interface MethodThenValue {
  a(): void,
  [dup]: number,
}
export interface Overloads {
  a(x: number): number,
  [dup]: boolean,
  a(x: string): string,
}

// A `declare class` keeps its fields apart from its methods and accessors, so
// a value key, which adds a field, shadows them rather than being ordered
// against them. Its static side keeps one map, so a key there is ordered
// against the overloads as in an interface.
declare export class DcValueThenMethod {
  [dup]: number, // ERROR: a field may not shadow a method incompatibly
  a(): void,
}
declare export class DcMethodThenValue {
  a(): void,
  [dup]: number, // ERROR: reported the same way in the other order
}
declare export class DcStaticOverloads {
  static a(x: number): number,
  static [dup]: boolean,
  static a(x: string): string,
}

// An interface merged into a `declare class` of the same name adds its members
// to the proto side, where the class's own field shadows them. A computed key
// is no exception, so the class still wins.
declare export class MergedHolder {
  a: string,
}
interface MergedHolder { // ERROR: the merged key conflicts with the class field
  [dup]: number,
}

// A computed key ordered against a getter and setter pair must resolve the
// same way across the boundary, and a literal key must still split the pair.
const acc = 'p';
export interface AccessorFirst {
  get p(): number,
  set p(x: number): void,
  [acc]: boolean,
}
export interface AccessorLast {
  [acc]: boolean,
  get p(): number,
  set p(x: number): void,
}
export interface AccessorMiddle {
  get p(): number,
  [acc]: boolean,
  set p(x: number): void,
}
export interface AccessorSplit {
  get a(): number,
  ['a']: boolean,
  set a(x: number): void,
}

// A name bound in both namespaces is read as the type in type position here
// too, so the member stays an index signature.
const K = 'a';
type K = 'b';
export interface Dual {
  [K]: number,
}

// An imported constant and variance must survive the boundary as well.
import {KEY} from './sharedkey';
export interface FromImportedConst {
  [KEY]: number,
}
export interface ReadOnlyKey {
  readonly ['a']: number,
  readonly [KEY]: string,
}

// A class key is rejected here, and the signature drops the member the same
// way the checker does, so the imported shape has nothing in it.
class Cls {}
export interface FromClass {
  [Cls]: number, // ERROR: a class object is not a key
}

// A `declare class` carries both readings across the boundary, on both sides.
const dcKey = 'dk';
declare export class Holder {
  ['a']: number,
  static ['b']: string,
  [dcKey]: number,
  static [dcKey]: string,
  [key: string]: boolean,
}
