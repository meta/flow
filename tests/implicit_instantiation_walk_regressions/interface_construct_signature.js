interface Ctor<T> {
  new (length: number): T;
}

declare class Box {}

interface BoxCtor {
  new (length: number): Box;
  new (items: ReadonlyArray<number>): Box;
}

declare const BoxCtorValue: BoxCtor;
declare function make<T>(ctor: Ctor<T>, length: number): T;

// An interface against an interface: the construct signatures too, as in structural subtyping.
const box = make(BoxCtorValue, 1); // ok
box as Box; // ok
box as string; // error
