[0, undefined] as [number, ?string]; // Ok, correct arity
[0] as [number, ?string]; // Error, arity is enforced

[] as [?number, string]; // error, since second element is not marked maybe


// Optional elements
type Single = [a: number];
type WithOpt = [a: number, b?: string];
type WithReadOnlyOpt = [a: number, readonly b?: string];

{
  declare const single: Single;
  single as WithOpt; // ERROR
  single as WithReadOnlyOpt; // OK
  single.length as 1; // OK
}

{
  declare const withOpt: WithOpt;
  withOpt as Single; // ERROR
  withOpt as WithReadOnlyOpt; // OK
  withOpt as [a: number, b?: string]; // OK
  withOpt.length as 1 | 2; // OK
  withOpt.length as 2; // ERROR

  withOpt[0] as number; // OK
  withOpt[1] as string | void; // OK
  withOpt[1] as string; // ERROR

  withOpt[1] = undefined; // ERROR
}
{
  declare const withReadOnlyOpt: WithReadOnlyOpt;
  withReadOnlyOpt as Single; // ERROR
  withReadOnlyOpt as WithOpt; // ERROR
  withReadOnlyOpt[1] = undefined; // ERROR
}
{
  declare const withWriteOnlyOpt: [a: number, writeonly b?: string | boolean];
  withWriteOnlyOpt as [a: number]; // ERROR
  withWriteOnlyOpt as [a: number, b?: string | boolean]; // ERROR
  withWriteOnlyOpt as [a: number, writeonly b?: string]; // OK
}
{
  declare const f11: (...args: [readonly foo?: string]) => number;
  declare const f12: (...args: [readonly foo: string | void]) => number;
  declare const f2: (foo?: string) => number;

  f11 as typeof f2; // error
  f12 as typeof f2; // ok
  f2 as typeof f11; // ok
  f2 as typeof f12; // ok
  f11 as typeof f12; // error
  f12 as typeof f11; // ok
  // from above, f12 = f2 <: f11
}

[1] as Single; // OK
[1, undefined] as Single; // ERROR

[1] as WithOpt; // OK
[1, 's'] as WithOpt; // OK
[1, undefined] as WithOpt; // ERROR
[1, true] as WithOpt; // ERROR

[1] as WithReadOnlyOpt; // OK
[1, 's'] as WithReadOnlyOpt; // OK
[1, undefined] as WithReadOnlyOpt; // ERROR
[1, true] as WithReadOnlyOpt; // ERROR

type InvalidReqAfterOpt = [a: number, b?: string, c: string]; // ERROR
{
  declare const x: unknown;
  x as InvalidReqAfterOpt; // OK - it's `any`
}
type InvalidReqAfterOptMultiple = [a: number, b?: string, c: string, d: boolean]; // ERROR (on first one)

// Refinements
{
  declare const x: [number] | [a: boolean, b?: string];
  if (x.length === 1) {
    x as [number] | [a: boolean, b?: string]; // OK
    x as empty; // ERROR
  } else {
    x as [a: boolean, b?: string]; // OK
    x[0] as boolean; // OK
    x as [number]; // ERROR
  }
}
{
  declare const x: [number] | [a: boolean, b: string, c?: string];
  if (x.length === 2) {
    x as [a: boolean, b: string, c?: string]; // OK
    x[0] as boolean; // OK
    x as [number]; // ERROR
  } else {
    x as [number] | [a: boolean, b: string, c?: string]; // OK
    x as empty; // ERROR
  }
}

// Array initializers
{
  const x: [a?: 0] = [];
  const y: [a?: 1] = [1];
}

// Array rest
{
  declare const x: [a: 1, b: 2, c?: 3, d?: 4];

  const [, ...rest0] = x;
  rest0 as [b: 2, c?: 3, d?: 4]; // OK
  rest0 as [a: 1, b: 2, c?: 3, d?: 4]; // ERROR
  rest0 as [a: 1, b?: 3, c?: 4]; // ERROR

  const [,,, ...rest1] = x;
  rest1 as [d?: 4]; // OK
  rest1 as [d?: empty]; // ERROR

  const [,,,, ...rest2] = x;
  rest2 as []; // OK
  rest2 as [1]; // ERROR
}

// `undefined` values
{
  [1, undefined] as [number, b?: string | void]; // OK
  type O = {a?: string};
  [1, undefined] as [number, b?: O['a']]; // OK
}

// `Optional` & `Required`
type AllRequired = [number, string];
[] as AllRequired; // ERROR
[] as Partial<AllRequired>; // OK

type AllOptional = [a?: number, b?: string];
[] as AllOptional; // OK
[] as Required<AllOptional>; // ERROR
[1, 's'] as Required<AllOptional>; // OK
