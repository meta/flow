// An alias below a union with `null` and `void` members is expanded first, as below a maybe type,
// so its own `null` and `void` are absorbed.
type MaybeBox<T> = {v: T} | null | void;
declare function nonNull<U>(x: U | null | void): U;
declare const box: MaybeBox<number>;
nonNull(box) as {v: number}; // ok
