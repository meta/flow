type A = {
  1: string,
};
{
  declare const o: A;

  o[1] as string; // OK
  o['1'] as string; // OK

  o[1] as empty; // ERROR
  o['1'] as empty; // ERROR
}

// With variance (legacy sigils: ambiguity with unary +/- on the numeric key)
type B = {
  -1: string, // ERROR
  +2: boolean, // ERROR
};
type C = {
  writeonly '1': string, // OK
  readonly '2': boolean, // OK
};
{
  declare const o: C;
  o[2] as boolean; // OK
}

// Finite numbers in annotations
type D = {
  1.1: true, // OK
  9007199254740992: false, // OK
};
