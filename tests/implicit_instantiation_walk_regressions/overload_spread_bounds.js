export type Range<out K> =
  | Readonly<{gte?: K, lt: K}>
  | Readonly<{gt: K, lt?: K}>
  | Readonly<{gte: K, lte?: K}>
  | Readonly<{gt?: K, lte: K}>;
export interface Cursor<out Yield, out Return, in Next> {
  next(value?: Next): Yield | Return;
}
type Source<K, V> = {
  direction: 'asc' | 'desc',
  iterator: (context: WeakMap<{...}, unknown>, range?: Range<K>) => Cursor<[K, V], void, K>,
  keyLength: number,
  ...
};
type SubscribableSource<K, V> = {
  ...Source<K, V>,
  subscribe: (handler: (K, V) => void, range?: Range<K>) => () => void,
  ...
};
type QuerySource<out K, out V> = {
  ...Source<K, V>,
  filter: ((predicate: typeof Boolean) => QuerySource<K, NonNullable<V>>) &
    ((predicate: (value: V) => boolean) => QuerySource<K, V>),
  map<V2>(projection: (V) => V2): QuerySource<K, V2>,
  ...
};

declare function first<K, V>(source: SubscribableSource<K, V>): Promise<V | void>;
declare function first<K, V>(source: Source<K, V>): Promise<V | void>;
declare var src: QuerySource<[number, string], {a: number}>;

const r = first(src);
r as Promise<{a: number} | void>;
