// A value flows into a refined opaque type through its lower bound only if it is in the refined
// upper bound
declare opaque type N super string | null extends ?string;
declare const n: N;
if (n != null) {
  n === ''; // ok
  let m = n;
  m = 'x'; // ok
  m = null; // error
}

declare opaque type V super 'a' | 1 extends string | number;
declare const v: V;
if (typeof v === 'string') {
  let w = v;
  w as V; // ok
  w = 'a'; // ok
  w = 1; // error
}

declare class Base {}
declare class A extends Base {}
declare class C extends Base {}
declare opaque type T super C extends Base;
declare const t: T;
if (t instanceof A) {
  let u = t;
  u = new A(); // error
}

declare opaque type R<X> super R<X> | null extends ?string;
declare const r: R<number>;
if (r != null) {
  let p = r;
  p = null; // error
}

declare opaque type AnyLower super any | null extends ?string;
declare const al: AnyLower;
if (al != null) {
  let p = al;
  p = null; // error
}

declare opaque type C0 super 'x' | null extends ?string;
declare opaque type C1 super C0 | null extends C0 | ?string;
declare opaque type C2 super C1 | null extends C1 | ?string;
declare const c: C2;
if (c != null) {
  let p = c;
  p = 'x'; // ok
  p = 'y'; // error
}
