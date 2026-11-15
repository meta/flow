// An array meets a union through the members that it can be a subtype of, as in the speculative
// match of the flow-based solve, which tries the members in order: an interface that it fits, such
// as `Iterable`, and an opaque type through its lower bound. Both come before `Array<string>`
// here, so `U` is the `?string` that the callback returns, rather than that member's `string`.
declare const nodes: ReadonlyArray<{name: ?string}>;

type Node = void | null | string | Iterable<Node>;
declare function takesNode(value?: ?string | Node | Array<string>): void;
takesNode(nodes.map(node => node.name)); // was an error

declare opaque type OpaqueNode super void | null | string | Iterable<OpaqueNode>;
declare function takesOpaqueNode(value?: ?string | OpaqueNode | Array<string>): void;
takesOpaqueNode(nodes.map(node => node.name)); // was an error
