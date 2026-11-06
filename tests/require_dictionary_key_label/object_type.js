type Unlabeled = {[string]: number}; // error
type Labeled = {[key: string]: number}; // ok
type ReadOnly = {readonly [string]: number}; // error
type Mixed = {foo: string, [number]: boolean}; // error
type Exact = {|[string]: number|}; // error
type Nested = {[key: string]: {[number]: string}}; // error on the inner key
type Alias = string;
type AliasKey = {[Alias]: number}; // error
type Generic<K extends string> = {[K]: number}; // error
type Union = {['a' | 'b']: number}; // error
type Mapped<O extends {...}> = {[K in keyof O]: O[K]}; // ok

// A computed key names one property, so it is not a dictionary.
type StringKey = {['a']: number}; // ok
type NumberKey = {[1]: number}; // ok
const k = 'k';
type ValueKey = {[k]: number}; // ok

let annot: {[string]: number} = {}; // error
function f(x: {[string]: number}): void {} // error
declare function g(x: {[string]: number}): void; // error

export type Exported = {[string]: number}; // error
