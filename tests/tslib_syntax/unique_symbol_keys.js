// Value-level `unique symbol` computed property keys on object literals.

declare const key1: unique symbol;
declare const key2: unique symbol;
declare const key3: unique symbol;

const o = {
  name: "Alice",
  [key1]: 1,
  [key2]: "hello",
};

// A symbol-keyed property resolves to its declared type on access.
o[key1] as number; // OK
o[key2] as string; // OK
o.name as string; // OK

// Accessing with the wrong expected type errors.
o[key1] as string; // ERROR: number is incompatible with string
o[key2] as number; // ERROR: string is incompatible with number

// A distinct unique symbol is a distinct key, so it is missing.
o[key3]; // ERROR: property is missing

// A unique symbol used as a key type is an indexer that a matching symbol
// access resolves through.
type Dict = {[key: typeof key1]: boolean};
declare const d: Dict;
d[key1] as boolean; // OK
d[key1] as number; // ERROR: boolean is incompatible with number

// A unique symbol reached through a generic type-parameter bound resolves to the
// named symbol property, just like a direct access. The key type here is a
// generic wrapping the `unique symbol`, not the bare `unique symbol` itself.
function readViaGeneric<K extends typeof key1>(k: K): number {
  return o[k]; // OK: resolves to the `[key1]: 1` property
}

function readViaGenericBad<K extends typeof key1>(k: K): string {
  return o[k]; // ERROR: number is incompatible with string
}
