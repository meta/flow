// A homomorphic mapped type over a type parameter whose bound is an intersection with an array
// or tuple member maps each member: the array or tuple as an array or tuple.

// tsc accepts.
function intersectionBound<T extends Iterable<unknown> & ReadonlyArray<unknown>>(
  m: {[K in keyof T]: T[K]},
): ReadonlyArray<unknown> {
  m.length as number; // OK
  return m; // OK
}

// tsc accepts.
function tupleIntersectionBound<T extends [1, 2] & ReadonlyArray<unknown>>(
  m: {[K in keyof T]: T[K]},
): ReadonlyArray<unknown> {
  return m; // OK
}

// tsc accepts. In the true branch, `T` is known to be a `ReadonlyArray<unknown>` too.
function narrowedInTrueBranch<T extends Iterable<unknown>>(
  m: T extends ReadonlyArray<unknown> ? {[K in keyof T]: T[K]} : empty,
): ReadonlyArray<unknown> {
  return m; // OK
}

// tsc rejects: the elements are arrays of numbers.
function wrongElement<T extends Iterable<unknown> & ReadonlyArray<number>>(
  m: {[K in keyof T]: Array<T[K]>},
): ReadonlyArray<number> {
  return m; // ERROR
}
