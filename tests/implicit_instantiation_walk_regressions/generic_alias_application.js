// A type parameter of the enclosing function meets an application of an alias that is a union with
// a bare type parameter, as `ReSome<T> = T | {...}`: the application is expanded first, so that the
// parameter matches the bare type parameter rather than its bound.
type ReSome<T> = T | {nested: number};
declare function valFromOption<T>(v: ReSome<T>): T;
declare function apply<V>(f: (v: V) => unknown, v: V): void;

function f<A>(x: A, g: (v: A) => void) {
  apply(g, valFromOption(x));
  valFromOption(x) as A;
  const v = valFromOption(x);
  apply(g, v);
  valFromOption(x) as empty; // error
}

// A bound that applies the same alias is compared with it argument by argument.
type Tree<T> = T | null | {[string]: Tree<T>, ...};
declare function parse<T extends string | number, O extends Tree<T>>(q: string): {[string]: O};
function parseWith<T extends string | number, O extends Tree<T>>(q: string): {[string]: O} {
  return parse(q);
}

declare function leafOf<T extends string | number, O extends Tree<T>>(o: O): T;

// The same application is reached through another type parameter's bound.
function throughParameter<T extends string | number, B extends Tree<T>, O extends B>(o: O) {
  leafOf(o) as T;
  leafOf(o) as empty; // ERROR: T
}

// An alias of an application still compares its arguments, not the whole rigid parameter.
type TreeAlias<T> = Tree<T>;
function throughAlias<T extends string | number, O extends TreeAlias<T>>(o: O) {
  leafOf(o) as T;
  leafOf(o) as empty; // ERROR: T
}

// Unrelated bounds still preserve the rigid parameter when the application expands.
type Box<X> = {value: X};
declare class BoxClass<X> { value: X }
function unrelatedAlias<X, A extends Box<X>>(x: A) {
  valFromOption(x) as A;
}
function unrelatedClass<X, A extends BoxClass<X>>(x: A) {
  valFromOption(x) as A;
}
function unboundedChain<B, A extends B>(x: A) {
  valFromOption(x) as A;
}
function unionBound<A extends string | number>(x: A) {
  valFromOption(x) as A;
}
function unrelatedRecursiveAlias<X, A extends Tree<X>>(x: A) {
  valFromOption(x) as A;
}
