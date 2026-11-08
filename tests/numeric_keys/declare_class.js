// Numeric literal property names in declare class

// Field with annotation
declare class A {
  0: string;
  1: boolean;
}
{
  declare const a: A;
  a[0] as string; // OK
  a[1] as boolean; // OK

  a[0] as empty; // ERROR
}

// Finite numeric keys in declare class
declare class B {
  1.1: true; // OK
  9007199254740992: false; // OK
}
