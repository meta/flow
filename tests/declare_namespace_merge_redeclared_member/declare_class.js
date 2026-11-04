declare class C {}
declare namespace C {
  interface Legend {}
  declare namespace Legend {} // ERROR
}

C;
