// @flow
// A computed method key `[expr](): T` in a `.js` interface, `declare class`, or
// object type. These were previously a parse error in `.js` positions, where
// only `[Symbol.X]()` was special-cased. A labeled `[key: K]: V` stays an
// indexer, and a literal key resolves to a named method reachable with a dot.

interface I {
  ['m'](): number;
  [42](): string;
  [key: string]: number; // indexer still coexists
}
declare const i: I;
i['m']() as number; // OK
i.m() as number; // OK: literal key is a named method
i[42]() as string; // OK
i['x'] as number; // OK: via the string indexer
i['m']() as string; // ERROR: number is not string

declare class C {
  ['dm'](): number;
  static ['sm'](): string;
}
declare const c: C;
c['dm']() as number; // OK
c.dm() as number; // OK: literal key is a named method
C.sm() as string; // OK: static literal key is a named static method
c['dm']() as string; // ERROR: number is not string

type OT = {['f'](): number};
declare const ot: OT;
ot.f() as number; // OK
ot['f']() as number; // OK
ot.f() as string; // ERROR: number is not string

// An unnamed indexer `[T]: V` (where the bracket holds a type, not a literal
// key) still parses as an indexer: it is told apart from a computed method by
// the token after `]`, with no backtracking.
type OI = {[key: string]: number};
declare const oi: OI;
oi['x'] as number; // OK: via the unnamed string indexer
oi['x'] as string; // ERROR: number is not string

// A computed method and an unnamed indexer coexist in one object type body.
type Mixed = {['run'](): number, [key: string]: number};
declare const mx: Mixed;
mx.run() as number; // OK: literal key is a named method
mx['other'] as number; // OK: via the unnamed indexer
mx.run() as string; // ERROR: number is not string

// A negative safe-integer key resolves to a named method reachable by bracket.
interface Neg {
  [-1](): number;
}
declare const neg: Neg;
neg[-1]() as number; // OK: negative safe-integer key is a named method
neg[-1]() as string; // ERROR: number is not string

// A bare primitive keyword in computed method key position is a value
// reference, exactly like any other identifier and matching a `.d.ts` body:
// here `string` is the value `string`, not the `string` type.
declare const string: 'run';
interface Prim {
  [string](): number;
}
declare const prim: Prim;
prim.run() as number; // OK: `string` resolves to the named method `run`
prim.run() as string; // ERROR: number is not string

// A value-name key, the ordinary spelling: the method is named by what the
// reference resolves to, not by how the reference is written.
const k = 'run';
interface Bare {
  [k](): number;
}
declare const bare: Bare;
bare.run() as number; // OK
bare.run() as string; // ERROR: number is not string

// A qualified value-name key reads as a member chain on the value.
const keys = {m: 'go'} as const;
interface Qualified {
  [keys.m](): number;
}
declare const qual: Qualified;
qual.go() as number; // OK
qual.go() as string; // ERROR: number is not string

// A key that names no one property leaves the member unnamed, the same way a
// non-computed method of an unknown name would.
declare const wide: string;
interface Wide {
  [wide](): number; // ERROR: `wide` names no one property
}

// Type parameters follow a computed key just as they follow a plain one.
interface Poly {
  ['id']<T>(x: T): T;
}
declare const poly: Poly;
poly.id(1) as number; // OK
poly.id(1) as string; // ERROR: number is not string

// A computed key parses ahead of the optional-method `?`, which stays gated on
// tslib syntax the same way a plain optional method is.
interface Opt {
  ['m']?(): number; // ERROR: optional method signatures are tslib-only
}

// A computed method key and a plain method of the same name become overloads
// of that name, and a call with no argument to tell them apart picks the first.
// `imp.js` checks each of these bodies across a module boundary, where the
// signature pipeline only learns a value name at merge and so has to order the
// overloads the same way from the other side.
const dup = 'a';

// A literal key is folded in by name, so it takes its place in source order.
interface LiteralFirst {
  ['a'](): 'q';
  a(): 'p';
}
declare const lf: LiteralFirst;
lf.a() as 'q'; // OK: the literal key is written first
lf.a() as 'p'; // ERROR: 'q' is not 'p'

// A value name is only known at merge, so its overload lands after every name
// the body writes outright, wherever the key itself sits.
interface ValueFirst {
  [dup](): 'q';
  a(): 'p';
}
declare const vf: ValueFirst;
vf.a() as 'p'; // OK: the value name goes last however early it is written
vf.a() as 'q'; // ERROR: 'p' is not 'q'
interface ValueLast {
  a(): 'p';
  [dup](): 'q';
}
declare const vl: ValueLast;
vl.a() as 'p'; // OK: and it is still last in this order
vl.a() as 'q'; // ERROR: 'p' is not 'q'

// Two value names of one property fold in back to front, so the last one
// written is the first overload among them.
const dup2 = 'a';
interface TwoValues {
  [dup](): 'q';
  [dup2](): 'z';
}
declare const tv: TwoValues;
tv.a() as 'z'; // OK: the later key is the earlier overload
tv.a() as 'q'; // ERROR: 'z' is not 'q'

// An accessor cannot share a name with a method, so one written after a value
// name takes the name outright rather than joining it.
interface AccessorLast {
  [dup](): 'q';
  get a(): 'p';
}
declare const al: AccessorLast;
al.a as 'p'; // OK: the accessor takes the name
al.a as 'q'; // ERROR: 'p' is not 'q'
interface AccessorFirst {
  get a(): 'p';
  [dup](): 'q';
}
declare const af: AccessorFirst;
af.a() as 'q'; // OK: nothing is written after the key, so the method wins
af.a() as 'p'; // ERROR: the accessor lost the name, so 'q' is not 'p'
