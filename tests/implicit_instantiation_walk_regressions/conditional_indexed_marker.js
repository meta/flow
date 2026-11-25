import {$nodesOfType, ListNode, registerNodeTransform} from 'ts_klass';

// A conditional type whose check type indexes a type parameter, as Lexical's `Klass<T>`, takes
// both branches: the class gives `T` its instances.
$nodesOfType(ListNode)[0].getListType(); // ok
registerNodeTransform(ListNode, node => node.getListType()); // ok
$nodesOfType(ListNode)[0].nope; // error: `nope` is missing in `ListNode`

// The same without overloads as with them.
declare function f<T extends {readonly k: unknown, ...}>(x: T['k'] extends string ? {s: T} : {n: T}): T;
declare function g<T extends {readonly k: unknown, ...}>(x: T['k'] extends string ? {s: T} : {n: T}): T;
declare function g(x: boolean): boolean;
f({n: {k: 1}}).k as number; // ok
g({n: {k: 1}}).k as number; // ok

// The branches meet the argument as the members of a union: a branch that is `T` itself gets the
// argument only if the other branch gets nothing.
declare function h<T extends {readonly k: unknown, ...}>(x: T['k'] extends string ? {readonly a: T} : T): T;
h({a: {k: 'x'}}).k as string; // ok
h({k: 1}).k as number; // ok

// The first branch that the argument fits gives `T` its evidence.
declare function i<T extends {readonly k: unknown, ...}>(
  x: T['k'] extends string ? {readonly a: T, ...} : {readonly b: T, ...},
): T;
i({a: {k: 1}, b: {k: 'x'}}); // error: `b` must be `{k: number}`
