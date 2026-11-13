---
title: Type Casts and Satisfies Expressions
slug: /types/casting
description: "Use as to expose a target type or satisfies to check compatibility while retaining an expression's inferred type."
---

import {SinceVersion} from '../../components/VersionTags';

Type cast expressions (`value as T`) check compatibility and give the result the target type `T`.
[Satisfies expressions](#toc-satisfies) (`value satisfies T`) also check compatibility, but keep the operand's inferred type.
Neither changes the value at runtime.

```js flow-check
const x = 42 as number;
```

:::info TypeScript comparison
Flow's `as` only widens or asserts, while TypeScript's `as` permits unsafe downcasts that invent properties. The cast `{foo: 1} as {foo: number, bar: string}` type-checks in TypeScript (asserting a non-existent `bar`) but errors in Flow. See [as casts are safer in Flow](../flow-vs-typescript.md#toc-as-casts) for the full comparison.
:::

## When to use this {#toc-when-to-use}

Use casts to widen a value to a less specific type or to verify that a value matches an expected type at a specific point in your code. For narrowing a value to a more specific type, prefer [refinements](../lang/refinements.md) or [type guards](./type-guards.md) — casting through [`any`](./any.md) is unsafe and should be a last resort.

## Type Cast Expression Syntax {#toc-type-cast-expression-syntax}

In order to create a type cast expression, use the keyword `as` to cast the value to a type:

```js
value as Type
```

This can also be referred to as an "as expression".

Type cast expressions can appear anywhere an expression can appear:

```js
let val = value as Type;
let obj = {prop: value as Type};
let arr = [value as Type, value as Type] as Array<Type>;
```

The value itself can also be an expression:

```js flow-check
2 + 2 as number;
```

Note that the `as` operator has the same [precedence](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Operators/Operator_precedence#table) as `in` and `instanceof`.
Because of this, parentheses around the expression might be required:

```js flow-check
1 === 1 as boolean; // Error!
// Above same as `1 === (1 as boolean)

(1 === 1) as boolean; // Works!
```

Additionally, when in the context of an expression statement, expressions which could ambiguously parse as statements need parens:
```js flow-check
({a: 1}) as {a: number}; // Needs parens to disambiguate from block statement
const x = {a: 1} as {a: number}; // No parens needed, as not in expression statement context
```

When you strip the types all that is left is the value:

```js
value as Type;
```

Is transformed into:
```js
value;
```

### Type Assertions {#toc-type-assertions}

Using type cast expressions you can assert that values are certain types.

```js flow-check
const value = 42;

value as 42;     // Works!
value as number; // Works!
value as string; // Error!
```

Asserting types in this way works the same as types do anywhere else.

### Type Casting {#toc-type-casting}

When you write a type cast expression, the result of that expression is the
value with the provided type. If you hold onto the resulting value, it will
have the new type.

```js flow-check
const value = 42;

value as 42;     // Works!
value as number; // Works!

let newValue = value as number;

newValue as 42;     // Error!
newValue as number; // Works!
```

Unsafe downcasts are not allowed:
```js flow-check
const fooObj = {foo: 1};
const otherObj = fooObj as {foo: number, bar: string};  // ERROR
```

### Adoption of `as` syntax
To use the `as` keyword for type casts, you need to upgrade your infrastructure so that it supports the syntax:
- Prettier: 3.1+
- Babel: use [flow-parser/babel-plugin](https://www.npmjs.com/package/flow-parser), see our [Babel guide](../tools/babel.md) for more details.
- ESLint: use [flow-eslint](https://www.npmjs.com/package/flow-eslint), see our [ESLint guide](../tools/eslint.md) for more details.

## Satisfies Expressions {#toc-satisfies}

<SinceVersion version="0.339" />

Like a class's `implements` clause, `satisfies` checks compatibility with a contract.
It applies to an expression and preserves its inferred type.

```js flow-check
const source = {port: 8080} as const;
const widened = source as {readonly port: number};
const checked = source satisfies {readonly port: number};

widened.port as 8080; // ERROR: the result property type is number
checked.port as 8080; // OK: the operand's literal type is retained
```

Unlike `as`, `satisfies` is not a cast: its result keeps the operand's type.
When [types are stripped](../tools/babel.md), both operators disappear and only the operand remains.
Neither validates data at runtime. Use [runtime checks and refinements](../lang/refinements.md) for untrusted data.

### Combining with `as const` {#toc-as-const-satisfies}

`satisfies` does not itself make an object read-only or preserve every literal type.
Apply [`as const`](./const-expression.md) first to retain literal and discriminant types and read-only typing. Then use `satisfies` to check the contract:

```js flow-check
type Transport =
  | {readonly kind: 'http', readonly port: number}
  | {readonly kind: 'socket', readonly path: string};
const config = {kind: 'http', port: 8080} as const satisfies Transport;

config.kind as 'http'; // OK: the specific union arm is retained
config.port as 8080; // OK
config.port = 3000; // ERROR: read-only property
```

The read-only typing applies only to static checks, **not runtime freezing**. Neither `as const` nor `satisfies` calls `Object.freeze`.

### Adoption {#toc-satisfies-adoption}

To use `satisfies`, set `experimental.tslib_syntax=true` in `.flowconfig` and use Flow and flow-parser 0.339.0+ and [Prettier 3.10.0+](../tools/prettier.md) with its built-in `flow` parser.

## When to use `as` versus `satisfies` {#toc-as-vs-satisfies}

- Use `as` (or a variable annotation `const value: T = ...`) when you intentionally want subsequent code to use the target type. This is useful for exposing a public interface or a wider abstraction. Flow still rejects unsafe downcasts.
- Use `satisfies` to check a contract for configuration objects, lookup maps, or handler tables while keeping their known keys and specific property types.
- Use `as const satisfies` when you also need to retain literal and discriminant types and a read-only structure for constants.

Casting a lookup map to a dictionary lets subsequent code access general string keys. With `satisfies`, the result keeps exactly the keys you provided:

```js flow-check
type Lookup = {readonly [string]: string};
const source = {home: '/', settings: '/settings'};
const widened = source as Lookup;
const annotated: Lookup = source;
const checked = source satisfies Lookup;

widened.missing as string; // OK statically, but the runtime property is absent
annotated.missing as string; // OK statically for the same reason
checked.home as string; // OK
checked.missing; // ERROR: not one of the known keys
```

You can explicitly annotate function parameters in a handler table to make their contracts visible.
The check still retains each function's specific result type:

```js flow-check
type Handlers = {readonly length: (text: string) => number};
const handlers = {
  length: (text: string) => text.length,
} satisfies Handlers;
handlers.length('hello') as number; // OK
handlers.length(42); // ERROR: text must be a string

const answer = ((): 42 => 42) satisfies () => number;
answer() as 42; // OK: the specific return type is retained
```

## Using type cast expressions {#toc-using-type-cast-expressions}

> **Note:** We're going to go through a stripped down example for
> demonstrating how to make use of type cast expressions. This example is not
> solved well in practice.

### Type Casting through `any` {#toc-type-casting-through-any}

Because type casts work the same as all other type annotations, you can only
cast values to less specific types. You cannot change the type or make it
something more specific.

But you can use [any](./any.md) to cast to whatever type you want.

```js flow-check
let value = 42;

value as number; // Works!
value as string; // Error!

let newValue = value as any as string;

newValue as number; // Error!
newValue as string; // Works!
```

By casting the value to `any`, you can then cast to whatever you want.

This is unsafe and not recommended. But it's sometimes useful when you are
doing something with a value which is very difficult or impossible to type and
want to make sure that the result has the desired type.

:::info TypeScript comparison
The TypeScript idiom `value as unknown as T` does not translate directly: Flow's [`unknown`](./unknown.md) is a true top type that cannot be downcast, so the two-step escape hatch must go through [`any`](./any.md) instead — `value as any as T`.
:::

For example, the following function for cloning an object.

```js flow-check
function cloneObject(obj: any) {
  const clone: {[string]: unknown} = {};

  Object.keys(obj).forEach(key => {
    clone[key] = obj[key];
  });

  return clone;
}
```

It would be hard to create a type for this because we're creating a new object
based on another object.

If we cast through any, we can return a type which is more useful.

```js flow-check
function cloneObject<T extends {readonly [key: string]: unknown }>(obj: T): T {
  const clone: {[string]: unknown} = {};

  Object.keys(obj).forEach(key => {
    clone[key] = obj[key];
  });

  return clone as any as T;
}

const clone = cloneObject({
  foo: 1,
  bar: true,
  baz: 'three'
});

clone.foo as number;  // Works!
clone.bar as boolean; // Works!
clone.baz as string;  // Works!
```

## See Also {#toc-see-also}

- [Refinements](../lang/refinements.md) — prefer narrowing types via conditional checks over casting
- [Type Guards](./type-guards.md) — custom refinement functions as a safe alternative to casting
- [Any](./any.md) — casting through `any` for unsafe type conversions (last resort)
- [Subtypes](../lang/subtypes.md) — understanding which casts are allowed based on subtyping rules
- [Const Expressions](./const-expression.md) — `as const` for narrowing to literal and read-only types
- [Modernizing Legacy Flow Syntax](../modernizing-legacy-syntax.md) — migrating legacy `(x: T)` colon casts to the `as` syntax
