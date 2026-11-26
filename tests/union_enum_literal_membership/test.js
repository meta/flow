// @flow

type Nums = 1 | 2 | 3 | 4.5 | -1;
type Bools = true | false;
type BigInts = 1n | 2n;
type Mixed = 1 | 'a' | true | 1n | null | undefined;

1 as Nums; // ok
4.5 as Nums; // ok
-1 as Nums; // ok
5 as Nums; // error

true as Bools; // ok

2n as BigInts; // ok
3n as BigInts; // error

1 as Mixed; // ok
true as Mixed; // ok
1n as Mixed; // ok
2 as Mixed; // error
false as Mixed; // error

const values = {A: 1, B: 2, C: 3} as const;
declare function keyOf<K, V>(value: V, values: Readonly<{[key: K]: V, ...}>): K;
keyOf(values.A, values) as 'A' | 'B' | 'C'; // ok
keyOf(values.A, values) as 'A' | 'B'; // error
