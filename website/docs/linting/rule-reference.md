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

### `deprecated-type` {#toc-deprecated-type}
Triggered on the `bool` type, which is just an alias for `boolean`. Just use `boolean` instead.

```js flow-check
// flowlint deprecated-type:error

type A = Array<bool>; // Error
```

### `nested-component` {#toc-nested-component}
Triggers when a [component](../react/component-syntax.md) is defined directly inside another component or hook. Nested components are problematic because React cannot preserve the state of a nested component across re-renders of the parent — each render creates a brand new component type, so React always unmounts and remounts it.

This lint is enabled as an error by default. It applies to [component syntax](../react/component-syntax.md) declarations, which require `component_syntax=true` in your `.flowconfig`.

```js flow-check
import * as React from 'react';

component Outer() {
  component Inner() { // Error
    return null;
  }
  return <Inner />;
}
```

The lint also fires when a component is defined inside a hook:
```js flow-check
import * as React from 'react';

hook useItems() {
  component ItemView() { // Error
    return null;
  }
  return ItemView;
}
```

To fix, move the nested component to the top level:
```js flow-check
import * as React from 'react';

component Inner() {
  return null;
}

component Outer() {
  return <Inner />;
}
```

### `nested-hook` {#toc-nested-hook}
Triggers when a [hook](../react/hook-syntax.md) is defined directly inside another component or hook. Nested hooks are problematic because they break the [Rules of Hooks](https://react.dev/reference/rules/rules-of-hooks) — React relies on hooks being called in a consistent order at the top level of a component or hook, and a nested hook definition obscures the call structure.

This lint is enabled as an error by default. It applies to [hook syntax](../react/hook-syntax.md) declarations, which require `component_syntax=true` in your `.flowconfig`.

```js flow-check
import * as React from 'react';
import {useState} from 'react';

component Foo() {
  hook useNested() { // Error
    return useState();
  }
  return null;
}
```

The lint also fires when a hook is defined inside another hook:
```js flow-check
import {useState} from 'react';

hook useFoo() {
  hook useNested() { // Error
    return useState();
  }
  return useNested();
}
```

To fix, move the nested hook to the top level:
```js flow-check
import * as React from 'react';
import {useState} from 'react';

hook useNested() {
  return useState();
}

component Foo() {
  useNested();
  return null;
}
```

### `libdef-override` {#toc-libdef-override}
Triggers when a [library definition](../libdefs/index.md) overrides an existing built-in definition. This can happen when a `.js.flow` library file or a `flow-typed` stub re-declares a global variable, type, or module that Flow already provides in its builtins. Overriding built-in definitions can lead to surprising behaviors, because the order in which library files are loaded affects which definition wins.

This lint is enabled as an error by default.

There are two forms of this error. The first is a **name override**, which fires when a library definition re-declares a global variable or type that already exists:
```js
// In a library definition file (.js.flow or [libs])
declare const globalThis: mixed; // Error: overrides built-in globalThis

declare type React$Node = string; // Error: overrides built-in React$Node
```

The second is a **module override**, which fires when a library definition re-declares a module that is already declared elsewhere:
```js
// In a library definition file
declare module 'react' { // Error: overrides built-in react module
  declare module.exports: any;
}
```

This error also fires when the same library file is included twice (for example, if both a directory and its subdirectory are listed in the `[libs]` section of `.flowconfig`), causing all declarations in that file to conflict with themselves.

To fix the error, remove the redundant declaration from your library definition. If you intentionally need to override a built-in, you can suppress the error with a `$FlowFixMe` comment:
```js
// $FlowFixMe[libdef-override]
declare type React$Node = string | number | null;
```

You can also disable this lint for an entire project in your `.flowconfig`:
```
[lints]
libdef-override=off
```

### `internal-type` {#toc-internal-type}
Triggers when you use an internal Flow type directly in your code. Internal types are implementation details that have public-facing alternatives you should use instead.

This lint is enabled as an error by default.

There are two categories of internal types that trigger this lint:

**Dollar-prefixed utility types** such as `$Omit`, `$Enum`, and `$EnumValue` have non-dollar equivalents (`Omit`, `Enum`, `EnumValue`):

```js flow-check
type A = $Omit<{x: number, y: number}, 'x'>; // Error
type B = Omit<{x: number, y: number}, 'x'>; // Ok
```

**`React$` types** such as `React$Node` and `React$Context` should be accessed via the `React` namespace using dot syntax (`React.Node`, `React.Context`):

```js flow-check
import * as React from 'react';

type A = React$Node; // Error
type B = React.Node; // Ok
```

### `invalid-this-arg` {#toc-invalid-this-arg}
Triggers when `call`, `apply` or `bind` is used to give a method a receiver other than the one it was read from. Rebinding a method whose `this` is receiver-specific runs its body against an object it was never written for, which is unsound in a class hierarchy — see [Rebinding `this`](../types/classes.md#toc-this-rebinding) for why. The permitted shapes are `obj.method.call(obj, ...)` and `this.method.bind(this)`.

This lint is enabled as an error by default.

```js flow-check
type Counter = {
  count: number,
  increment(this: Counter): number,
};

declare const counter: Counter;
declare const other: Counter;

counter.increment.call(counter); // Ok
counter.increment.call(other); // Error
```

A method that accepts any receiver (`this: unknown`, `this: any`, or no `this` annotation and no use of `this` in the body) is not restricted, and a `call`/`apply`/`bind` member that resolves on the receiver itself rather than on `Function.prototype` is not affected.

### `nonstrict-import` {#toc-nonstrict-import}
Used in conjunction with [Flow Strict](../strict/index.md). Triggers when importing a non `@flow strict` module. When enabled, dependencies of a `@flow strict` module must also be `@flow strict`.

### `react-intrinsic-overlap` {#toc-react-intrinsic-overlap}
Triggers when a local definition shares its name with an intrinsic JSX element (such as `div`, `span`, or `input`) and has a type that could be used as a React component. Because JSX treats lowercase element names as intrinsics, writing `<div />` always refers to the HTML element, never to a local binding called `div`. If the local binding is a function, class, abstract component, callable object, or `mixed`, this overlap is likely a mistake that leads to confusing behavior.

This lint is off by default.

A local function whose name overlaps with an intrinsic:
```js flow-check
// flowlint react-intrinsic-overlap:error
import * as React from 'react';

declare function div(): React.Node; // ERROR
<div />;
```

A local variable with a non-component type does not trigger the lint, because there is no ambiguity:
```js flow-check
// flowlint react-intrinsic-overlap:error
import * as React from 'react';

const div = 2 / 3;
<div />; // OK
```

To fix the error, rename the local definition so it does not collide with an intrinsic element name:
```js flow-check
// flowlint react-intrinsic-overlap:error
import * as React from 'react';

declare function MyDiv(): React.Node;
<MyDiv />;
```

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

### `unsafe-object-assign` {#toc-unsafe-object-assign}
Triggers on any use of `Object.assign`. Flow's support for `Object.assign` is unsafe, and you should use object spreads instead.

This lint is enabled as an error by default.

`Object.assign` mutates the first argument in place, which is difficult to track through the type system. The spread syntax (`{...obj1, ...obj2}`) is a safer alternative because it always creates a new object, and Flow can type it precisely.

```js flow-check
const defaults = {color: 'red', size: 10};
const overrides = {color: 'blue'};

const config = Object.assign(defaults, overrides); // Error
```

Use the spread syntax instead:

```js flow-check
const defaults = {color: 'red', size: 10};
const overrides = {color: 'blue'};

const config = {...defaults, ...overrides}; // Ok
```

### `untyped-import` {#toc-untyped-import}
Triggers when you import from an untyped file. Importing from an untyped file
results in those imports being typed as `any`, which is unsafe.

### `untyped-type-import` {#toc-untyped-type-import}
Triggers when you import a type from an untyped file. Importing a type from an
untyped file results in an `any` alias, which is typically not the intended behavior.
Enabling this lint brings extra attention to this case and can help improve Flow
coverage of typed files by limiting the spread of implicit `any` types.

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
