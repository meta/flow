// When several members of a union match a type by its head but none fits it, the speculative
// match of the flow-based solve goes on to the bare marker: `S` gets the hint's `'c'` and `'d'`.
type E = 'a' | 'b' | 'c' | 'd';
declare var ab: Array<'a' | 'b'>;
const abcd: Array<E> = ab.concat(['c' as 'c']); // ok

// The hint's `string` reaches `S`, so the literal is widened.
const strings: Array<string> = ab.concat(['x']); // ok
