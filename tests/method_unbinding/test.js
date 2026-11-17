class A {
    m() {}
    n = () => {}
}

class B extends A {
    x() {}
}

interface I {
    m() : void;
    n : () => void;
}

type J = interface {
    m() : void;
    n : () => void;
}

type O = {
    m() : void;
    n : () => void;
}

let a = new A();
let b = new B();
declare const i : I;
declare const j : J;
declare const o  : O;

/* tests */

a.m; // ok
a.n; // ok

b.m; // ok
b.n; // ok
b.x; // ok

i.m; // ok
i.n; // ok

j.m; // ok
j.n; // ok

o.m; // ok
o.n; // ok

let {m : mb} = b; // ok
let {x : xb} = b; // ok
let {n : nb} = b; // ok

let {m : ma} = a; // ok
let {n : na} = a; // ok

let {m : mi} = i; // ok
let {n : ni} = i; // ok

let {m : mj} = j; // ok
let {n : nj} = j; // ok

let {m : mo} = o; // ok
let {n : no} = o; // ok

a.m(); // ok
a.n(); // ok

b.m(); // ok
b.x(); // ok
b.n(); // ok

i.m(); // ok
i.n(); // ok

j.m(); // ok
j.n(); // ok

o.m(); // ok
o.n(); // ok

let x1 = {...a}; // ok (methods not own)
let x2 = {...b}; // ok (methods not own)
let x3 = {...i}; // cannot spread interface
let x4 = {...j}; // cannot spread interface
let x5 = {...o}; // ok

if (a.m) {} // ok
if (a.n) {} // ok

if (b.m) {} // ok
if (b.n) {} // ok
if (b.x) {} // ok

if (j.m) {} // ok
if (j.n) {} // ok

if (i.m) {} // ok
if (i.n) {} // ok


type T = {
  f: (x: string) => void,
};

import X from './lib.js';

const ob : T = {
    f: X.method,
};

const staticMethod = X.method;
staticMethod(); // ok

// Assignment to a method remains read-only.
{
  class A {
     m(): void {}
  }
  class B extends A {
  }
  declare const b: B;
  b.m = () => {}; // ERROR: cannot-write
}
