declare function parse(input: string): number;
declare function parse(input: ReadonlyArray<string>): ReadonlyArray<number>;

declare function apply<A, R>(f: (A) => R, a: A): R;

// Overloads against one signature: the last overload, as TypeScript infers from the signatures of
// an overloaded function.
const n = apply(parse, ['1']); // ok
n as ReadonlyArray<number>; // ok
n as number; // error
