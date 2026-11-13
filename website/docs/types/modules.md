---
title: Module Types
slug: /types/modules
description: "How to import and export Flow types between modules, including type-only imports and CommonJS support."
---

import {SinceVersion} from '../../components/VersionTags';

Flow lets you share types between files using `import type` and `export type` syntax.

```js
import type {MyType} from './types';
```

## Importing and exporting types {#toc-importing-and-exporting-types}

You can export [type aliases](./aliases.md), [interfaces](./interfaces.md), and [classes](./classes.md) from one file and import them in another.

**`exports.js`**

```js flow-check
export default class MyClass {};
export type MyObject = { /* ... */ };
export interface MyInterface { /* ... */ };
```

**`imports.js`**

```js
import type MyClass, {MyObject, MyInterface} from './exports';
```

> ***Don't forget to add `@flow` at the top of your file, otherwise Flow won't report errors***.

## Importing and exporting values as types {#toc-importing-and-exporting-values}

Flow also supports importing the type of values exported by other modules using [`typeof`](./typeof.md).

**`exports.js`**

```js flow-check
const myNumber = 42;
export default myNumber;
export class MyClass { /* ... */ };
```

**`imports.js`**

```js
import typeof MyNumber from './exports';
import typeof {MyClass} from './exports';

const x: MyNumber = 1; // Works: like using `number`
```

Just like other type imports, this code can be stripped away by a compiler so
that it does not add a runtime dependency on the other module.

When the imported value is generic — for example, a function or React hook with type parameters — the imported type stays parameterizable. You can write `useState<number>` against an `import typeof {useState} from 'react'`-imported type to obtain the `number`-specialization:

```js flow-check
import typeof {useState} from 'react';

hook useCounter(useStateNum: useState<number>): number {
  const [count, setCount] = useStateNum(0);
  setCount(c => c + 1);
  return count;
}
```

For the component-specific gotcha when `import typeof` is applied to a generic component's default export, see [`import typeof` with generic components](../react/component-syntax.md#toc-import-typeof-generics).

## Inline import type annotations {#toc-inline-import-types}

<SinceVersion version="0.339" />

An inline import type annotation refers to an exported type directly, so you don't need a local `import type` binding.

The examples below use this **`config.js`** file:

```js
export type Options = {port: number};
export type Box<T> = {value: T};

export class Config {
  port: number = 8080;
}

export class Nested {
  static Config: typeof Config = Config;
}

const defaultPort: number = 8080;
export default defaultPort;
```

In another file, a named `import type` and an inline annotation refer to the same exported type. This also works for type-only exports:

```js
import type {Options} from './config';

type InlineOptions = import('./config').Options;
const named: Options = {port: 8080}; // OK
const inline: InlineOptions = named; // OK
const wrong: InlineOptions = {port: 'http'}; // ERROR

type NumberBox = import('./config').Box<number>;
const box: NumberBox = {value: 42}; // OK
```

Pass type arguments after the exported name, as in `Box<number>`. A class export gives you the type of its instances, not of the constructor value. You can also reach nested names that the module exposes:

```js
type ConfigInstance = import('./config').Config;
declare const config: ConfigInstance;
config.port as number; // OK
0 as ConfigInstance; // ERROR: not a Config instance

type NestedInstance = import('./config').Nested.Config;
config as NestedInstance; // OK
```

### Types of imported values {#toc-inline-imported-values}

Use [`typeof`](./typeof.md#toc-imported-values) to get the type of an exported value. This form is the inline counterpart of `import typeof`. Select the default export with `.default`, or omit the member to get the type of the module-namespace value:

```js
import typeof {Config as NamedConstructor} from './config';

type Constructor = typeof import('./config').Config;
type DefaultPort = typeof import('./config').default;
type ModuleNamespace = typeof import('./config');

declare const exports: ModuleNamespace;
exports.Config as Constructor; // OK: class value, not an instance
exports.Config as NamedConstructor; // OK: same as the named import
exports.default as DefaultPort; // OK: number
exports.Options; // ERROR: a type-only export is not a value
```

### Resolution and runtime behavior {#toc-inline-import-resolution}

A bare `import('./config')` is not a valid type on its own. To refer to the module namespace, write `typeof import('./config')`. Flow also checks that each referenced name is exported:

```js
type Module = import('./config'); // ERROR: use typeof for the namespace
type Missing = import('./config').NotExported; // ERROR: no such export
```

The module argument must be a string literal; a variable or other expression is not allowed. Flow uses its normal module-resolution rules during type checking, so the referenced module must be resolvable.

These forms appear only in type annotations. At runtime, JavaScript's `import('./config')` expression loads and executes the module and returns a promise; an inline import type does none of these things. When you [strip the annotations](../tools/babel.md), the inline imports disappear and no runtime import is emitted.

### Adoption {#toc-inline-import-types-adoption}

To use this syntax, set `experimental.tslib_syntax=true` in `.flowconfig` and use Flow and flow-parser 0.339.0+ and Prettier 3.10.0+ with its built-in `flow` parser.

## See Also {#toc-see-also}

- [Declaration Files](../declarations/index.md) — `.flow` files that declare types for untyped modules
- [Library Definitions](../libdefs/index.md) — declaring types for third-party packages
- [Typeof Types](./typeof.md) — `import typeof` for extracting the type of an imported value
- [Type Aliases](./aliases.md) — defining named types that can be exported and imported
