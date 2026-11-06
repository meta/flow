// @flow

function foo<T>(x: T): {f: T} {
//        ^
  return { f: x };
}

const bar = <T>(x: T): {f: T} => ({ f: x });
//    ^

const obj = {
//    ^
  m<T>(x: T): {f: T} {
    return { f: x };
  }
}
