// @flow strict

// this file should not time out

declare function useState<S>(initialState: S): [S, (value: S) => void];

const [array, update] = useState<Array<number>>([]);

const add1 = (x: number) => update([...array, x]);
const add2 = (x: number) => update([...array, x]);
update([...array]);
update([...array]);
