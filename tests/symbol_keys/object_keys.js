// Runtime key enumeration (`Object.keys` / `Object.entries`) omits `unique
// symbol` keys, matching JavaScript, even though type-level `keyof` includes
// them.
declare const s: unique symbol;
const o = {[s]: 1, a: 2, b: 3};

const ks = Object.keys(o);
ks as Array<'a' | 'b'>; // OK: no symbol key in the enumeration

const es = Object.entries(o);
es as Array<['a' | 'b', unknown]>; // OK: no symbol key in the enumeration

// A `unique symbol` *indexer* is likewise omitted by runtime enumeration: it
// must never be stringified into a bogus string key. A string indexer, by
// contrast, does contribute string keys.
declare const s2: unique symbol;
declare const symIndexed: {[key: typeof s2]: boolean};
const symKeys = Object.keys(symIndexed);
symKeys as Array<empty>; // OK: symbol indexer contributes no runtime key

declare const strIndexed: {[k: string]: boolean};
const strKeys = Object.keys(strIndexed);
strKeys as Array<string>; // OK: string indexer contributes string keys
