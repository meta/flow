class C {}
declare namespace C {
  type Legend = number;
  declare namespace Legend {} // ERROR
}

C;
