type D<T> = {readonly a: D<ReadonlyArray<T>>, readonly r: D<T | void>, readonly v: T};
type E<T> = {readonly a: E<ReadonlyArray<T>>, readonly r: E<T | void>, readonly v: T};
declare const d: D<string>;
const e: E<number> = d; // error
