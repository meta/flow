import {myInstance} from './exports';

// Bare import() returns module namespace - not usable as a type directly
type ImportedModule = import('./exports'); // ERROR: value-as-type

// Qualified import type annot: Class
type ImportedClass = import('./exports').MyClass; // OK
myInstance as ImportedClass; // OK
0 as ImportedClass; // ERROR: number ~> MyClass

// Qualified import type annot: Type alias
type ImportedType = import('./exports').MyType; // OK
({name: "test", count: 1}) as ImportedType; // OK
({name: "test"}) as ImportedType; // ERROR: missing count

// Access nested type through multiple property accesses
type Inner = import('./exports').Nested.Inner; // OK
myInstance as Inner; // OK
0 as Inner; // ERROR: number ~> Inner

// Type only exports
type T = import('./export_type_only').T; // OK
true as T; // OK
0 as T; // ERROR: number ~> boolean

// The same types, read from signatures
import type * as Sig from './signature';
myInstance as Sig.ImportedClass; // OK
0 as Sig.ImportedClass; // ERROR: number ~> MyClass
({name: "test"}) as Sig.ImportedType; // ERROR: missing count
myInstance as Sig.Inner; // OK
0 as Sig.Inner; // ERROR: number ~> Inner
0 as Sig.T; // ERROR: number ~> boolean
[1, 2] as Sig.Pair; // OK
['a', 'b'] as Sig.Pair; // ERROR: string ~> number
