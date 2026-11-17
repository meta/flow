// A bracketed key that names a *type* is an index signature, whatever that
// type turns out to be. Only a literal or the name of a value is a computed
// key, so a type alias that happens to hold one literal today does not quietly
// become a required property.

type AliasA = 'a';
interface OneAlias {
  [key: AliasA]: number,
}
declare const oneAlias: OneAlias;
oneAlias['a'] as number; // OK via the indexer
oneAlias['a'] as string; // ERROR: number is not string
const emptyAlias: OneAlias = {}; // OK, an index signature requires nothing
oneAlias['b']; // ERROR: `b` is not in the 'a' indexer key set
oneAlias.a; // ERROR: an index signature answers no named read, as a key would

// `keyof X` is a type, so it is an index signature even when `X` has exactly
// one key.
type OneKey = {a: number};
interface KeyofOne {
  [key: keyof OneKey]: string,
}
declare const keyofOne: KeyofOne;
keyofOne['a'] as string; // OK via the indexer
keyofOne['a'] as number; // ERROR: string is not number

// A type parameter is a type, and a literal key next to one is still a named
// property, since a parameter in the *values* cannot change what the keys are.
interface LiteralBound<T extends 'a'> {
  [key: T]: number,
}
interface ValueParam<T> {
  ['a']: T,
}
{
  declare const x: LiteralBound<'a'>;
  x['a'] as number; // OK via the indexer
  const empty: LiteralBound<'a'> = {}; // OK, an index signature requires nothing
  declare const y: ValueParam<number>;
  y.a as number; // OK, a named property
  const emptyParam: ValueParam<number> = {}; // ERROR: `a` is required
}

// A class or an enum binds a value, so a bracketed key naming one is read as
// that value: the class object or the enum object, neither of which is a key.
// The error names the labeled form, which is the index signature that was
// meant.
class C {}
enum En {
  A,
}
interface FromClass {
  [C]: number, // ERROR: a class object is not a key
}
interface FromEnum {
  [En]: number, // ERROR: an enum object is not a key
}
{
  declare const x: FromClass;
  x.anything; // ERROR: the member was dropped, so nothing is there
  declare const y: FromEnum;
  y.anything; // ERROR: the member was dropped here too
}

interface LabeledClass {
  [key: C]: number,
}
{
  declare const x: LabeledClass;
  x['anything']; // ERROR: `anything` is not a `C`, so it is not in the key set
  const empty: LabeledClass = {}; // OK, an index signature requires nothing
}

// An imported class or enum is read the same way a local one is, since an
// imported name says nothing about what it was declared as.
import {C as ImportedC, E as ImportedE} from './clsenum';
interface FromImportedClass {
  [ImportedC]: number, // ERROR: a class object is not a key
}
interface FromImportedEnum {
  [ImportedE]: number, // ERROR: an enum object is not a key
}
{
  declare const x: FromImportedClass;
  x.anything; // ERROR: the member was dropped, so nothing is there
  declare const y: FromImportedEnum;
  y.anything; // ERROR: the member was dropped here too
}

// A `declare class` body classifies its keys the same way.
declare class WithTypeKey {
  [key: AliasA]: number,
  static [C]: string, // ERROR: a class object is not a key
}
const withTypeKey = new WithTypeKey();
withTypeKey['a'] as number; // OK via the indexer
withTypeKey.a; // ERROR: an index signature answers no named read
WithTypeKey.anything; // ERROR: the static member was dropped

// A class/namespace merge can expose a type through the class name, but the
// head still binds the class value, so the unlabeled form is a computed key.
declare class Merged {}
declare namespace Merged {
  type Key = 'merged';
}
interface UnlabeledMerged {
  [Merged.Key]: number, // ERROR: reads `Merged.Key` as a value
}
interface LabeledMerged {
  [key: Merged.Key]: number,
}
{
  declare const x: LabeledMerged;
  x['merged'] as number; // OK via the indexer
  x['other']; // ERROR: `other` is not in the 'merged' indexer key set
  declare const y: UnlabeledMerged;
  y.merged; // ERROR: the rejected member was dropped, so nothing is there
}
