// @flow

// In a tuple type written in Flow, only the last spread can have an unknown number of elements.
type Concat<A extends ReadonlyArray<unknown>, B extends ReadonlyArray<unknown>> = [...A, ...B]; // ERROR
declare const arrays: Concat<Array<string>, ReadonlyArray<number>>;
arrays as empty; // OK: the tuple type is `any`
