---
title: Functions
slug: /types/functions
description: "How to type functions in Flow, including parameters, return types, rest parameters, generics, constructor types, and callable objects."
---

import {SinceVersion} from '../../components/VersionTags';

Functions can be typed at their parameters (input) and return value (output).

```js flow-check
function concat(a: string, b: string): string {
  return a + b;
}

concat("foo", "bar"); // Works!
concat(true, false);  // Error!
```

Return types can often be omitted thanks to inference, and parameter types can be inferred from context:

```js flow-check
[1, 2, 3].map(x => x * x);
```

## Syntax of functions

There are three forms of functions that each have their own slightly different syntax.

### Function Declarations

```js flow-check
function func(str: string, bool?: boolean, ...nums: Array<number>): void {
  // ...
}
```

### Arrow Functions {#toc-arrow-functions}

```js flow-check
let func = (str: string, bool?: boolean, ...nums: Array<number>): void => {
  // ...
};
```

### Function Types

```js flow-check
type T = (str: string, bool?: boolean, ...nums: Array<number>) => void;
```

You may also optionally leave out the parameter names.

```js flow-check
type T = (string, boolean | void, Array<number>) => void;
```

You might use these functions types for something like a callback.

```js flow-check
function func(callback: (error: Error | null, value: string | null) => void) {
  // ...
}
```

### Type arguments

Functions can have type arguments:

```js flow-check
function f<T>(x: T): Array<T> {
  return [x];
}

const g = <T>(x: T): Array<T> => [x];

type H = <T>(T) => Array<T>;
```

## Function Parameters

Function parameters can have types by adding a colon `:` followed by the type
after the name of the parameter.

```js flow-check
function func(param1: string, param2: boolean) {
  // ...
}
```

### Optional Parameters

You can also have optional parameters by adding a question mark `?` after the
name of the parameter and before the colon `:`.

```js flow-check
function func(optionalValue?: string) {
  // ...
}
```

Optional parameters will accept missing, `undefined`, or matching types. But
they will not accept `null`.

```js flow-check
function func(optionalValue?: string) {
  // ...
}

func();          // Works.
func(undefined); // Works.
func("string");  // Works.

func(null);      // Error!
```

### Rest Parameters

JavaScript also supports having rest parameters or parameters that collect an
array of arguments at the end of a list of parameters. These have an ellipsis
`...` before them.

You can also add type annotations for rest parameters using the same syntax but
with an `Array`.

```js flow-check
function func(...args: Array<number>) {
  // ...
}
```

You can pass as many arguments as you want into a rest parameter.

```js flow-check
function func(...args: Array<number>) {
  // ...
}

func();        // Works.
func(1);       // Works.
func(1, 2);    // Works.
func(1, 2, 3); // Works.
```

> Note: If you add a type annotation to a rest parameter, it must be an
> array-like type (e.g., `Array<T>`, `ReadonlyArray<T>`, or a tuple type).

### `this` parameter

Every function in JavaScript is called with a special context named `this`. Flow
allows you to annotate the type for this context by adding a special parameter at
the start of the function's parameter list:

```js flow-check
type Counter = {
  x: number,
  getX: (this: Counter) => number,
};

function getX(this: Counter): number {
  return this.x;
}

const counter: Counter = {x: 42, getX};
const num: number = counter.getX();
const str: string = counter.getX(); // Error!
```

