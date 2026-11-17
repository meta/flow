type Exact = Record<'foo' | 'bar', number>;

declare const exact: Exact;
exact as {foo: number, bar: number}; // OK!
exact as {foo: number}; // ERROR, mising bar

type Indexed = Record<'foo' | string, number>;
declare const indexed: Indexed;
indexed as {foo: number, [key: string]: number}; // OK
indexed as {foo: number}; // ERROR, missing indexer

// Record with number key type
type NumIndexed = Record<number, string>;
declare const numIndexed: NumIndexed;
numIndexed as {[key: number]: string}; // OK
numIndexed as {1: string}; // ERROR, missing indexer

// Record with symbol key type
type SymIndexed = Record<symbol, boolean>;
declare const symIndexed: SymIndexed;
symIndexed as {[key: symbol]: boolean}; // OK
symIndexed as {foo: boolean}; // ERROR, missing indexer

// Record with PropertyKey
type AnyKeyed = Record<PropertyKey, number>;
declare const anyKeyed: AnyKeyed;
anyKeyed.foo as number; // OK
anyKeyed[0] as number; // OK
