export opaque type O = {a: string};
export opaque type Op<T> = {v: T};

declare function g<T>(x: {a: T}): T;
declare const o: O;

// In the file that defines it, an opaque type is its underlying type.
g(o) as string; // ok
g(o) as empty; // error: `string`
