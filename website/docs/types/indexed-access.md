---
title: Indexed Access Types
slug: /types/indexed-access
description: "How to use indexed access types to extract a property type from an object, array, or tuple type."
---

Indexed access types extract the type of a property from an [object](./objects.md), [array](./arrays.md), or [tuple](./tuples.md) type.

```js flow-check
type Person = {name: string, age: number};
type Age = Person['age']; // number
```

:::info TypeScript comparison
`T[K]` indexed access works the same as TypeScript, including the use of `keyof` and unions of literal keys as the index. The [Optional Indexed Access Type](#toc-optional-indexed-access-types) `Obj?.['prop']` is Flow-only — it mirrors the runtime `?.` operator at the type level, returning `void` when the source is nullish. See [Flow-only syntactic forms](../flow-vs-typescript.md#toc-flow-only-syntax) for the full comparison.
:::

## When to use this {#toc-when-to-use}

Use indexed access types to derive a property's type from an existing type rather than duplicating it. This keeps types in sync — if the source type changes, the extracted type updates automatically. For transformations across all keys, use [mapped types](./mapped-types.md) instead.

## Usage {#toc-indexed-access-type-usage}

Access an object type's property:
```js flow-check
type Cat = {
  name: string,
  age: number,
  hungry: boolean,
};

type Hungry = Cat['hungry']; // type Hungry = boolean
const isHungry: Hungry = true; // OK - `Hungry` is an alias for `boolean`

// The index can be a type, not just a literal:
type AgeProp = 'age';
type Age = Cat[AgeProp]; // type Age = number
const catAge: Age = 6; // OK - `Age` is an alias for `number`
```

For a [symbol-keyed property](./objects.md#toc-symbol-keyed-properties), apply `typeof` to the symbol and use the resulting type as the index:
```js flow-check
const id = Symbol('id');
type Tagged = {[id]: number, name: string};

type Id = Tagged[typeof id]; // number
const value: Id = 1; // OK
const wrong: Id = 'one'; // Error
```

Access an array type's element, by getting the type at the array's indices (which are `number`s):
```js flow-check
type CatNames = Array<string>;

type CatName = CatNames[number]; // type CatName = string
const myCatsName: CatName = 'whiskers'; // OK - `CatName` is an alias for `string`
```

Access a tuple type's elements:
```js flow-check
type Pair = [string, number];

const name: Pair[0] = 'whiskers'; // OK - `Pair[0]` is an alias for `string`
const age: Pair[1] = 6; // OK - `Pair[1]` is an alias for `number`
const wrong: Pair[2] = true; // Error - `Pair` only has two elements
```

The index can be a union, including the result of calling [`keyof`](./utilities.md#toc-keys):
```js flow-check
type Cat = {
  name: string,
  age: number,
  hungry: boolean,
};

type CatValues = Cat[keyof Cat]; // type CatValues = string | number | boolean
```

The index can also be a generic:
```js flow-check
function getProp<O extends {readonly [string]: unknown}, K extends keyof O>(o: O, k: K): O[K] {
  return o[k];
}

const x: number = getProp({a: 42}, 'a'); // OK
const y: string = getProp({a: 42}, 'a'); // Error - `number` is not a `string`
getProp({a: 42}, 'b'); // Error - `b` does not exist in object type
```

You can nest these accesses:
```js flow-check
type Cat = {
  name: string,
  age: number,
  hungry: boolean,
  personality: {
    friendly: boolean,
    hungerLevel: number,
  }
};

type Friendly = Cat['personality']['friendly']; // type Friendly = boolean
const isFriendly: Friendly = true; // Pet the cat
```


## Optional Indexed Access Types {#toc-optional-indexed-access-types}

Optional Indexed Access Types work like optional chaining. They allow you to access properties from nullable object types:

```js
type T = Obj?.['prop'];
```

Like optional chaining, the resulting types of Optional Indexed Access Types include `void`.
If you have a long chain of nested optional accesses, you can wrap the entire thing with a `NonNullable<...>` if you don't want `void` in your resulting type.

Example:

```js flow-check
type TasksContent = ?{
  tasks?: Array<{
    items?: {
      metadata?: {
        title: string,
        completed: boolean,
      },
    },
  }>,
};

type TaskData = TasksContent?.['tasks']?.[number]?.['items']?.['metadata'];
```

There is one small difference between optional chaining and Optional Indexed Access Types.
If the object type you access is not nullable, the resulting type in optional chaining will not include `void`.
With Optional Indexed Access Types, for implementation reasons, the resulting type will always include `void`.
However, if your object type is not nullable then you don't need to use an Optional Indexed Access Type, but should just use a regular Indexed Access Type.


## Adoption {#toc-adoption}

To use Indexed Access Types, you need to upgrade your infrastructure so that it supports the syntax:

- `flow` and `flow-parser`: 0.155
- `prettier`: 2.3.2
- `babel`: 7.14

Install [`eslint-plugin-fb-flow`](https://www.npmjs.com/package/eslint-plugin-fb-flow), and add `fb-flow` to your ESLint plugin list.
Then enable the rule in your ESLint config:

```
'fb-flow/use-indexed-access-type': 1,
```

## See Also {#toc-see-also}

- [Objects](./objects.md) — the primary type indexed access is used with
- [Mapped Types](./mapped-types.md) — transforming object types, often combined with indexed access
- [Conditional Types](./conditional.md) — type-level conditionals, frequently used alongside indexed access
- [Utility Types](./utilities.md) — `keyof`, `Pick`, and other built-in type helpers
- [Modernizing Legacy Flow Syntax](../modernizing-legacy-syntax.md) — migrating the legacy `$PropertyType` / `$ElementType` utilities to `T[K]` indexed access
