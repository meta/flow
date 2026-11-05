declare const groups: Readonly<{[string]: ReadonlyArray<string>}>;
declare const first: ?string;
declare function entries<K, V>(o: Readonly<{[K]: V}>): Array<[K, V]>;

const keys = entries(groups).reduce(
  (acc, [key, values]) => {
    acc.push(key);
    return acc;
  },
  first == null ? [] : [first],
);
