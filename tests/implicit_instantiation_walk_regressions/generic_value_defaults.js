// The default of a type parameter of a generic value refers to the earlier ones.
declare function apply<R>(f: () => R): R;
declare function g<A = string, B = A>(): B;

const a = apply(g);
a as string;
a as empty; // error

declare function h<A = string, B = Array<A>>(): B;

const b = apply(h);
b as Array<string>;
