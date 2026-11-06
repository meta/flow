// @flow

class A {
//    ^
  m() { this; }
//^
//       ^
}

// class_t (t=Ty.Any)
var Any: any;


// class_t (t=Ty.Top)
var b: Class<unknown>;
//  ^




class C1 {}
class C2 {}

// class_t (t=Ty.Inter)
var i: Class<C1 & C2>;
//  ^

// class_t (t=Ty.Union)
var u: Class<C1 | C2>;
//  ^
