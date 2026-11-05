declare interface Thenable {
  then(resolve: () => unknown, reject?: () => unknown): unknown;
}

declare function resolve<T>(value: Promise<T> | T): Promise<T>;
declare const thenable: Thenable;

// A member that the argument does not fit even with the markers open gives no evidence, and the
// argument goes to the bare type parameter.
resolve(thenable) as Promise<Thenable>; // ok
