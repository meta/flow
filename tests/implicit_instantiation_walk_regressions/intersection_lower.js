declare function i<T>(x: ReadonlyArray<T>): T;
declare const a: Array<string> & Array<number>;

// The first member of an intersection that fits, as in subtyping.
i(a) as string; // ok
i(a) as empty; // error: `string`
