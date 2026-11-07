// Nested patterns preserve interface and class property types.
interface Named {
  name: string;
}
class NamedClass implements Named {
  name: string;
}
class InheritedNamedClass extends NamedClass {}
interface InheritedNamed extends Named {
  extra: number;
}
interface GenericNamed<T> {
  name: T;
}

declare const maybeNamed: ?Named;
declare const maybeClass: ?NamedClass;
declare const maybeInheritedClass: ?InheritedNamedClass;
declare const maybeInherited: ?InheritedNamed;
declare const maybeGeneric: ?GenericNamed<string>;
declare const anonymous: ?interface {name: string};
declare const other: boolean;

// Structural objects provide a control for nested nullable property extraction.
type NamedObject = {name: string, ...};
declare const maybeObject: ?NamedObject;
const objectControl = match ([maybeObject, other]) {
  [{const name, ...}, _] => name as string, // OK
  _ => '', // OK
};

// Nested nullable interfaces preserve readable property types.
const original = match ([maybeNamed, other]) {
  [{const name, ...}, _] => name as string, // OK
  _ => '', // OK
};

// Structural patterns extract properties from nullable class instances.
const classInput = match ([maybeClass, other]) {
  [{const name, ...}, _] => name as string, // OK
  _ => '', // OK
};

// Class patterns preserve the property types of nullable instances.
const classPattern = match ([maybeClass, other]) {
  [NamedClass {const name, ...}, _] => name as string, // OK
  _ => '', // OK
};

// Inherited class fields remain readable in nested patterns.
const inheritedClass = match ([maybeInheritedClass]) {
  [{const name, ...}] => name as string, // OK
  _ => '', // OK
};

// Inherited interface fields retain their declared types.
match ([maybeInherited]) {
  [{const name, const extra, ...}] => { // OK
    name as string; // OK
    extra as number; // OK
    extra as string; // ERROR: the property type is not empty
  },
  _ => {}, // OK
};

// Generic interface arguments determine extracted property types.
const genericInterface = match ([maybeGeneric]) {
  [{const name, ...}] => name as string, // OK
  _ => '', // OK
};

// Anonymous interface properties support nested extraction.
const anonymousInterface = match ([anonymous]) {
  [{const name, ...}] => name as string, // OK
  _ => '', // OK
};

// Annotated tuples preserve nested property binding types.
const tuple: [?Named, boolean] = [maybeNamed, other];
const annotatedTuple = match (tuple) {
  [{name: const value, ...}, _] => value as string, // OK
  _ => '', // OK
};

// Nested object patterns support nullable interface properties.
const nestedObject = match ({value: maybeNamed}) {
  {value: {const name, ...}} => name as string, // OK
  _ => '', // OK
};

// Wildcard property patterns can inspect nullable interface fields.
const wildcardProperty = match ([maybeNamed]) {
  [{name: _, ...}] => true, // OK
  _ => false, // OK
};

// Earlier nullish arms leave readable non-null interface properties.
const previousNull = match ([maybeNamed]) {
  [null | undefined] => '', // OK
  [{const name, ...}] => name as string, // OK
};

// Optional interface properties preserve their possible void value.
interface OptionalNamed {
  readonly name?: string;
}
declare const optional: ?OptionalNamed;
match ([optional]) {
  [{const name, ...}] => { // OK
    name as string | void; // OK
    name as number; // ERROR: retain the declared property type
  },
  _ => {}, // OK
};

// An earlier wildcard arm keeps an absent property from becoming reachable.
match ([optional]) {
  [{name: _, ...}] => {}, // OK
  [{const name, ...}] => {}, // ERROR: unused, do not resurrect an absent property
  _ => {}, // OK
};

// Recursive interface properties support nested extraction.
interface RecursiveNamed {
  name: string;
  next: ?RecursiveNamed;
}
declare const recursive: ?RecursiveNamed;
const recursiveProperty = match ([recursive]) {
  [{next: {const name, ...}, ...}] => name as string, // OK
  _ => '', // OK
};

