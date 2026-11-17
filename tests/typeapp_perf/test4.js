/* This test exercises the abstract_targ logic used in the typeapp expansion
 * cache to detect loops involving implicitly instantiated type arguments */

type A<out T> = {
  m<U>(f: (x: T) => U): A<U>,
};

type B<out T> = {
  m<U>(f: (x: T) => U): B<U>,
}

declare const a: A<number>;
var b: B<number> = a;
