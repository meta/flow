// @flow

class Base { }

class A<X extends Base> {
//    ^
//      ^
  x: X;
  m() {}
}

class B<X extends Base> extends A<X> {
//                              ^
  constructor() {
    super(); // TODO
//    ^
  }

  m() {
    this.x;
//    ^
//       ^
    super.m();
//    ^
  }
}
