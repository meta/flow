// @flow

class Foo {
  bar: Bar
}

class Bar {
  baz: Baz
}

class Baz {
  qux: number
}

declare const foo: ?Foo;
const n = foo?.bar.baz.qux;
//    ^
//        ^
//             ^
//                 ^
//                     ^
