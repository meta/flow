// Numeric literal property names in interface

// Field with annotation
interface A {
  0: string;
  1: boolean;
}
{
  declare const a: A;
  a[0] as string; // OK
  a[1] as boolean; // OK
  a['0'] as string; // OK

  a[0] as empty; // ERROR
}

// Finite numeric keys in interface
interface B {
  1.1: true; // OK
  9007199254740992: false; // OK
}

// Variance with numeric key in interface (legacy sigils: ambiguity with unary +/-)
interface C {
  -1: string; // ERROR
  +2: boolean; // ERROR
}
