// `empty` gives evidence only to a bare marker, as subtyping finds `empty` a subtype of anything
// else at once: the omitted `initialItem` says nothing about `T` through `?T`.
type Selection<T> = Readonly<{selectedItem: ?T, ref: {current: ?T}}>;
declare function useSelection<T>(keyOf: (item: T) => string, initialItem?: ?T): Selection<T>;
type Item = {name: string};
declare function key(item: Item): string;
const s = useSelection(key);
s as Selection<Item>; // ok
