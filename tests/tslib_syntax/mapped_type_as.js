type O = {a: number, b: string};

// Identity / no-op
type Id = {[K in keyof O as K]: O[K]};
declare const i: Id;
i as {a: number, b: string}; // OK
({a: 1, b: 's'}) as Id; // OK

// Filter via empty (Omit-like)
type WithoutB = {[K in keyof O as K extends 'b' ? empty : K]: O[K]};
declare const w: WithoutB;
w as {a: number}; // OK
w.b; // ERROR - missing

// Pick-by-value via conditional
type PickStrings<T> = {[K in keyof T as T[K] extends string ? K : empty]: T[K]};
type S = PickStrings<{a: number, b: string, c: string}>;
declare const s: S;
s as {b: string, c: string}; // OK
s.a; // ERROR - missing

// Constant remap with collision (many source keys -> one new key)
type Same = {[K in keyof O as 'X']: O[K]};
declare const sa: Same;
sa.X as number | string; // OK - collision policy: union value types
sa.X as number; // ERROR

// Conditional rename to a literal
type RenameA = {[K in keyof O as K extends 'a' ? 'AA' : K]: O[K]};
declare const ra: RenameA;
ra.AA as number; // OK
ra.b as string; // OK
ra.a; // ERROR - renamed away

// Indexed-access remap (no template literals required)
type Mapping = {a: 'X', b: 'Y'};
type Indexed = {[K in keyof O as Mapping[K]]: O[K]};
declare const ix: Indexed;
ix.X as number; // OK
ix.Y as string; // OK
ix.a; // ERROR - renamed away

// Empty mapped type via `as empty`
type Empty = {[K in keyof O as empty]: O[K]};
declare const ev: Empty;
ev as {}; // OK
ev.a; // ERROR - missing

// Source-key K used in value position is the original key
type R = {[K in keyof O as K extends 'a' ? 'A' : K]: K};
declare const r: R;
r.A as 'a'; // OK
r.b as 'b'; // OK

// Composes with -readonly and -?
type Mut<T> = {-readonly [K in keyof T as K]-?: T[K]};
type M = Mut<{readonly a?: number, readonly b?: string}>;
declare const m: M;
m.a as number; // OK
m.a = 5; // OK

// Composes with +readonly
type Frozen = {+readonly [K in keyof O as K]: O[K]};
declare const f: Frozen;
f.a = 5; // ERROR - readonly

// Optional-merge on collision: `a?` should remain optional after merge
type SrcOpt = {a?: number, b: string};
type Coll = {[K in keyof SrcOpt as 'X']: SrcOpt[K]};
declare const co: Coll;
co.X as number | string | void; // OK - 'a' was optional, merged result is optional
({X: 1}) as Coll; // OK - X is optional, value can be number
({}) as Coll; // OK - X is optional, can be omitted

// Polarity-merge on collision: if any contributor is readonly, the merged property must remain
// readonly (TS "loses write permission" semantics).
type SrcRO = {readonly a: number, b: string};
type CollRO = {[K in keyof SrcRO as 'X']: SrcRO[K]};
declare const cro: CollRO;
cro.X as number | string; // OK
cro.X = 1; // ERROR - readonly survives the collision

// Non-homomorphic mapped type with `as` - source is a plain union of literals
type NonHomo = {[K in 'a' | 'b' as K extends 'a' ? 'A' : K]: K};
declare const nh: NonHomo;
nh.A as 'a'; // OK
nh.b as 'b'; // OK
nh.a; // ERROR - renamed

// Indexer source: dict key is remapped via identity
type Dict = {[s: string]: number};
type RD = {[K in keyof Dict as K]: Dict[K]};
declare const rd: RD;
const x: number = rd.foo; // OK

// Indexer source: `as empty` drops indexer
type Drop = {[K in keyof Dict as empty]: Dict[K]};
declare const dr: Drop;
dr as {}; // OK - exact, no indexer
dr.foo; // ERROR

// Identity remap preserves Inexact source obj_kind: the result must remain assignable to an
// inexact object that mentions only a subset of its fields.
type InexactSrc = {a: number, b: string, ...};
type IdentityInexact = {[K in keyof InexactSrc as K]: InexactSrc[K]};
declare const ii: IdentityInexact;
const _ii_inexact: {a: number, ...} = ii; // OK - identity preserves inexactness
const _ii_exact_bad: { a: number, b: string } = ii; // ERROR - source was inexact

// Non-identity remap on an Inexact source: with no surviving destination indexer, the result
// is exact (the only known properties are the explicitly-named destinations).
type ConstInexact = {[K in keyof InexactSrc as 'X']: InexactSrc[K]};
declare const ci: ConstInexact;
ci.X as number | string; // OK
const _ci_exact: { X: number | string } = ci; // OK - non-identity remap closes the shape
ci.notThere; // ERROR - source's open shape did not propagate

// Semi-homomorphic with non-literal selected indexer keys preserves the indexer
type PickAs<T, K extends keyof T> = {[P in K as P]: T[P]};
type SemiDict = PickAs<{[s: string]: number}, string>;
declare const sd: SemiDict;
const _sd_val: number = sd.foo; // OK - string indexer retained

// Tuple/array source rejected in v1
type TupRen = {[K in keyof [string, number] as K]: [string, number][K]}; // ERROR - v1 limitation
