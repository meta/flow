---
title: Unions
slug: /types/unions
description: "How to use union types in Flow to represent values that can be one of several different types."
---

A **union type** represents a value that is ***one of*** a set of types.

```js flow-check
function size(value: number | string): number {
  return typeof value === 'number' ? value : value.length;
}

size(1);       // Works!
size('three'); // Works!
size(true);    // Error!
```

## When to use this {#toc-when-to-use}

Use unions when a value can be one of several types, such as a function that handles different kinds of events or responses. For unions of string or number literals (e.g. `'success' | 'error'`), consider using [Flow Enums](../enums/index.md) instead, which provide exhaustiveness checking and a more structured API. If you need a nullable type, use the [maybe type](./maybe.md) shorthand `?T` rather than writing `T | null | undefined`. For values that must satisfy multiple types at once rather than one of them, use [intersections](./intersections.md) instead.

## Union type syntax {#toc-union-type-syntax}

Union types are any number of types which are joined by a vertical bar `|`.

```js
Type1 | Type2 | ... | TypeN
```

You may also add a leading vertical bar which is useful when breaking union
types onto multiple lines.

```js
type Foo =
  | Type1
  | Type2
  | ...
  | TypeN
```

Each of the members of a union type can be any type, even another union type.

```js flow-check
type Numbers = 1 | 2;
type Colors = 'red' | 'blue'

type Fish = Numbers | Colors;
```

If you have enabled [Flow Enums](../enums/index.md), they may be an alternative to unions of [literal types](./literals.md).

## Union shorthands

The union of some type `T` with `null` or `undefined` is common, so we provide a shorthand called [maybe types](./maybe.md), by using the `?` prefix. The type `?T` is equivalent to `T | null | undefined`:

```js flow-check
function maybeString(x: ?string) { /* ... */ }
maybeString('hi'); // Works!
maybeString(null); // Works!
maybeString(undefined); // Works!
```

The type that accepts every possible value is [`unknown`](./unknown.md):

```js flow-check
function everything(x: unknown) { /* ... */ }
everything(1); // Works!
everything(true); // Works!
everything(null); // Works!
everything({foo: 1}); // Works!
everything(new Error()); // Works!
```

## Unions & Refinements {#toc-unions-refinements}

When you have a value which is a union type it's often useful to break it apart
and handle each individual type separately. With union types in Flow you can
[refine](../lang/refinements.md) the value down to a single type.

For example, if we have a value with a union type that is a `number`, a
`boolean`, or a `string`, we can treat the number case separately by using
JavaScript's `typeof` operator.

```js flow-check
function toStringPrimitives(value: number | boolean | string) {
  if (typeof value === 'number') {
    return value.toLocaleString([], {maximumSignificantDigits: 3}); // Works!
  }
  // ...
}
```

By checking the `typeof` our value and testing to see if it is a `number`, Flow
knows that inside of that block it is only a number. We can then write code
which treats our value as a number inside of that block.

## Union types requires one in, but all out {#toc-union-types-requires-one-in-but-all-out}

When calling a function that accepts a union type we must pass in ***one of
those types***. But inside of the function we are required to handle ***all of
the possible types***.

Let's rewrite the function to handle each type individually using [refinements](../lang/refinements.md).

```js flow-check
function toStringPrimitives(value: number | boolean | string): string {
  if (typeof value === 'number') {
    return String(value);
  } else if (typeof value === 'boolean') {
    return String(value);
  }
  return value; // If we got here, it's a `string`!
}
```

If we do not handle each possible type of our value, Flow will give us an error:

```js flow-check
function toStringPrimitives(value: number | boolean | string): string {
  if (typeof value === 'number') {
    return String(value);
  }
  return value; // Error!
}
```

## Disjoint Object Unions {#toc-disjoint-object-unions}

There's a special type of union in Flow known as a "disjoint object union" which can
be used with [refinements](../lang/refinements.md). These disjoint object unions are
made up of any number of object types which are each tagged by a single property.

For example, imagine we have a function for handling a response from a server
after we've sent it a request. When the request is successful, we'll get back
an object with a `type` property set to `'success'` and a `value` that we've
updated.

```js
{type: 'success', value: 23}
```

When the request fails, we'll get back an object with `type` set to `'error'`
and an `error` property describing the error.

```js
{type: 'error', error: 'Bad request'}
```

We can try to express both of these objects in a single object type. However,
we'll quickly run into issues where we know a property exists based on the
`type` property but Flow does not.

```js flow-check
type Response = {
  type: 'success' | 'error',
  value?: number,
  error?: string
};

function handleResponse(response: Response) {
  if (response.type === 'success') {
    const value: number = response.value; // Error!
  } else {
    const error: string = response.error; // Error!
  }
}
```

