---
title: Arrays
slug: /types/arrays
description: "How to type arrays in Flow, including Array<T> syntax, read-only arrays, and array utility types."
---

Array types represent variable-length lists where all items have the same type.

```js flow-check
const arr: Array<number> = [1, 2, 3];
```

:::info TypeScript comparison
TypeScript's `readonly T[]` [shorthand](../flow-vs-typescript.md#toc-ts-only-syntax) is not supported in Flow, use `ReadonlyArray<T>` instead (TS supports both). Mutable `Array<T>` is also [invariant in Flow but covariant in TS](../flow-vs-typescript.md#toc-variance-arrays), which can block unsafe widenings that TS would accept.
:::

## When to use this {#toc-when-to-use}

Use arrays when you have a variable-length list of values that all share the same type. When the list has a fixed number of elements where each position has a specific type, use [tuples](./tuples.md) instead.

## `Array` Type {#toc-array-type}

The type `Array<T>` represents an array of items of type `T`.
For example, an array of numbers would be `Array<number>`:

```js flow-check
const arr: Array<number> = [1, 2, 3];
```

You can put any type within `Array<T>`:

```js flow-check
const arr1: Array<boolean> = [true, false, true];
const arr2: Array<string> = ["A", "B", "C"];
const arr3: Array<unknown> = [1, true, "three"];
```

## `ReadonlyArray<T>` {#toc-readonlyarray}

You can use the type `ReadonlyArray<T>` instead of `Array<T>` to represent a [read-only](../lang/variance.md) array which cannot be mutated.
You can't write to a read-only array directly, and can't use methods which mutate the array like `.push()`, `.unshift()`, etc.

```js flow-check
const readonlyArray: ReadonlyArray<number> = [1, 2, 3]

const first = readonlyArray[0]; // OK to read
readonlyArray[1] = 20;          // Error!
readonlyArray.push(4);          // Error!
readonlyArray.unshift(4);       // Error!
```

Note that an array of type `ReadonlyArray<T>` can still have mutable _elements_:

```js flow-check
const readonlyArray: ReadonlyArray<{x: number}> = [{x: 1}];
readonlyArray[0] = {x: 42}; // Error!
readonlyArray[0].x = 42; // Works
```

