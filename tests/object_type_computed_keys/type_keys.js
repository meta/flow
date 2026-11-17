// A bracketed key that names a *type* is an index signature, whatever that
// type turns out to be. Only a literal or the name of a value is a computed
// key, so a type alias that happens to hold one literal today does not quietly
// become a required property.

type AliasA = 'a';
type OneAlias = {[key: AliasA]: number};
declare const oneAlias: OneAlias;
oneAlias.a as number; // OK via the indexer
oneAlias.a as string; // ERROR: number is not string
const emptyAlias: OneAlias = {}; // OK, an index signature requires nothing
oneAlias.b; // ERROR: `b` is not in the 'a' indexer key set

// An indexed access resolving to a literal is a type, so also an index
// signature.
type Accessed = {foo: 'foo'};
type FromAccess = {[key: Accessed['foo']]: number};
declare const fromAccess: FromAccess;
fromAccess.foo as number; // OK via the indexer
const emptyAccess: FromAccess = {}; // OK, an index signature requires nothing
fromAccess.bar; // ERROR: `bar` is not in the 'foo' indexer key set

// `keyof X` is a type, so it is an index signature even when `X` has exactly
// one key.
type OneKey = {a: number};
type KeyofOne = {[key: keyof OneKey]: string};
declare const keyofOne: KeyofOne;
keyofOne.a as string; // OK via the indexer
keyofOne.a as number; // ERROR: string is not number
const emptyKeyofOne: KeyofOne = {}; // OK

// A type parameter, and anything mentioning one, is a type.
type Unbounded<T> = {[key: keyof T]: string, ...};
{
  declare const x: Unbounded<{a: number, b: number}>;
  x.a as string; // OK via the indexer
  x.anything as string; // ERROR: `anything` is not in the `keyof T` key set
}

type LiteralBound<T extends 'a'> = {[key: T]: number};
{
  declare const x: LiteralBound<'a'>;
  x.a as number; // OK via the indexer
  x.a as string; // ERROR: number is not string
}

// A generic alias applied to a parameter is a type application, so an index
// signature.
type Filter<T extends string> = T;
type Applied<T> = {[key: Filter<T>]: number};
{
  declare const x: Applied<'a'>;
  x.a as number; // OK via the indexer
  const empty: Applied<'a'> = {}; // OK, an index signature requires nothing
}

// A class or an enum binds a value, so a bracketed key naming one is read as
// that value: the class object or the enum object, neither of which is a key.
// Whether the name is local or imported cannot change this, since an imported
// name says nothing about what it was declared as, so both are rejected and
// both are told to add a label.
class C {}
enum En {
  A,
}
type FromClass = {[C]: number}; // ERROR: a class object is not a key
type FromEnum = {[En]: number}; // ERROR: an enum object is not a key
{
  declare const x: FromClass;
  x.anything; // ERROR: the member was dropped, so nothing is there
}

// The labeled form is the index signature that was meant.
type LabeledClass = {[key: C]: number};
{
  declare const x: LabeledClass;
  x.anything; // ERROR: `anything` is not a `C`, so it is not in the key set
  const empty: LabeledClass = {}; // OK, an index signature requires nothing
}

// A class/namespace merge can expose a type through the class name, but the
// head still binds the class value, so the unlabeled form is a computed key.
// Label the key to explicitly ask for an index signature over the merged type.
declare class Merged {}
declare namespace Merged {
  type Key = 'merged';
}
type UnlabeledMerged = {[Merged.Key]: number}; // ERROR: reads `Merged.Key` as a value
type LabeledMerged = {[key: Merged.Key]: number}; // OK: explicit indexer with label

// The merged name on its own is rejected the way a plain class name is, and is
// told the same thing, since the merge only changes what the value side holds.
type BareMerged = {[Merged]: number}; // ERROR: the merged value is not a key
{
  declare const x: LabeledMerged;
  x.merged as number; // OK via the indexer
  x.other; // ERROR: `other` is not in the 'merged' indexer key set
  const empty: LabeledMerged = {}; // OK, an index signature requires nothing
  declare const y: BareMerged;
  y.anything; // ERROR: the member was dropped, so nothing is there
}

// A type parameter buried in the *values* cannot change what the keys are, so
// a literal key next to one is still a named property.
type ValueParam<T> = {['a']: T};
{
  declare const x: ValueParam<number>;
  x.a as number; // OK, named property
  x.b; // ERROR: property `b` is missing (named, not an indexer)
}

// A namespace import reaches a type through a qualified name, so a qualified
// name headed by an import is an index signature. `import * as A` and
// `import {a}` are the same kind of binding, so neither heads a computed key
// when qualified.
import * as NsKeys from './nskeys';
type FromNs = {[key: NsKeys.K]: number};
{
  declare const x: FromNs;
  x.a as number; // OK via the indexer
  x.b; // ERROR: `b` is not in the 'a' indexer key set
  const empty: FromNs = {}; // OK, an index signature requires nothing
}

// The namespace is in the type namespace too, so a bare one is read as a type
// and reports what it did before computed keys existed, rather than becoming a
// computed key over the module object.
type FromNsBare = {[key: NsKeys]: number}; // ERROR: a namespace is not a type

// The imported spelling is rejected the same way as the local one above.
import {C as ImportedC, E as ImportedE} from './clsenum';
type FromImportedClass = {[ImportedC]: number}; // ERROR: a class object is not a key
type FromImportedEnum = {[ImportedE]: number}; // ERROR: an enum object is not a key
{
  declare const x: FromImportedClass;
  x.anything; // ERROR: the member was dropped, so nothing is there
  declare const y: FromImportedEnum;
  y.anything; // ERROR: the member was dropped, so nothing is there
}

// A `declare class` is rejected as a bare key the same way a `class` is.
declare class Declared {}
type FromDeclareClass = {[Declared]: number}; // ERROR: a class object is not a key
{
  declare const x: FromDeclareClass;
  x.anything; // ERROR: the member was dropped, so nothing is there
}
