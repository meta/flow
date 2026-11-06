// @flow

class A<T> {
  static m() {}
//       ^
}

class B<T> extends A<T> {
  static m(): Class<B<T>> {
//       ^
    // TODO the type 'Class<A>' is not parseable when 'A' is polymorphic
    this as Class<A<T>>;
//    ^
    this as Class<B<T>>;
    super.m();
//     ^
    return this;
  }
}

var bCtor = B.m();
//  ^
var b = new bCtor;
//  ^
