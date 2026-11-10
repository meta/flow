// A spread of a type parameter passes the type parameter itself to a rest parameter, as the
// flow-based solve keeps it: `TA` is `A`.
declare function inner<TA extends Array<unknown>>(cb: (...args: TA) => void, ...args: TA): void;
function outer<A extends Array<unknown>>(cb: (...args: A) => void, ...args: A): void {
  inner(cb, ...args); // ok
}
