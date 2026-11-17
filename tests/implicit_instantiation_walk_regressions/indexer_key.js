// A property that a type lacks meets its indexer only if the name of the property fits the key of
// the indexer, as in subtyping. Here the hint for the callback of `then` is the default of `R`,
// `{items: List<string>}`, and `List<U>`, the return type of `flatMap`, gets no evidence from
// `items` through its `number` indexer.
declare class Thenable<T> {
  then<R = T>(onfulfilled: (value: T) => R | Thenable<R>): Thenable<R>;
}
declare class List<T> {
  [index: number]: T;
  flatMap<U>(callback: (value: T) => ReadonlyArray<U> | U): List<U>;
}
declare const loaded: Thenable<{items: List<string>}>;
declare const c: boolean;

const names = loaded.then(data => data.items.flatMap(x => (c ? [{v: x}] : [])));
names as Thenable<List<{v: string}>>; // was an error
