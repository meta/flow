---
title: Literal Types
slug: /types/literals
description: "How to use literal types in Flow to restrict values to specific strings, numbers, booleans, or bigints."
---

Literal types restrict a value to a specific [primitive](./primitives.md) — a particular string, number, boolean, or bigint.

```js flow-check
function acceptsTwo(value: 2) { /* ... */ }

acceptsTwo(2);   // Works!
acceptsTwo(3);   // Error!
```

Literal types support booleans (`true`, `false`), numbers (`42`, `3.14`), strings (`"foo"`), and bigints (`42n`). They are commonly combined with [union types](./unions.md):

```js flow-check
function getColor(name: "success" | "warning" | "danger") {
  switch (name) {
    case "success" : return "green";
    case "warning" : return "yellow";
    case "danger"  : return "red";
  }
}

getColor("success"); // Works!
getColor("danger");  // Works!

getColor("error");   // Error!
```

## When to use this {#toc-when-to-use}

Use literal types when you need to restrict a value to specific constants rather than a broad type like `string` or `number`. They are especially useful in [unions](./unions.md) to define a fixed set of allowed values (e.g. `"success" | "error"`). When you want to preserve a literal value's narrow type inside an object or array — where properties widen by default — reach for [`as const`](./const-expression.md); see [Literal widening](#toc-literal-widening) below. For a more structured alternative to unions of literals, consider [Flow Enums](../enums/index.md).

## Literal widening {#toc-literal-widening}

A `const` binding preserves a primitive literal's narrow type; a `let` binding widens it to the primitive base, because `let` allows reassignment and the narrower type would no longer be sound:

```js flow-check
const c = "success";
c as "success"; // Works! — c has the literal type "success"

let l = "success";
l as "success"; // Error — l has been widened to string
```

Object and array literals widen further: each property widens to its primitive base, and array literals widen from a tuple to `Array<base>` — both even when the surrounding binding is `const`, because the contents are still mutable. Use [`as const`](./const-expression.md) to preserve the literal types, keep the tuple structure, and mark the contents read-only:

```js flow-check
const o      = {x: 1};                 // type is {x: number}
const oConst = {x: 1} as const;        // type is {readonly x: 1}

oConst.x as 1; // Works!
o.x as 1;      // Error — o.x is number

const arr   = ["a", "b"];               // type is Array<string>
const tuple = ["a", "b"] as const;      // type is Readonly<["a", "b"]>

tuple[0] as "a";                   // Works!
arr[0] as "a";                     // Error — arr[0] is string
tuple as ReadonlyArray<"a" | "b">; // Works — Readonly<["a", "b"]> is a subtype
```

Because `as const` produces read-only properties, a value created with `as const` cannot flow into a parameter whose property is writable (object properties are [invariant](../lang/variance.md) by default). Mark the consuming parameter `readonly` to accept it:

```js flow-check
const e = {kind: "click"} as const;

function setKind(o: {kind: "click"}) {}
setKind(e); // Error — `kind` is read-only on `e` but writable on the parameter

function readKind(o: {readonly kind: "click"}) {}
readKind(e); // Works!
```

## See Also {#toc-see-also}

- [Primitive Types](./primitives.md) — the base types (`number`, `string`, etc.) that literals specialize
- [Template Literal Types](./template-literal-types.md) — composing string literals and patterns from other types
- [Unions](./unions.md) — commonly used with literals to define finite sets of values
- [Const Expressions](./const-expression.md) — `as const` for preserving literal types past assignment, especially inside containers
- [Variance](../lang/variance.md) — why a `readonly` value can't flow into a writable slot
- [Flow Enums](../enums/index.md) — a structured alternative to unions of literal types
