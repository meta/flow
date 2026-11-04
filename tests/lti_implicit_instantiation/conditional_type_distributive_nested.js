// The nested conditional type sees `T` in the member of the distributive conditional type and is
// decided, as in TypeScript.
type Nested<P> = P extends unknown ? (string extends keyof P ? empty : P) : empty;
declare function f<T>(x: Nested<{v: T}>): T;
const r = f({v: 1});
r as number; // ok
r as empty; // error: number ~> empty

// `T` in the extends type of the nested conditional type is undecided, so it takes its default, as
// it does without the distributive conditional type.
type InExtends<P> = P extends unknown ? ({readonly v: string} extends P ? P : number) : empty;
declare function g<T = unknown>(x: InExtends<{readonly v: T}>): T;
const s = g({v: 'a'});
s as empty; // error: unknown ~> empty
