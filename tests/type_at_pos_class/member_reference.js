// @flow

class A {
  p: number;
  m(): string { return ''; }
  get g(): boolean { return true; }
  set s(v: boolean) {}
  static sp: number;
}

declare var a: A;
declare var maybeA: ?A;

// A reference to a member is framed as the declaration it resolves to, which
// takes expanding the receiver's type: the property's own type looks the same
// whether it was declared as a field, a method, or an accessor.
a.p;
//^
a.m();
//^
a.g;
//^
a.s = true;
//^
A.sp;
//^
maybeA?.p;
//      ^

// An object literal has no name to qualify the property with.
const o = {x: 1, f() {}, h: () => 2};
o.x;
//^
o.f();
//^
o.h();
//^
