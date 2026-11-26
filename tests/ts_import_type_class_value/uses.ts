// tsc reports errors on the same lines.
import type { LexicalNode, I, f, Foo, KEY } from './node';

declare const nodeClass: typeof LexicalNode;
new nodeClass().getKey() as number; // error: string ~> number
declare class SubNode extends LexicalNode {}
new SubNode().getKey() as number; // error: string ~> number
new LexicalNode(); // error: imported with `import type`

declare const fn: typeof f;
fn() as number; // error: string ~> number
declare const notType: f; // error: `f` is a value
declare const notValue: typeof I; // error: `I` is only a type

declare const fooValue: typeof Foo; // the const
fooValue.b as string; // error: number ~> string
declare const foo: Foo; // the interface
foo.a as number; // error: string ~> number

// `typeof` reads a member of a namespace by its value, which an `export type` keeps.
import * as NS from './reexport';
declare const throughNamespace: typeof NS.LexicalNode;
new throughNamespace().getKey() as number; // error: string ~> number
NS.LexicalNode; // error: exported with `export type`
declare class SubOfReexport extends NS.LexicalNode {} // error: `extends` reads the namespace object
declare const reexportedInterface: typeof NS.I; // error: `I` is only a type
declare const typesOnly: typeof NS.Types; // error: `Types` has no value

declare const keyed: { [KEY]: number };
keyed.key as string; // error: number ~> string
({ [KEY]: 1 }); // error: imported with `import type`
class EmittedKeyed { [KEY] = 1 } // error: imported with `import type`
