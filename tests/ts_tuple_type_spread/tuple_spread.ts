// In a tuple type written in TypeScript, as in TypeScript, a spread of unknown length and the
// spreads of unknown length and optional elements after it are one rest element of their union.
type Concat<A extends ReadonlyArray<unknown>, B extends ReadonlyArray<unknown>> = [...A, ...B];
declare const arrays: Concat<Array<string>, ReadonlyArray<number>>;
const arraysOk: Array<string | number> = arrays; // OK
const arraysBad: Array<string> = arrays; // ERROR: number ~> string

type Middle<A extends ReadonlyArray<unknown>, B extends ReadonlyArray<unknown>> = [boolean, ...A, 1, ...B];
declare const middle: Middle<Array<string>, Array<number>>;
const middleOk: [boolean, ...Array<string | number>] = middle; // OK
const middleBad: [boolean, ...Array<string | 1>] = middle; // ERROR: number ~> string | 1

type OptionalAfter<A extends ReadonlyArray<unknown>> = [...A, boolean?];
declare const optionalAfter: OptionalAfter<Array<string>>;
const optionalAfterOk: Array<string | boolean | undefined> = optionalAfter; // OK

type Elems<T extends ReadonlyArray<unknown>> = T;

// Spreads of type parameters are not merged, since what they are is not known yet.
type ConcatOfGenerics<A extends ReadonlyArray<string>, B extends ReadonlyArray<number>> = Elems<[...A, ...B]>; // ERROR (TypeScript accepts)
function concatOfGenerics<A extends unknown[], B extends unknown[]>(): [...A, ...B] {
  return ['x']; // ERROR at the type (TypeScript errors here)
}
function arrayThenGeneric<A extends unknown[]>(): [...string[], ...A] {
  return ['x']; // ERROR at the type (TypeScript errors here)
}
function genericThenArray<A extends unknown[]>(): [...A, ...string[]] {
  return ['x']; // ERROR at the type (TypeScript errors here)
}

// The rest element of a tuple type with no other elements is a TypeScript array, as the tuple type is.
export type Collapsed = Concat<Array<string>, Array<number>>;

// A required element after them would be a leading rest element, which a tuple type cannot have.
type Leading<A extends ReadonlyArray<unknown>> = [...A, boolean]; // ERROR (TypeScript accepts)
declare const leading: Leading<Array<string>>;
leading as never; // OK: the tuple type is `any`
type LeadingFromTuple<A extends ReadonlyArray<unknown>> = [...A, ...[boolean]]; // ERROR (TypeScript accepts)
declare const leadingFromTuple: LeadingFromTuple<Array<string>>;
leadingFromTuple as never; // OK: the tuple type is `any`
