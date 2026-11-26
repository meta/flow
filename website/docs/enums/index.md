---
title: Flow Enums
description: "Define a fixed set of constants which create their own type. Exhaustively checked in switch statements."
slug: /enums
---

Flow Enums define a fixed set of constants which create their own type.

Unlike other features of Flow, Flow Enums exist as values at runtime, as well as existing as types.

To get started, see [how to enable Flow Enums in your project](#toc-enabling-enums).

:::info TypeScript comparison
Flow Enums and TypeScript's `enum` look superficially similar but differ in many details — Flow Enums are nominally typed, enforce exhaustive checking, block implicit coercion, require explicit initialization for number enums, supply methods like `.cast`, and have more definition restrictions. See [Flow and TypeScript enums comparison](../flow-vs-typescript.md#toc-flow-enums) for more.
:::

## Benefits {#toc-benefits}
Enums provide several benefits over existing patterns:

* Reduce repetition: Enum declarations provide both the type and the value of the enum.
* Improve Flow performance: Enums are guaranteed to have good type-checking performance,
  unlike unions which may be expensive to type-check in certain situations.
* Enable new functionality: Enums come with a `cast` [method](./using-enums.md#toc-methods), which converts from a primitive type to an enum type safely.
* Enhance safety: Enums define their own type which does not implicitly coerce to and from other types (e.g. from `string`s),
  and are required to be [exhaustively checked in switch statements](./using-enums.md#toc-exhaustively-checking-enums-with-a-switch). These properties can help prevent logic bugs.


## Quickstart {#toc-quickstart}

#### [Defining enums](./defining-enums.md) {#toc-defining-enums-defining-enums}
An enum named `Status` with three members: `Active`, `Paused`, and `Off`.

```js flow-check
enum Status {
  Active,
  Paused,
  Off,
}
```
By default, enums define members with string values which mirror their names. You can also explicitly set values:
```js flow-check
enum Status {
  Active = 'active',
  Paused = 'paused',
  Off = 'off',
}
```
You can use numbers as well:
```js flow-check
enum Status {
  Active = 1,
  Paused = 2,
  Off = 3,
}
```
Values must be unique, literals, and all of the same type. Check out the [full docs on defining enums](./defining-enums.md) to learn more.


#### [Using enums](./using-enums.md) {#toc-using-enums-using-enums}
Access enum members with dot access, use the enum name as a type annotation, and cast between an enum and its representation type:

```js flow-check
enum Status {
  Active,
  Paused,
  Off,
}
declare const someString: string;

// Access an enum member with dot access:
Status.Active;

// Use the enum name as a type annotation:
const status: Status = Status.Active;

// Cast from the representation type (in this case, a `string`) to the enum type:
const maybeStatus: Status | undefined = Status.cast(someString);

// Provide a default value with the `??` operator:
const statusWithDefault: Status = Status.cast(someString) ?? Status.Off;

// Cast an enum type to its representation type (must be done explicitly):
status as string;
```
Read more about the [other methods enums provide](./using-enums.md#toc-methods), including `isValid`, `members`, and `getName`.

Checks of enums in `switch` statements are exhaustive - we ensure you check all members:
```js flow-check
enum Status {
  Active,
  Paused,
  Off,
}
const status: Status = Status.Active;

switch (status) { // Error: incomplete exhaustive check
  case Status.Active:  break;
  case Status.Paused: break;
  // We forgot to add `case: Status.Off:` here, resulting in the error.
  // Using `default:` would also work to check all remaining members.
}
```
Read more about [exhaustively checking enums](./using-enums.md#toc-exhaustively-checking-enums-with-a-switch).

Check out the [the full docs on using enums](./using-enums.md) to learn more.


## Enabling Enums {#toc-enabling-enums}

To enable Flow Enums in your project:

1. Enabled by default since Flow v0.317. Before that, add `enums=true` in your `.flowconfig` under the `[options]` heading.
2. Add the Babel transform [babel-plugin-transform-flow-enums](https://www.npmjs.com/package/babel-plugin-transform-flow-enums) to your dev dependencies and Babel config. It converts enum declarations into calls to the runtime.
3. Add the runtime package [flow-enums-runtime](https://www.npmjs.com/package/flow-enums-runtime) to your production dependencies.

We also recommend enabling these ESLint rules to prevent common `switch` statement issues:
- [no-fallthrough](https://eslint.org/docs/rules/no-fallthrough): Prevents accidentally forgetting `break` statements.
- [no-case-declarations](https://eslint.org/docs/rules/no-case-declarations): Prevents lexically scoped declarations in switch cases without a block.

Additional Flow Enums ESLint rules are available in [eslint-plugin-fb-flow](https://www.npmjs.com/package/eslint-plugin-fb-flow).


## When to use Flow Enums {#toc-when-to-use-flow-enums}
If you previously defined a union type of literals, you can use an enum to define that type instead. Instead of

```js flow-check
type Status =
  | 'Active'
  | 'Paused'
  | 'Off';

const x: Status = 'Active';
```

or
```js flow-check
const Status = Object.freeze({
  Active: 'Active',
  Paused: 'Paused',
  Off: 'Off',
});
type StatusType = keyof typeof Status;
const x: StatusType = Status.Active;
```

you can use:
```js flow-check
enum Status {
  Active,
  Paused,
  Off,
}
const x: Status = Status.Active;
```

See [migrating from legacy patterns](./migrating-legacy-patterns.md) to learn more about migrating legacy JavaScript enum patterns to Flow Enums.


## When to not use Flow Enums {#toc-when-to-not-use-flow-enums}
Enums are designed to cover many use cases and exhibit certain benefits. The design makes a variety of trade-offs to make this happen, and in certain situations,
these trade-offs might not be right for you. In those cases, you can continue to use existing patterns to satisfy your use cases.
[Read more about those situations](./using-enums.md#toc-when-to-not-use-enums).
