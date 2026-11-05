// `Readonly<{...A, key: K}>` is made in full: its fields are read-only, so `K` gets
// both keys as lower bounds rather than an exact `'actions'`.
import * as React from 'react';

type Key = 'id' | 'name' | 'actions';
type Item = {id: string};
type CommonColumn = Readonly<{header: string}>;
type ContentColumn<K, I> = Readonly<{
  ...CommonColumn,
  key: K,
  cell?: (Readonly<{item: I}>) => unknown,
}>;
type SpacerColumn<K> = Readonly<{...CommonColumn, key: K, spacer: true}>;
type Column<K, I> = ContentColumn<K, I> | SpacerColumn<K>;
type TableProps<I, K> = Readonly<{
  columns: ReadonlyArray<Column<K, I>>,
  items: ReadonlyArray<I>,
}>;

component Table<I, K>(...props: TableProps<I, K>) {
  return null;
}

const ID: Column<Key, Item> = {key: 'id', header: 'ID'};

function F(items: Array<Item>): React.Node {
  const columns = [ID, {key: 'actions' as const, header: 'Actions'}];
  return <Table columns={columns} items={items} />; // ok: K = Key
}
