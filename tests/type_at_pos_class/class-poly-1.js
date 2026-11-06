// @flow

export class A<T> {
  x: T;
  m(): T { return this.x; }
}

declare const a: A<number>;
var x = a.m();
//  ^
//        ^
