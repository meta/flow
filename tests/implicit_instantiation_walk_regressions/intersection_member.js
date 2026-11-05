declare opaque type Tag;
declare opaque type ATag: Tag;
declare opaque type BTag: Tag;
declare opaque type Query<TTag, out TData, TVariables extends {...}>;
type AKey = {readonly $value?: {a: number}, readonly $tags: ATag, ...};
type BKey = {readonly $value?: {b: number}, readonly $tags: BTag, ...};

declare const query: Query<BTag, {b: number}, {}>;
declare function usePage<
  TTag extends Tag,
  TVariables extends {...},
  TData,
  TKey extends ?{readonly $tags: TTag, ...},
>(
  query: Query<TTag, TData, TVariables>,
  key: TKey,
): TData;
declare const key: AKey & BKey;

usePage(query, key) as {b: number};
