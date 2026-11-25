import {anyBound, defineExtension, empty, first, id, keyOf, linked, refLike, withDefault} from 'repro';

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

// One that a part of the arguments or the hint reaches is still
// underconstrained: TypeScript infers `null` from `null` against `T | null`
// and `{a: any}` from `'a'` against `keyof T`.
refLike(null); // ERROR: `T` is underconstrained
keyOf('a'); // ERROR: `T` is underconstrained

// So is one whose bound is `any` after an error.
linked(); // ERROR: `T` is underconstrained

// The type parameters of a Flow generic are still underconstrained.
declare function defineFlow<Config extends {...}, Output>(extension: {
  config?: Config,
  build?: (config: Config) => Output,
}): {config?: Config, build?: (config: Config) => Output};
defineFlow({}); // ERROR: `Config` and `Output` are underconstrained
