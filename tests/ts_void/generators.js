// An unannotated generator's `TNext` is `void`, as the library's
// `Iterator<T, TReturn = void, TNext = void>`.
function* g() {
  yield 1;
}
const a: Iterable<number> = g(); // OK
const b: Iterator<number> = g(); // OK
const c: Iterator<number, void, number> = g(); // ERROR
