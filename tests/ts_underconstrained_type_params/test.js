import {anyBound, defineExtension, empty, erroneousBound, first, id, keyOf, keyOfCond, keyOfOpts, keyOfWith, make, refLike, withDefault} from 'repro';
import {useState} from 'react';

// As in TypeScript, a type parameter of a TypeScript generic that no part of
// the arguments or the hint reaches takes its bound, or `unknown` without one.
const ext = defineExtension({name: 'root'});
ext.config as void | {...}; // OK
ext.build?.({}) as unknown; // OK
ext.build?.({}) as void | string; // ERROR: `Output` is `unknown`

// Its default when it has one, as before.
withDefault().t.k as 'd'; // OK

// A written `any` bound stands for `unknown`.
anyBound().t as number; // ERROR: `T` is `unknown`

// The hint of an enclosing call still decides it.
const nums: Array<number> = id(empty()); // OK
const n: number = first(empty()); // OK

// A hint that leaves the enclosing call's type parameter undecided reaches
// nothing: TypeScript infers nothing from an outer type parameter without
// inferences.
declare function idf<S>(f: () => S): S;
const hinted = idf(() => defineExtension({name: 'root'})); // OK
hinted.build?.({}) as void | string; // ERROR: `Output` is `unknown`
idf(() => first(empty())); // OK
hook useExtension() {
  const [extension] = useState(() => defineExtension({name: 'root'})); // OK
  extension.config?.a; // ERROR: `ConfigBase` has no `a`
}

// One that a part of the arguments or the hint reaches is still
// underconstrained: TypeScript infers `null` from `null` against `T | null`
// and `{a: any}` from `'a'` against `keyof T`.
refLike(null); // ERROR: `T` is underconstrained
keyOf('a'); // ERROR: `T` is underconstrained

// So is one of an inner call under a hint that leaves such a type parameter
// undecided: TypeScript infers `{a: any}` for both.
keyOfWith('a', () => make()); // ERROR: `T` of `make` is underconstrained
keyOfWith('a', () => idf(() => make())); // ERROR: `T` of `make` is underconstrained
keyOfCond('a', () => make()); // ERROR: `T` of `make` is underconstrained
keyOfOpts({key: 'a', make: n => make()}); // ERROR: `T` of `make` is underconstrained

// So is one whose bound is `any` after an error.
erroneousBound(); // ERROR: `T` is underconstrained

// The type parameters of a Flow generic are still underconstrained.
declare function defineFlow<Config extends {...}, Output>(extension: {
  config?: Config,
  build?: (config: Config) => Output,
}): {config?: Config, build?: (config: Config) => Output};
defineFlow({}); // ERROR: `Config` and `Output` are underconstrained
