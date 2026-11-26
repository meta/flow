/**
 * @format
 * @flow
 */

(() => {
  return 42; // Error: We should point to 42.
}) as () => string;

const f = () => {
  return 42;
};
f as () => string; // Error: We should point to f.

declare function myFn1(): number;
declare function myFn2(x: number): void;

({
  fn1: myFn1, // Error: We should point to just the `myFn1` property.
}) as {
  fn1: () => undefined,
};

({
  fn2: myFn2, // Error: We should point to just the `myFn1` property.
}) as {
  fn2: (x: string) => void,
};

({
  fn2: myFn2, // Error: We should point to just the `myFn1` property.
}) as {
  fn2: () => void,
};