The main advantage to using `ReadonlyArray` instead of `Array` is that `ReadonlyArray`'s
type parameter is [*covariant*](../lang/variance.md#toc-covariance)
while `Array`'s type parameter is [*invariant*](../lang/variance.md#toc-invariance).
That means that `ReadonlyArray<number>` is a subtype of `ReadonlyArray<number | string>` while
`Array<number>` is NOT a subtype of `Array<number | string>`. So it's often useful to use
`ReadonlyArray` in type annotations for arrays of various types of elements.
Take, for instance, the following scenario:

```js flow-check
const someOperation = (arr: Array<number | string>) => {
  // Here we could do `arr.push('a string')`
}

const array: Array<number> = [1];
someOperation(array) // Error!
```

Since the parameter `arr` of the `someOperation` function is typed as a mutable
`Array`, pushing a string into it would be possible inside that scope, which
would then break the type contract of the outside `array` variable. By
annotating the parameter as `ReadonlyArray` instead in this case, Flow can be
sure this won't happen and no errors will occur:

```js flow-check
const someOperation = (arr: ReadonlyArray<number | string>) => {
  // Nothing can be added to `arr`
}

const array: Array<number> = [1];
someOperation(array); // Works
```

`ReadonlyArray<unknown>` represents the supertype of all arrays and all [tuples](./tuples.md):

```js flow-check
const tup: [number, string] = [1, 'hi'];
const arr: Array<number> = [1, 2];

function f(xs: ReadonlyArray<unknown>) { /* ... */ }

f(tup); // Works
f(arr); // Works
```

## Empty Array Literals {#toc-empty-array-literals}

Empty array literals (`[]`) are handled specially in Flow when it comes to their [annotation requirements](../lang/annotation-requirement.md).
What makes them special is that they do not contain enough information to
determine their type, and at the same time they are too common to always
require type annotations in their immediate context.

So, to type empty arrays Flow follows these rules:

### Contextual Inference

First, if [contextual information](../lang/annotation-requirement.md#toc-contextual-typing) exists,
we'll use it to determine the array element type:

```js flow-check
function takesStringArray(x: Array<string>): void {}

const arr1: Array<string> = [];
takesStringArray([]);
```

In both cases, the `[]` will be typed as an `Array<string>`.

Note that for the contextual information to work, the type needs to be available
right at the definition of the array. This means that the last two lines in the following
code will error:

```js flow-check
function takesStringArray(x: Array<string>): void {}

const arr2 = []; // Error
takesStringArray(arr2); // Error
```

The second error is due to `arr2` being inferred as `Array<empty>` which leads to another error at the call
to `takesStringArray`.

### Initializer Inference

Flow allows another way to determine the types of empty arrays when they are immediately assigned to a variable:

```js
const arr3 = [];
```

The way it does this is reminiscent of the typing of
[variables without initializers](../lang/variables.md#toc-variables-declared-without-initializers):
Flow tries to choose the "first" *assignment* or *assignments*
to the variable to define its type. In the case of empty arrays, an assignment is either
* an indexed write statement `a[i] = e;`, or
* an array `push` call `a.push(e)`.

In either case the type of `e` is used as the type of the array element.

Here are some examples:

#### Straight-line Code
Once the first assignment has been found, the type of the array element is pinned to that of the
assigned expression. Subsequent writes to array with elements of
a different type are errors:

```js flow-check
const arr3 = [];
arr3.push(42); // arr3 is inferred as Array<number>
arr3.push("abc"); // Error!
```

#### Conditional Code

If the array is assigned in sibling branches of conditional statements, the type
of the array element is pinned to the union of the assigned types:

```js flow-check
declare const cond: boolean;

const arr4 = [];
if (cond) {
  arr4[0] = 42;
} else {
  arr4.push("abc");
}
// arr4 is inferred as Array<number | string>
arr4.push("def"); // Works
arr4[0] = true; // Error!
```

#### Nearer Scope Wins

Shallow scope of assignment is preferred when there are multiple scopes where assignments happen:

```js flow-check
const arr5 = [];
function f() {
  arr5.push(42); // Error!
}
f();
arr5.push("abc"); // This assignment wins. arr5 is inferred as Array<string>
arr5.push(1); // Error!
```

## Array access can be unsafe {#toc-array-access-is-unsafe}

By default, accessing an element of an array gives you back the element type `T`, even though the value might actually be `undefined` at runtime. One way this happens is reading an index that is out of bounds:

```js flow-check
const array: Array<number> = [0, 1, 2];
const value: number = array[3]; // Works
                         // ^ undefined
```

It also happens when the array is "sparse" and the element was never set:

```js flow-check
const array: Array<number> = [];

array[0] = 0;
array[2] = 2;

const value: number = array[1]; // Works
                         // ^ undefined
```

In both cases Flow accepts the code and types `value` as `number`, even though the value is `undefined` at runtime.

To catch this, enable the [`no_unchecked_indexed_access`](../config/options.md#toc-no-unchecked-indexed-access) option. With it on, every array (and dictionary) element access returns `T | undefined` instead of `T`, so Flow makes you handle the `undefined` case before using the value:

```js flow-check
const array: Array<number> = [0, 1, 2];
const value: number | undefined = array[1];

if (value !== undefined) {
  // number
}
```

## Discouraged Array Type Shorthand Syntax {#toc-array-type-shorthand-syntax}

There is an alternative to the `Array<T>` syntax: `T[]`.
This syntax is discouraged and may be deprecated in the future.

```js flow-check
const arr: number[] = [0, 1, 2, 3];
```

Just note that `?Type[]` is the equivalent of `?Array<T>` and not `Array<?T>`.

```js flow-check
const arr1: ?number[] = null;   // Works
const arr2: ?number[] = [1, 2]; // Works
const arr3: ?number[] = [null]; // Error!
```

If you want to make it `Array<?T>` you can use parenthesis like: `(?Type)[]`

```js flow-check
const arr1: (?number)[] = null;   // Error!
const arr2: (?number)[] = [1, 2]; // Works
const arr3: (?number)[] = [null]; // Works
```

## See Also {#toc-see-also}

- [Tuples](./tuples.md) — fixed-length collections where each element can have a different type
- [Variance](../lang/variance.md) — why `ReadonlyArray` is covariant and `Array` is invariant
- [Objects](./objects.md) — another core data structure type in Flow
