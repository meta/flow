// An application meets the applications of the same class among the members of a union through
// the first that it fits with the markers open and whose evidence agrees, and none if no member
// does, as in speculation.
declare function box<T>(x: T): Set<T>;
declare function arr<T>(x: Array<T>): Array<T>;
arr([box(box('a')), box(box('b'))]); // ok

// The hint `Promise<X> | X` of the inner `then` says nothing about `TResult2` through
// `Promise<X>`, which the callback's object does not fit.
declare function explain(): Promise<?string>;
declare var timedOut: Promise<{done: false}>;
const r = Promise.race([explain().then(card => ({done: true, card})), timedOut]);
r.then(x => {
  x.done as boolean; // ok
});
