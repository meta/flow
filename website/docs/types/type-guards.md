---
title: Type Guards
slug: /types/type-guards
description: "How to write type guard functions in Flow to create custom type predicates for use in refinements."
---

Type guards are functions whose return type is annotated with `param is PredicateType`, narrowing the parameter's type when they return `true`.

```js
function predicate(param: InputType): param is PredicateType {
  return <some_expression>;
}
```

The type of this function can also be written as a type annotation:
```js
type PredicateFunc = (param: InputType) => param is PredicateType;
```

:::info TypeScript comparison
Unlike TypeScript, Flow validates the body of an `x is T` predicate function in both directions (positive and negative) and forbids parameter reassignment along the path to `return`. TypeScript checks that the predicate type relates to the parameter type but does not check the body. When only the positive direction holds, Flow has `implies x is T` (one-sided type guards) — a Flow-only feature with no TS equivalent. See [Flow's type guard bodies are validated](../flow-vs-typescript.md#toc-type-guard-validation) for the full comparison.
:::

## When to use this {#toc-when-to-use}

Use type guards when built-in [refinements](../lang/refinements.md) (`typeof`, `instanceof`, equality checks) aren't sufficient to narrow a type. They let you encapsulate custom narrowing logic in a reusable function — for example, filtering arrays to a specific subtype with `.filter()`, or checking domain-specific invariants that Flow can't infer on its own. Use [one-sided type guards](#toc-one-sided-type-guards) (`implies`) when the predicate is only meaningful in the true case.

## Basic Usage {#toc-basic-usage}

Let's see a simple example where we define a type guard function and then use it to refine some values.

### Defining a type guard function

```js flow-check
type A = { type: "A"; data: string };
type B = { type: "B"; data: number };
type AorB = A | B;

function isA(value: AorB): value is A {
  return value.type === "A";
}
```
We have defined a data type `AorB` that is a disjoint union of two types `A` and `B` that each have a property `type` used as tag.

We have also written a *user defined type guard* function `isA` defined over objects of type `AorB`. This function returns `true` when the value of the `type` property of its input is `"A"`. Using the definitions of `A` and `B`, Flow can prove that when the value of `type` is `"A"` then the type of `value` will be `A`.

### Using a type guard function to refine values

Functions that have a declared type guard can be used to refine values in conditionals. In the example above, we can use `isA` to refine a variable of type `AorB` to just `A`:

```js flow-check
type A = { type: "A"; data: string };
type B = { type: "B"; data: number };
type AorB = A | B;

function isA(value: AorB): value is A {
  return value.type === "A";
}

function test(x: AorB) {
  if (isA(x)) {
    // `x` has now been refined to type A.
    // We can assign it variables of type A ...
    const y: A = x;
    // ...and access A's properties through `x`
    const stringData: string = x.data;

    // As a sanity check, the following assignment to B will error
    const error: B = x; // Error
  }
}
```
In the then-branch of the conditional `if (isA(x))`, `x` will have the type `A`.

### One-sided Type Guards {#toc-one-sided-type-guards}

In some cases we may want to declare that a type guard function only refines the then-branch of a conditional. Consider for example the function
```js flow-check
function isPositive(n: ?number): boolean {
  return n != null && n > 0;
}
```
If we declared `n is number` as the type guard of this function then in the following code we would be able to establish that `n` is `null | void` in the else-branch. This is not true, however, since `n` could just be a non-negative number:
```js flow-check
function isPositiveUnsound(n: ?number): n is number {
  return n != null && n > 0; // Error
}

declare const n: ?number;
if (isPositiveUnsound(n)) {
  // n is number here
} else {
  // n would be inferred as null | void, but could actually be a non-negative number
}
```

One-sided type guards, which we annotate as `implies param is PredicateType`, let us specify that a predicate narrows the type in only the positive case. For example,
```js flow-check
function isPositive(n: ?number): implies n is number {
  return n != null && n > 0;
}
```
Now, we'll get the following behavior:
```js flow-check
function isPositive(n: ?number): implies n is number {
  return n != null && n > 0;
}

declare const n: ?number;
if (isPositive(n)) {
  n as number; // OK: n is number here
} else {
  n as ?number; // OK: n is still ?number
}
```

### `this` Type guards

Sometimes, it is useful to declare a type predicate over the class instance on which a method is called.
This can be done by adding `this is Type` as return annotation for this method.
Calling this method in a conditional context will refine the instance to the type `Type`.

For example, consider the following class declarations:
```js flow-check
declare class Shape {
  isCircle(): this is Circle;
  isSquare(): this is Square;
}

declare class Circle {
  radius: number;
}

declare class Square {
  side: number;
}
```
The `this` type guard annotations allow us to refine a `Shape`-typed value to either `Circle` or `Square`:
```js flow-check
declare class Shape {
  isCircle(): this is Circle;
  isSquare(): this is Square;
}
declare class Circle { radius: number }
declare class Square { side: number }

function area(shape: Shape): number {
  if (shape.isCircle()) {
    // shape is now a Circle
    return Math.PI * shape.radius ** 2;
  } else if (shape.isSquare()) {
    // shape is now a Square
    return shape.side ** 2;
  } else {
    throw new Error('unknown shape');
  }
}
```

A `this` type guard can also be declared on a method of a regular class. Just like
[type guards over parameters](#toc-consistency-checks-of-type-guard-functions), the body of such a
method is checked for consistency against the declared guard type. The only refinements over `this`
that Flow understands are `instanceof` checks and calls to other `this` type guards:
```js flow-check
class Shape {
  isCircle(): this is Circle {
    return this instanceof Circle; // OK
  }

  isCircleWithRadius(): implies this is Circle {
    return this.isCircle() && this.name === 'circle'; // OK
  }

  isSquare(): this is Square {
    return true; // Error: Shape is not compatible with Square
  }

  name: string;
}

class Circle extends Shape { radius: number }
class Square extends Shape { side: number }
```

**Note:** The `this` type guard annotation is only allowed in the return annotation on non-static class and interface methods. For example the following are invalid forms:
```js flow-check
declare class A {}
declare class D {}

declare class InvalidStatic {
  static m(): this is D; // Error
}

type InvalidTypeAlias = (x: unknown) => this is A; // Error

declare function invalidFunction(this: unknown): this is A; // Error

const invalidObject = {
  m(): this is A { return true; }, // Error
};
```

## Refine with `Array.filter`

Flow recognizes when you call `filter` on an array of type `Array<T>` with a callback function that holds a type guard with type `(value: T) => value is S`.
It will use this to produce an output array of type `Array<S>`. Note that `S` needs to be a subtype of the type of the array element `T`.

For example
```js flow-check
type Success = Readonly<{type: 'success', value: 23}>;
type Error = Readonly<{type: 'error', error: string}>;

type Response =
  | Success
  | Error

function filterSuccess(response: Array<Response>): Array<Success> {
  return response.filter(
    (response): response is Success => response.type === 'success'
  );
}

function filterError1(response: Array<Response>): Array<Error> {
  const result = response.filter(
    (response): response is Success => response.type === 'success'
  );
  return result; // Error
}

function filterError2(response: Array<Response>): Array<Error> {
  const result = response.filter(
    (response): response is Error => response.type === 'success' // Error
  );
  return result;
}
```
In `filterError1`, filtering produces `Array<Success>` that is not compatible with the expected return type `Array<Error>`.

In `filterError2`, the predicate `response.type === 'success'` is used to refine `Response`s to `Success`s, not `Error`s.

Note that it is not necessary to provide a type guard annotation for the argument of `.filter()`.
This will be inferred from the body of the arrow function:
```js flow-check
type Success = Readonly<{type: 'success', value: 23}>;
type Error = Readonly<{type: 'error', error: string}>;
type Response = Success | Error;

function filterSuccessShort(response: Array<Response>): Array<Success> {
  return response.filter(
    response => response.type === 'success'
  );
}
```


## Defining Type Guard Functions {#toc-restrictions-of-type-guard-functions}

To ensure that refinement with type guard functions is sound, Flow runs a number of checks associated with these functions.

### Predicate parameter is a regular parameter to the function

In a type guard annotation of the form `parameter is Type`, `parameter` needs to belong to the current function's parameter list.
```js flow-check
function missing(param: unknown): prop is number { // Error
  return typeof param === "number";
}
```

It cannot be a parameter bound in a destructuring pattern, or a rest parameter:
```js flow-check
function destructuring({prop}: {prop: unknown}): prop is number { // Error
  return typeof prop === "number";
}
```
```js flow-check
function rest(...value: Array<unknown>): value is Array<unknown> { // Error
  return Array.isArray(value);
}
```
### Predicate type is consistent with the parameter type

The type guard `Type` needs to be compatible with the type of the parameter. In other words, given a definition
```js
function isT(x: ParamType): x is Type {
  return ...
}
```
Flow will check that `Type` is a subtype of `ParamType`. So the following will be an error:
```js flow-check
function isNumber(x: string): x is number { // Error
  return typeof x === "number";
}
```

### Type guard function returns boolean

A type guard function needs to return a boolean expression. The following are invalid declarations:
```js flow-check
function isNumberNoReturn(x: string): x is string {} // Error
```
```js flow-check
function nonMaybe<V extends {...}>(x: ?V): x is V {
  return x; // Error
}
```
A correct version of `nonMaybe` would be
```js flow-check
function nonMaybe<V extends {...}>(x: ?V): x is V {
  return !!x;
}
```

### Predicate type is consistent with refined type {#toc-consistency-checks-of-type-guard-functions}

In addition to the above checks, Flow also ensures that the declared type guard is consistent with the check happening in the body of the function. To establish soundness, Flow needs to guarantee two things:

1. The type of the refined parameter at the return location *after* the predicate of the return expression has been applied is a subtype of the guard type. For example, the following definitions are correct:
```js flow-check
function numOrStr(x: unknown): x is number | string {
  return (typeof x === "number" || typeof x === "string");
}

function numOrStrWithException(x: unknown): x is number | string {
  if (typeof x === "number") {
    return true;
  } else {
    if (typeof x === "string") {
        return true;
    } else {
        throw new Error("");
    }
  }
}
```
But in the following Flow will raise errors:
```js flow-check
function numOrStrError(x: unknown): x is number | string {
  return (typeof x === "number" || typeof x === "boolean"); // Error [incompatible-type-guard]
}
```

2. Type guard functions can be used to refine the else-branch of conditionals. For example,
```js flow-check
function isNumber(x: unknown): x is number {
  return typeof x === "number";
}

declare const value: number | string;
if (isNumber(value)) {
  value as number; // okay
} else {
  value as string; // also okay
}
```
Therefore, the inverse form of the first requirement also needs to hold. Specifically, if we negate the predicate encoded in the function, and use it to refine the input, then the result must not overlap with the type guard at all. This condition is equivalent to checking that the type guard refined with the negation of the function predicate is a subtype of `empty`. For example the following raises an error:
```js flow-check
function isPosNum(x: unknown): x is number {
    return typeof x === 'number' && x > 0; // Error [incompatible-type-guard]
}
```
This is because the negation of the predicate of `isPosNum` is "`x` is not a number or `x<=0`". This predicate is equivalent to the empty predicate and does not refine the input type it is applied to.

If you're seeing errors related to this check, consider using a one-sided type guard (write `implies x is T`). One-sided type guards do not require this check, since they do not refine the else-branch of conditionals.

3. The parameter that is refined cannot be reassigned in the body of the type guard function. Therefore the following are errors:
```js flow-check
function isNumberError1(x: unknown): x is number {
  x = 1;
  return typeof x === "number"; // Error [function-predicate]
}
```
```js flow-check
function isNumberError2(x: unknown): x is number { // Error [function-predicate]
  function foo() {
    x = 1;
  }
  foo();
  return typeof x === "number";
}
```

A closure that captures the parameter only invalidates the guard if the closure is actually *called* between the original parameter and the `return`. Defining the closure without invoking it is fine:

```js flow-check
function isNumber(x: unknown): x is number {
  const reset = () => { x = 1; }; // OK - never invoked
  return typeof x === "number";
}
```


## Type Guards with Generic Union Members {#toc-type-guards-with-generics}

Type guard functions can fail Flow's [consistency checks](#toc-consistency-checks-of-type-guard-functions) when a union member uses a generic type parameter in the property you refine on. Flow cannot prove the refined type is compatible with the guard type because the generic could overlap with the guard's literal:

```js flow-check
type Known = {tag: 'known', value: string};
type Dynamic<T> = {tag: T, data: number};
type Item<T> = Known | Dynamic<T>;

function isKnown<T>(x: Item<T>): x is Known {
  return x.tag === 'known'; // ERROR
}
```

The error occurs because `T` could be `'known'`, making it impossible to distinguish `Known` from `Dynamic<T>` by checking `tag` alone.

To fix this, add a separate literal discriminant property that does not use the generic. This gives Flow a concrete tag to refine on:

```js flow-check
type Known = {type: 'known', tag: 'known', value: string};
type Dynamic<T> = {type: 'dynamic', tag: T, data: number};
type Item<T> = Known | Dynamic<T>;

function isKnown<T>(x: Item<T>): x is Known {
  return x.type === 'known';
}
```

## Adoption {#toc-adoption}

To use type guards, you need to upgrade your infrastructure so that it supports the syntax:

- `prettier`: the latest version, using its built-in `flow` parser (see [our Prettier guide](../tools/prettier.md)).
- `babel` with `flow-parser/babel-plugin`. See [our Babel guide](../tools/babel.md) for setup instructions.
- `eslint` with `flow-eslint`. See [our ESLint guide](../tools/eslint.md) for setup instructions.

## See Also {#toc-see-also}

- [Refinements](../lang/refinements.md) — built-in type narrowing with `typeof`, `instanceof`, and equality checks
- [Assertion Functions](./assertion-functions.md) — narrow values after a call returns normally
- [Unions](./unions.md) — type guards are commonly used to narrow union types
- [Functions](./functions.md) — type guard syntax builds on function return type annotations
