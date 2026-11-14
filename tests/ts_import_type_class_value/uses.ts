// tsc reports errors on the same lines, except `typeof Foo`.
import type { LexicalNode, I, f, Foo } from './node';

declare const nodeClass: typeof LexicalNode;
new nodeClass().getKey() as number; // error: string ~> number
declare class SubNode extends LexicalNode {}
new SubNode().getKey() as number; // error: string ~> number
new LexicalNode(); // error: imported with `import type`

declare const fn: typeof f;
fn() as number; // error: string ~> number
declare const notType: f; // error: `f` is a value
declare const notValue: typeof I; // error: `I` is only a type

declare const fooValue: typeof Foo; // error: Flow binds the interface, tsc reads the const
declare const foo: Foo; // the interface
foo.a as number; // error: string ~> number
