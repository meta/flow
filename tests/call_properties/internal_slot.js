type O = {
  [[call]](): void;
 ...}

({}) as O; // err: no callable property
(function() { return 0 }) as O; // err: number ~> void
(function() {}) as O; // ok

interface I {
  [[call]](): void;
}

({}) as I; // err: no callable property
(function() { return 0 }) as I; // err: number ~> void
(function() {}) as I; // ok

declare class C1 {
  static [[call]](): void;
}
C1() as empty; // error: void ~> empty

declare const mixed_callable: { [[call]]: unknown, ... };
mixed_callable();

declare const annot_callable: { [[call]]: Fn, ... }
type Fn = (x: string) => number;
annot_callable("foo") as number; // OK
annot_callable(0); // error: number ~> string
annot_callable("foo") as empty; // error: number ~> empty
