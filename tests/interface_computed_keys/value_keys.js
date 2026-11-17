// A bracketed key written as the name of a value is a computed key, read the
// way the value-level `{[k]: v}` reads it: as the value's type.

const key = 'foo';
interface FromConst {
  [key]: number,
}
declare const fromConst: FromConst;
fromConst.foo as number; // OK
fromConst.foo as string; // ERROR: number is not string
fromConst.bar; // ERROR: property `bar` is missing (named, not an indexer)

// A qualified name reads the property off the value, again as `typeof` would.
const keys = {foo: 'a', bar: 'b'} as const;
interface FromQualified {
  [keys.foo]: number,
  [keys.bar]: string,
}
declare const fromQualified: FromQualified;
fromQualified.a as number; // OK
fromQualified.b as string; // OK
fromQualified.foo; // ERROR: the key is the value `'a'`, not the name `foo`

// A static read off a class is an ordinary property read, so a class name
// heading a qualified key is a computed key, even though a bare one is
// rejected.
declare class Statics {
  static k: 'kk';
}
interface FromStatic {
  [Statics.k]: number,
}
declare const fromStatic: FromStatic;
fromStatic.kk as number; // OK, named from the static's value
fromStatic.k; // ERROR: the key is the value `'kk'`, not the name `k`

// A value whose type is not a single literal names no property at all, so the
// key is rejected and the member is dropped. Reading it as an index signature
// would answer a question the author did not ask.
declare const wide: string;
interface FromWide {
  [wide]: number, // ERROR: `wide` names no one property
}
declare const fromWide: FromWide;
fromWide.anything; // ERROR: the member was dropped, so nothing is there

// A static whose type is not a single literal is told the same thing, and not
// to add a label: the key did reach a value, it is just not one that names a
// property.
declare class WideStatics {
  static n: number;
}
interface FromWideStatic {
  [WideStatics.n]: number, // ERROR: names no one property
}
const emptyWideStatic: FromWideStatic = {}; // OK, the member was dropped

// A `declare class` reads a value key on the side it is written on, so one
// constant can key both an instance and a static property.
const dcKey = 'dk';
declare class WithValueKey {
  [dcKey]: number,
  static [dcKey]: string,
}
new WithValueKey().dk as number; // OK
WithValueKey.dk as string; // OK
new WithValueKey().dk as string; // ERROR: number is not string
WithValueKey.dk as number; // ERROR: string is not number

// An imported constant keys a member, which is why a named import heads a
// computed key: a key can be declared once and used from anywhere.
import {KEY} from './sharedkey';
interface FromImportedConst {
  [KEY]: number,
}
declare const fromImportedConst: FromImportedConst;
fromImportedConst.shared as number; // OK, named from the imported constant
fromImportedConst.KEY; // ERROR: the key is the value, not the binding's name

// An enum member has the enum type rather than a literal type, so it names no
// one property, and `[key: E]` is the index signature that was meant.
import {E} from './clsenum';
interface FromEnumMember {
  [E.A]: number, // ERROR: names no one property
}
const emptyEnumMember: FromEnumMember = {}; // OK, the member was dropped

// A value key and a plain member of the same name resolve by source order,
// last in source winning, whether the plain member is a property or a method.
const dup = 'a';
interface ValueThenPlain {
  [dup]: number,
  a: string,
}
declare const valueThenPlain: ValueThenPlain;
valueThenPlain.a as string; // OK, the later plain property wins
valueThenPlain.a as number; // ERROR: string is not number

interface PlainThenValue {
  a: string,
  [dup]: number,
}
declare const plainThenValue: PlainThenValue;
plainThenValue.a as number; // OK, the later value key wins
plainThenValue.a as string; // ERROR: number is not string

interface ValueThenMethod {
  [dup]: number,
  a(): void,
}
declare const valueThenMethod: ValueThenMethod;
valueThenMethod.a() as void; // OK, the later method wins
valueThenMethod.a as number; // ERROR: the method is not a number

interface MethodThenValue {
  a(): void,
  [dup]: number,
}
declare const methodThenValue: MethodThenValue;
methodThenValue.a as number; // OK, the later value key wins
methodThenValue.a(); // ERROR: a number is not callable

