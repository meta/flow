// An instance of a class that extends `Array` matches a member of a union that concretizes to an
// array type, as the speculative match of the flow-based solve flows it there: also an alias of an
// array type or a read-only array.
class Strings extends Array<string> {}
declare const strings: Strings;

type List<T> = Array<T>;
declare function list<X>(x: List<X> | X): X;
list(strings) as string; // ok

declare function readonly<X>(x: Readonly<Array<X>> | X): X;
readonly(strings) as string; // ok

// But not a tuple: subtyping relates no instance to a tuple type.
declare function tuple<X>(x: [X] | Set<X>): X;
tuple(strings) as empty; // error: `Strings` is not a `Set`, and `X` is not `string`
