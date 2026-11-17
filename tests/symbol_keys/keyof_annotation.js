// `keyof` over a `unique symbol` indexer in an annotation position is computed
// through annotation/signature inference (a distinct path from the checker's
// dispatch). The symbol indexer key must be included as the symbol type itself,
// not stringified, so this agrees with the dispatch path.
import type {IndexedI, IndexedC, SKey} from './keys';
import {s as importedS} from './keys';

declare const s: unique symbol;
declare const key: keyof {[key: typeof s]: number};

key as typeof s; // OK: the symbol indexer key is included, not stringified
key as string; // ERROR: the key is a `unique symbol`, not a string

// An interface and a `declare class` reach annotation inference through a
// different arm than an object type, so each needs its own case.
declare const ikey: keyof IndexedI;
ikey as typeof importedS; // OK
ikey as string; // ERROR: the key is a `unique symbol`, not a string

declare const ckey: keyof IndexedC;
ckey as typeof importedS; // OK
ckey as string; // ERROR: the key is a `unique symbol`, not a string

// The indexer key can also be written as a cross-module alias rather than a
// bare `unique symbol`, so it arrives behind an annotation.
declare const akey: keyof {[k: SKey]: number};
akey as typeof importedS; // OK
akey as string; // ERROR: the key is a `unique symbol`, not a string
