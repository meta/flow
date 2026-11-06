// @flow

function outer<TOuter>(y: TOuter) {
//       ^
  function inner<T>(x: T, z: TOuter | T): T {
//         ^
    y;
//  ^
    inner(x, x);
//  ^
    return x;
//         ^
  }
  const x = inner(1, y);
//          ^

  function inner_<T>(x: T): T {
//         ^
    return x;
  }
}
