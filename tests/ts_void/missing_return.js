declare const b: boolean;

// As in TypeScript, a function with a return annotation and no `return`
// statement must return a type that includes `void`, or is `any` or
// `undefined`.
function v(): void {} // OK
function nv(): number | void {} // OK
function a(): any {} // OK
function u(): undefined {} // OK
type V = void;
function alias(): V {} // OK
const arrow = (): undefined => {}; // OK

function n(): number {} // ERROR
function unk(): unknown {} // ERROR
function nu(): number | undefined {} // ERROR
function m(): ?string {} // ERROR
function gen<T>(): T {} // ERROR
const arrowNu = (): number | undefined => {}; // ERROR
class C {
  m(): unknown {} // ERROR
}

// With a `return` statement, its end returns `undefined`.
function someReturns(): number | undefined {
  if (b) return 1;
} // OK
function bareReturn(): undefined {
  if (b) return;
} // OK
function someReturnsNumber(): number {
  if (b) return 1;
} // ERROR

// Only when the end is reachable.
function throws(): number {
  throw 'never';
} // OK
function exhaustive(x: 'a' | 'b'): number {
  switch (x) {
    case 'a':
      return 1;
    case 'b':
      return 2;
  }
} // OK

// What an async function's `Promise` resolves to, or a generator returns.
async function av(): Promise<void> {} // OK
async function au(): Promise<undefined> {} // OK
async function anu(): Promise<number | undefined> {} // ERROR
function* gv(): Generator<number, void, void> {
  yield 1;
} // OK
function* gu(): Generator<number, undefined, void> {
  yield 1;
} // OK
function* gnu(): Generator<number, number | undefined, void> {
  yield 1;
} // ERROR

// A constructor returns its instance.
class D {
  constructor(): undefined {} // OK
}

// Without a return annotation, its inferred return type is `void`, unless its
// contextual return type admits `undefined`.
const f: () => number | undefined = () => {}; // OK
