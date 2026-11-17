// `keyof` over a symbol index signature whose key is not known yet when the
// key set is built: the key is written behind a type-level computation, or the
// object type arrives from a generic call. Such a key has to be sent through a
// use type that waits for it to resolve. Reading the key in place instead sees
// an unresolved type and falls back to stringifying it, which turns a symbol
// key into a bogus string key.

import type {DeferredDict} from './deferred_key';
import {s as importedS} from './deferred_key';

declare const s: unique symbol;
declare const t: unique symbol;

// A key written behind a type alias application.
type Id<T> = T;
declare const aliasDict: {[k: Id<typeof s>]: number};
declare const aliasKey: keyof typeof aliasDict;

aliasKey as typeof s; // OK
aliasKey as string; // ERROR: the key is a `unique symbol`, not a string

// A key written behind a conditional type.
type PickKey<B> = B extends true ? typeof s : string;
declare const condDict: {[k: PickKey<true>]: number};
declare const condKey: keyof typeof condDict;

condKey as typeof s; // OK
condKey as string; // ERROR: the key is a `unique symbol`, not a string

// The false branch of the same conditional is a string key, which still
// stringifies.
declare const condStrDict: {[k: PickKey<false>]: number};
declare const condStrKey: keyof typeof condStrDict;

condStrKey as string; // OK
condStrKey as typeof s; // ERROR: the key is a string, not a symbol

// A key deferred the same way, in a type built by another module, so the key
// set is built by signature inference rather than by the checker.
declare const importedKey: keyof DeferredDict;

importedKey as typeof importedS; // OK
importedKey as string; // ERROR: the key is a `unique symbol`, not a string

// The object type can arrive from a generic call, inferred from the argument.
declare function keysOf<O>(o: O): keyof O;

declare const symDict: {[key: typeof s]: number};

keysOf(symDict) as typeof s; // OK
keysOf(symDict) as string; // ERROR: the key is a `unique symbol`, not a string
keysOf(symDict) as typeof t; // ERROR: a distinct symbol is not a key

// A string indexer through the same path still stringifies.
declare const strDict: {[key: string]: number};

keysOf(strDict) as string; // OK
keysOf(strDict) as typeof s; // ERROR: the key is a string, not a symbol

// The object type can also be written by the callee, so that the indexer key is
// only known once the type argument is solved.
declare function keysOfDict<K>(o: {[k: K]: number}): keyof {[k: K]: number};

keysOfDict(symDict) as typeof s; // OK
keysOfDict(symDict) as string; // ERROR: the key is a `unique symbol`, not a string

// The type argument can be given explicitly rather than inferred.
keysOf<{[key: typeof s]: number}>(symDict) as typeof s; // OK
keysOf<{[key: typeof s]: number}>(symDict) as string; // ERROR: the key is a `unique symbol`, not a string

// A generic key bounded by a `unique symbol` stays that symbol in the key set
// rather than widening to `string`.
declare function keysOfBounded<K extends typeof s>(o: {[k: K]: number}): keyof {
  [k: K]: number,
};

keysOfBounded(symDict) as typeof s; // OK
keysOfBounded(symDict) as string; // ERROR: the key is a `unique symbol`, not a string
