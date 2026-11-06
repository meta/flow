// @flow

declare let x: string;

class A {
  get x(): string {
//    ^
    return x;
  }
  set x(value: ?string) {
//    ^
    x = value || "default";
  }
}
