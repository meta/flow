// A type argument solved from an invariant position is the type argument met there, not the type
// it resolves to: `TItem` of `select` is the element type that `dataSource` solved for the hook
// result `rows`.
type Item = {id: string, n: ?number};

declare opaque type DataSource<TItem>;
declare function dataSource<TItem>(
  items: ReadonlyArray<TItem>,
  options: Readonly<{getID: (v: TItem) => ?string}>,
): DataSource<TItem>;
declare function select<TItem, TItemKey>(
  x: Readonly<{
    dataSource: DataSource<TItem>,
    getIDForItem: (item: TItem) => TItemKey,
  }>,
): void;
declare hook useMemo<T>(create: () => T): T;

hook useTable(items: ReadonlyArray<Item>): void {
  const rows = useMemo(() =>
    items.map(item => (item.n != null ? item : {...item, n: 1})),
  );
  const ds = useMemo(() => dataSource([...rows], {getID: item => item.id}));
  select({dataSource: ds, getIDForItem: (item: Item) => item.id}); // okay
}
