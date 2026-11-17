// `Readonly` of a class instance is an inline interface with the read-only props of the
// instance, which what flows to it meets structurally, as in the flow-based solve: `S` is `string`
// from the `contents` of the read-only `ValueLoadable` that the callback returns.
class BaseLoadable<T> {
  map<S>(_map: (v: T) => Promise<S> | Loadable<S> | S): Loadable<S> {
    throw new Error();
  }
}
class ValueLoadable<T> extends BaseLoadable<T> {
  contents: T;
}
class ErrorLoadable<T> extends BaseLoadable<T> {
  contents: unknown;
}
type Loadable<out T> = Readonly<ValueLoadable<T>> | Readonly<ErrorLoadable<T>>;
declare function loadableWithValue<T>(value: T): Readonly<ValueLoadable<T>>;

const loadable = loadableWithValue('VALUE').map(value => loadableWithValue(value)); // was an error
loadable as Loadable<string>;
loadable as Loadable<number>; // error

// Two inline interfaces, such as `Readonly` of interfaces, meet structurally: neither is the same
// class as the other.
type Result<T> = [?T, ?string];
type Info<V, S> = Readonly<interface {serialize: (value?: ?V) => Result<S>}>;
declare function use<V, S>(info: Info<V, S>): S;
declare const info: Readonly<interface {serialize: (value?: ?string) => Result<number>}>;

use(info) as number; // ok
