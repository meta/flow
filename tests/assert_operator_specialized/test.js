declare const arr: Array<number>;
arr[0]!; // ok
declare const roarr: ReadonlyArray<number>;
roarr[0]!; // ok
declare const tuple: [?number];
tuple[0]!; // error

declare const exact_obj: { a?: number }
exact_obj.a!; // error
exact_obj['a']!; // error

declare const indexer: {[key: string]: number, foo: string};
indexer.a!; // ok
indexer.foo!; // error
indexer['a']; // ok
indexer['foo']!; // error

// Specialized assertions on named chain properties report the exact assertion span.
declare const nullableObject: ?{child: ?{prop: number}};
nullableObject?.child!.prop; // ERROR
