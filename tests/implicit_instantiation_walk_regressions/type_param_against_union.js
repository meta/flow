// A type parameter that no member of a union takes meets the union by its bound: the walk pins
// `X` to `T`, and the check of the call compares `T` with the union through the bound of `T`.
type U = {readonly tag: 'A'} | {readonly tag: 'B'};

declare function id<X extends U>(x: X): X;

function boundedByIntersectionWithUnion<T extends U & {...}>(t: T): T {
  return id(t); // ok
}

declare function orNull<X extends {a: 1} & {b: 2}>(x: X): X | null;

function boundedByIntersection<T extends {a: 1} & {b: 2}>(t: T): T | null {
  return orNull(t); // ok
}
