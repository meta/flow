type Plugin<K> = {transform: (K) => K};
declare function table<K>(keys: ReadonlyArray<K>, plugin: Plugin<K>): void;
declare const keys: ReadonlyArray<string>;

// `K` is the key of an indexer that the call does not pass, so nothing needs `'name'` precise.
declare function useResize<K>(config: {
  widths?: {[key: K]: number},
  keys: ReadonlyArray<K>,
}): Plugin<K>;
const resize = useResize({keys: ['name']});
table(keys, resize); // was an error

// The keys of an object that meets the indexer are generalized too.
declare function useRows<K>(config: {render: () => {[key: K]: number}}): Plugin<K>;
declare const field: string;
const rows = useRows({render: () => ({content: 1, [`answer_${field}`]: 2})});
table(keys, rows); // was an error

// An index still keeps its literal precise.
declare function get<O extends {...}, K extends string>(o: O, k: K): O[K];
get({a: 1, b: 'b'}, 'a') as number;
get({a: 1, b: 'b'}, 'a') as string; // error
