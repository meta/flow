//@flow

declare function takes_num_arr(n: number, ...x: Array<number>): void;

function f<X extends Array<number>, R>(fn: (...args: X) => R, x: X, arr: Array<number>) {
  fn(...x) as R;
  fn(...arr) as R; // nope

  takes_num_arr(...x);
}

function g<X extends Array<number>>(A: X): X {
  var B = [...A];
  if (A as any) {
    return B;
  } else {
    return [...B, 42]; // nope
  }
}

function h<X extends Array<string>>(x: X) {
  var y: Array<string> = ['3', ...x];
  var z: X = ['3', ...x]; // nope
}

function w<A extends [number, number]>(x: A, f: (n: number, ...rest: A) => void) {
  f(...x); // nope
  f(42, 42, 42); // nope
  f(42, ...x);
  f(42, ...x, ...x); // nope
}
function w2<A extends Array<number>>(x: A, f: (n: number, ...rest: A) => void) {
  f(...x);
  f(42, 42, 42); // nope
  f(42, 42, 42, ...x); // should be nope
  f(42, ...x);
  f(42, ...x, ...x);
}
function w3<A extends Array<number>>(x: A, f: (a: A) => void) {
  f([42, 42, 42]); // nope
  f([42, ...x]); // should be nope
  f([...x]);
  f([...x, ...x]);
}

function l<X extends ReadonlyArray<number>, Y extends X>(x: X, y: Y) {
  [...x, ...y] as Y; // should be nope
  [...x, ...y] as X; // should be nope
}

function l2<X extends Array<number>, Y extends X>(x: X, y: Y) {
  [...x, ...y] as Y; // should be nope
  [...x, ...y] as X; // ok
}

function a<X extends ReadonlyArray<number>>(x: X): X {
  return [...x];
}

function b<X extends ReadonlyArray<number>>(x: X): X {
  return [...x, ...x]; // nope, because X could be a tuple
}

function c<X extends ReadonlyArray<number>>(x: X): X {
  return [42, ...x]; // error
}

function d<X extends [number, number]>(x: X): X {
  return [...x, ...x]; // error
}
