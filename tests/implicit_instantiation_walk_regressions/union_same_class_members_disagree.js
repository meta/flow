// An application meets the applications of the same class among the members of a union first, and
// when none of them agrees with the evidence of the arguments, the other members, as in the
// speculative match of the flow-based solve. The hint of the inner `new Map` below is the union of
// the `nodes` of both entries: the `Map` of the second entry itself, whose `K` is `"n1"`, which
// `"n2"` does not fit, and the `ReadonlyMap<string, Node>` of the first, which gives `K` = `string`.
type Node = Readonly<{label: string, ids: ReadonlyArray<string>}>;
type Trace = Readonly<{nodes: ReadonlyMap<string, Node>}>;
declare function ids<T>(xs: Array<T>): Array<T>;

const traces: ReadonlyMap<string, Trace> = new Map([
  ['t1', {nodes: new Map([['m1', {label: 'a', ids: ids(['x'])}]]) as ReadonlyMap<string, Node>}],
  [
    't2',
    {
      nodes: new Map([ // was an error
        ['n1', {label: 'b', ids: ids(['x'])}],
        ['n2', {label: 'c', ids: ids(['y'])}],
      ]),
    },
  ],
]);
