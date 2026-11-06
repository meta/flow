// @flow

class A {
  static m() {}
//       ^
}

class B extends A {
  static m(): Class<this> {
//       ^
    this as Class<A>;
//    ^
    this as Class<B>;
    super.m();
//     ^
    return this;
  }
}

var bCtor = B.m();
//  ^
var b = new bCtor;
//  ^

class C {
  // NOTE here Flow infers 'this', even though this is a static member
  static readonly y: this;
//                ^
}
