//@flow

var e = new Array(10);

function f1<X>(x: X, escape: boolean, n: number): X | number {
  if (escape) {
    e[n] = x;
  }
  return e[n];
}

var c = class e {
  f<X>(x: X, escape: boolean): X | Class<this> {
    if (escape) {
      e = x;
    }
    return e;
  }
};

var a = Object.create(null, {x: {value: 42}});

function f2<X>(x: X, escape: boolean): X | number {
  if (escape) {
    a.x = x;
  }
  return a.x;
}

var a2 = Object.keys({x: 42});

function f3<X>(x: X, escape: boolean, n: number): X | string {
  if (escape) {
    a2[n] = x;
  }
  return a2[n];
}

function f4(escape: boolean, n: number): void {
  function g<X>(x: X): X | ((a: boolean, b: number) => void) {
    if (escape) {
      g = x;
    }
    return g;
  }
}

class C {}

function f5<X>(x: X, escape: boolean): X | Class<C> {
  if (escape) {
    C = x;
  }
  return C;
}

class D {
  f<X>(x: X, escape: boolean): X {
    if (escape) {
      D = x;
    }
    return D;
  }
}

var u = (<X>(x: X, escape: boolean): X => {
  if (escape) {
    u = x;
  }
  return u;
})(42, true);

for (var x0 of []) {
  function f<X>(y: X) {
    x0 = y;
  }
}

for (var [x1] of []) {
  function f<X>(y: X) {
    x1 = y;
  }
}

for (var [...x2] of []) {
  function f<X>(y: X) {
    x2 = y;
  }
}

for (var {...x3} of []) {
  function f<X>(y: X) {
    x3 = y;
  }
}

try {
} catch (e) {
  function f<X>(y: X) {
    e = y;
  }
}

for (const xe of [42 as any]) {
  let {...x} = xe;
  function f<X>(y: X) {
    x = y;
  }
}

for (const xe of [42 as any]) {
  let {x: y} = xe;
  function f<X>(z: X) {
    y = z;
  }
}

var aaa = 1;
function foo<T extends {}>(x: Omit<T, empty>) {
  aaa = x;
}

var x4 = [];
function f<T>(t: T) {
  x4[0] = t;
}
