function case1() {
  declare class IMap<K, V> {
    static <K, V>(iter: ?Iterable<[K, V]>): IMap<K, V>;
    static <K extends string, V>(object: ?{readonly [k: K]: V, ...}): IMap<K, V>;
  }

  type Style = {color: string};
  type Params = {
    title?: string,
    style?: string | Style,
    ...
  };

  declare const params: Params;

  const m = IMap(params);
}

function case2() {
  type PlainObjInput<K, V> = Readonly<{[key: K]: V, __proto__: null}>;

  declare class IMap<K, V> {
    static <K, V>(values?: Iterable<[K, V]> | PlainObjInput<K, V>): IMap<K, V>;
  }

  const m = IMap([
    ['1', {id: '1'}],
    ['2', {id: '2'}],
  ]);
}

function case3() {
  interface I { readonly get: () => I }

  class Base { readonly get: () => this = () => this; }

  declare class Path<out S> {
    constructor(s: interface {readonly get: () => S, m(x: S): void}): void;
  }

  class C extends Base implements I {
    m(x: this): void {} // error: `this` in an input position
    path(): Path<this> { return new Path(this); }
  }
}

function case4() {
  type WithoutKey<T> = Pick<T, Exclude<keyof T, 'key'>>;

  declare function list<T extends {...}>(
    items: ReadonlyArray<T | {key: string, ...}>,
    f: (item: WithoutKey<T>) => void,
  ): void;

  list([{label: 'Yes', value: true}], item => {
    item.value;
  });
}

function case5() {
  declare function useState<S>(initialState: (() => S) | S): [S, (S) => void];

  const [now] = useState(Date.now);

  type User = Readonly<{p: ?(string | number)}>;

  declare var rows: Array<{p: ?string}>;

  const a = rows.map(r => ({p: r.p}));
  useState<Array<User>>(a.concat(a));
}
