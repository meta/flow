declare function flatten<T>(nested: Iterable<Iterable<T>>): Array<T>;
declare const rows: ReadonlyArray<ReadonlyArray<number>>;

// A type argument of an enclosing type compared structurally (`Iterable<T>` in
// `Iterable<Iterable<T>>`) is not a recursive expansion of it: the nesting limit lets it through.
const flat = flatten(rows); // ok
flat as Array<number>; // ok
flat as Array<string>; // error
