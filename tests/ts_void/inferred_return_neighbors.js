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
