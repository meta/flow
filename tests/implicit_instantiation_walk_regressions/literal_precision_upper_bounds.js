// The flows to the upper bounds of a type parameter keep precise the literals that subtyping marks
// as needed precise: also those of a type met in an invariant position, with which the type
// parameter is unified, and also against an upper bound with a literal of a value.

// A type met in an invariant position, against an annotation of the result.
declare class IM<out K, out V> {
  key(): K;
  val(): V;
}
declare function single<K, V>(iter: Iterable<[K, V]>): IM<K, V>;
const b: IM<'a' | 'b', 'on' | 'off'> = single(new Map([['a', 'on']])); // ok

// A lower bound, against an upper bound with a literal of a value.
const C = 'CONSIDERATION';
declare function sel<T>(onChange: (v: T) => void, value: ?T): T;
declare const onChange: (v: 'POST' | typeof C) => void;
sel(onChange, C); // ok
sel(onChange, 'OTHER'); // error

// A generic call with a literal in the argument of an overloaded one.
declare class IMap<out K, out V> {
  static <K, V>(iter: ?Iterable<[K, V]>): IMap<K, V>;
  static <K extends string, V>(object: ?{readonly [k: K]: V, ...}): IMap<K, V>;
}
declare const id: string;
declare function Rec<TValue>(spec: TValue): void;
Rec(IMap({[id]: IMap({x: true})})); // ok
