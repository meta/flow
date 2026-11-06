// @flow

// Members of a `declare class` body are qualified with the class name, just
// like members of a class. An object type written inside a member's annotation
// is not part of the class, so its own members stay unqualified.

declare class D {
  p: number;
//^
  m(): void;
//^
  get g(): string;
//    ^
  set s(x: number): void;
//    ^
  static sp: number;
//       ^
  nested: {inner: number};
//^
//         ^
}
