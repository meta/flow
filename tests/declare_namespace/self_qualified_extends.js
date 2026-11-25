// An interface in a namespace that extends another member through the
// namespace's own name.

declare namespace SelfRef {
  declare export interface Base<T> {
    clone(): this;
    value: T;
  }
  declare export interface Child extends SelfRef.Base<string> {
    extra: number;
  }
  declare namespace Inner {
    declare export interface Cloneable {
      clone(): this;
    }
  }
  declare export interface Nested extends SelfRef.Inner.Cloneable {
    nested: boolean;
  }
}

declare const child: SelfRef.Child;
child.value as string; // OK
child.clone().extra as number; // OK
child.value as number; // ERROR

declare const nested: SelfRef.Nested;
nested.clone().nested as boolean; // OK
nested.clone().nested as string; // ERROR
