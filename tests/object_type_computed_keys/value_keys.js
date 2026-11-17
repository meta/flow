// A bracketed key written as the name of a value is a computed key, read the
// way the value-level `{[k]: v}` reads it: as the value's type.

const key = 'foo';
type FromConst = {[key]: number};
declare const fromConst: FromConst;
fromConst.foo as number; // OK
fromConst.foo as string; // ERROR: number is not string
fromConst.bar; // ERROR: property `bar` is missing (named, not an indexer)

// A qualified name reads the property off the value, again as `typeof` would.
const keys = {foo: 'a', bar: 'b'} as const;
type FromQualified = {[keys.foo]: number, [keys.bar]: string};
declare const fromQualified: FromQualified;
fromQualified.a as number; // OK
fromQualified.b as string; // OK
fromQualified.foo; // ERROR: the key is the value `'a'`, not the name `foo`

// A static read off a class is an ordinary property read, so a class name
// heading a qualified key is a computed key, even though a bare one is
// rejected. The key names the property the static's value spells.
declare class Holder {
  static k: 'kk';
}
type FromStatic = {[Holder.k]: number};
declare const fromStatic: FromStatic;
fromStatic.kk as number; // OK, named from the static's value
fromStatic.k; // ERROR: the key is the value `'kk'`, not the name `k`

// A static whose value is not a single literal names no property either, and
// it is told what a computed key needs rather than to add a label. A class
// heading the key is not enough on its own to mean an index signature: this
// key did reach a value, it is just not one that names a property.
declare class WideHolder {
  static n: number;
}
type FromWideStatic = {[WideHolder.n]: number}; // ERROR: names no one property
const emptyWideStatic: FromWideStatic = {}; // OK, the member was dropped

// A misspelled static is told the same thing, and not to add a label. Only the
// namespace half of a merge can put a type behind a qualified name, so a plain
// class heading the key leaves nothing for a label to reach.
type FromMissingStatic = {[WideHolder.nope]: number}; // ERROR: names no one property

// A number-valued binding names the property its value spells.
const numKey = 7;
type FromNumber = {[numKey]: boolean};
declare const fromNumber: FromNumber;
fromNumber[7] as boolean; // OK
fromNumber[8]; // ERROR: property `8` is missing

// A value whose type is not a single literal names no property at all. Reading
// it as an index signature would answer a question the author did not ask, so
// the key is rejected and the member is dropped.
declare const wide: string;
type FromWide = {[wide]: number}; // ERROR: `wide` names no one property
declare const fromWide: FromWide;
fromWide.anything; // ERROR: the member was dropped, so nothing is there

// A `let` is read the same way, and is rejected the same way.
let mutable = 'm';
type FromLet = {[mutable]: number}; // ERROR: `mutable` names no one property
const emptyLet: FromLet = {}; // OK, the member was dropped

// A value key and a plain property of the same name resolve by source order.
// `exp.js` exports the same pair, and `imp.js` must agree.
const dup = 'a';
type ValueThenPlain = {[dup]: number, a: string};
declare const valueThenPlain: ValueThenPlain;
valueThenPlain.a as string; // OK, the later plain property wins
valueThenPlain.a as number; // ERROR: string is not number

type PlainThenValue = {a: string, [dup]: number};
declare const plainThenValue: PlainThenValue;
plainThenValue.a as number; // OK, the later value key wins
plainThenValue.a as string; // ERROR: number is not string

// A value key can be optional, like a literal one.
type OptValue = {[key]?: number};
declare const optValue: OptValue;
optValue.foo as number | void; // OK
optValue.foo as number; // ERROR: property is optional, so `void` is not a number

// A key that reads back the object being defined is an ordinary cycle, and is
// reported as one rather than needing an answer of its own.
declare const cyclic: Cyclic; // ERROR: definition cycle through the key
const cyclicKey = cyclic.x;
type Cyclic = {x: 'k', [cyclicKey]: number}; // ERROR: the cyclic key names nothing

