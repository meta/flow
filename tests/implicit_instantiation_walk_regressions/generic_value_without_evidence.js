// A generic function argument whose type parameters get no evidence stands for a fresh type
// variable, as in the flow-based solve: it does not fix `TItem` to their bound, and neither does a
// later generic argument instantiated with the solution so far. The return hint decides.
type Row<T> = {item: T, index: number};
type A = {type: 'a', id: string};
type B = {type: 'b', key: string};
type Item = A | B;
type Activation<T> = {transform: (Row<T>) => Row<T>};

declare function useActivation<TItem>(props: {
  getID: (Row<TItem>) => string,
  skip: (Row<TItem>) => boolean,
}): Activation<TItem>;

function getID<T extends Item>({item}: Row<T>): string {
  return item.type;
}

function skip<T extends Item>({item}: Row<T>): boolean {
  return item.type !== 'a';
}

function useMine<T extends Item>(): Activation<T> {
  return useActivation({getID, skip}); // ok: TItem = T
}
