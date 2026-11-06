// @flow

function foo<T extends number | string>(x: T): number {
//       ^
  if (typeof x === 'number') {
    return x
  } else {
    return 1;
  }
}
