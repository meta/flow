---
title: Maybe Types
slug: /types/maybe
description: "How to use maybe types (?T) in Flow to represent values that can be null or undefined."
---

`?T` is shorthand for the [union](./unions.md) `T | null | undefined`. It accepts the value, `null`, or `undefined`.

```js flow-check
function acceptsMaybeNumber(value: ?number) {
  // ...
}

acceptsMaybeNumber(42);        // Works!
acceptsMaybeNumber();          // Works! (implicitly undefined)
acceptsMaybeNumber(undefined); // Works!
acceptsMaybeNumber(null);      // Works!
acceptsMaybeNumber("42");      // Error!
```

:::info TypeScript comparison
Flow's `?T` shorthand has no equivalent TypeScript shorthand — it expands to `T | null | undefined`. See [Flow and TypeScript type spellings](../flow-vs-typescript.md#toc-type-spellings) for the full comparison.
:::

## When to use this {#toc-when-to-use}

Prefer `?T` for values that may be absent entirely (optional config, uninitialized state). Use `T | null` instead when you want to accept `null` but still require the caller to explicitly pass an argument. For optional object properties, use the `key?: T` syntax rather than `key: ?T` — see [optional properties](./objects.md#toc-optional-object-type-properties).

## Maybe types as function parameters {#toc-maybe-types-as-function-parameters}

Flow allows callers to omit an argument whose parameter type includes `undefined`, including `?T`. This differs from TypeScript, where a required parameter typed `T | null | undefined` still requires an argument. If you want to require callers to always pass an argument while still accepting `null`, use `T | null` instead:

```js flow-check
function acceptsNull(value: number | null) {
  // ...
}

acceptsNull(42);   // Works!
acceptsNull(null); // Works!
acceptsNull();     // Error! Argument is required.
```

In the case of objects, a **missing** property is not the same thing as an explicitly `undefined` property.

```js flow-check
function acceptsMaybeProp({value}: {value: ?number}) {
  // ...
}

acceptsMaybeProp({value: undefined}); // Works!
acceptsMaybeProp({});                 // Error!
```

If you want to allow missing properties, use the [optional property](./objects.md#toc-optional-object-type-properties) syntax, where the `?` is placed _before_ the colon.
It is also possible to combine both syntaxes for an optional maybe type, for example `{value?: ?number}`.

## Refining maybe types {#toc-refining-maybe-types}

Imagine we have the type `?number`, if we want to use that value as a `number`
we'll need to first check that it is not `null` or `undefined`.

```js flow-check
function acceptsMaybeNumber(value: ?number): number {
  if (value !== null && value !== undefined) {
    return value * 2;
  }
  return 0;
}
```

You can simplify the two checks against `null` and `undefined` using a single
`!= null` check which will do both.

```js flow-check
function acceptsMaybeNumber(value: ?number): number {
  if (value != null) {
    return value * 2;
  }
  return 0;
}
```
Most double equal checks are discouraged in JavaScript, but the above pattern is safe (it checks for exactly `null` and `undefined`).

You could also flip it around, and check to make sure that the value has a type
of `number` before using it.

```js flow-check
function acceptsMaybeNumber(value: ?number): number {
  if (typeof value === 'number') {
    return value * 2;
  }
  return 0;
}
```

However, type refinements can be lost. For instance, calling a function after refining the type of an object's property will invalidate this refinement.
Consult the [refinement invalidations](../lang/refinements.md#toc-refinement-invalidations) docs for more details, to understand why Flow works this way,
and how you can avoid this common pitfall.

## See Also {#toc-see-also}

- [Unions](./unions.md) — the general form: `?T` is shorthand for `T | null | undefined`
- [Refinements](../lang/refinements.md) — how to narrow maybe types before use
- [Primitives](./primitives.md) — `null` and `undefined` types, and optional parameters
