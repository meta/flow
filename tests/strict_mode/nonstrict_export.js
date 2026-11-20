// @flow

export type S = number;

function g(o: {a: number}) {
  o?.a; // No error: not a strict file
}
