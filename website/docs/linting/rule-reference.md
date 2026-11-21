---
title: Lint Rule Reference
slug: /linting/rule-reference
description: "Reference for all available Flow lint rules, including ambiguous-object-type, sketchy-null, unclear-type, and more."
---

### `all` {#toc-all}
While `all` isn't technically a lint rule, it's worth mentioning here. `all` sets the default
level for lint rules that don't have a level set explicitly. `all` can only
occur as the first entry in a `.flowconfig` or as the first rule in a `--lints`
flag. It's not allowed in comments at all because it would have different
semantics than would be expected.

### `ambiguous-object-type` {#toc-ambiguous-object-type}
Triggers when you use object type syntax without explicitly specifying exactness or inexactness.

```js flow-check
// flowlint ambiguous-object-type:error

type A = {x: number}; // Error
type B = {x: number, ...} // Ok
```

### `nonstrict-import` {#toc-nonstrict-import}
Used in conjunction with [Flow Strict](../strict/index.md). Triggers when importing a non `@flow strict` module. When enabled, dependencies of a `@flow strict` module must also be `@flow strict`.

### `sketchy-null` {#toc-sketchy-null}
Triggers when you do an existence check on a value that can be either null/undefined or falsey.

For example:
```js flow-check
// flowlint sketchy-null:error

const x: ?number = 5;
if (x) {} // Error: sketchy because x could be either null or 0.

const y: number = 5;
if (y) {} // not sketchy because y can't be null, only 0.

const z: ?{foo: number} = {foo: 5};
if (z) {} // not sketchy, because z can't be falsey, only null/undefined.
```

Setting `sketchy-null` sets the level for all sketchy null checks, but there are more granular rules for particular types. These are:
* `sketchy-null-bool`
* `sketchy-null-number`
* `sketchy-null-string`
* `sketchy-null-mixed`
* `sketchy-null-bigint`

The type-specific variants are useful for specifying that some types of sketchy null checks are acceptable while others should be errors/warnings. For example, if you want to allow boolean sketchy null checks (for the pattern of treating undefined optional booleans as false) but forbid other types of sketchy null checks, you can do so with this `.flowconfig` `[lints]` section:
```
[lints]
sketchy-null=warn
sketchy-null-bool=off
```
and now
```js
function foo (bar: ?boolean): void {
  if (bar) {
    ...
  } else {
    ...
  }
}
```
doesn't report a warning.

Suppressing one type of sketchy null check only suppresses that type, so, for example
```js flow-check
// flowlint sketchy-null:error, sketchy-null-bool:off
const x: ?(number | boolean) = 0;
if (x) {} // Error
```
would still have a `sketchy-null-number` error on line 3.

### `sketchy-number` {#toc-sketchy-number}
Triggers when a `number` is used in a manner which may lead to unexpected results if the value is falsy.
Currently, this lint triggers if a `number` appears in:
* the left-hand side of an `&&` expression.

As a motivating example, consider this common idiom in React:

```js
{showFoo && <Foo />}
```

Here, `showFoo` is a boolean which controls whether or not to display the `<Foo />` element. If `showFoo` is true, then this evaluates to `{<Foo />}`. If `showFoo` is false, then this evaluates to `{false}`, which doesn't display anything.

Now suppose that instead of a boolean, we have a numerical value representing, say, the number of comments on a post. We want to display a count of the comments, unless there are no comments. We might naively try to do something similar to the boolean case:

```js
{count && <>[{count} comments]</>}
```

If `count` is, say, `5`, then this displays "[5 comments]". However, if `count` is `0`, then this displays "0" instead of displaying nothing. (This problem is unique to `number` because `0` and `NaN` are the only falsy values which React renders with a visible result.) This could be subtly dangerous: if this immediately follows another numerical value, it might appear to the user that we have multiplied that value by 10! Instead, we should do a proper conditional check:

```js
{count ? <>[{count} comments]</> : null}
```

