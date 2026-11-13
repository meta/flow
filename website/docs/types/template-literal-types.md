---
title: Template Literal Types
slug: /types/template-literal-types
description: "How to use template literal types in Flow to construct string types and extract parts of string patterns."
---

import {SinceVersion} from '../../components/VersionTags';

Template literal types construct string types from other types using backticks and `${...}` interpolations. Unlike JavaScript template literals, they do not construct string values at runtime.

## Basic Usage <SinceVersion version="0.339" /> {#toc-basic-usage}

```js flow-check
type Field = 'name' | 'email';
type EventName = `${Field}Changed`;

'nameChanged' as EventName; // OK
'emailChanged' as EventName; // OK
```

`EventName` is the union `'nameChanged' | 'emailChanged'`.

## When to use this {#toc-when-to-use}

Use template literal types for event names, string-based protocols, or strings with a required prefix or suffix. For unrelated constants, a plain [union of literal types](./literals.md) is simpler. Flow checks these annotations statically. They do not validate untrusted strings at runtime.

## Literal Interpolation and Unions {#toc-literal-interpolation}

Each interpolated literal contributes its string representation. Flow formats numbers by their value, not their source spelling, and omits the `n` suffix from bigints. Booleans contribute `'true'` or `'false'`; `null` becomes `'null'`, and `void` becomes `'undefined'`.

```js flow-check
type Label = `${'item'}-${0xff}`;
type State = `enabled:${true | false}`;
type Id = `id:${42n}`;
type Missing = `${null | void}`;

'item-255' as Label; // OK
'enabled:false' as State; // OK
'id:42' as Id; // OK
'undefined' as Missing; // OK
'item-0xff' as Label; // ERROR: the number's value is 255
```

When you put unions in multiple placeholders, Flow produces every combination of their members. This is called a *cross-product*:

```js flow-check
type Name = `${'a' | 'b'}-${'x' | 'y'}`;

'a-x' as Name; // OK
'b-y' as Name; // OK
'a-z' as Name; // ERROR
```

`Name` is exactly `'a-x' | 'a-y' | 'b-x' | 'b-y'`.

## Open-ended String Patterns {#toc-string-patterns}

The broad types `string`, `number`, and `bigint` describe sets of strings when used as placeholders. They do not make the template accept non-string values. A fixed prefix or suffix makes the template narrower than `string`, while `${string}` alone accepts any string literal.

```js flow-check
type DataKey = `data-${string}`;
type NumericId = `item-${number}`;
type IntegerId = `item-${bigint}`;

'data-name' as DataKey; // OK
'item-1.5' as NumericId; // OK
'item-42' as IntegerId; // OK
'other-name' as DataKey; // ERROR: wrong prefix
'item-many' as NumericId; // ERROR: not numeric text
'item-1.5' as IntegerId; // ERROR: not integer text
'item-42n' as IntegerId; // ERROR: the placeholder does not include n

declare const key: DataKey;
key as string; // OK
```

### Numeric caveats {#toc-numeric-caveats}

`${number}` is not a strict decimal, CSS, or range validator. It accepts exponent and radix forms, surrounding whitespace, and even a whitespace-only numeric portion. It rejects `NaN`, `Infinity`, and numeric separators.

`${bigint}` follows different lexical rules. It rejects fractions, exponents, whitespace, a leading `+`, decimal leading zeros, and separators.

```js flow-check
type NumericText = `${number}`;

'1e3' as NumericText; // OK
'0xff' as NumericText; // OK
' ' as NumericText; // OK
'NaN' as NumericText; // ERROR
'Infinity' as NumericText; // ERROR
'1_000' as NumericText; // ERROR
```

## Generic Inference {#toc-generic-inference}

Flow can infer a [type parameter](./generics.md) from the part of a string matched by a placeholder:

```js flow-check
declare function stripPrefix<T extends string>(key: `data-${T}`): T;

stripPrefix('data-name') as 'name'; // OK
stripPrefix('data-name') as 'email'; // ERROR
```

The declaration says that the function returns the suffix. You still need to implement that behavior at runtime.

## Conditional Inference {#toc-conditional-inference}

In a [conditional type](./conditional.md#toc-infer-type), `infer` extracts the part of a string that matches a placeholder. The false branch determines the result for strings that do not match:

```js flow-check
type DataName<S extends string> = S extends `data-${infer Name}` ? Name : empty;

'name' as DataName<'data-name'>; // OK
'email' as DataName<'data-name'>; // ERROR
'name' as DataName<'name'>; // ERROR: the result is empty
```

## String Casing {#toc-string-casing}

You can combine templates with the existing [`Uppercase`, `Lowercase`, `Capitalize`, and `Uncapitalize` utilities](./utilities.md#toc-string-casing):

```js flow-check
type EventName = `on${Capitalize<'click' | 'change'>}`;

'onClick' as EventName; // OK
'onChange' as EventName; // OK
'onclick' as EventName; // ERROR
```

## Templates as Mapped Keys {#toc-mapped-keys}

Use a template literal type in a mapped type's `as` clause to rename properties, such as turning `mark` into `onmark`. The [key remapping](./mapped-types.md#toc-key-remapping) example keeps each original property's value type. See [filtering keys](./mapped-types.md#toc-filtering-keys) when the source also has symbol keys.

## Limits {#toc-limits}

Placeholders must be compatible with `string | number | bigint | boolean | null | void`. Flow does not support object-like types or `unknown` as placeholders:

```js flow-check
type ObjectText = `${{name: string}}`; // ERROR
type UnknownText = `${unknown}`; // ERROR
```

Flow will not expand a union cross-product with more than 10,000 combinations. Keep combinations small, or generate an explicit union ahead of time to avoid expanding a large set during type checking.

## Adoption {#toc-adoption}

To use template literal types, set `experimental.tslib_syntax=true` in `.flowconfig` and use Flow and flow-parser 0.339.0+ and [Prettier 3.10.0+ with its built-in `flow` parser](../tools/prettier.md).

## See Also {#toc-see-also}

- [Literal Types](./literals.md) — specific primitive values as types
- [Generics](./generics.md) — parameterizing string patterns
- [Conditional Types](./conditional.md#toc-infer-type) — extracting types with `infer`
- [String Utilities](./utilities.md#toc-string-casing) — transforming casing
- [Mapped Types](./mapped-types.md) — constructing object types from keys
