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
