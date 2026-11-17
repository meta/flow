---
title: Generics
slug: /types/generics
description: "How to use generic (polymorphic) types in Flow to write reusable, type-safe functions, classes, and type aliases."
---

Generics (or polymorphic types) let you write functions, classes, and type aliases that work with any type while preserving type relationships.

```js flow-check
function identity<T>(value: T): T {
  return value;
}
```

:::info TypeScript comparison
Two divergences from TypeScript: generic type arguments [cannot be omitted](../flow-vs-typescript.md#toc-generic-default-omission) when every parameter has a default — write `Foo<>`, not `Foo`. And type parameters are [invariant by default](../flow-vs-typescript.md#toc-variance-generics), so widening `C<number>` to `C<number | string>` is rejected; opt into variance with `out T` / `in T`. TS infers variance from usage with compatibility exceptions, which can leave read-write fields with weaker static guarantees.
:::

## When to use this {#toc-when-to-use}

Use generics when a function, class, or type alias needs to work across multiple types while preserving the relationship between inputs and outputs. If the type doesn't matter and you don't need to track it, use [`unknown`](./unknown.md) instead. If there are a fixed number of specific types, a [union](./unions.md) may be clearer.

> **Warning:** Flow does not infer generic types. If you want something to have a
generic type, **annotate it**. Otherwise, Flow may infer a type that is less
polymorphic than you expect.

## Syntax of generics {#toc-syntax-of-generics}

There are a number of different places where generic types appear in syntax.

### Functions with generics {#toc-functions-with-generics}

Functions can create generics by adding the type parameter list `<T>` before
the function parameter list.

You can use generics in the same places you'd add any other type in a function
(parameter or return types).

```js flow-check
function method<T>(param: T): T {
  return param;
}

const f = function<T>(param: T): T {
  return param;
}
```

### Function types with generics {#toc-function-types-with-generics}

Function types can create generics in the same way as normal functions, by
adding the type parameter list `<T>` before the function type parameter list.

You can use generics in the same places you'd add any other type in a function
type (parameter or return types).

```js
<T>(param: T) => T
```

Which then gets used as its own type.

```js flow-check
function method(func: <T>(param: T) => T) {
  // ...
}
```

### Classes with generics {#toc-classes-with-generics}

Classes can create generics by placing the type parameter list before the body
of the class.

```js flow-check
class Item<T> {
  // ...
}
```

You can use generics in the same places you'd add any other type in a class
(property types and method parameter/return types).

```js flow-check
class Item<T> {
  prop: T;

  constructor(param: T) {
    this.prop = param;
  }

  method(): T {
    return this.prop;
  }
}
```

### Type aliases with generics {#toc-type-aliases-with-generics}

```js flow-check
type Item<T> = {
  foo: T,
  bar: T,
};
```

### Interfaces with generics {#toc-interfaces-with-generics}

```js flow-check
interface Item<T> {
  foo: T,
  bar: T,
}
```

### Supplying Type Arguments to Callables {#toc-supplying-type-arguments-to-callables}

You can give callable entities type arguments for their generics directly in the call:

```js flow-check
function doSomething<T>(param: T): T {
  // ...
  return param;
}

doSomething<number>(3);
```

You can also give generic classes type arguments directly in the `new` expression:
```js flow-check
class GenericClass<T> {}
const c = new GenericClass<number>();
```

If you only want to specify some of the type arguments, you can use `_` to let flow infer a type for you:

```js flow-check
class GenericClass<T, U=string, V=number>{}
const c = new GenericClass<boolean, _, string>();
```

> **Warning:** For performance purposes, we always recommend you annotate with
concrete arguments when you can. `_` is not unsafe, but it is slower than explicitly
specifying the type arguments.

## Behavior of generics {#toc-behavior-of-generics}

### Generics act like variables {#toc-generics-act-like-variables}

Generic types work a lot like variables or function parameters except that they
are used for types. You can use them whenever they are in scope.

```js flow-check
function constant<T>(value: T): () => T {
  return function(): T {
    return value;
  };
}
```

### Create as many generics as you need {#toc-create-as-many-generics-as-you-need}

You can have as many of these generics as you need in the type parameter list,
naming them whatever you want:

```js flow-check
function identity<One, Two, Three>(one: One, two: Two, three: Three) {
  // ...
}
```

### Generics track values around {#toc-generics-track-values-around}

When using a generic type for a value, Flow will track the value and make sure
that you aren't replacing it with something else.

```js flow-check
function identity<T>(value: T): T {
  return "foo"; // Error!
}
```
```js flow-check
function identity<T>(value: T): T {
  value = "foo"; // Error!
  return value;  // Error!
}
```

Flow tracks the specific type of the value you pass through a generic, letting
you use it later.

```js flow-check
function identity<T>(value: T): T {
  return value;
}

let one: 1 = identity(1);
let two: 2 = identity(2);
let three: 3 = identity(42); // Error
```

### Adding types to generics {#toc-adding-types-to-generics}

Similar to `unknown`, generics have an "unknown" type. You're not allowed to use
a generic as if it were a specific type.

```js flow-check
function logFoo<T>(obj: T): T {
  console.log(obj.foo); // Error!
  return obj;
}
```

You could refine the type, but the generic will still allow any type to be
passed in.

```js flow-check
function logFoo<T>(obj: T): T {
  if (obj && obj.foo) {
    console.log(obj.foo); // Works.
  }
  return obj;
}

logFoo({ foo: 'foo', bar: 'bar' });  // Works.
logFoo({ bar: 'bar' }); // Works. :(
```

Instead, you could add a bound to your generic using the `extends` keyword.

```js flow-check
function logFoo<T extends {foo: string, ...}>(obj: T): T {
  console.log(obj.foo); // Works!
  return obj;
}

logFoo({ foo: 'foo', bar: 'bar' });  // Works!
logFoo({ bar: 'bar' }); // Error!
```

This way you can keep the behavior of generics while only allowing certain
types to be used.

```js flow-check
function identity<T extends number>(value: T): T {
  return value;
}

let one: 1 = identity(1);
let two: 2 = identity(2);
let three: "three" = identity("three"); // Error!
```

### Generic types act as bounds {#toc-generic-types-act-as-bounds}

```js flow-check
function identity<T>(val: T): T {
  return val;
}

let foo: 'foo' = 'foo';           // Works!
let bar: 'bar' = identity('bar'); // Works!
```

In Flow, most of the time when you pass one type into another you lose the
original type. So that when you pass a specific type into a less specific one
Flow "forgets" it was once something more specific.

```js flow-check
function identity(val: string): string {
  return val;
}

let foo: 'foo' = 'foo';           // Works!
let bar: 'bar' = identity('bar'); // Error!
```

Generics allow you to hold onto the more specific type while adding a
constraint. In this way types on generics act as "bounds".

```js flow-check
function identity<T extends string>(val: T): T {
  return val;
}

let foo: 'foo' = 'foo';           // Works!
let bar: 'bar' = identity('bar'); // Works!
```

Note that when you have a value with a bound generic type, you can't use it as
if it were a more specific type.

```js flow-check
function identity<T extends string>(val: T): T {
  let str: string = val; // Works!
  let bar: 'bar'  = val; // Error!
  return val;
}

identity('bar');
```

:::info Legacy bound syntax
The `extends` keyword for generic bounds is enabled by default as of Flow 0.302. Older code may use the legacy colon syntax (e.g. `<T: string>` instead of `<T extends string>`). See [Modernizing Legacy Syntax](../modernizing-legacy-syntax.md#toc-deprecated-syntax) for migration details.
:::

### Parameterized generics {#toc-parameterized-generics}

Generics sometimes allow you to pass types in like arguments to a function.
These are known as parameterized generics (or parametric polymorphism).

For example, a type alias with a generic is parameterized. When you go to use
it you will have to provide a type argument.

```js flow-check
type Item<T> = {
  prop: T,
}

let item: Item<string> = {
  prop: "value"
};
```

You can think of this like passing arguments to a function, only the return
value is a type that you can use.

Classes (when being used as a type), type aliases, and interfaces all require
that you pass type arguments. Functions and function types do not have
parameterized generics.

***Classes***

```js flow-check
class Item<T> {
  prop: T;
  constructor(param: T) {
    this.prop = param;
  }
}

let item1: Item<number> = new Item(42); // Works!
let item2: Item = new Item(42); // Error!
```

***Type Aliases***

```js flow-check
type Item<T> = {
  prop: T,
};

let item1: Item<number> = { prop: 42 }; // Works!
let item2: Item = { prop: 42 }; // Error!
```

***Interfaces***

```js flow-check
interface HasProp<T> {
  prop: T,
}

class Item {
  prop: string;
}

Item.prototype as HasProp<string>; // Works!
Item.prototype as HasProp; // Error!
```

### Adding defaults to parameterized generics {#toc-adding-defaults-to-parameterized-generics}

You can also provide defaults for parameterized generics just like parameters
of a function.

```js flow-check
type Item<T extends number = 1> = {
  prop: T,
};

let foo: Item<> = { prop: 1 };
let bar: Item<2> = { prop: 2 };
```

You must always include the brackets `<>` when using the type (just like
parentheses for a function call). Writing the bare name `Item` (without `<>`) errors with `[missing-type-arg]` even when every parameter has a default. The bare name is reserved for the unapplied type constructor — applying it always requires explicit type arguments or an empty `<>` to fall back on defaults.

### Variance Keywords {#toc-variance-sigils}

You can also specify the subtyping behavior of a generic via variance keywords.
By default, generics behave invariantly, but you may add an `out` keyword
before their declaration to make them behave covariantly, or an `in` keyword
before their declaration to make them behave contravariantly. See
[our docs on variance](../lang/variance.md) for a more information on variance
in Flow.

Variance keywords allow you to be more specific about how you intend to
use your generics, giving Flow the power to do more precise type checking.
For example, you may want this relationship to hold:

```js flow-check
type GenericBox<out T> = T;

const x: GenericBox<number> = 3;
x as GenericBox<number| string>;
```

The example above could not be accomplished without the `out` variance keyword:

```js flow-check
type GenericBoxError<T> = T;

const x: GenericBoxError<number> = 3;
x as GenericBoxError<number| string>; // Error: number | string is not compatible with number.
```

Note that if you annotate your generic with variance keywords then Flow will
check to make sure those types only appear in positions that make sense for
that variance keyword. For example, you cannot declare a generic type parameter
to behave covariantly and use it in a contravariant position:

```js flow-check
type NotActuallyCovariant<out T> = (x: T) => void; // Error
```

:::info Legacy sigil syntax
The `out` and `in` variance keywords are enabled by default as of Flow 0.315. Older code may use the legacy `+` and `-` sigils (e.g. `<+T>` instead of `<out T>`, `<-T>` instead of `<in T>`); these are still recognized but deprecated. See [Modernizing Legacy Syntax](../modernizing-legacy-syntax.md#toc-deprecated-syntax) for migration details.
:::

## See Also {#toc-see-also}

- [Type Aliases](./aliases.md) — defining named types, often combined with generics
- [Functions](./functions.md) — using generics in function signatures
- [Variance](../lang/variance.md) — how covariance and contravariance interact with generics
- [Conditional Types](./conditional.md) — types computed from generic type parameters
- [Mapped Types](./mapped-types.md) — transforming object types using generics
- [Modernizing Legacy Flow Syntax](../modernizing-legacy-syntax.md) — migrating legacy `<T: Bound>` bounds and `+T` / `-T` variance sigils to modern keyword forms
