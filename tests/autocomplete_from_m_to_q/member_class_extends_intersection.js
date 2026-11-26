// @flow
declare class A {
  a: number;
}
declare const WithObject: Class<A> & {extra: number};
class FromObject extends WithObject {}

new FromObject().
//               ^
