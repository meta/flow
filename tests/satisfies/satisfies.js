// Basic: the resulting expression keeps the inferred type.
{
  const n = 1 satisfies number; // OK
  n as number; // OK
}

// Subtype check still happens.
{
  "hello" satisfies number; // ERROR: string is not a number
}

// `as const` preserves literal types through `satisfies`.
{
  const o = {a: 1} as const;
  const x = o satisfies {readonly a: number};
  x.a as 1; // OK — literal type `1` preserved
}

// `as const` array preserves tuple-ness with literal element types through `satisfies`.
{
  const arr = [1, 2, 3] as const satisfies ReadonlyArray<number>;
  arr[0] as 1; // OK
  arr[1] as 2; // OK
  arr[2] as 3; // OK
}

// Object literal preserves precise property structure.
// Unlike `as`, the inferred type keeps the specific known keys —
// accessing an unknown key is still an error.
{
  const o = {x: 1, y: 2, z: 3} satisfies {readonly [key: string]: number};
  o.x as number; // OK — `x` is still accessible by name
  o.invalid; // ERROR: property `invalid` is missing — inferred type kept the known keys
}

// satisfies validates that an expression matches a wider type
// without losing the specific inferred type. With `as const`, the
// per-property literal types survive.
{
  type Colors = "red" | "green" | "blue";
  const palette = {
    red: "#ff0000",
    green: "#00ff00",
    blue: "#0000ff",
  } as const satisfies {readonly [K in Colors]: string};

  palette.red as "#ff0000"; // OK — literal preserved by `as const`
  palette.green as "#00ff00"; // OK
  palette.blue as "#0000ff"; // OK
}

// Missing property fails the satisfies check (against a mapped type).
{
  type Colors2 = "red" | "green" | "blue";
  ({
    red: "#ff0000",
    green: "#00ff00",
  }) satisfies {[K in Colors2]: string}; // ERROR: missing `blue`
}

// Wrong property type fails the satisfies check.
{
  ({x: "not a number"}) satisfies {readonly x: number}; // ERROR: string is not number
}

// satisfies does not widen the type of the expression to the annotation.
{
  declare const u: number;
  const v = u satisfies number | string; // OK
  v as number; // OK — preserves the more specific `number` type
}

// Function expression: the inferred return type (literal `42`) is more
// specific than the annotation's `number`. satisfies validates the shape
// but the narrower return type survives.
{
  const fn = ((): 42 => 42) satisfies () => number;
  fn() as 42; // OK — kept the specific `42` return, not widened to `number`
}

// satisfies in `class extends` position: the inferred type (the more
// specific subclass) is preserved, not widened to the satisfies annotation.
{
  class Base { greet(): string { return "hi"; } }
  class Sub extends Base { bonus(): boolean { return true; } }
  class D extends (Sub satisfies typeof Base) {}
  new D().greet() as string; // OK — inherited from Base via Sub
  new D().bonus() as boolean; // OK — `bonus` survived (typeof Sub kept, not widened to typeof Base)
}

// Subtype check against an interface succeeds and result keeps the class type.
{
  interface Animal { readonly name: string }
  class Dog { readonly name: string = "Rex"; bark(): void {} }
  const d = new Dog() satisfies Animal;
  d.bark(); // OK — still typed as Dog, not Animal
}

// Discriminated union: with `as const`, the specific arm is preserved
// so the arm's members are accessible without narrowing.
{
  type Shape =
    | {readonly kind: "circle", readonly radius: number}
    | {readonly kind: "square", readonly side: number};
  const s = {kind: "circle", radius: 5} as const satisfies Shape;
  s.kind as "circle"; // OK — narrowed arm preserved
  s.radius as 5; // OK — literal value preserved by `as const`
}

// Optional property in annotation stays *missing* from the inferred type,
// rather than becoming `boolean | void`.
{
  const c = {name: "x"} satisfies {readonly name: string, readonly debug?: boolean};
  c.name as string; // OK
  c.debug; // ERROR: `debug` was never provided; satisfies doesn't add it
}

// satisfies returns a value, so it composes (nests and chains). The inner
// `as const satisfies` preserves the literal type `1`, and the outer
// satisfies validates against the wider annotation without widening it.
{
  const r = {a: ({b: 1} as const satisfies {readonly b: number})} satisfies {
    readonly a: {readonly b: number},
  };
  r.a.b as 1; // OK — literal `1` survived both satisfies layers
}

// Excess property on an exact target fails (Flow object types are exact by default).
{
  ({a: 1, b: 2}) satisfies {readonly a: number}; // ERROR: extra `b`
}

// Inexact target accepts the same value.
{
  ({a: 1, b: 2}) satisfies {readonly a: number, ...}; // OK
}

// satisfies empty rejects any non-empty value.
{
  1 satisfies empty; // ERROR
}

// satisfies unknown is permissive — any value satisfies `unknown`,
// and the result keeps the original specific type.
{
  const x = {a: 1} satisfies unknown; // OK
  x.a as number; // OK — inferred type preserved, not widened to `unknown`
}

// Generic inference uses the satisfies result's inferred type,
// not the annotation, so callers see the specific shape.
{
  declare function id<T>(x: T): T;
  const r = id({a: 1} as const satisfies {readonly [key: string]: number});
  r.a as 1; // OK — T inferred as {readonly a: 1}; if it were the indexer type, `a` would be `number`
}
