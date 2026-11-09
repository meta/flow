// A conditional return type that the type parameters leave undecided does not fail an overload
// against the hint, as in the flow-based solve, where it is stuck on type variables.
type EntryPoint<P> = {p: P};
declare var n: number;
declare function entryPoint<T>(x: T): T extends empty ? empty : EntryPoint<{readonly a: T}>;
declare function entryPoint<T, U>(x: T, y: (u: U) => void): EntryPoint<U>;
declare function nested<P>(cb: () => {ep: EntryPoint<P>, params: P}): void;
nested(() => ({ep: entryPoint(n), params: {a: 1}})); // ok