// Two overloads of one name are one member in the signature, which learns a
// value key's name only at merge, so a key written between them loses to the
// two of them together, the way it loses to a getter and setter pair.
interface Overloads {
  a(x: number): number,
  [dup]: boolean,
  a(x: string): string,
}
declare const overloads: Overloads;
overloads.a(1) as number; // OK, the first overload survived the key
overloads.a('s') as string; // OK, and so did the second
overloads.a(true); // ERROR: still a method, so a boolean matches no overload

// A `declare class` is not structural: a field and a method of one name are
// two members, and the field shadows the method whichever order they are
// written in. A value key adds a field, so unlike in an interface it is not
// ordered against a method at all, and the shadowing is what is reported.
declare class DcValueThenMethod {
  [dup]: number, // ERROR: a field may not shadow a method incompatibly
  a(): void,
}
new DcValueThenMethod().a as number; // OK, the field is what is read
declare class DcMethodThenValue {
  a(): void,
  [dup]: number, // ERROR: reported the same way in the other order
}
new DcMethodThenValue().a as number; // OK, the field is read here too

// A getter and setter pair sits on the proto side of a `declare class`, so a
// value key does not split it and is not ordered against it. It shadows the
// pair whole.
declare class DcAccessor {
  get a(): number,
  [dup]: boolean, // ERROR: the field shadows both halves incompatibly
  set a(x: number): void,
}
new DcAccessor().a as boolean; // OK, the field is what is read

// The static side of a `declare class` keeps one map, as an interface body
// does, so a value key there is ordered against the overloads and loses.
declare class DcStaticOverloads {
  static a(x: number): number,
  static [dup]: boolean,
  static a(x: string): string,
}
DcStaticOverloads.a(1) as number; // OK, the key lost to the overloads
DcStaticOverloads.a('s') as string; // OK, both of them survived

// A getter and setter pair is two members sharing a name, so a value key is
// ordered against both of them, not just the first.
const acc = 'p';
interface AccessorFirst {
  get p(): number,
  set p(x: number): void,
  [acc]: boolean,
}
declare const accessorFirst: AccessorFirst;
accessorFirst.p as boolean; // OK, the key follows both halves
accessorFirst.p as number; // ERROR: boolean is not number

interface AccessorLast {
  [acc]: boolean,
  get p(): number,
  set p(x: number): void,
}
declare const accessorLast: AccessorLast;
accessorLast.p as number; // OK, both halves follow the key
accessorLast.p as boolean; // ERROR: number is not boolean

// A value key written between the two halves loses to the pair, since the pair
// ends after it. The signature pipeline learns the key's name only at merge,
// by which time the pair is one member, so a key that split it there could not
// be reproduced. A literal key, whose name is known while the members are
// still being folded in source order, does split the pair (see `within.js`).
interface AccessorMiddle {
  get p(): number,
  [acc]: boolean,
  set p(x: number): void,
}
declare const accessorMiddle: AccessorMiddle;
accessorMiddle.p as number; // OK, the pair wins whole
accessorMiddle.p as boolean; // ERROR: number is not boolean

// A name bound in both namespaces, as `const K` next to `type K`, is read as
// the type in type position, so the member is an index signature.
const K = 'a';
type K = 'b';
interface Dual {
  [key: K]: number,
}
declare const dual: Dual;
dual['b'] as number; // OK via the indexer over 'b'
dual['a']; // ERROR: 'a' is the value, and the type won

// A value key can be optional, like a literal one.
interface OptValue {
  [key]?: number,
}
declare const optValue: OptValue;
optValue.foo as number | void; // OK
optValue.foo as number; // ERROR: the property is optional, so `void` is not a number

// Variance carries onto a value-keyed property too.
interface ReadOnlyValueKey {
  readonly [KEY]: string,
}
declare const readOnlyValueKey: ReadOnlyValueKey;
readOnlyValueKey.shared as string; // OK
readOnlyValueKey.shared = 'x'; // ERROR: property `shared` is not writable
