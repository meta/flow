---
title: Const Expressions
slug: /types/const-expression
description: "How to use as const to create const-expressions that infer literal and read-only types in Flow."
---

The `as const` modifier tells Flow to infer the narrowest possible type for a literal expression — [literal types](./literals.md) for primitives and read-only types for containers.

```js flow-check
const x = 42 as const;           // type is 42, not number
const a = [1, 2] as const;       // type is Readonly<[1, 2]>, not Array<number>
const o = {x: 1} as const;       // type is Readonly<{x: 1}>, not {x: number}
```

## When to use this {#toc-when-to-use}

Use `as const` when you need Flow to preserve exact literal values and read-only structure — for example, keeping specific string values in a configuration object or ensuring an array is treated as a fixed-length [tuple](./tuples.md). If you only need an array or object literal to be read-only without narrowing to literal types, use the [`Readonly`](./utilities.md#toc-readonly) utility type instead.

## Typing for Const Expressions {#toc-const-expression-typing}

The inferred type of const-expressions is the [singleton type](./literals.md) for primitive values
and the read-only versions for container types. Array literals are inferred as tuple types.

Here are some examples of primitive values:
```js flow-check
42 as const; // inferred type is 42

"hello" as const; // inferred type is "hello"
```

Containers become read-only and the modifier is applied deeply
```js flow-check
const o1 = {f: 42} as const; // {readonly f: 42}

const t1 = [42, "hello"] as const; // Readonly<[42, "hello"]>

const o2 = {f: {g: 42}} as const; // {readonly f: {readonly g: 42}}
```

Note that the effect of the modifier does not persist through variables. For example
in
```js flow-check
const nonConstObject = { g: 42 };
const constObject = { f: nonConstObject } as const;
```
the type of `nonConstObject` will be `{g: number}` and the type of `constObject` will
be `{readonly f: {g: number}}`. In other words, only the top-level property `f` will
be read-only.

Finally, `as const` works on literals directly, and on `const` variables that are initialized with a primitive literal. Applying it to other expressions — for instance a value read from a `let` binding — is an error:
```js flow-check
let x = 1;
const y = x as const; // Error
```

## Typical const-expression example {#toc-const-expression-example}

A common pattern where const-expressions are useful is in enum-like
structures that are not expected to be mutated. For example
```js flow-check
export const STATUS = {
  INIT: 'INIT',
  LOADING: 'LOADING',
  SUCCESS: 'SUCCESS',
  ERROR: 'ERROR',
} as const;
```
The type of `STATUS.INIT` is `"INIT"`, the type of `STATUS.LOADING` is `"LOADING"` and so on.

Combine `as const` with [`satisfies`](./casting.md#toc-as-const-satisfies) to check a configuration contract while keeping these literal types. The read-only typing does not freeze the object at runtime.

With this definition it is also possible to effectively lift the values of the various fields
to type annotations. For example:
```js flow-check
const STATUS = {
  INIT: 'INIT',
  LOADING: 'LOADING',
  SUCCESS: 'SUCCESS',
  ERROR: 'ERROR',
} as const;

type State =
  | { readonly kind: typeof STATUS.INIT; }
  | { readonly kind: typeof STATUS.LOADING; progress: number; }
  | { readonly kind: typeof STATUS.SUCCESS; result: string; }
  | { readonly kind: typeof STATUS.ERROR; msg: string; };
```
Without the use of `as const` the type `typeof STATUS.INIT` would be `string`, which
would make it unsuitable as a distinguishing tag in a disjoint union.

## Adoption of `as const` syntax
To use the `as const` syntax, you need to upgrade your infrastructure:
- Prettier: 3.1+
- Babel: use [flow-parser/babel-plugin](https://www.npmjs.com/package/flow-parser), see our [Babel guide](../tools/babel.md) for more details.
- ESLint: use [flow-eslint](https://www.npmjs.com/package/flow-eslint), see our [ESLint guide](../tools/eslint.md) for more details.

## `const` Type Parameters

Sometimes it is useful to specify that an argument to a function is always expected
to be a const-expression. In such cases, you can annotate the type parameter with
the `const` modifier. We refer to these type parameters as const-type parameters.

When are `const` type parameters useful?

One example is when you want to enforce that all arguments passed to a function
`foo` with signature
```
declare function foo<X>(x: X): X;
```
need to be treated as const-expressions. One way to support this is by always
calling `foo` with `as const` on its argument:
```js flow-check
declare function foo<X>(x: X): X;

const x1 = foo({ f: 42 } as const);
const x2 = foo([42, "hello"] as const);
```
The variables `x1` and `x2` will have the types `{readonly f: 42}` and `Readonly<[42, "hello"]>`,
respectively.

To avoid repeating and potentially forgetting to pass `as const`, you can use the
`const` modifier on type parameter `X`:
```js flow-check
declare function constFoo<const X>(x: X): X;

const y1 = constFoo({ f: 42 });
const y2 = constFoo([42, "hello"]);
```
The variables `y1` and `y2` will have the same type as `x1` and `x2`, respectively.

## Adoption of `const` type parameter syntax
To use the `const` type parameter syntax, you need to upgrade your infrastructure:
- Prettier: 3.5+
- Babel: use [flow-parser/babel-plugin](https://www.npmjs.com/package/flow-parser), see our [Babel guide](../tools/babel.md) for more details.
- ESLint: use [flow-eslint](https://www.npmjs.com/package/flow-eslint), see our [ESLint guide](../tools/eslint.md) for more details.

## See Also {#toc-see-also}

- [Literal Types](./literals.md) — the literal types that `as const` infers for primitive values
- [Tuples](./tuples.md) — `as const` on arrays produces read-only tuple types
- [Type Casts and Satisfies Expressions](./casting.md#toc-as-vs-satisfies) — how to expose a target type or retain the inferred type
- [Generics](./generics.md) — `const` type parameters for enforcing const-expression inference on function arguments
- [Variance](../lang/variance.md) — `as const` produces read-only (covariant) properties
