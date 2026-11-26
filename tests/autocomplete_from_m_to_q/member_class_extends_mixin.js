// @flow
declare class A {
  a: number;
}
interface M {
  m: boolean;
}
interface MC {
  new (...args: Array<any>): M;
}
declare const Mixed: MC & Class<A>;
class FromMixin extends Mixed {}

new FromMixin().
//              ^
