// Method without return type — OK in `.d.ts`
declare class M1 {
  foo(x: number);
}

// Constructor without return type: `new` gives the instance
declare class C1 {
  constructor(x?: number);
}

// Setter without return type
declare class S1 {
  set foo(v: number);
}

// Getter + setter pair
declare class S2 {
  get foo(): number;
  set foo(v: number);
}

// Generic
declare class S3<T> {
  set foo(v: T);
}

// With accessibility modifier
declare class S4 {
  protected set foo(v: number);
}

// Explicit `: void` still accepted (regression guard)
declare class S5 {
  set foo(v: number): void;
}

declare class S6 {
  get foo(); // ERROR — getter without return type still errors (asymmetry preserved)
}

export {M1, C1, S1, S2, S3, S4, S5, S6};
