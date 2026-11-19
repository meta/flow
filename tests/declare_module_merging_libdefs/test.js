import {first, second, overloaded, NS, C, duplicate} from 'm';
import type {First, Second, FirstLocal, SecondLocal, FromFirst, FromSecond, Merged} from 'm';
import {implicitFirst, implicitSecond} from 'implicit';

first as number; // ok
second as string; // ok
1 as First; // ok
'' as Second; // ok
first as string; // error: number ~> string
second as number; // error: string ~> number

// Each block keeps its own scope.
1 as FirstLocal; // ok
'' as SecondLocal; // ok

// A block refers to the exports of the other blocks by name.
'' as FromSecond; // ok
1 as FromFirst; // ok
1 as FromSecond; // error: number ~> string

// Functions become overloads.
overloaded(1) as number; // ok
overloaded('') as string; // ok
overloaded(true); // error: no overload

// Interfaces, namespaces, and classes with interfaces merge.
declare const merged: Merged;
merged.a as number; // ok
merged.b as string; // ok
NS.a as number; // ok
NS.b as string; // ok
declare const c: C;
c.a as number; // ok
c.b as string; // ok

// The later library keeps a name that doesn't merge.
duplicate as string; // ok
duplicate as number; // error: string ~> number

implicitFirst as number; // ok
implicitSecond as string; // ok

require('cjs') as string; // ok
require('cjs') as number; // error: string ~> number

import {Nested, TypesThenValues, ValuesThenTypes} from 'ns';
import type {Types} from 'ns';

// Namespaces with only types merge.
1 as Types.FirstType; // ok
'' as Types.SecondType; // ok
1 as TypesThenValues.SecondType; // error: number ~> string
TypesThenValues.firstValue as number; // ok
'' as ValuesThenTypes.FirstType; // error: string ~> number
ValuesThenTypes.secondValue as string; // ok

// A namespace refers to the members of the other blocks' namespaces by name.
'' as Nested.FromSecond; // ok
1 as Nested.FromSecond; // error: number ~> string
'' as Nested.FromFirst; // error: string ~> number
Nested.fromSecond as number; // error: string ~> number
Nested.Inner.first as number; // ok
Nested.Inner.second as string; // ok
Nested.fromInner as number; // error: string ~> number
