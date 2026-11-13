---
title: Mapped Types
slug: /types/mapped-types
description: "How to use mapped types in Flow to transform object types or tuple types by iterating over their keys."
---

Mapped types transform object types by iterating over their keys and producing new property types.

```js flow-check
type O = {foo: number, bar: string};
type ReadOnly = {readonly [key in keyof O]: O[key]};
```

:::info TypeScript comparison
Mapped types match TypeScript including distributivity, `keyof`-based source iteration, and the homomorphic `{[K in keyof T]: ...}` form.
:::

## When to use this {#toc-when-to-use}

Use mapped types when you need to systematically transform every property of an existing object type — for example, making all properties optional, read-only, or wrapping their values. If you only need to change a few specific properties, [object type spread](./objects.md#object-type-spread) is simpler.

## Basic Usage {#toc-basic-usage}

Mapped types have syntax similar to indexed object types but use the `in` keyword:
```js flow-check
type O = {foo: number, bar: string};

type Methodify<T> = () => T;

type MappedType = {[key in keyof O]: Methodify<O[key]>};
```

In this example, `MappedType` has all of the keys from `O` with all of the value types transformed by
`Methoditfy<O[key]>`. The `key` variable is substituted for each key in `O` when creating the property, so
this type evaluates to:
```
{
  foo: Methodify<O['foo']>,
  bar: Methodify<O['bar']>,
}
= {
  foo: () => number,
  bar: () => string,
}
```

## Mapped Type Sources {#toc-mapped-type-sources}

We call the type that comes after the `in` keyword the *source* of the mapped type. The source of
a mapped type must be a subtype of `string | number | symbol`:
```js flow-check
type MappedType = {[key in boolean]: number}; // ERROR!
```

Typically, you'll want to create a mapped type based on another object type. In this case, you
should write your mapped type using an inline `keyof`:
```js flow-check
type GetterOf<T> = () => T;
type Obj = {foo: number, bar: string};
type MappedObj = {[key in keyof Obj]: GetterOf<Obj[key]>};
```

But you do not need to use an object to generate a mapped type. You can also use a union of string
literal types to represent the keys of an object type:
```js flow-check
type Union = 'foo' | 'bar' | 'baz';
type MappedType = {[key in Union]: number};
// = {foo: number, bar: number, baz: number};
```

Importantly, when using string literals the union is collapsed into a *single object type*:
```js flow-check
type MappedTypeFromKeys<Keys extends string> = {[key in Keys]: number};
type MappedFooAndBar = MappedTypeFromKeys<'foo' | 'bar'>;
// = {foo: number, bar: number}, not {foo: number} | {bar: number}
```

If you use a type like `number` or `string` in the source of your mapped type then Flow will create
an indexer:
```js flow-check
type MappedTypeFromKeys<Keys extends string> = {[key in Keys]: number};
type MappedFooAndBarWithIndexer = MappedTypeFromKeys<'foo' | 'bar' | string>;
// = {foo: number, bar: number, [string]: number}
```

A source can also contain [unique symbols](./primitives.md#toc-unique-symbols). Each becomes a [symbol-keyed property](./objects.md#toc-symbol-keyed-properties) in the result. Mapped types over `keyof` likewise preserve the source's symbol keys:
```js flow-check
const id = Symbol('id');
const version = Symbol('version');

type Meta = {[K in typeof id | typeof version]: string};
// = {[id]: string, [version]: string}
declare const meta: Meta;
meta[id] as string; // OK
meta[Symbol()]; // ERROR: not a key of `Meta`
```

## Distributive Mapped Types {#toc-distributive-mapped-types}

When the mapped type uses an inline `keyof` or a type parameter bound by `keyof`
Flow will distribute the mapped type over unions of object types.

For example:
```js flow-check
// This mapped type uses keyof inline
type MakeAllValuesNumber<O extends {...}> = {[key in keyof O]: number};
type ObjWithFoo = {foo: string};
type ObjWithBar = {bar: string};

type DistributedMappedType = MakeAllValuesNumber<
  | ObjWithFoo
  | ObjWithBar
>; // = {foo: number} | {bar: number};

// This mapped type uses a type parameter bound by keyof
type Pick<O extends {...}, Keys extends keyof O> = {[key in Keys]: O[key]};
type O1 = {foo: number, bar: number};
type O2 = {bar: string, baz: number};
type PickBar = Pick<O1 | O2, 'bar'>; // = {bar: number} | {bar: string};
```

Distributive mapped types will also distribute over `null` and `undefined`:
```js flow-check
type Distributive<O extends ?{...}> = {[key in keyof O]: O[key]};
type Obj = {foo: number};
type MaybeMapped = Distributive<?Obj>;// = ?{foo: number}
null as MaybeMapped; // OK
undefined as MaybeMapped; // OK
({foo: 3}) as MaybeMapped; // OK
```

## Property Modifiers {#toc-property-modifiers}

### Adding variance and optionality {#toc-adding-variance-and-optionality}

You can add `readonly` or `writeonly` variance modifiers and the optionality modifier `?` in mapped types:
```js flow-check
type O = {foo: number, bar: string}
type ReadOnlyPartialO = {readonly [key in keyof O]?: O[key]}; // = {readonly foo?: number, readonly bar?: string};
```

### Removing optionality with `-?` {#toc-removing-optionality}

The `?` modifier makes every property optional. Prefixing it with `-` does the opposite: `-?` removes optionality, turning every optional property into a required one. This works like the [`Required<T>`](./utilities.md#toc-required) utility type.

```js flow-check
type O = {foo: number, bar?: string};
type RequiredO = {[key in keyof O]-?: O[key]}; // = {foo: number, bar: string}
({foo: 1}) as RequiredO; // ERROR: bar is now required
```

`-?` only clears the optionality of properties that were optional in the source. It does not strip `void` or `undefined` from a value type that was already required:
```js flow-check
type T = {a: string | void};
type Mapped = {[K in keyof T]-?: T[K]};
declare const m: Mapped;
m.a as string | void; // OK: `a` was not optional, so its value type is unchanged
m.a as string; // ERROR: void is still part of the value type
```

`-?` also works on tuple inputs, where it makes elements no longer optional:
```js flow-check
type Tuple = [a: string, b?: number, c?: boolean];
type AllRequired = {[K in keyof Tuple]-?: Tuple[K]};
(['']) as AllRequired; // ERROR: elements 1 and 2 are now required
declare const t: AllRequired;
t as [string, number, boolean]; // OK
```

> Note: Flow does not yet support `+readonly`/`-readonly` variance modifiers. They are [coming soon](../flow-vs-typescript.md#toc-coming-soon).

## Mapped Type on Arrays {#toc-mapped-type-on-arrays}

Mapped type also works on array or tuple inputs. If the mapped type is in the form of

```
{[K in keyof <type_1>]: <type_2>}
```

then `type_1` is allowed to be an array or tuple type.

This feature will be especially useful if you want to map over elements of a tuple:

```js flow-check
type Tuple = [readonly a: number, b?: string];
type MappedTuple = {[K in keyof Tuple]: Tuple[K] extends number ? boolean : string};
const a: MappedTuple[0] = true;
const b: MappedTuple[1] = '';
'' as MappedTuple[0] // error
false as MappedTuple[1] // error
declare const mapped: MappedTuple;
mapped[0] = true; // error: cannot-write
```

Property modifiers apply to the elements of an array input. Variance modifiers change the variance of each tuple element, and `readonly` turns an array into a `ReadonlyArray`. Arrays cannot be made write-only.

```js flow-check
type Tuple = [readonly a: number, b?: string];
type Supported1 = {[K in keyof Tuple]?: string};
type Supported2 = {readonly [K in keyof Tuple]: string};
type Supported3 = {writeonly [K in keyof Tuple]: string};
type Supported4 = {readonly [K in keyof Array<number>]: string};
type Unsupported = {writeonly [K in keyof Array<number>]: string}; // Error
```

## Adoption {#toc-adoption}

To use mapped types, you need to upgrade your infrastructure so that it supports the syntax:

- `prettier`: 3
- `babel` with `flow-parser/babel-plugin`. See [our Babel guide](../tools/babel.md) for setup instructions.
- `eslint` with `flow-eslint`. See [our ESLint guide](../tools/eslint.md) for setup instructions.

## See Also {#toc-see-also}

- [Indexed Access Types](./indexed-access.md) — extracting property types, commonly used in mapped type bodies
- [Conditional Types](./conditional.md) — another advanced type feature for type-level logic
- [Generics](./generics.md) — mapped types rely on generic type parameters
- [Utility Types](./utilities.md) — built-in types like `Partial`, `Readonly`, and `Pick` (which mapped types generalize)
- [Modernizing Legacy Flow Syntax](../modernizing-legacy-syntax.md) — migrating the legacy `$ObjMap` / `$TupleMap` utilities that mapped types replace