// Readable interface indexers determine extracted property types.
interface Indexed {
  [key: string]: number;
}
declare const indexed: ?Indexed;
const indexedProperty = match ([indexed]) {
  [{foo: const value, ...}] => value as number, // OK
  _ => 0, // OK
};

// Cached narrowed properties take precedence over their declared types.
interface TaggedNamed {
  kind: 'a' | 'b';
  name: string;
}
declare const tagged: ?TaggedNamed;
match ([tagged]) {
  [{kind: 'a', ...}] => {}, // OK
  [{const kind, const name, ...}] => { // OK
    kind as 'b'; // OK
    kind as 'a'; // ERROR: a cached narrowed property must win over the original type
    name as string; // OK
  },
  _ => {}, // OK
};

// Guards leave property bindings available to later patterns.
const guarded = match ([maybeNamed, other]) {
  [{const name, ...}, true] if (other) => name as string, // OK
  [{const name, ...}, _] => name as string, // OK
  _ => '', // OK
};

// Ordinary access still rejects nullable interface properties.
maybeNamed.name; // ERROR: nullable property access

// Write-only fields remain unreadable after nested extraction.
interface WriteOnlyNamed {
  writeonly name: string;
}
declare const writeonlyValue: ?WriteOnlyNamed;
const unreadable = match ([writeonlyValue]) {
  [{const name, ...}] => name, // ERROR: the property is not readable
  _ => null, // OK
};

// An indexer cannot make an inherited write-only field readable.
interface IndexedWriteOnlyNamed extends WriteOnlyNamed {
  [key: string]: string;
}
declare const indexedWriteonlyValue: ?IndexedWriteOnlyNamed;
declare const indexedNonNullable: IndexedWriteOnlyNamed;
indexedNonNullable.name; // ERROR: ordinary access to the inherited field is not readable
const unreadableInheritedProperty = match ([indexedWriteonlyValue]) {
  [{const name, ...}] => name, // ERROR: an indexer does not make the inherited field readable
  _ => null, // OK
};

// Named write-only fields override readable indexers in either parent order.
interface ReadableNameIndexer {
  [key: string]: string;
}
interface WriteOnlyBeforeIndexer extends WriteOnlyNamed, ReadableNameIndexer {}
interface IndexerBeforeWriteOnly extends ReadableNameIndexer, WriteOnlyNamed {}
declare const writeonlyBeforeIndexer: ?WriteOnlyBeforeIndexer;
declare const indexerBeforeWriteonly: ?IndexerBeforeWriteOnly;
const unreadableFirstParent = match ([writeonlyBeforeIndexer]) {
  [{const name, ...}] => name, // ERROR: named declarations take precedence over indexers
  _ => null, // OK
};
const unreadableSecondParent = match ([indexerBeforeWriteonly]) {
  [{const name, ...}] => name, // ERROR: parent order must not make the field readable
  _ => null, // OK
};

// Extracted class methods still require their receiver.
class MethodClass {
  name: string;
  method(): string { return this.name; }
}
declare const methodClass: ?MethodClass;
const unboundMethod = match ([methodClass]) {
  [{method: const method, ...}] => method(), // ERROR: requires a receiver
  _ => '', // OK
};

// Extracted interface methods still require their receiver.
interface MethodInterface {
  method(): string;
}
declare const methodInterface: ?MethodInterface;
const unboundInterfaceMethod = match ([methodInterface]) {
  [{method: const method, ...}] => method(), // ERROR: requires a receiver
  _ => '', // OK
};

// Object patterns remain unused on non-object inputs.
declare const onlyNumber: number;
const notAnObject = match ([onlyNumber]) {
  [{const name, ...}] => name, // ERROR: unused
  _ => null, // OK
};

