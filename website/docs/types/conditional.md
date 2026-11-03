---
title: Conditional Types
slug: /types/conditional
description: "How to use conditional types in Flow to choose between two output types based on a type-level condition."
---

Conditional types choose between two output types based on a type-level check: `CheckType extends ExtendsType ? TrueType : FalseType`.

```js flow-check
type IsString<T> = T extends string ? 'yes' : 'no';

type A = IsString<string>; // 'yes'
type B = IsString<number>; // 'no'
```

:::info TypeScript comparison
Conditional types match TypeScript in shape and most behaviors: distributivity over unions, `infer` on the right-hand side of `extends` (including the `infer T extends Bound` constraint form), and the `[T] extends [...]` non-distributive opt-out all work the same way.
:::

## When to use this {#toc-when-to-use}

Use conditional types when a type needs to depend on another type — for example, extracting parts of a generic type or expressing return types that vary with input. For simple cases where you know the concrete types upfront, a plain [union](./unions.md) is usually clearer. For functions with multiple signatures, consider [overloads](./intersections.md#toc-intersection-of-function-types) instead.

## Basic Usage {#toc-basic-usage}

If `CheckType` is a subtype of `ExtendsType`, the conditional type will be evaluated to `TrueType`. Otherwise, it will be evaluated to `FalseType`.

The following example illustrates both cases.

```js flow-check
class Animal {}
class Dog extends Animal {}

type TypeofAnimal = Dog extends Animal ? 'animal' : 'unknown'; // evaluates to 'animal'
type TypeofString = string extends Animal ? 'animal' : 'unknown'; // evaluates to 'unknown'
```

## Generic Conditional Types {#toc-generic-conditional-types}

This might not look very useful, since you already know what type it will evaluate to. However, combining with generics, you can perform complex computations over types. For example, you can write down a type-level `typeof` operator:

```js flow-check
type TypeOf<T> =
  T extends null ? 'null' :
  T extends void ? 'undefined' :
  T extends string ? 'string' :
  T extends number ? 'number' :
  T extends boolean ? 'boolean' :
  T extends (...ReadonlyArray<empty>)=>unknown ? 'function' : 'object'

type T1 = TypeOf<null>; // evaluates to 'null'
type T2 = TypeOf<void>; // evaluates to 'undefined'
type T3 = TypeOf<string>; // evaluates to 'string'
type T4 = TypeOf<number>; // evaluates to 'number'
type T5 = TypeOf<boolean>; // evaluates to 'boolean'
type T6 = TypeOf<(string)=>boolean>; // evaluates to 'function'
type T7 = TypeOf<{foo: string}>; // evaluates to 'object'
```

## Function return types dependent on input types {#toc-dependent-function-return}

Conditional types also allow you to intuitively describe the conditions for choosing different function overloads:

```js flow-check
declare function wrap<T>(value: T): T extends string ? { type: 'string', value: string }
                                  : T extends number ? { type: 'number', value: number }
                                  : { type: 'unsupported' }

const v1 = wrap(3);   // has type { type: 'number', value: number }
const v2 = wrap('4'); // has type { type: 'string', value: string }
const v3 = wrap({});  // has type { type: 'unsupported' }
```

The above example can also be written with function overload:

```js flow-check
declare function wrap(value: string): { type: 'string', value: string }
declare function wrap(value: number): { type: 'number', value: number }
declare function wrap(value: unknown): { type: 'unsupported' }

const v1 = wrap(3);   // has type { type: 'number', value: number }
const v2 = wrap('4'); // has type { type: 'string', value: string }
const v3 = wrap({});  // has type { type: 'unsupported' }
```

## Inferring Within Conditional Types {#toc-infer-type}

You can use the power of conditional types to extract parts of a type using `infer` types. For example, the builtin `ReturnType` is powered by conditional types:

```js flow-check
type ReturnType<T> = T extends (...args: ReadonlyArray<empty>) => infer Return ? Return : empty;

type N = ReturnType<(string) => number>; // evaluates to `number`
type S = ReturnType<(number) => string>; // evaluates to `string`
```

We used the infer type here to introduce a new generic type variable named `Return`, which can be used in the type branch of the conditional type. Infer types can only appear on the right hand side of the `extends` clause in conditional types. Flow will perform a subtyping check between the check type and the extends type to automatically figure out its type based on the input type `T`.

In the example of `type N = ReturnType<(string) => number>`, Flow checks if `(string) => number` is a subtype of `(...args: ReadonlyArray<empty>) => infer Return`, and during this process `Return` is constrained to `number`.

When doing extractions like the above example, you usually want the conditional type to always choose the true branch where the type is successfully extracted. For example, silently choosing the false branch is not great:

```js flow-check
type ExtractReturnTypeNoValidation<T> =
  T extends (...args: ReadonlyArray<empty>) => infer Return ? Return : any;

1 as ExtractReturnTypeNoValidation<string>; // no error :(
```

Instead, you might want Flow to error when the input is not a function type. This can be accomplished by adding constraints to the type parameter:

```js flow-check
type ReturnType<T extends (...args: ReadonlyArray<empty>) => unknown> =
  T extends (...args: ReadonlyArray<empty>) => infer Return ? Return : any;

1 as ReturnType<(string) => number>;
1 as ReturnType<string>; // Error
```

### Constraining Inferred Types with `extends` {#toc-infer-extends}

An `infer` variable can carry its own constraint, written `infer T extends Bound`. The bound does two things.

It acts as a filter on the candidate type. If the type matched against `infer X` does not satisfy the bound, the `extends` check fails and the conditional type evaluates to its false branch:

```js flow-check
type ElementIfNumeric<T> = T extends Array<infer X extends number> ? X : false;

type A = ElementIfNumeric<Array<number>>; // evaluates to number
type B = ElementIfNumeric<Array<string>>; // string violates the bound, so evaluates to false
1 as A; // OK
1 as B; // ERROR: number is not false
```

It also supplies a default. When an `infer` variable is underconstrained, because the matched type never determines it, the variable falls back to its bound rather than to `mixed`:

```js flow-check
// Without a bound, an underconstrained infer defaults to mixed:
type Unbounded<T> = T extends null | [infer X] ? X : number;
type R1 = Unbounded<null>; // X is never matched, so evaluates to mixed
1 as R1; // OK

// With a bound, it defaults to that bound instead:
type Bounded<T> = T extends null | [infer X extends string] ? X : number;
type R2 = Bounded<null>; // X is never matched, so evaluates to string
'' as R2; // OK
1 as R2; // ERROR: number is not string
```

## Distributive Conditional Types {#toc-distributive-conditional-type}

When a generic conditional type is given a union type as a type argument, the conditional _distributes_ over the union's members. For example, the `TypeOf` example above can distribute over a union:

```js flow-check
type TypeOf<T> =
  T extends null ? 'null' :
  T extends void ? 'undefined' :
  T extends string ? 'string' : 'other';

type StringOrNull = TypeOf<string | null>; // evaluates to 'string' | 'null'
```

This works by first breaking up the union type, and then passing each type to the conditional type to be evaluated separately. In the example above, this looks something like:

```
TypeOf<string | null>
--> (break up the union) --> TypeOf<string> | TypeOf<null>
--> (evaluate each conditional type separately) --> 'string' | 'null'
```

If you want to avoid this behavior, you can wrap the check type and extends type with unary tuple type:

```js flow-check
type NonDistributiveTypeOf<T> =
  [T] extends [null] ? 'null' :
  [T] extends [void] ? 'undefined' :
  [T] extends [string] ? 'string' : 'other';

type Other = NonDistributiveTypeOf<string | null>; // evaluates to 'other'
```

This trick works because Flow will only enable the distributive behavior of conditional type if the check type is a generic type. The example above does not choose any true branch of the conditional type, because `[string | null]` is not a subtype of `[null]`, `[void]`, or `[string]`, since tuples are [invariantly](../lang/variance.md#toc-invariance) typed.

## Adoption {#toc-adoption}

To use conditional types, you need to upgrade your infrastructure so that it supports the syntax:

- `prettier`: the latest version, using its built-in `flow` parser (see [our Prettier guide](../tools/prettier.md)).
- `babel` with `flow-parser/babel-plugin`. See [our Babel guide](../tools/babel.md) for setup instructions.
- `eslint` with `flow-eslint`. See [our ESLint guide](../tools/eslint.md) for setup instructions.

## See Also {#toc-see-also}

- [Mapped Types](./mapped-types.md) — another advanced type for transforming object types
- [Indexed Access Types](./indexed-access.md) — extracting property types, often combined with conditional types
- [Generics](./generics.md) — conditional types are most useful with generic type parameters
- [Utility Types](./utilities.md) — built-in type transformations like `Partial`, `Readonly`, and `Pick`
