declare function t<T>(x: [T], y: T): T;
declare const tup: [number];

// The read-write elements of a tuple are invariant.
t(tup, 'x'); // error: `string` is not `number`

// Those of a tuple literal are covariant.
t([1], 'x') as number | string; // ok
