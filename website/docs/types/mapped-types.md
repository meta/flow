---
title: Mapped Types
slug: /types/mapped-types
description: "How to use mapped types in Flow to transform object types or tuple types by iterating over their keys."
---

import {SinceVersion} from '../../components/VersionTags';

Mapped types transform object types by iterating over their keys and producing new property types.

```js flow-check
type O = {foo: number, bar: string};
type ReadOnly = {readonly [key in keyof O]: O[key]};
```

:::info TypeScript comparison
Flow shares TypeScript's `keyof`-based iteration, homomorphic `{[K in keyof T]: ...}` form, and distributivity over object unions. Flow's source restrictions, exactness, and variance rules still apply.
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

## Key remapping via `as` <SinceVersion version="0.339" /> {#toc-key-remapping}

Add an `as` clause after the source to choose a new key for each property. You can use [template literal types](./template-literal-types.md) to construct names:

```js flow-check
type Events = {mark: number, measure: string};
type Handlers = {
  [K in keyof Events as `on${K}`]: (event: Events[K]) => void
};

declare const handlers: Handlers;
handlers.onmark(1); // OK
handlers.onmeasure('start'); // OK
handlers.onmark('start'); // ERROR: the original mark value type is number
handlers.mark; // ERROR: the key is now onmark
```

`Handlers` has `onmark` and `onmeasure` properties. The value expression still uses the **original** key `K`. For example, `Events[K]` reads `Events['mark']`, not `Events['onmark']`.

The `as` expression can also be a conditional type or an [indexed access type](./indexed-access.md). It must return a valid property-key type (`string | number | symbol`) or return `empty` to omit the key. The source still follows the [mapped type source rules](#toc-mapped-type-sources). You cannot use an arbitrary union of objects as the source.

### Filtering keys {#toc-filtering-keys}

Return [`empty`](./empty.md) from the `as` clause to remove a property:

```js flow-check
type PickStrings<T> = {
  [K in keyof T as T[K] extends string ? K : empty]: T[K]
};
type Labels = PickStrings<{count: number, name: string, description: string}>;

declare const labels: Labels;
labels.name as string; // OK
labels.description as string; // OK
labels.count; // ERROR: the number-valued property was filtered out
```

Remapping can preserve or produce [unique-symbol keys](./primitives.md#toc-unique-symbols). If the source also has symbols and you want to rename only its string keys, filter with `K extends string` before interpolating `K`:

```js flow-check
const id = Symbol('id');
type Events = {[id]: number, change: string};
type StringHandlers<T> = {
  [K in keyof T as K extends string ? `on${K}` : empty]: (event: T[K]) => void
};

declare const handlers: StringHandlers<Events>;
handlers.onchange('new'); // OK
handlers[id]; // ERROR: symbol keys were filtered out
```

### Collisions and object shape {#toc-remapping-collisions}

If multiple source keys map to the same destination, Flow combines their value types into a union:

```js flow-check
type Source = {count: number, label: string};
type Combined = {[K in keyof Source as 'value']: Source[K]};

declare const combined: Combined;
combined.value as number | string; // OK
combined.value as number; // ERROR: the value can also be a string
```

Without explicit property modifiers, a merged property is optional if any contributor is optional. It remains read-only if any contributor is `readonly`. You can omit an optional merged property, and reading it includes `void` in the value type. Use the [property modifiers](#toc-property-modifiers) to change these attributes.

Identity remapping (`as K`) over an inline `keyof` preserves an inexact source's open shape and dictionary indexers. A nonidentity remapping produces an exact object when no destination indexer survives. Dropping every indexer and property with `as empty` produces `{}`. Do not assume that remapping an open-ended string pattern behaves the same as remapping a finite union of names.

### Array and tuple restriction {#toc-remapping-arrays}

Flow does not support key remapping on array or tuple sources, **even for the identity `as K`**. The existing [mapping without an `as` clause](#toc-mapped-type-on-arrays) still works:

```js flow-check
type Tuple = [string, number];
type ArraySource = Array<number>;
type MappedTuple = {[K in keyof Tuple]: Tuple[K]};
type MappedArray = {[K in keyof ArraySource]: ArraySource[K]};
type RemappedTuple = {[K in keyof Tuple as K]: Tuple[K]}; // ERROR: tuple remapping
type RemappedArray = {[K in keyof ArraySource as K]: ArraySource[K]}; // ERROR: array remapping
```

### Adoption {#toc-key-remapping-adoption}

To use key remapping and signed `+readonly`/`-readonly` modifiers, set `experimental.tslib_syntax=true` in `.flowconfig` and use Flow and flow-parser 0.339.0+ and [Prettier 3.10.0+ with its built-in `flow` parser](../tools/prettier.md).

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

### Adding and removing `readonly` <SinceVersion version="0.339" /> {#toc-signed-readonly}

`+readonly` works like plain `readonly`, making every output property read-only. `-readonly` removes a source property's `readonly` modifier. An already-mutable property stays mutable:

```js flow-check
type ReadOnly<T> = {+readonly [K in keyof T]: T[K]};
type Mutable<T> = {-readonly [K in keyof T]: T[K]};
type Settings = {volume: number};

declare const readOnly: ReadOnly<Settings>;
readOnly.volume as number; // OK
readOnly.volume = 1; // ERROR: the property is read-only

declare const mutable: Mutable<ReadOnly<Settings>>;
mutable.volume as number; // OK
mutable.volume = 1; // OK: readonly was removed
```

You can combine `-readonly` with key remapping and use the existing `-?` modifier to remove optionality:

```js flow-check
type MutableRequired<T> = {-readonly [K in keyof T as K]-?: T[K]};
type Settings = {readonly volume?: number};

declare const settings: MutableRequired<Settings>;
settings.volume as number; // OK: the property is required
settings.volume = 1; // OK: the property is mutable
({}) as MutableRequired<Settings>; // ERROR: volume is now required
```

These modifiers transform types, not runtime objects. They do not freeze, unfreeze, or copy an object. Removing `readonly` does not let you assign a read-only value directly to a mutable type. You still need to provide a value compatible with the result.

`-readonly` does not remove `writeonly`. If the value expression uses `T[K]`, the input properties must still be readable:

```js flow-check
type Mutable<T> = {-readonly [K in keyof T]: T[K]};
type Output = {writeonly value: number};

declare const output: Mutable<Output>; // ERROR: T[K] cannot read the input property
output.value as number; // ERROR: the property is still write-only
output.value = 1; // OK: writes remain allowed
```

### Adding and removing `writeonly` <SinceVersion version="0.339" /> {#toc-signed-writeonly}

`+writeonly` works like plain `writeonly`, making every output property write-only. `-writeonly` removes write-only variance, making those output properties read-write. It does not remove an existing `readonly` modifier.

```js flow-check
// Add writeonly to readable input properties.
type WriteOnly<T> = {+writeonly [K in keyof T]: T[K]};
type Settings = {volume: number};
declare const writeOnly: WriteOnly<Settings>;
writeOnly.volume as number; // ERROR: write-only
writeOnly.volume = 1; // OK

// Remove writeonly without reading a write-only input property.
type ReadWrite<T> = {-writeonly [K in keyof T]: number};
declare const readWrite: ReadWrite<WriteOnly<Settings>>;
readWrite.volume as number; // OK
readWrite.volume = 1; // OK
```

Removing `writeonly` affects output modifiers, not what the value expression can read. `T[K]` still cannot read a write-only input property, so `ReadWrite` intentionally uses `number` instead.

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
