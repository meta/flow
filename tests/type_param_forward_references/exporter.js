// @flow

export declare function exportedForward<A extends B, B>(a: A, b: B): A; // OK: bound forward-references a later sibling

export type ExportedChain<A extends B, C, B extends C> = [A, B, C]; // OK: chained forward bounds

export type ExportedBadDefault<A extends number = B, B = string> = [A, B]; // ERROR: default forward reference

export type ExportedShadow<A extends <A>() => A> = A; // OK: A inside the bound is a fresh binder, not a ref of the outer A

export type ExportedNestedBadCheckedDefault<A extends number = <X>() => B, B = string> = [A, B]; // ERRORS: invalid default and forward reference

export type ExportedCycle<A extends B, B extends A> = [A, B]; // ERROR: circular bounds

export type ExportedNestedCycle<A extends Array<B>, B extends Array<A>> = [A, B]; // OK: the bounds refer to each other through type arguments

export type ExportedMixedCycle<A extends B, B = A> = [A, B]; // ERROR: bound/default cycle

export declare function exportedInferForward<A extends B, B>(a: A): B; // OK: B is inferred from A's bound

export interface Comparable<T> {
  compareTo(other: T): number;
}

export declare function exportedMax<T extends Comparable<T>>(a: T, b: T): T; // OK: the bound refers to T through a type argument

export type ExportedSelfBound<T extends ReadonlyArray<T>> = T; // OK: the bound refers to T through a type argument
