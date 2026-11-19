// A spread of a union of maybe types makes a union of unions of objects, which the elements of
// `items` are walked against, so `T` gets their `id`s as well as `selected`.
type Item<T> = {...(?{title: string} | ?{node: number}), id: T};
declare function select<T>(items: ReadonlyArray<Item<T>>, selected: T): T;

select([{id: '1', title: 'a'}, {id: '2', title: 'b'}], '1') as '1' | '2'; // was an error
select([{id: '1', title: 'a'}, {id: '2', title: 'b'}], '1') as '1'; // error

// So does a spread of a maybe union type.
type Other<T> = {...?({title: string} | {node: number}), id: T};
declare function other<T>(items: ReadonlyArray<Other<T>>, selected: T): T;

other([{id: '1', title: 'a'}, {id: '2', node: 1}], '1') as '1' | '2'; // was an error

// An element that has the properties of only one of the objects but does not fit it fails, as the
// flow-based solve tries that object speculatively with the others: it gives `T` no evidence, and
// the error is on `items`, not on `onChange`.
type WithIcon<T> = {...(?{title: string} | ?{node: number}), id: T, icon?: {uri: string}};
declare function pick<T>(
  items: ReadonlyArray<WithIcon<T>>,
  selected: T,
  onChange: (id: T) => void,
): void;
declare const withStringIcons: ReadonlyArray<{id: string, title: string, icon: string}>;
declare function onPick(id: '1' | '2'): void;
declare const picked: '1' | '2';

pick(withStringIcons, picked, onPick); // error

// With a bare marker among the members, such an element goes on to the marker: `S` gets the
// members of the hint that `T` does not have.
type Combined =
  | Readonly<{key: string, value: number}>
  | Readonly<{key: string, value?: null}>
  | Readonly<{key?: null, value: number}>;
declare const toUpdate: ReadonlyArray<Readonly<{key: string, value: number}>>;
declare const toDelete: ReadonlyArray<string>;
declare const toCreate: ReadonlyArray<number>;

const combined: Array<Combined> = toUpdate
  .concat(toDelete.map(key => ({key})))
  .concat(toCreate.map(value => ({value}))); // ok
