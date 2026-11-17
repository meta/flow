type Tuple = [number, number];
type MappedTypeOnTuple = {[K in keyof Tuple]: Tuple[K] extends number ? string : empty};

// MappedTypeOnTuple should still work as a lower bound
function foo1(arr: MappedTypeOnTuple): [1, 2] {
  return arr; // error
}
// MappedTypeOnTuple should still work as a upper bound
function foo2(arr: [number, number]): MappedTypeOnTuple {
  return arr; // error
}

type F<T> = Readonly<{
  log: (f: () => T) => void,
  ...
}>;

declare function map<A extends ReadonlyArray<unknown>>(o2: Promise<A>): {[K in keyof A]: A[K] extends F<infer V> ? V : empty};
const boo = Promise.all([{log: (f: () => string) => {}}]);
map(boo) as [readonly v: number]; // error

declare const issue2674: {[K in keyof [unknown, unknown]]: 'FOO'};

issue2674[0] as 'FOO'; // ok
issue2674[0] as 'BAR'; // error
issue2674[0] = 'BAR'; // error
