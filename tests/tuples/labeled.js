// Labeled
type A = [foo: string, bar: number];

["s", 1] as A; // OK
[true, 1] as A; // ERROR

// Mixed Labeled and non-labeled
type B = [string, bar: number];

["s", 1] as B; // OK
["s", true] as B; // ERROR

// Labels are ignored - no cycles here
{
  declare const a: [foo: string];
  const foo = [...a];
}

// Reserved words can label required tuple elements.
{
  type FnCall = [function: string, numkeys: number];
  type TypeofLabel = [typeof: string];

  const a: FnCall = ["myfunc", 2]; // OK
  const b: TypeofLabel = ["x"]; // OK
}

// Reserved-word labels can also mark optional tuple elements.
{
  type OptionalFn = [function?: string];

  const c: OptionalFn = []; // OK
}
