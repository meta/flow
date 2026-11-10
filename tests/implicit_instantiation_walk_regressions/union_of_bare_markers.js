// A type that fits no other member of a union of several bare markers goes to the first of them,
// which the speculative match of the flow-based solve tries first: `A` gets the hint's `string`
// and `number`.
declare function pair<A, B>(a: A, b: B): Array<A | B>;
const mixed: Array<string | number> = pair(1, 'a'); // ok
