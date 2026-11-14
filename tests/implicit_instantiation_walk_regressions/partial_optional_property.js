// `Partial` of an object type meets what flows to it as the partial object, whose fields are
// optional, as in subtyping: an optional field of the argument meets an optional field of the
// parameter, and the type argument does not take its `void`.
declare class Obs<T> {}

type Options<T> = Partial<{default: T}>;
declare function mk<T>(options: ?Options<T>): Obs<T>;
function f<V>(options: Options<V>): Obs<V> {
  return mk(options);
}

type Options2<T> = Partial<{default: T, onChanged: (v: T) => unknown}>;
declare function mk2<T>(options: ?Options2<T>): Obs<T>;
function f2<V>(options: Options2<V>): Obs<V> {
  return mk2(options);
}

// `Partial` of an alias of an object type, of an alias of a union, and of a spread.
type Base<T> = {default: T, onChanged: (v: T) => unknown};
declare function mk3<T>(options: ?Partial<Base<T>>): Obs<T>;
function f3<V>(options: Partial<Base<V>>): Obs<V> {
  return mk3(options);
}

type U<T> = {kind: 'a', default: T} | {kind: 'b', other: T};
declare function mk4<T>(options: ?Partial<U<T>>): Obs<T>;
function f4<V>(options: Partial<U<V>>): Obs<V> {
  return mk4(options);
}

type A = {kind: string};
declare function mk5<T>(options: ?Partial<{...A, default: T}>): Obs<T>;
function f5<V>(options: Partial<{...A, default: V}>): Obs<V> {
  return mk5(options);
}
