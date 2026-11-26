//@flow

type P = {c?: () => void, b?: {c?: {d: number, ...}, ...}, ...};
declare const cc: P;
if (cc.b?.c) {
  var xxx: number = cc.b.c.d;
}

declare const a: ?{
  b?: {c?: {d: number, ...}, e: number, f: ?() => number, g: ?() => number, ...},
 ...};

if (a && a.b) {
  a.b as {...}; //ok
  a.b.c as ?{...}; //ok
  a.b.c?.d as ?number; //ok
  a.b.c.d; // bad
  a?.b.c?.d as ?number; // ok, one unneeded optional chain
  a?.b.e as number; // ok, one unneeded optional chain
  a.b?.e as number; // ok, one unneeded optional chain
}

if (a && a.b) {
  a.b?.f() as ?number; // unneeded chain and bad
}

if (a && a.b) {
  a.b.f?.() as ?number; // ok
}

if (a && a.b) {
  a.b?.f?.() as ?number; // ok, one unneeded optional chain
}

if (a && a.b && a.b.g) {
  a.b.g() as number; // ok
}

if (a && a.b && a.b.g) {
  a.b.g?.() as number; // ok, unneeded chain
}

if (a && a.b && a.b.g) {
  a.b?.g() as number; // ok, unneeded chain
}

if (a && a.b && a.b.g) {
  a.b?.g?.() as number; // ok, two unneeded chains
} else {
  a as {...}; // should fail, sanity check
  a as null | undefined; // should fail, sanity check
}

function f<T extends any>(x: ?T) {
  if (x?.a === null) { // constant-condition error
    return;
  }
  if (x) {
    x.a as empty; // ok
  }
  x.a as empty; // should fail
}

if (a?.b) {
  a.b as {...}; //ok
  a.b.c as ?{...}; //ok
  a.b.c?.d as ?number; //ok
  a.b.c.d; // bad
  a?.b.c?.d as ?number; // ok, one unneeded optional chain
  a?.b.e as number; // ok, one unneeded optional chain
  a.b?.e as number; // ok, one unneeded optional chain
}

if (a?.b) {
  a.b?.f() as ?number; // unneeded chain and bad
}

if (a?.b) {
  a.b.f?.() as ?number; // ok
}

if (a?.b) {
  a.b?.f?.() as ?number; // ok, one unneeded optional chain
}

if (a?.b?.g) {
  a.b.g() as number; // ok
}

if (a?.b?.g) {
  a.b.g?.() as number; // ok, unneeded chain
}

if (a?.b?.g) {
  a.b?.g() as number; // ok, unneeded chain
}

if (a?.b?.g) {
  a.b?.g?.() as number; // ok, two unneeded chains
} else {
  a as {...}; // should fail, sanity check
  a as null | undefined; // should fail, sanity check
}
type Z = {a: 'hello ', value: number} | {b: 'goodbye', value: string};
declare const b: ?{x: boolean, y?: boolean, z: Z, w?: {u: () => number, ...}, ...};

if (b?.x) {
  b.x as true; // ok
} else {
  b.x as false; // nope
}

if (b?.w.u) {
  // error here
  b as {...}; // ok
  b.w.u(); // ok, because of error in predicate
}

if (b && b.w.u) {
  // error consistent with above
  b.w.u(); // no error consistent with above
}

if (b?.z.a) {
  b.z.value as number; // yes
} else {
  b.z.value as string; // error
}

if (b?.y) {
  // sketchy null
  b.y as true; // ok
} else {
  b.y as false; // nope
}

declare const c: Array<?Array<{a: number, b: {c: {d: string, ...}, ...}, ...}>>;
if (c[0]?.[0].b.c) {
  c[0][0].a as number; //yes
  c[0][0].b as {...}; //yes
  c[1][0] as {...}; // no
  c[0][1].a as number; // yes
}

declare let d: ?{a?: () => {b?: {c: number, ...}, ...}, d: number, ...};
if (d?.a?.().b) {
  d.a().b.c as number; // nope, never was a refinement in the first place
}

declare const a11: ?({a: string, ...} | {...});
declare const b11: ?{...};
// No error on looking up a11?.a, but that's consistent with non-optional behavior
var x11: empty = a11?.a || b11?.a;

declare const e: ?string;
if (e?.length) {
}

function havoc_d() {
  d = null;
}
