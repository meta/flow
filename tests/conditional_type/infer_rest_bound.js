// An `infer` type of a tuple spread or a rest parameter is an array, unless an `infer` type of the
// same name has a bound.
type RestOfTuple<T> = T extends [unknown, ...infer R] ? R : empty;
type RestOfNamedTuple<T> = T extends [first: unknown, ...rest: infer R] ? R : empty;
type RestOfParams<F> = F extends (x: unknown, ...args: infer A) => unknown ? A : empty;
type RestAndElement<T> = T extends [infer R, ...infer R] ? R : empty;
type BoundedElement<T> = T extends [infer R extends unknown, ...infer R] ? R : empty;

function restOfTuple<T>(x: RestOfTuple<T>): ReadonlyArray<unknown> {
  return x; // ok
}
function restOfNamedTuple<T>(x: RestOfNamedTuple<T>): ReadonlyArray<unknown> {
  return x; // ok
}
function restOfParams<F>(x: RestOfParams<F>): ReadonlyArray<unknown> {
  return x; // ok
}
function restAndElement<T>(x: RestAndElement<T>): ReadonlyArray<unknown> {
  return x; // ok
}
function boundedElement<T>(x: BoundedElement<T>): ReadonlyArray<unknown> {
  return x; // error: unknown ~> ReadonlyArray<unknown>
}
