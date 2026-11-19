import {FooClassExported, FooClassStringExported, barFExported, barGenericExported, EscapedGenericExported} from './with-targs-exported';

class Foo<T> {}
declare const FooClass: typeof Foo<string>;

declare function bar<T>(): T;
declare const barF: typeof bar<string>;

new FooClass() as Foo<number>; // error: string ~> number
barF() as number; // error: string ~> number
new FooClassStringExported() as FooClassExported<number>; // error: string ~> number
barFExported() as number; // error: string ~> number

type Invalid1 = typeof barF<string>; // error
type Invalid2 = typeof FooClass<string>; // error
type Invalid3 = typeof barFExported<string>; // error
type Invalid4 = typeof FooClassStringExported<string>; // error
type Invalid5 = typeof bar<string, string>; // error: too many type arguments
type Invalid6 = typeof Foo<string, string>; // error: too many type arguments

declare const emptyValue: empty;
emptyValue as Invalid1;
emptyValue as Invalid2;
emptyValue as Invalid3;
emptyValue as Invalid4;
emptyValue as Invalid5;
emptyValue as Invalid6;

declare function barGeneric<X>(cb: typeof bar<X>): X;

barGeneric<number>(() => 1); // ok
barGeneric<string>(() => 1); // error: number ~> string
barGeneric(() => 1) as number; // ok
barGeneric(() => 1) as string; // error: number ~> string
barGenericExported<number>(() => 1); // ok
barGenericExported<string>(() => 1); // error: number ~> string
barGenericExported(() => 1) as number; // ok
barGenericExported(() => 1) as string; // error: number ~> string

class EscapedGeneric<T> {
  static x: T; // error: static members cannot reference T
}
emptyValue as typeof EscapedGeneric.x<number, boolean>; // ok: x is any
emptyValue as typeof EscapedGenericExported.x<number, boolean>; // ok: x is any

// Type arguments apply to the overloads that take that many
declare function overloaded<A, B>(x: A, y: B): [A, B];
declare function overloaded<T>(x: T): T;
type Ret<F> = F extends (...args: ReadonlyArray<empty>) => infer R ? R : empty;
1 as Ret<typeof overloaded<number>>; // ok
