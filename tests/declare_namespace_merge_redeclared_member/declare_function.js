declare function f(): void;
declare namespace f {
  interface Legend {}
  declare namespace Legend {} // ERROR
}

f;
