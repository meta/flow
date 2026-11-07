// The type guard of a filter callback is inferred when an element can be `null` or `undefined`
// inside an optional or maybe type. The walk's bound checks optimize the union of the elements,
// which the refinement then filters whole instead of member by member.
declare const o: {a: string, b?: string};

[o.a, o.b].filter(v => v != null) as Array<string>;
[o.a, o.b].filter(v => v !== undefined) as Array<string>;
[o.a, o.b].filter(v => v === undefined) as Array<string>; // error

// An element whose type is a type variable resolved to another one.
component C(c: boolean, d: ?number) {
  [c ? null : d].filter(v => v != null) as Array<number>;
  return null;
}

// An element whose maybe type wraps another one.
type Dict<V> = {[string]: ?V, ...};
function f(ids: Array<string>, map?: Dict<?{id: string}>) {
  ids.map(id => map?.[id]).filter(t => t != null) as Array<{id: string}>;
}