Trying to combine these two separate types into a single one will only cause us
trouble.

Instead, if we create a union type of both object types, Flow will be able to
know which object we're using based on the `type` property.

```js flow-check
type Response =
  | {type: 'success', value: number}
  | {type: 'error', error: string};

function handleResponse(response: Response) {
  if (response.type === 'success') {
    const value: number = response.value; // Works!
  } else {
    const error: string = response.error; // Works!
  }
}
```

In order to use this pattern, there must be a key that is in every object in your union (in our example above, `type`),
and every object must set a different [literal type](./literals.md) for that key (in our example, the string `'success'`, and the string `'error'`).
You can use any kind of literal type, including numbers and booleans.

### Disjoint unions with generic types {#toc-disjoint-unions-with-generic-types}

Disjoint union refinement requires the discriminant property to have a literal type. When the discriminant comes from a generic type parameter, Flow cannot narrow the union because the generic hasn't been resolved to a specific literal yet:

```js flow-check
type PayloadMap = {
  PING: string,
  PONG: number,
};

function handle<T extends keyof PayloadMap>(
  type: T,
  payload: PayloadMap[T],
) {
  if (type === 'PING') {
    payload as string; // Error! payload is still PayloadMap[T]
  }
}
```

To work around this, restructure the code so the discriminant is a concrete literal type. For example, use a disjoint union directly:

```js flow-check
type Message =
  | {type: 'PING', payload: string}
  | {type: 'PONG', payload: number};

function handle(message: Message) {
  if (message.type === 'PING') {
    message.payload as string; // Works!
  }
}
```

### Disjoint object unions with exact types {#toc-disjoint-unions-with-exact-types}

Disjoint unions require you to use a single property to distinguish each object
type. You cannot distinguish two different [inexact objects](./objects.md#exact-and-inexact-object-types) by different properties.

```js flow-check
type Success = {success: true, value: boolean, ...};
type Failed  = {error: true, message: string, ...};

function handleResponse(response:  Success | Failed) {
  if (response.success) {
    const value: boolean = response.value; // Error!
  }
}
```

This is because in Flow it is okay to pass an object value with more properties
than the inexact object type expects (because of [width subtyping](../lang/depth-subtyping.md#toc-width-subtyping)).

```js flow-check
type Success = {success: true, value: boolean, ...};
type Failed  = {error: true, message: string, ...};

function handleResponse(response:  Success | Failed) {
  // ...
}

handleResponse({
  success: true,
  error: true,
  value: true,
  message: 'hi'
});
```

Unless the objects somehow conflict with one another there is no way to
distinguish them.

However, to get around this you could use **exact object types**.

```js flow-check
type Success = {success: true, value: boolean};
type Failed  = {error: true, message: string};

type Response = Success | Failed;

function handleResponse(response: Response) {
  if (response.success) {
    const value: boolean = response.value;
  } else {
    const message: string = response.message;
  }
}
```

With exact object types, we cannot have additional properties, so the objects
conflict with one another and we are able to distinguish which is which.

## Disjoint tuple unions

Like disjoint object unions explained above, you can also define disjoint tuple unions.
These are unions of tuple types, where each tuple is tagged by a particular element. For example:

```js flow-check
type Response =
  | ['success', number]
  | ['error', string];

function handleResponse(response: Response) {
  if (response[0] === 'success') {
    const value: number = response[1]; // Works!
  } else {
    const error: string = response[1]; // Works!
  }
}
```

This feature is particularly useful for function arguments, which are tuples.
Note the use of [tuple element labels](./tuples.md#tuple-element-labels) to make the code more clear.

```js flow-check
function prettyPrint(
  ...args: ['currency', dollars: number, cents: number]
         | ['choice', boolean]
): string {
  switch (args[0]) {
    case 'currency':
      return args[1] + '.' + args[2];
    case 'choice':
      return args[1] ? 'yes' : 'no';
  }
}
// Argument types based on the first arg
prettyPrint("currency", 1, 50); // OK
prettyPrint("choice", true); // OK

prettyPrint("currency", 1); // ERROR - missing arg
prettyPrint("currency", true); // ERROR - wrong type arg
```

## See Also {#toc-see-also}

- [Intersections](./intersections.md) — the dual of unions: values that are *all of* a set of types
- [Refinements](../lang/refinements.md) — how to narrow union types to specific members
- [Maybe Types](./maybe.md) — shorthand for `T | null | undefined`
- [Literal Types](./literals.md) — using specific values as types, commonly combined with unions
- [Flow Enums](../enums/index.md) — a structured alternative to unions of literal types
- [Match Expressions](../match/index.md) — pattern matching for exhaustively handling union members
