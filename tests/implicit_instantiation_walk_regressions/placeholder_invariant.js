// A call that is not solved yet when the hint of another argument is computed holds a
// placeholder. A type parameter that only such a type meets in an invariant position is pinned to
// it, so the return hint does not decide it: `V` gives `box` no hint, and `T` is what
// `items.map(...)` returns, `string`.
declare class Box<T> {
  v: T;
}
declare function nonnull<T>(x: ?T): T;
declare function box<T>(values: ReadonlyArray<T>): Box<T>;
declare function first<V>(value: Box<V>): V | undefined;
declare const items: ReadonlyArray<{id: string}>;

nonnull(first(box(items.map(x => x.id)))) as string; // was an error
nonnull(first(box(items.map(x => x.id)))) as number; // error

// An argument that gives the type parameter a lower bound decides it, so `byString` gets the
// hint `Sorter<{id: string}>`.
type Sorter<T> = (a: T, b: T) => number;
declare function sortBy<T>(arr: ReadonlyArray<T>, sorters: ReadonlyArray<Sorter<T>>): Array<T>;
declare function byString<T>(fn: (v: T) => string): Sorter<T>;
function sorted() {
  let sortedItems = [...items];
  sortedItems = sortBy(sortedItems, [byString(item => item.id)]); // ok
  return sortedItems;
}

// So does another type met in an invariant position, here from `source`.
declare class Source<T> {
  item: T;
}
type Column<T> = {render: (item: T) => string};
declare function column<T>(): Column<T>;
declare function table<T>(columns: ReadonlyArray<Column<T>>, source: Source<T>): void;
declare const source: Source<{id: string}>;
table([column()], source); // ok
