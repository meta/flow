---
title: Type Refinements
slug: /lang/refinements
description: "How Flow narrows types based on conditional checks, typeof, equality, and other runtime tests."
---

Refinements allow us to narrow the type of a value based on conditional tests.

For example, in the function below `value` is a [union](../types/unions.md) of `"A"` or `"B"`.

```js flow-check
function func(value: "A" | "B") {
  if (value === "A") {
    value as "A";
  }
}
```

Inside of the `if` block we know that value must be `"A"` because that's the only
time the if-statement will be true.

The ability for a static type checker to be able to tell that the value inside
the if statement must be `"A"` is known as a refinement.

Next we'll add an `else` block to our if statement.

```js flow-check
function func(value: "A" | "B") {
  if (value === "A") {
    value as "A";
  } else {
    value as "B";
  }
}
```

Inside of the `else` block we know that value must be `"B"` because it can only
be `"A"` or `"B"` and we've removed `"A"` from the possibilities.

:::info TypeScript comparison
Both Flow and TypeScript narrow via `typeof` / `instanceof` / equality / type guards, but the rules for when a refinement is dropped diverge. See [Flow and TypeScript refinement invalidation comparison](../flow-vs-typescript.md#toc-refinement-invalidation) for more.
:::

## Ways to refine in Flow

### `typeof` checks
You can use a `typeof value === "<type>"` check to refine a value to one of the categories supported by the [`typeof`](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Operators/typeof) operator.

The `typeof` operator can output `"undefined"`,`"boolean"`, `"number"`, `"bigint"`, `"string"`, `"symbol"`, `"function"`, or `"object"`.

Keep in mind that the `typeof` operator will return `"object"` for objects, but also `null` and arrays as well.

```js flow-check
function func(value: unknown) {
  if (typeof value === "string") {
    value as string;
  } else if (typeof value === "boolean") {
    value as boolean;
  } else if (typeof value === "object") {
    // `value` could be null, an array, or an object
    value as null | interface {} | ReadonlyArray<unknown>;
  }
}
```

To check for `null`, use a `value === null` [equality](#equality-checks) check.

```js flow-check
function func(value: unknown) {
  if (value === null) {
    value as null; // `value` is null
  }
}
```

To check for [arrays](../types/arrays.md), use `Array.isArray`:

```js flow-check
function func(value: unknown) {
  if (Array.isArray(value)) {
    value as ReadonlyArray<unknown>; // `value` is an array
  }
}
```

### Equality checks

As shown in the introductory example, you can use an equality check to narrow a value to a specific type.
This also applies to equality checks made in `switch` statements.

```js flow-check
function func(value: "A" | "B" | "C") {
  if (value === "A") {
    value as "A";
  } else {
    value as "B" | "C";
  }

  switch (value) {
    case "A":
      value as "A";
      break;
    case "B":
      value as "B";
      break;
    case "C":
      value as "C";
      break;
  }
}
```

While in general it is not recommended to use `==` in JavaScript, due to the coercions it performs,
doing `value == null` (or `value != null`) checks `value` exactly for `null` and `undefined`.
This works well with Flow's [maybe](../types/maybe.md) types, which create a union with `null` and `undefined`.

```js flow-check
function func(value: ?string) {
  if (value != null) {
    value as string;
  } else {
    value as null | undefined;
  }
}
```

You can refine a union of object types based on a common tag, which we call [disjoint object unions](../types/unions.md#toc-disjoint-object-unions):

```js flow-check
type A = {type: "A", s: string};
type B = {type: "B", n: number};

function func(value: A | B) {
  if (value.type === "A") {
    // `value` is A
    value.s as string; // Works
  } else {
    // `value` is B
    value.n as number; // Works
  }
}
```

### Truthiness checks

You can use non-booleans in JavaScript conditionals.
`0`, `NaN`, `""`, `null`, and `undefined` will all coerce to `false` (and so are considered "falsey").
Other values will coerce to `true` (and so are considered "truthy").

```js flow-check
function func(value: ?string) {
  if (value) {
    value as string; // Works
  } else {
    value as null | undefined; // Error! Could still be the empty string ""
  }
}
```

You can see in the above example why doing a truthy check when your value can be a string or number is not suggested:
it is possible to unintentionally check against the `""` or `0`.
We created a [Flow lint](../linting/index.md) called [sketchy-null](../linting/rule-reference.md#toc-sketchy-null) to guard against this scenario:

```js flow-check
// flowlint sketchy-null:error
function func(value: ?string) {
  if (value) { // Error!
  }
}
```

### `in` checks

You can use the [in](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Operators/in)
operator to check if a property exists on an object (either in its own properties, or up the prototype chain).
This can be used to refine a union of objects:

```js flow-check
function func(obj: {foo: string, value: boolean} | {bar: string, value: number}) {
  if ('foo' in obj) {
    obj.value as boolean; // Works!
  } else {
    obj.value as number; // Works!
  }
}
```

This works best on unions of [exact objects](../types/objects.md#exact-and-inexact-object-types), since in the negation we know the property does not exist.
We cannot say the same for [inexact objects](../types/objects.md#exact-and-inexact-object-types), [interfaces](../types/interfaces.md), and [instance types](../types/classes.md),
since they may have other unknown properties, including the one we are checking.
Additionally, [optional properties](../types/objects.md#toc-optional-object-type-properties) may or may not exist, so are not particularly useful to check against.

If you want to refine a union of [tuple types](../types/tuples.md) based on whether an element exists,
check the [length](../types/tuples.md#length-refinement) property instead of attempting to use `in`.

### `instanceof` checks

You can use the [instanceof](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Operators/instanceof) operator to narrow a value as well.
It checks if the supplied constructor's prototype is anywhere in a value's prototype chain.

```js flow-check
class A {
  amaze(): void {}
}
class B extends A {
  build(): void {}
}

function func(value: unknown) {
  if (value instanceof B) {
    value.amaze(); // Works
    value.build(); // Works
  }

  if (value instanceof A) {
    value.amaze(); // Works
    value.build(); // Error
  }

  if (value instanceof Object) {
    value.toString(); // Works
  }
}
```

### Assignments

Flow follows your control flow and narrows the type of a variable after you have assigned to it.

```js flow-check
declare const b: boolean;

let x: ?string = b ? "str" : null;

x as ?string;

x = "hi";

// We know `x` must now be a string after the assignment
x as string; // Works
```

### Type Guards

You can create a reusable refinement by defining a function which is a [type guard](../types/type-guards.md).

```js flow-check
function nonMaybe<T>(x: ?T): implies x is T {
  return x != null;
}

function func(value: ?string) {
  if (nonMaybe(value)) {
    value as string; // Works!
  }
}
```

## Limitations {#toc-limitations}

### Refinement Invalidations {#toc-refinement-invalidations}

Flow drops a refinement when intervening code could have changed the underlying value at the refined storage location. Concretely, a refinement is invalidated when:

- The refined binding or property is written to (`x = ...`, `obj.k = ...`).
- A refinement on an *object property* is in scope, and the property is reachable through aliasing or could be mutated by a callee (a bare call between the check and the use drops the refinement).
- A refinement on a binding *captured by a closure* is in scope, and an intervening call could invoke that closure.

A bare call to a function that does not visibly touch the refined location does **not**, on its own, drop a refinement on a *local* binding. This is the most common over-correction: users assume any function call invalidates refinements when actually only property and closure-captured refinements are conservative across calls.

```js flow-check
declare function sideEffect(): void;

function localCase(x: ?number) {
  if (x != null) {
    sideEffect(); // bare call does NOT drop the refinement on a local
    const a: number = x; // OK
  }
}
```

The reason a property refinement is dropped across a call is that we don't know that the callee hasn't mutated the property through an alias. Imagine the following scenario:

```js flow-check
const obj: {prop?: string} = {prop: "test"};

function otherFunc() {
  if (Math.random() > 0.5) {
    delete obj.prop;
  }
}

function func(value: {prop?: string}) {
  if (value.prop) {
    otherFunc();
    value.prop.charAt(0); // Error!
  }
}

func(obj);
```

Inside of `otherFunc()` we sometimes remove `prop`. Flow doesn't know if the
`if (value.prop)` check is still true, so it invalidates the refinement.

Marking the parameter type readonly (`Readonly<{prop?: string}>` or `{readonly prop?: string}`) doesn't help — readonly only constrains what *this* code can do through *this* view, and the callee may hold a non-readonly reference to the same object.

There's a straightforward way to get around this. Store the value before
calling another function and use the stored value instead. This way you can
prevent the refinement from invalidating.

```js flow-check
function otherFunc() { /* ... */ }

function func(value: {prop?: string}) {
  if (value.prop) {
    const prop = value.prop;
    otherFunc();
    prop.charAt(0);
  }
}
```

### Destructured bindings can't be used to refine {#toc-destructured-bindings}

When you destructure a value out of a discriminated union, the resulting bindings each carry the full union type independent of one another. Refining the destructured sentinel binding does *not* narrow its sibling bindings:

```js flow-check
type FormField =
  | {kind: 'text', value: string}
  | {kind: 'number', value: number};

declare const field: FormField;
const {kind, value} = field;
if (kind === 'text') {
  const s: string = value; // Error: `value` keeps its full `string | number` type
}
```

The fix is to refine through the original value instead of destructuring up front:

```js flow-check
type FormField =
  | {kind: 'text', value: string}
  | {kind: 'number', value: number};

declare const field: FormField;
if (field.kind === 'text') {
  const s: string = field.value; // OK
}
```

Alternatively, a [`match` expression or statement](../match/index.md) handles matching and destructuring in one form — each arm matches the sentinel and binds `value` at its per-arm type:

```js flow-check
type FormField =
  | {kind: 'text', value: string}
  | {kind: 'number', value: number};

declare const field: FormField;
match (field) {
  {kind: 'text', const value} => {
    const s: string = value;
  }
  {kind: 'number', const value} => {
    const n: number = value;
  }
}
```

### Refining a generic value does not narrow other values sharing the same type parameter

When a function is generic, narrowing one parameter based on a runtime check does not narrow other parameters that share the same type parameter. Flow treats the type parameter as a single, fixed type for the entire call — refining the *value* of one parameter does not change the *type* `T` itself.

```js flow-check
type Row = {kind: 'row', cols: number};
type Grid = {kind: 'grid', rows: number, cols: number};
type Layout = Row | Grid;

function updateLayout<T extends Layout>(
  layout: T,
  setLayout: (value: T) => void,
): void {
  if (layout.kind === 'row') {
    // `layout` is narrowed to `Row`, but `setLayout` is still `(T) => void`
    setLayout({kind: 'row', cols: 3}); // ERROR
  }
}
```

To work around this, avoid the generic and use the concrete union type instead. Refinement then works as expected because `setLayout` accepts any `Layout` value:

```js flow-check
type Row = {kind: 'row', cols: number};
type Grid = {kind: 'grid', rows: number, cols: number};
type Layout = Row | Grid;

function updateLayout(
  layout: Layout,
  setLayout: (value: Layout) => void,
): void {
  if (layout.kind === 'row') {
    setLayout({kind: 'row', cols: 3});
  }
}
```

## See Also {#toc-see-also}

- [Type Guards](../types/type-guards.md) — custom functions that act as user-defined refinements
- [Unions](../types/unions.md) — refinements are most commonly used to narrow union types
- [Maybe Types](../types/maybe.md) — refining nullable values with `!= null` checks
- [Refining `unknown`](../types/unknown.md#toc-refining-unknown) — idioms specific to narrowing `unknown` values
- [Variables](./variables.md) — how variable declarations interact with refinement
- [Match Expressions](../match/index.md) — pattern matching with its own narrowing and exhaustiveness checking