The context comes from the receiver of the call. For receiver-specific methods,
`call`, `apply` and `bind` may only re-supply that same receiver — see
[Rebinding `this`](./classes.md#toc-this-rebinding).

This parameter has no effect at runtime, and is erased along with types when Flow is transformed into JavaScript.
When present, `this` parameters must always appear at the very beginning of the function's parameter list, and must
have an annotation. Additionally, [arrow functions](#toc-arrow-functions) may not have a `this` parameter annotation, as
these functions bind their `this` parameter at the definition site, rather than the call site.

If an explicit `this` parameter is not provided, Flow will attempt to infer one based on usage. If `this` is not mentioned
in the body of the function, Flow will infer `unknown` for its `this` parameter.


## Function Returns

Function returns can also add a type using a colon `:` followed by the type
after the list of parameters.

```js flow-check
function func(): number {
  return 1;
}
```

Return types ensure that every branch of your function returns the same type.
This prevents you from accidentally not returning a value under certain
conditions.

```js flow-check
function func(): boolean { // Error
  if (Math.random() > 0.5) {
    return true;
  }
}
```

Async functions implicitly return a promise, so the return type must always be a `Promise`.

```js flow-check
async function func(): Promise<number> {
  return 123;
}
```

You can also use a special return type to declare that your function refines the type of a parameter.
These are called [type guards](./type-guards.md):

```js flow-check
function isString(x: unknown): x is string {
  return typeof x === 'string';
}
```

## Constructor Types <SinceVersion version="0.339" /> {#toc-constructor-types}

A constructor type `new (...) => T` describes a value that you can call with `new` to produce `T`. Use it to accept a class constructor in a function, such as this generic factory:

```js flow-check
function create<T>(C: new () => T): T {
  return new C();
}

class Item {
  value: number = 1;
}

create(Item).value as number; // OK
```

Unlike a function type `(...) => T`, a constructor type describes construction, not an ordinary function call. It is a type annotation, not syntax for declaring a runtime function. For the distinction between an instance type and the class value, see [Classes in annotations](./classes.md#toc-classes-in-annotations).

Flow checks constructor arguments when you use `new`. If a constructor requires an argument, you cannot assign it to a zero-argument constructor type:

```js flow-check
class Item {
  name: string;
  constructor(name: string) {
    this.name = name;
  }
}

const C: new (name: string) => Item = Item; // OK
new C('book').name as string; // OK
new C(123); // Error! `name` must be a string
new C(); // Error! `name` is required
const noArgs: new () => Item = Item; // Error! `Item` requires an argument
```

You can capture the parameter list with a [tuple type](./tuples.md) to write a variadic factory:

```js flow-check
function instantiate<T, Args extends ReadonlyArray<unknown>>(
  C: new (...args: Args) => T,
  ...args: Args
): T {
  return new C(...args);
}

class Point {
  x: number;
  label: string;
  constructor(x: number, label: string) {
    this.x = x;
    this.label = label;
  }
}

instantiate(Point, 1, 'start').x as number; // OK
instantiate(Point, 'bad', 'start'); // Error! The first argument must be a number
```

When you use a class as the result type, Flow preserves its [nominal class identity](../lang/nominal-structural.md). To accept different classes that share a structural contract, use an [interface](./interfaces.md) as the result type rather than a plain object type:

```js flow-check
class Item {
  value: number = 1;
}
class OtherItem {
  value: number = 2;
}

const specific: new () => Item = Item; // OK
const wrong: new () => Item = OtherItem; // Error! Different class identity

interface WithValue {
  value: number;
}
const structural: new () => WithValue = OtherItem; // OK
new structural().value as number; // OK
```

In ordinary Flow source, `{new(): T}` describes an object **method named `new`**, not TypeScript's object construct signature. An interface can instead declare a construct signature:

```js flow-check
declare const methods: {new(): {value: number}};
methods.new().value as number; // OK: method call
new methods(); // Error! Not a constructor

interface ItemConstructor {
  new(): {value: number};
}
declare const C: ItemConstructor;
new C().value as number; // OK: construction
```

[`ConstructorParameters<C>`](./utilities.md#toc-constructor-parameters) extracts the constructor's parameter tuple. To extract its result type, use [`InstanceType<C>`](./utilities.md#toc-instance-type). Both utilities accept constructor types.

### Abstract constructor types {#toc-abstract-constructor-types}

Use `abstract new (...) => T` when a constructor value may be an [abstract class](./classes.md#toc-abstract-classes). This type accepts both abstract and concrete constructors with compatible parameters and results. You cannot construct an instance through it, even if the underlying class is concrete:

```js flow-check
abstract class Animal {
  abstract makeSound(): string;
}
class Dog extends Animal {
  makeSound(): string {
    return 'woof';
  }
}

const base = Animal as abstract new () => Animal; // OK
const pet = Dog as abstract new () => Animal; // OK
new base(); // Error! Abstract constructor type
new pet(); // Error! Still an abstract constructor type

Animal as new () => Animal; // Error! Abstract class in a concrete constructor slot
const concrete = Dog as new () => Animal; // OK
new concrete().makeSound() as string; // OK
```

### Adoption {#toc-constructor-types-adoption}

To use this syntax, set `experimental.tslib_syntax=true` in `.flowconfig` and use Flow and flow-parser 0.339.0+ and Prettier 3.10.0+ with its built-in [`flow` parser](../tools/prettier.md).

## Callable Objects

You can type functions with static properties as callable objects:

```js flow-check
type CallableObj = {
  (number, number): number,
  bar: string,
  ...
};

function add(x: number, y: number) {
  return x + y;
}

add.bar = "hello world";

add as CallableObj;
```

Callable objects can also have multiple call signatures. A function assigned to this type must satisfy every signature:

```js flow-check
type Formatter = {
  (value: string): string,
  (value: number): string,
  ...
};

const format: Formatter = (value: string | number): string => String(value);

const badFormat: Formatter = (value: number): string => String(value); // Error!
```

In general, functions can have properties assigned to them if they are function declarations, or
simple variable declarations of the form `const f = () => ...`. The properties must be assigned in
the format `f.prop = <expr>;`, in the same statement list as the function definition (i.e. not conditionally).

Note that the object representing the static properties assigned to the function is inexact.

When you annotate a variable with a plain function type, Flow does not infer extra static properties:

```js flow-check
type Callback = () => void;

const callback: Callback = () => {};

callback.description = "runs later"; // Error!
```

## Overloaded functions
You can use intersection types to define [overloaded function types](./intersections.md#toc-intersection-of-function-types):

```js flow-check
declare const fn:
  & ((x: 'string') => string)
  & ((x: 'number') => number)

const s: string = fn('string');
const n: number = fn('number');
```

## Any function

If you want to specify you want to allow any function, and do not care what it is, you can use this pattern:

```js flow-check
function useCallback<T extends (...ReadonlyArray<empty>) => unknown>(
  callback: T,
  inputs: ?ReadonlyArray<unknown>,
): T {
  return callback;
}
useCallback((x: string) => true); // OK
useCallback((x: number) => [1]); // OK
```

You could use type arguments to capture the arguments and return type, to do more complicated transformations:

```js flow-check
function func<TArgs extends ReadonlyArray<unknown>, TReturn>(
  callback: (...TArgs) => TReturn,
): (boolean, ...TArgs) => Array<TReturn> {
  return (b, ...args): Array<TReturn> => {
    if (b) {
      return [callback(...args)];
    } else {
      return [];
    }
  };
}

const f: (boolean, string, number) => Array<string> =
  func((x: string, y: number) => x.slice(y)); // OK
```

The type `Function` refers to the global `Function` class. It exposes function
properties such as `name` and `length`, but has no call signature. Use a function
type with explicit parameters and a return type when you need to call the function.

## See Also {#toc-see-also}

- [Generics](./generics.md) — parameterized types used with functions for type-safe reusable code
- [Type Guards](./type-guards.md) — special return types that refine parameter types
- [Intersections](./intersections.md) — defining overloaded function types
- [Subtypes](../lang/subtypes.md) — how function subtyping works (contravariant parameters, covariant returns)
