class C<T> {
  x: T;
  m1(x: T) {}
  m2(x: T) {}
}

var c1 = new C<number | string>;
c1.m1(0);
c1.m2("");
c1.x as void; // error: number|string ~> void

var c2 = new C<string>;
c2.m1(0); // error: number ~> string
c2.m2("");
c2.x as void; // error: string ~> void

class NonPoly {}
new NonPoly<string>; // error: expected polymorphic type

new C<string, number>; // error: more than 1 type argument
