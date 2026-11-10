// A union that subtyping does not know to have null or void members, such as the type of a
// conditional expression, meets a maybe type member by member: its `null` gives `U` `empty`, as in
// the flow-based solve, and the solution `{...} | empty` keeps its literals.
declare function compactMap<T, U>(arr: ReadonlyArray<T>, fn: (t: T) => ?U): Array<U>;
declare function useMemo<T>(f: () => T, deps: ReadonlyArray<unknown>): T;
declare var edges: ReadonlyArray<{node: ?{id: string}}>;
const buckets = useMemo(
  () =>
    compactMap(edges, ({node}) =>
      node == null ? null : {type: 'bucket', id: node.id},
    ),
  [],
);
buckets as Array<{type: 'bucket', id: string}>; // ok
