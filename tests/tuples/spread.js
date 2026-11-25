// Basic
type A = [1, 2];

type Same = [...A]; // OK
{
  [1, 2] as Same; // OK
  declare const x: Same;
  x as [1, 2]; // OK

  [1] as Same; // ERROR: arity
  x as [1, 2, 3]; // ERROR: arity

  [1, 99] as Same; // ERROR: element type
}

type Middle = [0, ...A, 3]; // OK
{
  [0, 1, 2, 3] as Middle; // OK
  declare const x: Middle;
  x as [0, 1, 2, 3]; // OK

  [0, 1, 2, 3, 4] as Middle; // ERROR: arity
  x as [0, 1, 2]; // ERROR: arity

  [0, 1, 99, 3] as Middle; // ERROR: element type
}

// Required after optional
type Optionals = [a: 1, b?: 2];
type ReqAfterOptionalInline = [0, ...Optionals, 3]; // ERROR
type ReqAfterOptionalSpread = [...Optionals, ...A]; // ERROR

// Unions
type Union = [1, 2] | [6];
type FromUnion = [0, ...Union, 9]; // OK
[0, 1, 2, 9] as FromUnion; // OK
[0, 6, 9] as FromUnion; // OK
[0, 9] as FromUnion; // ERROR

// Invalid spread
type InvalidSpread1 = [1, ...2]; // ERROR
{
  declare const x: InvalidSpread1;
  x as empty; // OK
}
type Alias = {};
type InvalidSpread2 = [1, ...Alias]; // ERROR
{
  declare const x: InvalidSpread2;
  x as empty; // OK
}

// Array spread
type ArraySpread = [...Array<unknown>]; // OK: an array
{
  declare const x: ArraySpread;
  x as Array<unknown>; // OK
}
type ROArraySpread = [...ReadonlyArray<unknown>]; // OK: an array
{
  declare const x: ROArraySpread;
  x as Array<unknown>; // OK
}

// Spreading `any`
type AnySpread = [1, 2, ...any]; // OK
{
  declare const x: AnySpread;
  x as empty; // OK
}

// Spreading `empty`
type EmptySpread = [1, 2, ...empty]; // OK
{
  declare const x: AnySpread;
  x as empty; // OK
}

// Tuple-like array spread
{
  const x = [1, 2];
  type TupleArrSpread = [0, ...typeof x]; // OK
  [0, 1, 2] as TupleArrSpread; // OK
}

// Generics
type WithTail<T> = [1, ...T];
[1, 2, 3] as WithTail<[2, 3]>; // OK
[1, 2, 66] as WithTail<[2, 3]>; // ERROR

// A spread of a type known only through its bound, when the bound is not an array type, is an array
// of the operand's element type: the bound is not taken apart.
type Elems<T extends ReadonlyArray<unknown>> = T;
type IntersectionBound<T extends [2, 3] & ReadonlyArray<unknown>> = Elems<[1, ...T]>; // OK
type NarrowedTail<T> = T extends ReadonlyArray<string> ? Elems<[1, ...T]> : []; // OK
type Recur<T> = T extends [infer H, ...infer Rest] ? [H, ...Recur<Rest>] : [];
type RecurOfGeneric<T> = Elems<Recur<T>>; // OK
type NotArray<T> = T extends string ? 1 : 2;
type NotArrayOfGeneric<T> = Elems<[...NotArray<T>]>; // ERROR
function unionLast<S extends [0], T extends [1] | [2, 3]>(
  x: [...S, ...T],
): ReadonlyArray<0 | 1 | 2 | 3> {
  return x; // OK
}
function unionNotLast<S extends [0], T extends [1] | [2, 3]>(x: [...S, ...T, 4]) {
  x as ReadonlyArray<unknown>; // ERROR at the type (TypeScript accepts)
}
type Big = [1] | [2] | [3] | [4] | [5] | [6] | [7] | [8] | [9] | [10] | [11] | [12] | [13] | [14] | [15] | [16] | [17] | [18] | [19] | [20];
function manySpreads<T extends Big>(x: [...T, ...T, ...T, ...T]) {
  x as ReadonlyArray<unknown>; // ERROR at the type, at the spreads of `T` before others (TypeScript accepts)
}
function intersectionOrder1<T extends ReadonlyArray<number> & [number]>(t: T): [...T, ...T] {
  return t; // ERROR at the type (TypeScript errors here)
}
function intersectionOrder2<T extends [number] & ReadonlyArray<number>>(t: T): [...T, ...T] {
  return t; // ERROR at the type (TypeScript errors here)
}
// TypeScript accepts this. Flow errors, because repositioning the tuple type while it is evaluated
// gives `empty`.
type B6<T> = T extends [infer H] ? (H extends 1 ? [0, ...B6<H>] : [1]) : [];
function b6<X>(x: B6<X>): ReadonlyArray<0 | 1> {
  return x;
}

// Reversal
declare function tail<T>(xs: [1, ...T]): T;
tail<[2, 3]>([ 1, 2, 3]); // OK
{
  const x = tail([1]); // OK
  x as []; // OK
}
{
  const x = tail([1, 2, 3]);
  x as [2, 3]; // OK
  x as [1, 2, 3]; // ERROR
}
tail([666, 2, 3]); // ERROR

declare function noReversal1<T>(xs: [1, ...T, 2]): T;
noReversal1([1, 9, 2]); // ERROR

declare function noReversal2<T>(xs: [...T, ...any]): T;
noReversal2([1, 2, 3]); // ERROR

// Spreading empty array/tuple
{
  [...[]] as []; // OK

  declare const x: [];
  [...x] as []; // OK
}

//  Order
{
  declare const x: [0, 1, ...[]];
  x[0] as empty; // ERROR
  x[0] as 0; // OK
  x[1] as 1; // OK
}
{
  declare const x: [0, 1, ...[2, 3]];
  x[0] as empty; // ERROR
  x[0] as 0; // OK
  x[1] as 1; // OK
  x[2] as 2; // OK
  x[3] as 3; // OK
}
