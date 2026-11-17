//@flow

var x = 42;

function f1<X>(a: X): X {
  x = a; // error, X ~> escape
  return a;
}

declare function ng(value: unknown): unknown;
class ObjectUtils {
  static stableCopy<T extends unknown>(value: T): T {
    return ng(value); // no escape errors
  }
}

function makeEqualFn<T>(customEqual?: (a: T, b: T) => boolean): (a: T, b: T) => boolean {
  return customEqual
    ? function (a, b) {
        return a === b || customEqual(a, b);
      }
    : (42 as any);
}

function o<X>(x: X) {
  // error, X ~> return
  return x;
}

var a = 42;

function f2<X>(b: boolean): (x: X) => X {
  if (b) {
    a = (x: X) => x;
  }
  if (typeof a === 'number') {
    return (x: X) => x;
  } else {
    return a;
  }
}

var xa = [];
function fa<T>(a: T, b: boolean): T {
  if (b) {
    xa[0] = a;
  }
  return xa[0];
}

function hh<X>({a}: {a: X}): X {
  return a;
}

function whhh<X>() {
  nonexistantGlobal = 42; // error
}
