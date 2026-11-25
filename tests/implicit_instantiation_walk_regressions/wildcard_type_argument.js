// An explicit `_` type argument behaves like the type it is inferred to. A plugin whose
// `TColumnKey` is `any` makes the table accept plugins that disagree on `TColumnKey`, whether
// the row header's `TColumnKey` is written or inferred.
type CellRender<TItem, TColumnKey> = (
  props: Readonly<{item: TItem, columnKey: TColumnKey}>,
) => unknown;
type Plugin<TItem, TColumnKey> = Readonly<{
  transformBodyCell?: (
    v: CellRender<TItem, TColumnKey>,
  ) => CellRender<TItem, TColumnKey>,
}>;
type Plugins<TItem, TColumnKey> = Readonly<{
  rowHeader?: Plugin<TItem, TColumnKey>,
  sortable?: Plugin<TItem, TColumnKey>,
  emptyState?: Plugin<TItem, TColumnKey>,
}>;

declare hook useRowHeader<TItem, TColumnKey>(
  x: Readonly<{key: TColumnKey}>,
): Plugin<TItem, TColumnKey>;
declare hook useSortable<TItem, TColumnKey>(): Plugin<TItem, TColumnKey>;
declare hook useEmptyState<TItem = any, TColumnKey = any>(): Plugin<
  TItem,
  TColumnKey,
>;
declare function table<TColumnKey, TItem>(
  plugins: Plugins<TItem, TColumnKey>,
): void;

type Item = {id: string};
type Col = 'a' | 'b';

hook useInferred(): void {
  const rowHeader = useRowHeader<Item, _>({key: 'name'});
  const sortable = useSortable<Item, Col>();
  const emptyState = useEmptyState();
  table({rowHeader, sortable, emptyState}); // okay
}

hook useWritten(): void {
  const rowHeader = useRowHeader<Item, string>({key: 'name'});
  const sortable = useSortable<Item, Col>();
  const emptyState = useEmptyState();
  table({rowHeader, sortable, emptyState}); // okay
}
