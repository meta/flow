type EmptyInexact = [...]; // OK

// Value subtypes of inexact
{
  [] as [...]; // OK
  [1] as [...]; // OK

  [1] as [1, ...]; // OK
  [1, 2] as [1, ...]; // OK

  [] as [1, ...]; // ERROR
  [false] as [1, ...]; // ERROR
}

// Access
{
  declare const x: [number, ...];
  x[0] as number; // OK
  x[1]; // ERROR - out of bounds
}

// LHS is inexact
{
  declare const x: [1, ...];

  // Both inexact
  x as [...]; // OK
  x as [1, ...]; // OK
  x as [1, 2, ...]; // ERROR

  // RHS is not-inexact: always error
  x as []; // ERROR
  x as [1]; // ERROR
}

// LHS has optional
{
  declare const x: [a?: 1, ...];

  x as [...]; // OK
  x as [1, ...]; // ERROR
  x as [a?: 1, ...]; // OK
  x as [1, 2, ...]; // ERROR
}

// Length
{
  declare const x: [1, ...];
  x.length as number; // OK
  x.length as 1; // ERROR - length is just `number`
}

// Generic
declare function f<T>(x: [T, ...]): T;
{
  declare const x: [number, ...];
  const r = f(x);
  r as number; // OK
}
{
  declare const x: [number];
  const r = f(x);
  r as number; // OK
}

function g<T, X extends [T, ...]>(x: X): Array<T> { // ERROR - unknown elements due to inexactness
  return [...x];
}

function h<T extends [...]>(x: T): T { // OK
  return [...x];
}

// Union
{
  declare const x: [number, string, ...] | [boolean, string]
  x[0] as number | boolean; // OK
  x[1] as string; // OK
  x[2]; // ERROR - out of bounds

  x as [readonly a: number | boolean, readonly b: string, ...]; // OK
}

// Spread into inexact is inexact
type SingleExact = [1];
type TargetInexact = [...SingleExact, ...];
{
  declare const x: TargetInexact;
  x as [1]; // ERROR - inexact
  x as [1, ...]; // OK
}

// Type spread of inexact
type A = [1, ...];
type B = [0, ...A]; // OK
{
  declare const b: B;
  b as [0, 1]; // ERROR - not inexact
  b as [0, 1, ...]; // OK
}
type C = [0, ...A, 2]; // ERROR - element after inexact spread

// Value spread of inexact
{
  declare const a: [1, ...];
  const b = [0 as const, ...a];
  b as [0, 1]; // ERROR - not inexact
  b as [0, 1, ...]; // OK
  [0, ...a, 2]; // ERROR - element after inexact spread
}

// elem_t is mixed if inexact
{
  declare const x: [number, ...];
  declare const n: number;

  x[n] as number; // ERROR - unknown elements due to inexactness
  x[n] as unknown; // OK

  const y = [...x];
  y[n] as number; // ERROR - unknown elements due to inexactness
  y[n] as unknown; // OK

  declare const z: B;
  z[n] as number; // ERROR - unknown elements due to inexactness
  z[n] as unknown; // OK
}

// Reversal of inexact tuple not allowed
declare function tail<T>(xs: [1, ...T, ...]): T;
{
  const x = tail([1]); // ERROR - underconstrained
}

// Call rest args
{
  const a = <T extends [...]>(args: T, f: (...rest: T) => void) => {
    f(...args); // OK
    f(0, ...args); // ERROR
    f(...args, 0); // ERROR
  };
  a([1, true], (x, y) => {
    x as number; // OK
    y as boolean; // OK
  });

  const b = <T extends [...]>(args: T, f: (n: number, ...rest: T) => void) => {
    f(1, ...args); // OK
  };
  b([true], (x, y) => {
    x as number; // OK
    y as boolean; // OK
  });

  const c = <T extends [number, ...]>(args: T, f: (...rest: T) => void) => {
    f(...args); // OK
  };
  c([true], (x) => {}); // ERROR
  c([1, true], (x, y) => {
    x as number; // OK
    y as boolean; // OK
  });

  const d = <T extends [...]>(args: T, f: (s: string) => void) => {
    f(...args); // ERROR
  };
}
