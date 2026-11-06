type X = renders null;
//   ^
type Y = renders (null | number);
//   ^
component Foo() { return null }
component Bar() { return null }

type Z = renders Foo;
//    ^
//   ^
type U = renders (Foo | Bar);
//   ^
type V = renders Foo | Bar;
//   ^

type MaybeRenders = renders? Foo;
//   ^

type StarRenders = renders* Foo;
//   ^

component Poly<T>() { return null }
type PolyElement = renders Poly<number>;
//   ^

type PolyElementNoTargs = renders Poly;
//   ^
type PolyElementNoTargs2 = renders Poly;
//                                 ^

component Poly2<T>() { return null }

type UnionPoly = renders (Poly | Poly2);
//   ^
