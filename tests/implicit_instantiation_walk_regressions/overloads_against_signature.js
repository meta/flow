declare function parse(input: string): number;
declare function parse(input: ReadonlyArray<string>): ReadonlyArray<number>;

declare function apply<A, R>(f: (A) => R, a: A): R;

// Overloads against one signature: the first overload that fits it with the markers open, as in
// the speculative match of the flow-based solve.
const n = apply(parse, ['1']); // error: the first overload takes a string
n as number; // ok
n as ReadonlyArray<number>; // error

declare function applyToArray<R>(f: (input: ReadonlyArray<string>) => R): R;
applyToArray(parse) as ReadonlyArray<number>; // ok: the first overload does not fit
