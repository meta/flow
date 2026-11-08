const s = 'foo' as const;
declare const g: string;

class C {
  [g]: number = 1; // ERROR: key is a general string, not a literal
  ['a' + 'b']: number = 2; // ERROR: key is not a single literal
  [1.5]: number = 3; // OK: finite numeric keys name ordinary properties
  [123n]: number = 4; // ERROR: bigint key is unsupported
  [g](): number { return 1; } // ERROR: method key is not a literal
  get [s](): number { return 1; } // ERROR: computed getter is unsupported
  set [s](x: number) {} // ERROR: computed setter is unsupported
}

declare const c: C;

c.a; // ERROR: nothing was added, property `a` is missing
