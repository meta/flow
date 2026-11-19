//@flow

declare class Map<K, V> {
  static <K, V>(_: void): Map<K, V>;
}
const x1: Map<unknown, unknown> = Map<unknown, unknown>();
const x2: Map<unknown, unknown> = Map();

declare class OneTarg<T> {
  static <K, V>(): OneTarg<K, V>; // error: too many type arguments
}

const y = OneTarg<string>(); // error, incorrect amount of args
const z1 = OneTarg<string, number>(); // no error, correct amount of args
