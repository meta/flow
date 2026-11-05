declare opaque type Box<T>;
type Loose = any;

declare function emptyBox(n: number): Box<Loose>;
declare function makeBox<T>(
  items: ReadonlyArray<T>,
  options: Readonly<{getKey: T => ?string}>,
): Box<T>;

type Row = {name: string, note: ?string, ...};

const items = [{name: 'a'}, {name: 'b', note: null}];
const getKey = (item: Row) => item.name;

declare function memo<T>(f: () => T, deps: ReadonlyArray<unknown>): T;
declare const empty: boolean;

const x = memo(() => (empty ? emptyBox(7) : makeBox(items, {getKey})), [empty]);