// A name bound in both namespaces, as `const K` next to `type K`, is read as
// the type in type position, so the member is an index signature. The scopes
// keep one binding per name, so the read itself has to say which was meant.
const K = 'a';
type K = 'b';
type Dual = {[key: K]: number};
declare const dual: Dual;
dual.b as number; // OK via the indexer over 'b'
dual.a; // ERROR: 'a' is the value, and the type won

// Declaring them the other way round reads the same.
type K2 = 'b';
const K2 = 'a';
type DualRev = {[key: K2]: number};
declare const dualRev: DualRev;
dualRev.b as number; // OK via the indexer over 'b'

// A getter and setter pair is two members sharing a name, so a computed key is
// ordered against both of them, not just the first.
const acc = 'a';
type AccessorFirst = {get a(): number, set a(x: number): void, [acc]: boolean};
declare const accessorFirst: AccessorFirst;
accessorFirst.a as boolean; // OK, the computed key follows both halves

type AccessorLast = {[acc]: boolean, get a(): number, set a(x: number): void};
declare const accessorLast: AccessorLast;
accessorLast.a as number; // OK, both halves follow the computed key

// A computed key written between the two halves loses to the pair, since the
// pair ends after it. Folding the members in source order instead would leave a
// write-only `a`, which is not what the signature can express, so the key has
// to lose here too. `exp.js` exports the same shape, and `imp.js` must agree.
type AccessorMiddle = {get a(): number, [acc]: boolean, set a(x: number): void};
declare const accessorMiddle: AccessorMiddle;
accessorMiddle.a as number; // OK, the pair wins whole
accessorMiddle.a as boolean; // ERROR: number is not boolean

// An imported constant keys a member. This is why a named import heads a
// computed key: a key can be declared once and used from anywhere. It is also
// the one place the signature pipeline's `ImportBinding` reading has to agree
// with the checking pipeline's `Kind::Import`.
import {KEY} from './sharedkey';
type FromImportedConst = {[KEY]: number};
declare const fromImportedConst: FromImportedConst;
fromImportedConst.shared as number; // OK, named from the imported constant
fromImportedConst.KEY; // ERROR: the key is the value, not the binding's name

// An imported class reads the same way a local one does, bare or qualified: a
// static read off it is a computed key, and the class itself is not.
import {C as ImportedHolder, E as ImportedEnum} from './clsenum';
type FromImportedStatic = {[ImportedHolder.k]: number};
declare const fromImportedStatic: FromImportedStatic;
fromImportedStatic.kk as number; // OK, named from the static's value
fromImportedStatic.k; // ERROR: the key is the value `'kk'`, not the name `k`

// An enum member has the enum type rather than a literal type, so it names no
// one property, and `{[key: E]: V}` is the index signature that was meant.
type FromEnumMember = {[ImportedEnum.A]: number}; // ERROR: names no one property
const emptyEnumMember: FromEnumMember = {}; // OK, the member was dropped

// A negative number literal names the property its value spells.
type NegKey = {[-1]: boolean};
declare const negKey: NegKey;
negKey[-1] as boolean; // OK
negKey['-1'] as boolean; // OK, the same property spelled as a string
negKey[1]; // ERROR: property `1` is missing

// Variance carries onto the named property, as it does onto an indexer.
type ReadOnlyKey = {readonly ['a']: number, readonly [KEY]: string};
declare const readOnlyKey: ReadOnlyKey;
readOnlyKey.a as number; // OK
readOnlyKey.shared as string; // OK
readOnlyKey.a = 1; // ERROR: property `a` is not writable
readOnlyKey.shared = 'x'; // ERROR: property `shared` is not writable either

type WriteOnlyKey = {writeonly ['a']: number};
declare const writeOnlyKey: WriteOnlyKey;
writeOnlyKey.a as number; // ERROR: property `a` is not readable
writeOnlyKey.a = 1; // OK
