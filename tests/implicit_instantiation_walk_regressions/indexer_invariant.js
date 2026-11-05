declare function d<V>(o: {[string]: V}, v: V): V;
declare const o: {a: number};

// A read-write property against a read-write indexer is invariant.
d(o, 'x'); // error: `string` is not `number`

// The properties of an object literal are covariant.
d({a: 1}, 'x') as number | string; // ok

declare function e<V>(o: {a: V, ...}, v: V): V;
declare const p: {[string]: number};

// So is a read-write indexer against a read-write property.
e(p, 'x'); // error: `string` is not `number`
