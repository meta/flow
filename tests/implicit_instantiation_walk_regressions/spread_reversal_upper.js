type Item = {id: string, count: ?number};

declare const item: {id: ?string, ...Item};
declare const onClick: Item => void;
declare function f<T>(x: {id: ?string, ...T}, g: T => void): void;

f(item, onClick);

type Config<D> = Readonly<{render: D => string}>;

declare function table<T>(props: {
  columns: Array<Config<T>>,
  data: Array<{id: ?string, ...T}>,
  onRowClick: T => void,
}): void;
declare const config: Config<Item>;
declare const data: Array<{id: ?string, ...Item}>;

table({columns: [config, config], data, onRowClick: onClick});
