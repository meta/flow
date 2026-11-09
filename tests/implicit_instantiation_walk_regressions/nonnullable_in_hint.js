// The hint says nothing through `NonNullable` of a bare marker, but `NonNullable` of another type
// is evaluated against it: `K` is `void`.
type Selector<T> = {(v: unknown): T};
type Box<K> = {k: K};
declare function select<K>(x?: K): Selector<NonNullable<Box<K>>>;
const s: Selector<Box<void>> = select(); // ok
