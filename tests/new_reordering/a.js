//@flow

class TimeOfDay {
  a(other: TimeOfDay): boolean {
    return true;
  }
}

function f(): number {
  return f();
}

function f_na() {
  if (true) {
    return 42;
  }
  return f_na();
}

function havoc_x() {
  x = null;
}

var x;
havoc_x();
x = x;

function odd(x: number) {
  if (x === 0) {
    return false;
  }
  return even(x - 1);
}

function even(x: number) {
  if (x === 0) {
    return true;
  }
  return odd(x - 1);
}

function is_zero(x: number): boolean {
  return x === 0 || is_zero(x);
}

function missing(x): number {
  return missing(x);
}

const math1 = {
  foo: 'Hello',
  subobj: {
    foo: is_zero,
    meth(x: number): boolean {
      return true;
    },
  },
  odd(x: number): boolean {
    if (x === 0) {
      return false;
    }
    return math1.even(x - 1);
  },
  even: (x: number): boolean => {
    if (x === 0) {
      return true;
    }
    return math1.odd(x - 1);
  },
};

const math2 = {
  foo: 'Hello',
  subobj: {
    foo: math1,
    meth(x: number) {
      return true;
    },
  },
  odd(x: number) {
    if (x === 0) {
      return false;
    }
    return math2.even(x - 1);
  },
  even: (x: number) => {
    if (x === 0) {
      return true;
    }
    return math2.odd(x - 1);
  },
};

const odd_obj = {
  unrelated() {
    return 42;
  },
  odd(x: number) {
    if (x === 0) {
      return false;
    }
    return even_obj.even(x - 1);
  },
};

const even_obj = {
  even: (x: number) => {
    if (x === 0) {
      return true;
    }
    return odd_obj.odd(x - 1);
  },
};

const recursiveArrow = () => (true ? 1 : recursiveArrow());

const moremath1 = {
  ...math1,
  sqrt(x: number): number {
    return moremath1.odd(x) ? 1 : 0;
  },
};

const moremath2 = {
  ...math2,
  sqrt(x: number): number {
    return moremath2.odd(x) ? 1 : 0;
  },
};

export const foo = (): number => foo();

{
  declare function g(x: any): any;
  declare function f(): any;
  g(function () {
    return f();
  });
}

import * as React from 'react';
{
  declare function F(x: any): any;
  declare function f(): any;
  <F
    attr={function () {
      return f();
    }}
  />;
}

{
  const Utils = {
    foo: [
      {
        x: function (): number {
          return 42;
        },
      },
      42,
    ],
    f(): Array<number | {x: () => number}> {
      return Utils.foo;
    }, //ok
    g(): [{x: () => number}, number] {
      return Utils.foo;
    }, //ok
    h(): empty {
      return Utils.foo;
    }, //err
  };

  Utils.foo as Array<number | {x: () => number}>;
}

{
  const Utils = {
    foo: [
      {
        x: function () {
          return 42;
        },
      },
      42,
    ],
    f() {
      return Utils.foo;
    },
  };
}

{
  const m = {
    f(x: H) {
      return x(x => 42);
    },
  };

  type S = H; // not included in error message

  type H = {(f: (x: number) => number): void, m: typeof m, s: S};
}

{
  function F(): number {
    return F.B();
  }
  F.A = (): number => 42;
  F.B = (): number => F.A();
}

{
  let x = function () {
    return x;
  };
}
