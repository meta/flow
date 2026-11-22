//@flow strict

declare opaque type T: unknown;
declare const o: { p: T };

// guard
if (o.p) {
  o.p as T; // OK
}

// refine
if (o.p != null) {
  o.p as T; // OK
}


declare opaque type TT: unknown;

declare let content:
| void
| null
| boolean
| number
| string
| TT;

if (content != null) {
  content = content ? 'TRUE' : 'FALSE'; // ok
}

{
  declare opaque type S;
  declare const x: ?S;

  if (x == null) {
    x as null | void; // ok
  }
}

{
  declare opaque type V:
    | {readonly tag: 'a', readonly a: number}
    | {readonly tag: 'b', readonly b: string};
  declare const v: V;
  if (v.tag === 'a') {
    v.a as number; // ok
    v as V; // ok
  }
  if ('b' in v) {
    v.b as string; // ok
  }
  declare const u: V | string;
  if (typeof u === 'object') {
    u as V; // ok
    u as empty; // error
  }
}

{
  // An opaque type that is a member of its own upper bound.
  declare opaque type D<X>: D<X> | null;
  declare const d: D<string>;
  if (d != null) {
    d as D<string>; // ok
    d as empty; // error
  }
}
