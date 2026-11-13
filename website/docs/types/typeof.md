---
title: Typeof Types
slug: /types/typeof
description: "How to use typeof types in Flow to extract the type of a value at the type level."
---

import {SinceVersion} from '../../components/VersionTags';

Flow's `typeof` type operator extracts the Flow type of a value, so you can use it as a type annotation.

```js flow-check
let size = 42;
let other: typeof size = 100; // type is number
```

## When to use this {#toc-when-to-use}

Use `typeof` to derive a type from an existing value rather than writing it manually. This is useful when a module exports a value but not its type, or when you want to keep annotations in sync with a variable's inferred type. If the type is already available as a named export, prefer importing it directly.

## `typeof` type syntax {#toc-typeof-type-syntax}


The `typeof` operator returns the Flow type of a given value to be used as a type.

```js flow-check
let num1 = 42;
let num2: typeof num1 = 3.14;     // Works!
let num3: typeof num1 = 'world';  // Error!

let bool1 = true;
let bool2: typeof bool1 = false;  // Works!
let bool3: typeof bool1 = 42;     // Error!

let str1 = 'hello';
let str2: typeof str1 = 'world'; // Works!
let str3: typeof str1 = false;   // Error!
```

The argument to `typeof` can be a variable, a member access, or the [inline import form](#toc-imported-values) described below. Arbitrary runtime expressions are not allowed:

```js flow-check
let obj1 = {foo: 1, bar: true, baz: 'three'};
let obj2: typeof obj1 = {foo: 42, bar: false, baz: 'hello'};
let num: typeof obj1.foo = 1;

let arr1 = [1, 2, 3];
let arr2: typeof arr1 = [3, 2, 1];

type T = typeof {a: 1}; // Error
```

### Imported values {#toc-imported-values}

<SinceVersion version="0.339" />

`typeof import('./config').Config` gives you the type of an exported value without a local import binding. `typeof import('./config')` gives you the type of the module-namespace value. These examples use the `config.js` file from [Modules](./modules.md#toc-inline-import-types):

```js
type Constructor = typeof import('./config').Config;
type DefaultPort = typeof import('./config').default;
type ModuleNamespace = typeof import('./config');
```

`Constructor` describes the class value itself, while `import('./config').Config` describes its instances. `DefaultPort` is the type of the default-exported number. None of these annotations performs a runtime dynamic import. For more examples, see [inline import annotations](./modules.md#toc-inline-imported-values). The [Adoption](./modules.md#toc-inline-import-types-adoption) section lists the required option and tooling versions.

## `typeof` inherits behaviors of inference {#toc-typeof-inherits-behaviors-of-inference}

When you use `typeof`, you're taking the results of Flow's inference and
asserting it as a type. While this can be very useful, it can also lead to some
unexpected results.

For example, when you use literal values in Flow, their inferred type is the
primitive that it belongs to. Thus, the number 42 has the inferred type of
`number`. You can see this when you use `typeof`.

```js flow-check
let num1 = 42;
let num2: typeof num1 = 3.14;    // Works!

let bool1 = true;
let bool2: typeof bool1 = false; // Works!

let str1 = 'hello';
let str2: typeof str1 = 'world'; // Works!
```

However, this only happens with the inferred type. If you specify the literal
type, it will be used in `typeof`.

```js flow-check
let num1: 42 = 42;
let num2: typeof num1 = 3.14;    // Error!

let bool1: true = true;
let bool2: typeof bool1 = false; // Error!

let str1: 'hello' = 'hello';
let str2: typeof str1 = 'world'; // Error!
```

## `typeof` inherits behaviors of other types {#toc-typeof-inherits-behaviors-of-other-types}

`typeof` preserves the behavior of the type it returns.

For example, if you use `typeof` with a class you need to remember that classes
are [*nominally* typed instead of *structurally* typed](../lang/nominal-structural.md). So that two classes with
the same exact shape are not considered equivalent.

```js flow-check
class MyClass {
  method(val: number) { /* ... */ }
}

class YourClass {
  method(val: number) { /* ... */ }
}

let test1: typeof MyClass = YourClass; // Error!
let test2: typeof MyClass = MyClass;   // Works!
```

## Differences from runtime `typeof` {#toc-differences-from-runtime-typeof}

JavaScript's runtime `typeof` operator returns a string like `"number"` or `"object"`, but it can't distinguish many types — for example, objects, arrays, and `null` all produce `"object"`:

```js flow-check
typeof {foo: true} === 'object';
typeof [true, false] === 'object';
typeof null === 'object';
```

Flow's `typeof` is a type-level operator that gives you the full Flow type of a value, preserving its exact structure. It is not used at runtime — it only exists in type annotations.

## See Also {#toc-see-also}

- [Classes](./classes.md) — `typeof` is commonly used with classes to refer to the class value (not instance) type
- [Modules](./modules.md) — `typeof` imports for getting the type of an imported value
- [Nominal & Structural Typing](../lang/nominal-structural.md) — `typeof` with classes preserves nominal typing behavior
