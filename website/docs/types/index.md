---
title: Type Annotations
slug: /types
description: "Learn how to add Flow type annotations to your code: Primitives, Objects, Functions, Classes, and more."
---

Adding type annotations is an important part of your interaction with Flow.

Flow can infer types for most of your code without explicit annotations.
For example, you don't have to produce annotations for common patterns like `Array.map`:

```js flow-check
["foo", "bar"].map(s => ( // s is inferred to have type string
  s.length
));
```

Still, there are places where you'll want to add types.

Imagine the following `concat` function for concatenating two strings together.

```js flow-check
function concat(a, b) { // Error
  return a + b;
}
```

You need to add annotations on parameters of `concat`, so that Flow can type
check its body. Now you'll get a warning from Flow if you are calling this
function with unexpected types.

```js flow-check
function concat(a: string, b: string) {
  return a + b;
}

concat("A", "B"); // Works!
concat(1, 2); // Error!
```

This guide will teach you the syntax and semantics of all the different types
you can have in Flow.

## See Also {#toc-see-also}

- [Primitive Types](./primitives.md) — `boolean`, `string`, `number`, `null`, `void`, `symbol`, `bigint`
- [Literal Types](./literals.md) — using specific values like `2` or `"foo"` as types
- [Template Literal Types](./template-literal-types.md) — constructing string types and matching string patterns
- [Objects](./objects.md) — typing the shape of JavaScript objects
- [Arrays](./arrays.md) — variable-length lists of values
- [Tuples](./tuples.md) — fixed-length lists with per-element types
- [Functions](./functions.md) — parameters, return types, and generics
- [Classes](./classes.md) — class fields, methods, and nominal typing
- [Interfaces](./interfaces.md) — structural typing for classes and objects
- [Unions](./unions.md) — values that are one of several types (`A | B`)
- [Intersections](./intersections.md) — values that are all of several types (`A & B`)
- [Generics](./generics.md) — parameterized types for reusable, type-safe code
- [Type Aliases](./aliases.md) — giving names to complex types
- [Opaque Type Aliases](./opaque-types.md) — type aliases that hide their underlying type
- [Utility Types](./utilities.md) — built-in type transformations like `Partial`, `Readonly`, `keyof`
