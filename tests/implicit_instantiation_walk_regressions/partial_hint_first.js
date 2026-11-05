type A = {type: 'A', id: string, a: {name: string}};
type B = {type: 'B', id: string, b: {count: number}};

declare function item<T extends A | B>(data: Partial<T>): T;

// What flows to `Partial<T>` bounds `T` from above after the return hint.
function itemB(data: Partial<B>): B {
  return item({type: 'B', ...data}); // ok
}
