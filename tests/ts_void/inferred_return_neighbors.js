// The hint of a function in a `match` case or a conditional branch includes the
// other cases' values, so looking up its contextual return type types those
// functions too, without looking theirs up again.
declare function f(s: string): void;
const fromMatch = (key: 'a' | 'b'): (() => void) =>
  match (key) {
    'a' => (() => {
      f('a');
    }),
    'b' => (() => {
      f('b');
    }),
  };
const fromConditional = (c: boolean): (() => void) =>
  c
    ? () => {
        f('a');
      }
    : () => {
        f('b');
      };
fromMatch('a')() as void; // OK
fromConditional(true)() as undefined; // ERROR
// Typing those values to look up the contextual return type does not leave
// them typed without their own hints.
type WithOptional = {p?: string, f: () => void};
const fromBranches = (c: boolean): WithOptional =>
  c ? {f: () => {}} : {f: () => {}}; // OK
const fromBadBranch = (c: boolean): WithOptional =>
  c ? {f: () => {}} : {p: 1, f: () => {}}; // ERROR
// Looking up the contextual return type of one argument leaves the hints of the
// other arguments intact.
declare const promise: {
  then<T>(onFulfilled: (value: string) => T, onRejected: () => T): T,
};
(() => promise)().then(
  id => id, // OK
  () => {},
);
// The contextual return type of a function that returns no value keeps the
// type parameters of a generic call: looking it up neither solves them from
// the other arguments nor types those arguments.
declare function gen(): Promise<?number>;
declare function genString(): Promise<string>;
declare function setN(v: ?number): void;
declare function pd<R>(
  p: Promise<R>,
  ok: (v: R) => unknown,
  err: (e: unknown) => unknown,
): void;
pd(gen(), v => setN(v), () => {}); // OK
pd(genString(), v => setN(v), () => {}); // ERROR
declare function subscribe<T>(
  fetch: () => Promise<T>,
  onNext: (d: T) => void,
  onDone: () => void,
): void;
subscribe(() => gen(), d => setN(d), () => {}); // OK
declare function onDone<R>(p: Promise<R>, cb: () => R): R;
declare const p: Promise<?number>;
onDone(p, () => {}) as ?number; // ERROR, `R` does not admit `undefined`
declare function both<A, R>(a: A, cb: () => R): R;
both<_, ?number>(1, () => {}); // OK, explicit type arguments are substituted
// A contextual type that is a type parameter gets what the return hint infers
// for it, also through the return hint of another generic call.
declare function memo<T>(f: T): T;
const memoized: () => ?number = memo(() => {}); // OK
declare function make<T>(f: () => T): T;
declare function each<T, U>(xs: Array<T>, f: (x: T) => U): Array<U>;
const handlers: Array<{onBack: () => ?boolean}> = make(() =>
  each([1], i => ({onBack: () => {}})),
); // OK
// Overloads are chosen by their numbers of type arguments and arguments.
declare function first<R>(cb: () => R): R;
declare function first(cb: () => ?number, n: number): void;
const firstResult: undefined = first(() => {}); // ERROR
// An async function's contextual return type is unwrapped, not instantiated.
declare function wrap<R>(cb: () => R): () => R;
const wrapped: () => Promise<?number> = wrap(async () => {}); // ERROR
