// A bracketed key written as a literal is a specific named property.
type One = {['a']: number};
declare const one: One;
one.a as number; // OK
one.a as string; // ERROR: number is not string
one.b; // ERROR: property `b` is missing (exact, no indexer default)
const emptyOne: One = {}; // ERROR: a named property is required, unlike an indexer

// Several literal keys coexist as named properties (no "multiple indexers").
type Multi = {['a']: number, ['b']: string};
declare const multi: Multi;
multi.a as number; // OK
multi.b as string; // OK

// A safe-integer number literal is the named property "42".
type Num = {[42]: boolean};
declare const num: Num;
num[42] as boolean; // OK
num['42'] as boolean; // OK
num[99]; // ERROR: property `99` is missing (named key, not a number indexer)

// A fractional number literal is a specific named property,
// using its canonical JavaScript string spelling.
type Frac = {[1.5]: boolean};
declare const frac: Frac;
frac[1.5] as boolean; // OK, the named property
frac[2.5]; // ERROR: property 2.5 is missing

// A general string key is an index signature.
type Dict = {[string]: number};
declare const dict: Dict;
dict.anything as number; // OK via the indexer
dict.anything as string; // ERROR: number is not string

// A union of literals is an index signature, not several named properties.
type Union = {['a' | 'b']: number};
declare const u: Union;
u.a as number; // OK via the indexer
u.c; // ERROR: `c` is not in the 'a' | 'b' indexer key set

// A named property, a literal-key property, and an index signature coexist.
// The literal key's own value type is what `bar` has, so the named property
// wins over the indexer rather than being folded into it.
type Mixed = {foo: string, ['bar']: boolean, [string]: number};
declare const mixed: Mixed;
mixed.foo as string; // OK
mixed.bar as boolean; // OK, named from the literal key
mixed.bar as number; // ERROR: boolean is not number, so this is not the indexer
mixed.other as number; // OK via the index signature

// An optional literal key is an optional named property, not an optional
// indexer (which is TSLib-only syntax): the property is kept, not rejected.
type Opt = {['a']?: number};
declare const opt: Opt;
opt.a as number | void; // OK
opt.a as number; // ERROR: property is optional, so `void` is not a number

// A bracketed `['__proto__']` key is an own named property, matching JS
// computed-key semantics: `{['__proto__']: v}` creates an own property, unlike
// `{__proto__: v}` which sets the prototype.
type Proto = {['__proto__']: number};
declare const p: Proto;
p.__proto__ as number; // OK
p.__proto__ as string; // ERROR: number is not string

// A labeled key `[label: K]` is a TS-style index signature, so it stays one
// even when the key is a literal.
type Labeled = {[label: 'a']: number};
declare const labeled: Labeled;
labeled.a as number; // OK via the indexer
labeled.a as string; // ERROR: number is not string
const emptyLabeled: Labeled = {}; // OK, an index signature requires nothing

// Two index signatures are still unsupported.
type TwoIndexers = {[string]: number, [number]: string}; // ERROR: multiple indexers

// A literal key and a plain property of the same name resolve by source order,
// last in source winning, as duplicate properties do.
type PlainWins = {['a']: number, a: string};
declare const plainWins: PlainWins;
plainWins.a as string; // OK, the later plain property wins
plainWins.a as number; // ERROR: string is not number

type LiteralWins = {a: string, ['a']: number};
declare const literalWins: LiteralWins;
literalWins.a as number; // OK, the later literal-key property wins
literalWins.a as string; // ERROR: number is not string

// Object type spread: overriding is the purpose of spread, so a later spread
// overrides an earlier literal-key property.
type Base = {a: number};
type SpreadOverridesLiteral = {['a']: string, ...Base};
declare const spreadOver: SpreadOverridesLiteral;
spreadOver.a as number; // OK, `...Base` overrides the earlier literal key
spreadOver.a as string; // ERROR: number is not string

// A literal-key property after a spread overrides the spread.
type LiteralOverridesSpread = {...Base, ['a']: string};
declare const litOver: LiteralOverridesSpread;
litOver.a as string; // OK, the literal key after the spread wins
litOver.a as number; // ERROR: string is not number
