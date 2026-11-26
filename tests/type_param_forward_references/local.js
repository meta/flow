// @flow

declare function forward<A extends B, B>(a: A, b: B): A; // OK: bound forward-references a later sibling

forward("ok", ""); // OK
forward<string, number>("bad", 0); // ERROR: string does not satisfy number

declare function chain<A extends B, C, B extends C>(a: A, b: B, c: C): A; // OK: chained forward bounds

chain(1, 2, 3); // OK
chain<string, number, number>("bad", 2, 3); // ERROR: string does not satisfy number

type Mixed<A extends B, B = A> = [A, B]; // ERROR: bound/default cycle
const mixed: Mixed<string> = ["a", "b"]; // OK: recovered cycle members are any

type BadDefault<A = B, B = string> = [A, B]; // ERROR: default forward reference
type SelfDefault<A = A> = A; // ERROR: default self reference
type BadCheckedDefault<A extends number = B, B = string> = [A, B]; // ERROR: ignore invalid default instead of checking it against number
type NestedBadCheckedDefault<A extends number = <X>() => B, B = string> = [A, B]; // ERRORS: invalid default and forward reference
const recoveredNestedBadDefault: NestedBadCheckedDefault<> = [true, "ok"]; // ERROR: default remains a function
type Outer<B> = <A = B, B = string>() => [A, B]; // ERROR: future B shadows outer B

type Shadow<A extends <A>() => A> = A; // OK: A inside the bound is a fresh binder, not a self reference
const badShadow: Shadow<number> = 1; // ERROR: number does not satisfy the function bound

const recoveredBadDefault: BadCheckedDefault<> = [true, "ok"]; // OK: invalid default is recovered as any

type Cycle<A extends B, B extends A> = [A, B]; // ERROR: circular bounds
const badCycle: Cycle<number, string> = [1, "bad"]; // ERROR: number does not satisfy string

type SelfBound<A extends A> = A; // ERROR: circular bound

type NestedCycle<A extends Array<B>, B extends Array<A>> = [A, B]; // OK: the bounds refer to each other through type arguments

type NestedSelfBound<A extends Array<A>> = A; // OK: the bound refers to A through a type argument

type OverlappingCycle<
  A extends B,
  B extends A & C, // ERROR: every member is recovered, one cycle diagnostic
  C extends B,
> = [A, B, C];

type NestedShadow<A extends <B>() => B, B extends Array<A>> = [A, B]; // OK: inner B shadows sibling B

type TwoCycles<
  A extends B,
  B extends A, // ERROR: first circular bound
  C extends D,
  D extends C, // ERROR: second, independent circular bound
> = [A, B, C, D];

type GlobalCollision = number;
declare function shadowsGlobal<A extends GlobalCollision, GlobalCollision extends string>(
  a: A,
  b: GlobalCollision,
): A; // OK: the tparam shadows the global of the same name

shadowsGlobal("ok", ""); // OK
