declare namespace LibMergedValue {
  type UsesClass = Box<string>; // ok: `Box` is the class declared in the other block
  type UsesValue = make<string>; // ERROR: `make` is the function declared in the other block
}
declare namespace LibMergedValue {
  declare class Box<T> {
    v: T;
  }
  declare function make(): void;
}
