// `empty` gives evidence only to a bare marker, as subtyping finds `empty` a subtype of anything
// else at once: the omitted `initialItem` says nothing about `T` through `?T`.
type Selection<T> = Readonly<{selectedItem: ?T, ref: {current: ?T}}>;
declare function useSelection<T>(keyOf: (item: T) => string, initialItem?: ?T): Selection<T>;
type Item = {name: string};
declare function key(item: Item): string;
const s = useSelection(key);
s as Selection<Item>; // ok
// A `null` argument for `T | null | undefined` gives `T` only `empty`, as for `?T`.
declare function boxOf<T>(x: T | null | undefined): {v: T};
boxOf(null).v as string; // ok
// The `null` and `undefined` of a union argument are absorbed instead, and give `T` nothing.
declare function nullthrows<T>(x: T | null | undefined): T;
declare function getMaybe<P extends {...}>(): {p: P} | null | undefined;
function fromMaybe<P extends {...}>(): {p: P} {
  return nullthrows(getMaybe()); // ok
}