### `unclear-type` {#toc-unclear-type}
Triggers when you use `any` as a type annotation. This type is unsafe.

```js flow-check
// flowlint unclear-type:error

declare const a: any; // Error
declare const c: Object; // OK
declare const d: Function; // OK
```

### `unnecessary-assertion` {#toc-unnecessary-assertion}
Triggers when an [assertion function](../types/assertion-functions.md) checks a condition which we know must be truthy based on the available type information. This is quite conservative: for example, if all we know about the condition is that it is a `boolean`, then the lint will not fire even if the condition must be `true` at runtime.

This lint does not trigger when we know a condition is always `false`. Calling an assertion function with `false` is a common way to throw in code that should be unreachable.

This lint was previously named `unnecessary-invariant`; that name is still accepted in configuration and suppression comments.

```js flow-check
// flowlint unnecessary-assertion:error
declare function invariant(condition: unknown): asserts condition;

declare const x: Array<string>; // Array is truthy
invariant(x); // Error
```

### `unnecessary-optional-chain` {#toc-unnecessary-optional-chain}

Triggers when you use `?.` where it isn't needed. This comes in two main flavors. The first is when the left-hand-side cannot be nullish:

```js flow-check
// flowlint unnecessary-optional-chain:error
type Foo = {
  bar: number
}

declare const foo: Foo;
foo?.bar; // Error
```

The second is when the left-hand-side could be nullish, but the short-circuiting behavior of `?.` is sufficient to handle it anyway:

```js flow-check
// flowlint unnecessary-optional-chain:error
type Foo = {
  bar: {
    baz: number
  }
}

declare const foo: ?Foo;
foo?.bar?.baz; // Error
```

In the second example, the first use of `?.` is valid, since `foo` is potentially nullish, but the second use of `?.` is unnecessary. The left-hand-side of the second `?.` (`foo?.bar`) can only be nullish as a result of `foo` being nullish, and when `foo` is nullish, short-circuiting lets us avoid the second `?.` altogether!

```js
foo?.bar.baz;
```

This makes it clear to the reader that `bar` is not a potentially nullish property.

### `unsafe-getters-setters` {#toc-unsafe-getters-setters}
Triggers when you use getters or setters. Getters and setters can have side
effects and are unsafe.

For example:

```js flow-check
// flowlint unsafe-getters-setters:error
let a = 1;
const o = {
  get a() { return a; }, // Error: unsafe-getters-setters
  set b(x: number) { a = x; }, // Error: unsafe-getters-setters
  c: 10,
};
```

### `untyped-import` {#toc-untyped-import}
Triggers when you import from an untyped file. Importing from an untyped file
results in those imports being typed as `any`, which is unsafe.

### `unused-promise` {#toc-unused-promise}
Triggers when a `Promise` is unused. This can be dangerous, because errors are potentially unhandled, and the code may not execute in the desired order.

A promise can be "used" by...
* `await`ing it
* Calling `.then` with a rejection handler (i.e., with two arguments)
* Calling `.catch`
* Calling `.finally`
* Storing it in a variable, passing it to a function, etc.

For example:

```js flow-check
// flowlint unused-promise:error
declare function foo(): Promise<void>;

async function bar() {
  await foo(); // ok
  foo(); // error, we forgot to await!
}

function baz() {
  foo().catch(err => {console.log(err)}); // ok
  foo(); // error
}
```

You can explicitly ignore the promise with the `void` operator (e.g., `void foo();`).

## See Also {#toc-see-also}

- [Flowlint Comments](./flowlint-comments.md) — controlling lint severity within a file using comments
- [`.flowconfig [lints]`](../config/lints.md) — configuring lint severities project-wide
- [Strict ES6 Import/Export Lints](./strict-import-export-lints.md) — additional lints that catch unsafe ES6 import/export patterns
- [Flow Strict](../strict/index.md) — stricter type checking that enables additional lint-like rules
