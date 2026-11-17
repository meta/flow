//
// Tests for subtyping PolyT ~> PolyT.

// Unsoundness issue #8766: we'd accept when the subtyping looked OK
// with the upper tparams set to their bounds, even if it wasn't OK
// for other possible arguments.
{
  (o: <T>() => unknown): (<S>() => S) => o; // should error

  function coerce<A, B>(x: A): B {
    const g = (o: <T>() => A): (<S>() => S) => o; // should error
    return g(<T>(): A => x)();
  }

  const a: number = coerce('a');
  a.toFixed();
}

// Test issue #8766, round 2.
{
  (f: <T>(x: T) => unknown): (<S>(x: unknown) => S) => f; // should error

  function coerce<A, B>(x: A): B {
    const g = (f: <T>(x: T) => unknown): (<S>(x: unknown) => S) => f; // should error
    return g(<T>(y: T): A => x)();
  }

  coerce('a') as empty;
  coerce<_, number>('a').toFixed();
}

// Test issue #8766, round 3 -- this time also testing for a subtle
// pitfall in implementing the rule ∀-local / S-All-Loc from the
// polymorphic-subtyping literature:
//   https://github.com/meta/flow/pull/8767#issuecomment-949402649
{
  const g = (f: <S, T extends S>(x: T) => T): <S, T>(x: T) => S => f; // should error
  const coerce: <B, A>(x: A) => B = g(<S, T extends S>(x: T): T => x);
}


// Conversely, we reject (but ideally shouldn't) a subtyping when it
// requires setting the lower tparams to particular types, or types in
// a particular relation to the upper tparams, other than setting the
// lower tparams to just equal the upper tparams.
{
  // This rejects because it requires (T => T) ~> (number => number)
  // for a generic T.
  (f: <T>(x: T) => T): (<S>(x: number) => number) => f; // should be ok, currently isn't

  // The same example would work with the upper type monomorphized
  // by substituting its parameter S, for any type S we choose:
  (f: <T>(x: T) => T): ((x: number) => number) => f; // ok

  // This rejects because it requires it to work with T = S,
  // whereas it can only work with T = S[].
  (f: <T>(x: T) => T): (<S>(x: S[]) => S[]) => f; // should be ok, currently isn't
}


// Test subtyping where lower has fewer or more parameters than upper.
// Currently these don't work; see #8767.
(f: <R, S>(x: R, y: S) => S): (<T>(x: T, y: T) => T) => f; // ok
(f: <R, S>(x: R, y: S) => S): (<T>(x: T, y: T) => boolean) => f; // error
(f: <R>(x: R, y: unknown) => R): (<S, T>(x: S, y: T) => S) => f; // ok
(f: <R>(x: R, y: unknown) => R): (<S, T>(x: S, y: T) => T) => f; // error


// Test subtyping where lower and upper have same number of parameters,
// but they don't line up when paired off in order.  These are rejected
// by the same approximation that causes us to reject fewer or more parameters.
(f: <S extends string, T extends number>() => { a: S, b: T })
  : <T extends number, S extends string>() => { a: S, b: T } => f; // ideally ok, known error


// Test subtyping where only one side is polymorphic.
(f: (x: unknown) => boolean): (<T>(x: T) => boolean) => f; // ok
(f: (x: number, y: number) => number): (<T>(x: T, y: T) => T) => f; // error
(f: <T>(x: T, y: T) => T): ((x: number, y: number) => number) => f; // ok
(f: <T>(x: T, y: T) => T): ((x: number, y: string) => number) => f; // error
