// A bracketed key written as a literal is a specific named property, in an
// interface and a `declare class` body as in an object type body.
interface One {
  ['a']: number,
}
declare const one: One;
one.a as number; // OK
one.a as string; // ERROR: number is not string
one.b; // ERROR: property `b` is missing (a named key, not an indexer)

// The property is required of anything that is a `One`, where an index
// signature would have asked for nothing.
const emptyOne: One = {}; // ERROR: a named property is required
class Impl implements One {
  a: number = 1;
}
class NotImpl implements One {} // ERROR: property `a` is missing

// Several literal keys coexist as named properties, where two index signatures
// cannot.
interface Multi {
  ['a']: number,
  ['b']: string,
}
declare const multi: Multi;
multi.a as number; // OK
multi.b as string; // OK

interface TwoIndexers {
  [key: string]: number,
  [key: number]: string, // ERROR: multiple indexers are still unsupported
}

// A safe-integer number literal is the named property "42".
interface Num {
  [42]: boolean,
}
declare const num: Num;
num[42] as boolean; // OK
num['42'] as boolean; // OK, the same property spelled as a string
num[99]; // ERROR: property `99` is missing (a named key, not a number indexer)

// A named property, a literal-key property and an index signature coexist. The
// literal key's own value type is what `bar` has, so the named property wins
// over the indexer rather than being folded into it.
interface Mixed {
  foo: string,
  ['bar']: boolean,
  [key: string]: number,
}
declare const mixed: Mixed;
mixed.foo as string; // OK
mixed.bar as boolean; // OK, named from the literal key
mixed.bar as number; // ERROR: boolean is not number, so this is not the indexer
mixed['other'] as number; // OK via the index signature
mixed.other; // ERROR: an interface index signature answers no named read

// An optional literal key is an optional named property. The key is read
// before the gate that rejects an optional indexer, which is TSLib-only
// syntax, so the property is kept rather than the member rejected.
interface Opt {
  ['a']?: number,
}
declare const opt: Opt;
opt.a as number | void; // OK
opt.a as number; // ERROR: the property is optional, so `void` is not a number

interface OptIndexer {
  [key: string]?: number, // ERROR: an optional index signature is still rejected
}

// A labeled key `[label: K]` is a TS-style index signature, so it stays one
// even when the key is a literal.
interface Labeled {
  [label: 'a']: number,
}
declare const labeled: Labeled;
labeled['a'] as number; // OK via the indexer
labeled['a'] as string; // ERROR: number is not string
labeled.a; // ERROR: unlike the literal key above, this key names no property
const emptyLabeled: Labeled = {}; // OK, an index signature requires nothing

// The inline `interface {}` type reads its keys the same way.
declare const inlineIface: interface {['a']: number};
inlineIface.a as number; // OK
inlineIface.a as string; // ERROR: number is not string

// A `declare class` body reads a bracketed key the same way, and `static` puts
// the property on the side it is written on.
declare class Holder {
  ['a']: number,
  static ['b']: string,
}
new Holder().a as number; // OK
new Holder().a as string; // ERROR: number is not string
Holder.b as string; // OK
Holder.a; // ERROR: `a` is an instance property, not a static one
new Holder().b; // ERROR: `b` is a static property, not an instance one

// A literal key is folded by name in source order, as a plain property is, so
// it splits a getter and setter pair written around it: the getter it replaces
// is gone, and the setter that follows stands alone.
interface AccessorSplit {
  get a(): number,
  ['a']: boolean,
  set a(x: number): void,
}
declare const accessorSplit: AccessorSplit;
accessorSplit.a; // ERROR: property `a` is not readable
accessorSplit.a = 1; // OK, the setter is what is left

// A literal key and a plain property of the same name resolve by source order,
// last in source winning, as two plain properties do.
interface PlainWins {
  ['a']: number,
  a: string,
}
declare const plainWins: PlainWins;
plainWins.a as string; // OK, the later plain property wins
plainWins.a as number; // ERROR: string is not number

interface LiteralWins {
  a: string,
  ['a']: number,
}
declare const literalWins: LiteralWins;
literalWins.a as number; // OK, the later literal-key property wins
literalWins.a as string; // ERROR: number is not string

// Variance carries onto the named property, as it does onto an indexer.
interface ReadOnlyKey {
  readonly ['a']: number,
}
declare const readOnlyKey: ReadOnlyKey;
readOnlyKey.a as number; // OK
readOnlyKey.a = 1; // ERROR: property `a` is not writable

interface WriteOnlyKey {
  writeonly ['a']: number,
}
declare const writeOnlyKey: WriteOnlyKey;
writeOnlyKey.a as number; // ERROR: property `a` is not readable
writeOnlyKey.a = 1; // OK
