// Exercises the signature (cross-module) pipeline: these types are consumed in
// `imp.js`, so their shapes come from `type_sig`, not the within-file checker.
export type One = {['a']: number};
export type Multi = {['a']: number, ['b']: string};
export type Num = {[42]: boolean};
export type Dict = {[key: string]: number};
export type Union = {[key: 'a' | 'b']: number};
export type Mixed = {foo: string, ['bar']: boolean, [key: string]: number};
export type Opt = {['a']?: number};
export type Proto = {['__proto__']: number};

// Value-name keys, both bare and qualified. The signature pipeline must read
// them as the value's type, the same way the checker does.
const key = 'foo';
export type FromConst = {[key]: number};
const keys = {foo: 'a'} as const;
export type FromQualified = {[keys.foo]: number};
declare const wide: string;
export type FromWide = {[wide]: number}; // ERROR: `wide` names no one property

// Type-name keys stay index signatures across the boundary too.
type AliasA = 'a';
export type OneAlias = {[key: AliasA]: number};
type OneKey = {a: number};
export type KeyofOne = {[key: keyof OneKey]: string};
export type LiteralBound<T extends 'a'> = {[key: T]: number};
export type ValueParam<T> = {['a']: T};

// A labeled key `[label: K]` is an index signature across the boundary too.
export type LabeledLit = {[label: 'a']: number};

// Same-name collisions must resolve by source order, last winning, so the
// imported type agrees with the local one. A value key collides the same way,
// even though the signature only learns its name at merge.
export type PlainWins = {['a']: number, a: string};
export type LiteralWins = {a: string, ['a']: number};
const dup = 'a';
export type ValueThenPlain = {[dup]: number, a: string};
export type PlainThenValue = {a: string, [dup]: number};

// Object type spread overriding must also agree across the boundary.
type Base = {a: number};
export type SpreadOverridesLiteral = {['a']: string, ...Base};
export type LiteralOverridesSpread = {...Base, ['a']: string};

// A name bound in both namespaces, and a computed key ordered against a getter
// and setter pair, must resolve the same way across the boundary.
const K = 'a';
type K = 'b';
export type Dual = {[key: K]: number};
const acc = 'a';
export type AccessorFirst = {get a(): number, set a(x: number): void, [acc]: boolean};
export type AccessorLast = {[acc]: boolean, get a(): number, set a(x: number): void};
export type AccessorMiddle = {get a(): number, [acc]: boolean, set a(x: number): void};

// An imported constant, a negated literal, and variance must all survive the
// boundary as well.
import {KEY} from './sharedkey';
export type FromImportedConst = {[KEY]: number};
export type NegKey = {[-1]: boolean};
export type ReadOnlyKey = {readonly ['a']: number, readonly [KEY]: string};
export type WriteOnlyKey = {writeonly ['a']: number};

// A class key is rejected here, and the signature drops the member the same
// way the checker does, so the imported type has nothing in it.
class Cls {}
export type FromClass = {[Cls]: number}; // ERROR: a class object is not a key

// A qualified type exposed by a class/namespace merge is still headed by the
// class value. The unlabeled form is rejected and dropped, while the label
// explicitly preserves the merged type as an index signature in the export.
declare class Merged {}
declare namespace Merged {
  type Key = 'merged';
}
export type UnlabeledMerged = {[Merged.Key]: number}; // ERROR: reads `Merged.Key` as a value
export type LabeledMerged = {[key: Merged.Key]: number}; // OK: explicit indexer with label
