---
title: Type Variance
slug: /lang/variance
description: "How type variance (covariance, contravariance, invariance) works in Flow's type system."
---

Variance is a topic that comes up fairly often in type systems. It is used to determine
how type parameters behave with respect to subtyping.

First we'll setup a couple of classes that extend one another.

```js flow-check
class Noun {}
class City extends Noun {}
class SanFrancisco extends City {}
```

We saw in the section on [generic types](../types/generics.md#toc-variance-sigils)
that it is possible to
use variance keywords to describe when a type parameter is used in an output position,
when it is used in an input position, and when it is used in either one.

Here we'll dive deeper into each one of these cases.

:::info TypeScript comparison
Flow defaults to stricter variance than TypeScript at every position where they diverge — mutable object properties, mutable arrays, generic type parameters, and class method parameters. Variance keywords mostly align (`readonly` on properties, `in` / `out` on type parameters); `writeonly` is Flow-only, and TS's `<in out T>` has no Flow counterpart because Flow's default is already invariance. See [Flow and TypeScript variance comparison](../flow-vs-typescript.md#toc-variance) for the per-position breakdown.
:::

## Covariance {#toc-covariance}

Consider for example the type
```js flow-check
type CovariantOf<X> = {
  readonly prop: X;
  getter(): X;
}
```
Here, `X` appears strictly in *output* positions: it is used to read out information
from objects `o` of type `CovariantOf<X>`, either through property accesses `o.prop`,
or through calls to `o.getter()`.

Notably, there is no way to input data through the reference to the object `o`,
given that `prop` is a readonly property.

When these conditions hold, we can use the `out` keyword to annotate `X` in the definition
of `CovariantOf`:
```js flow-check
type CovariantOf<out X> = {
  readonly prop: X;
  getter(): X;
}
```

These conditions have important implications on the way that we can treat an object
of type `CovariantOf<T>` with respect to subtyping. As a reminder, subtyping rules
help us answer the question: "given some context that expects values of type
`T`, is it safe to pass in values of type `S`?" If this is the case, then `S` is a
subtype of `T`.

Using our `CovariantOf` definition, and given that `City` is a subtype of `Noun`, it is
also the case that `CovariantOf<City>` is a subtype of `CovariantOf<Noun>`. Indeed
* it is safe to *read* a property `prop` of type `City` when a property
of type `Noun` is expected, and
* it is safe to *return* values of type `City` when calling `getter()`, when
values of type `Noun` are expected.

Combining these two, it will always be safe to use `CovariantOf<City>` whenever a
`CovariantOf<Noun>` is expected.

A commonly used example where covariance is used is [`ReadonlyArray<T>`](../types/arrays.md#toc-readonlyarray).
Just like with the `prop` property, one cannot use a `ReadonlyArray` reference to write data
to an array. This allows more flexible subtyping rules: Flow only needs to prove that
`S` is a subtype of `T` to determine that `ReadonlyArray<S>` is also a subtype
of `ReadonlyArray<T>`.

### The `this` type is restricted to covariant positions {#toc-this-covariant}

The `this` type follows the same rule. It's allowed in covariant positions — method return types and `readonly` fields — but rejected in method parameters and mutable fields, with `[incompatible-variance]`:

```js flow-check
class Builder {
  add(x: number): this { return this; } // OK: return type is covariant
  readonly origin: this | null = null; // OK: readonly field is covariant
  takesSelf(other: this): void {} // ERROR: input position
  parent: this | null = null; // ERROR: invariant field
}
```

This falls out of the same model that makes [mutable object properties](#toc-invariance) invariant: if `parent: this | null` were allowed on a base `Builder`, a subclass instance could be passed as a `Builder`, a bare `Builder` written into its `parent`, and a later `sb.parent.subclassMethod()` would type-check but crash at runtime — so Flow rejects the field outright.

The rewrite when you hit this is to name the class explicitly in the input or field position (`other: Builder`, `parent: Builder | null`) and accept the loss of the subclass type at that slot — or mark the field `readonly` so the position becomes covariant.


## Invariance {#toc-invariance}

Let's see what happens if we try to relax the restrictions on the use of `X` and make,
for example, `prop` be a read-write property. We arrive at the type definition
```js flow-check
type NonCovariantOf<X> = {
  prop: X;
  getter(): X;
};
```
Let's also declare a variable `nonCovariantCity` of type `NonCovariantOf<City>`.
Now, it is not safe to consider `nonCovariantCity` as an object of type `NonCovariantOf<Noun>`.
Were we allowed to do this, we could write a `Noun` into `prop`, invalidating the original type.
Flow catches this:
```js flow-check
class Noun {}
class City extends Noun {}

type NonCovariantOf<X> = {
  prop: X;
  getter(): X;
};

declare const nonCovariantCity: NonCovariantOf<City>;
const nonCovariantNoun: NonCovariantOf<Noun> = nonCovariantCity; // Error!
```


What distinguishes `NonCovariantOf` from the `CovariantOf` definition is that type parameter `X` is used both
in input and output positions, as it is being used to both read and write to
property `prop`. Such a type parameter is called *invariant* and is the default case
of variance, thus requiring no prepending keyword:
```js flow-check
type InvariantOf<X> = {
  prop: X;
  getter(): X;
  setter(x: X): void;
};
```
Assuming a variable `invariantCity` of type `InvariantOf<City>`,
it is *not* safe to use `invariantCity` in a context where:
- an `InvariantOf<Noun>` is needed, because we should not be able to write a `Noun` to property
`prop`.
- an `InvariantOf<SanFrancisco>` is needed, because reading `prop` could return a `City` which
may not be `SanFrancisco`.

In other words, `InvariantOf<City>` is neither a subtype of `InvariantOf<Noun>` nor
a subtype of `InvariantOf<SanFrancisco>`.


## Contravariance {#toc-contravariance}

When a type parameter is only used in *input* positions, we say that it is used in
a *contravariant* way. This means that it only appears in positions through which
we write data to the structure. We use the `in` keyword to annotate such a type
parameter, paired with the `writeonly` keyword to mark a contravariant property:

```js flow-check
type ContravariantOf<in X> = {
  writeonly prop: X;
  setter(x: X): void;
};
```
Common contravariant positions are write-only properties and "setter" functions.

An object of type `ContravariantOf<City>` can be used whenever an object of type
`ContravariantOf<SanFrancisco>` is expected, but not when a `ContravariantOf<Noun>` is.
In other words, `ContravariantOf<City>` is a subtype of `ContravariantOf<SanFrancisco>`, but not
`ContravariantOf<Noun>`.
This is because it is fine to write `SanFrancisco` into a property that can have any `City` written
to, but it is not safe to write just any `Noun`.

### Function parameter contravariance {#toc-function-parameter-contravariance}

Function parameters are always in an input (contravariant) position. This means a function that accepts
a more specific type cannot substitute for one that accepts a more general type. This commonly
surprises people when passing callbacks with exact object types:

```js flow-check
type Exact = {foo: string};
type Inexact = {foo: string, ...};

declare function acceptsExact(item: Exact): void;
declare function takesCallback(cb: (item: Inexact) => void): void;

takesCallback(acceptsExact); // Error!
```

This error occurs because `takesCallback` may call `cb` with an object that has extra properties
(since `Inexact` allows them). The callback `acceptsExact` only accepts objects with exactly `{foo: string}`,
so passing an inexact object to it would be unsound. Even though passing an exact object *directly*
to a function expecting an inexact one works (an exact type is a subtype of a compatible inexact type),
the function types are flipped due to contravariance.

## Input and Output Positions {#toc-input-output-positions}

Flow's error messages refer to "input positions" and "output positions" when
reporting variance errors. These terms correspond directly to the variance
concepts described above:

- An **output position** is a place where a value is *read out* of a type: return
  types, read-only properties, getter results. A type parameter marked with `out`
  (covariant) can only appear in output positions.
- An **input position** is a place where a value is *written into* a type: function
  parameters, write-only properties, setter arguments. A type parameter marked
  with `in` (contravariant) can only appear in input positions.
- A type parameter with no keyword (invariant) can appear in both input and output
  positions.

When you see an error like "Cannot use `T` in an input position because `T` is
expected to occur only in output positions," it means you have a type parameter
marked as covariant (`out T`) but you are using it somewhere that writes a value
in, such as a function parameter. Flow reports this as `[incompatible-variance]`:

```js flow-check
type Box<out T> = {
  get(): T;
  set(val: T): void; // Error [incompatible-variance]: T is in an input position but is expected only in output positions
};
```

The fix depends on your intent: if the type genuinely needs to both read and
write `T`, remove the `out` keyword to make `T` invariant. If the type should only
produce values of type `T` (never accept them), remove the setter.

## See Also {#toc-see-also}

- [Subtypes](./subtypes.md) — the underlying subtyping relationships that variance builds on
- [Generics](../types/generics.md) — variance keywords on generic type parameters
- [Arrays](../types/arrays.md) — `ReadonlyArray` (covariant) vs `Array` (invariant)
- [Interfaces](../types/interfaces.md) — covariant and contravariant interface properties
- [Objects](../types/objects.md) — read-only and write-only object properties
- [Modernizing Legacy Flow Syntax](../modernizing-legacy-syntax.md) — migrating `+` / `-` variance sigils to the `readonly` / `writeonly` and `in` / `out` keywords
