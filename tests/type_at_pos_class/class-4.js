// @flow

declare class Y {
  a: string;
}

class X extends Y {
  foo() {
    super.a as string;
//   ^
//        ^
  }
}
