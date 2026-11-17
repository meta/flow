type Item = {id: string, count: ?number};

declare const item: {id: ?string, ...Item};
declare const onClick: (item: Item) => void;
declare function f<T>(x: {id: ?string, ...T}, g: (y: T) => void): void;

f(item, onClick);

type Config<D> = Readonly<{render: (data: D) => string}>;

declare function table<T>(props: {
  columns: Array<Config<T>>,
  data: Array<{id: ?string, ...T}>,
  onRowClick: (row: T) => void,
}): void;
declare const config: Config<Item>;
declare const data: Array<{id: ?string, ...Item}>;

table({columns: [config, config], data, onRowClick: onClick});