// Undeclared interface fields retain ordinary missing-property errors.
const missingProperty = match ([maybeNamed]) {
  [{unknown: const unknown, ...}] => unknown, // ERROR: unknown property
  _ => null, // OK
};

// Extracted constructor values still require new.
class ExplicitConstructor {
  constructor() {}
}
declare const explicitInstance: ExplicitConstructor;
const ordinaryConstructor = explicitInstance.constructor; // OK
ordinaryConstructor(); // ERROR: an extracted class constructor requires new
const extractedNonNullableConstructor = match ([explicitInstance]) {
  [{constructor: const ctor, ...}] => ctor(), // ERROR: a class constructor requires new
};

// Nullable constructor extraction preserves the special constructor read.
declare const maybeExplicitInstance: ?ExplicitConstructor;
const extractedConstructor = match ([maybeExplicitInstance]) {
  [{constructor: const ctor, ...}] => ctor(), // ERROR: preserve the special constructor read
  _ => null, // OK
};

// Nested extraction preserves React deep-readonly restrictions on interface props.
interface DeepReadonlyValue {
  items: Array<number>;
}
component DeepReadonlyInterface(value: DeepReadonlyValue) {
  value.items.push(1); // ERROR: ordinary access preserves React deep-readonly
  const result = match ([value]) {
    [{const items, ...}] => items.push(1), // ERROR: React deep-readonly must be preserved
  };
  return null;
}

// Nested extraction preserves React deep-readonly restrictions on class props.
class DeepReadonlyClassValue {
  items: Array<number>;
}
component DeepReadonlyClass(value: DeepReadonlyClassValue) {
  value.items.push(1); // ERROR: ordinary access preserves React deep-readonly
  const result = match ([value]) {
    [{const items, ...}] => items.push(1), // ERROR: React deep-readonly must be preserved
  };
  return null;
}

// Whole-value aliases retain nullability after nested object matching.
const alias = match ([maybeNamed]) {
  [{...} as const value] => value as Named, // ERROR: still nullable
  _ => null, // OK
};

// Aliased superclass intersections preserve named write-only fields in either order.
type IndexerWriteOnlyAlias = ReadableNameIndexer & WriteOnlyNamed;
type WriteOnlyIndexerAlias = WriteOnlyNamed & ReadableNameIndexer;
interface AliasedIndexerFirst extends IndexerWriteOnlyAlias {}
interface AliasedWriteOnlyFirst extends WriteOnlyIndexerAlias {}
declare const aliasedIndexerFirst: AliasedIndexerFirst;
declare const aliasedWriteOnlyFirst: AliasedWriteOnlyFirst;
declare const maybeAliasedIndexerFirst: ?AliasedIndexerFirst;
declare const maybeAliasedWriteOnlyFirst: ?AliasedWriteOnlyFirst;
aliasedIndexerFirst.name; // ERROR: ordinary access is not readable
aliasedWriteOnlyFirst.name; // ERROR: ordinary access is not readable
const unreadableAliasedIndexerFirst = match ([maybeAliasedIndexerFirst]) {
  [{const name, ...}] => name, // ERROR: an aliased indexer cannot hide the named field
  _ => null, // OK
};
const unreadableAliasedWriteOnlyFirst = match ([maybeAliasedWriteOnlyFirst]) {
  [{const name, ...}] => name, // ERROR: alias member order does not affect readability
  _ => null, // OK
};

// Generic aliases retain inherited named property types ahead of indexers.
interface UnknownValueIndexer {
  [key: string]: unknown;
}
interface ReadableAliasValue<T> {
  readonly value: T;
}
type IndexedValueAlias<T> = UnknownValueIndexer & ReadableAliasValue<T>;
interface AliasedNumberValue extends IndexedValueAlias<number> {}
declare const aliasedNumberValue: ?AliasedNumberValue;
match ([aliasedNumberValue]) {
  [{const value, ...}] => {
    value as number; // OK
    value as string; // ERROR: preserve the generic named property type
  },
  _ => {}, // OK
}

