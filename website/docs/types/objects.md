---
title: Objects
slug: /types/objects
description: "How to type objects in Flow: exact vs inexact types, optional and read-only properties, indexers, object spread, and more."
---

Object types describe the shape of JavaScript objects.

```js flow-check
const obj: {foo: number, bar: boolean} = {foo: 1, bar: true};
```

Flow supports several variants:
- **[Exact object types](#exact-and-inexact-object-types)** (default): exactly a set of properties, e.g. `{a: number}`.
- **[Inexact object types](#exact-and-inexact-object-types)**: at least a set of properties, e.g. `{a: number, ...}`.
- **[Indexed object types](#toc-objects-as-maps)**: used as a map, e.g. `{[string]: boolean}`.

:::info TypeScript comparison
TypeScript's object types look identical but are open by default — `{x: number}` admits extra properties in TypeScript, while Flow's [exact-by-default](../flow-vs-typescript.md#toc-object-exactness) `{x: number}` rejects them. TypeScript's excess-property check only fires on direct literal assignment. The Flow spelling of TS's open shape is the inexact form `{x: number, ...}`. Two related divergences: Flow's [class/interface/object subtyping](../flow-vs-typescript.md#toc-classes-nominal) is asymmetric because classes are nominal, and combining object types uses Flow's [type-level spread](../flow-vs-typescript.md#toc-type-spread) `{...A, b: T}` rather than the TS intersection `A & {b: T}`.
:::

## When to use this {#toc-when-to-use}

Use object types for plain data and simple structures. When you need methods, inheritance, or nominal typing, use [classes](./classes.md). When you need to accept both class instances and plain objects with the same shape, use [interfaces](./interfaces.md).

## Optional object type properties {#toc-optional-object-type-properties}

In JavaScript, accessing a property that doesn't exist evaluates to
`undefined`. This is a common source of errors in JavaScript programs, so Flow
turns these into type errors.

```js flow-check
const obj = {foo: "bar"};
obj.bar; // Error!
```

If you have an object that sometimes does not have a property you can make it
an _optional property_ by adding a question mark `?` after the property name in
the object type.

```js flow-check
const obj: {foo?: boolean} = {};

obj.foo = true;    // Works!
obj.foo = 'hello'; // Error!
```

In addition to their set value type, these optional properties can either be
`void` or omitted altogether. However, they cannot be `null`.

```js flow-check
function acceptsObject(value: {foo?: string}) { /* ... */ }

acceptsObject({foo: "bar"});     // Works!
acceptsObject({foo: undefined}); // Works!
acceptsObject({});               // Works!

acceptsObject({foo: null});      // Error!
```


To make all properties in an object type optional, you can use the [`Partial`](./utilities.md#toc-partial) utility type:

```js flow-check
type Obj = {
  foo: string,
};

type PartialObj = Partial<Obj>; // Same as `{foo?: string}`
```

To make all properties in an object type required, you can use the [`Required`](./utilities.md#toc-required) utility type:

```js flow-check
type PartialObj = {
  foo?: string,
};

type Obj = Required<PartialObj>; // Same as `{foo: string}`
```

## Read-only object properties

You can add [variance](../lang/variance.md) annotations to your object properties.

To mark a property as read-only, you can use the `readonly` keyword.

```js flow-check
type Obj = {
  readonly foo: string,
};

function func(o: Obj) {
  const x: string = o.foo; // Works!
  o.foo = 'hi'; // Error!
}
```

To make all object properties in an object type read-only, you can use the [`Readonly`](./utilities.md#toc-readonly) utility type:

```js flow-check
type Obj = {
  foo: string,
};

type ReadOnlyObj = Readonly<Obj>; // Same as `{readonly foo: string}`
```

You can also mark your properties as write-only with the `writeonly` keyword.

```js flow-check
type Obj = {
  writeonly foo: string,
};

function func(o: Obj) {
  const x: string = o.foo; // Error!
  o.foo = 'hi'; // Works!
}
```

:::info Legacy sigil syntax
The `readonly` and `writeonly` keywords are enabled by default as of Flow 0.315. Older code may use the legacy `+` and `-` sigils (e.g. `{+foo: string}` instead of `{readonly foo: string}`, `{-foo: string}` instead of `{writeonly foo: string}`); these are still recognized but deprecated. See [Modernizing Legacy Syntax](../modernizing-legacy-syntax.md#toc-deprecated-syntax) for migration details.
:::


## Object methods {#toc-object-methods}

Method syntax in objects has the same runtime behavior as a function property. These two objects are equivalent at runtime:

```js flow-check
const a = {
  foo: function () { return 3; }
};
const b = {
  foo() { return 3; }
}
```

However, despite their equivalent runtime behavior, Flow checks them slightly differently. In particular, object
properties written with method syntax are [read-only](../lang/variance.md); Flow will not allow you to write a new value to them.

```js flow-check
const b = {
  foo() { return 3; }
}
b.foo = () => { return 2; } // Error!
```

Additionally, object methods do not allow the use of `this` in their bodies, in order to guarantee simple behavior
for their `this` parameters. Prefer to reference the object by name instead of using `this`.

```js flow-check
const a = {
  x: 3,
  foo() { return this.x; } // Error!
}
const b = {
  x: 3,
  foo(): number { return b.x; } // Works!
}
```

Because method shorthand is read-only while a function-typed field is mutable, the two have different variance at the property. Method shorthand only triggers function-input contravariance, so a wider-input function can flow into a narrower-input method slot. A mutable function-typed field is [invariant](../lang/variance.md), so the same assignment is rejected:

```js flow-check
type ByMethod = {compare(a: string, b: string): number};
type ByField = {compare: (a: string, b: string) => number};
declare const wider: {compare: (a: string | number, b: string | number) => number};

const m: ByMethod = wider; // Works — method shorthand is read-only
const f: ByField = wider;  // Error — function-typed field is mutable and therefore invariant

type ByReadonlyField = {readonly compare: (a: string, b: string) => number};
const rof: ByReadonlyField = wider; // Works — readonly field is covariant, like method shorthand
```

Marking the field `readonly` restores the covariant property position, which is why the third assignment succeeds where the mutable field rejected it. See [Flow's method-parameter contravariance](../flow-vs-typescript.md#toc-variance-methods) for the contrast with TypeScript's bivariance and the full mechanics.

## Object type inference {#toc-object-type-inference}

When you create an object value, its type is set at the creation point. You cannot add new properties,
or modify the type of existing properties.

```js flow-check
const obj = {
  foo: 1,
  bar: true,
};

const n: number  = obj.foo; // Works!
const b: boolean = obj.bar; // Works!

obj.UNKNOWN; // Error - prop `UNKNOWN` is not in the object value
obj.foo = true; // Error - `foo` is of type `number`
```

If you supply a type annotation, you can add properties missing in the object value as optional properties:

```js flow-check
const obj: {
  foo?: number,
  bar: boolean,
} = {
  // `foo` is not set here
  bar: true,
};

const n: number | void = obj.foo; // Works!
const b: boolean = obj.bar; // Works!

if (b) {
  obj.foo = 3; // Works!
}
```

You can also give a wider type for a particular property:

```js flow-check
const obj: {
  foo: number | string,
} = {
  foo: 1,
};

const foo: number | string = obj.foo; // Works!
obj.foo = "hi"; // Works!
```

The empty object can be interpreted as a [dictionary](#toc-objects-as-maps), if you supply the appropriate type annotation:

```js flow-check
const dict: {[string]: number} = {}; // Works!
```

You may need to add type annotations to an object literal, if it references itself recursively (beyond simple cases):

```js flow-check
const Utils = { // Error
  foo() {
    return Utils.bar();
  },
  bar() {
    return 1;
  }
};

const FixedUtils = { // Works!
  foo(): number {
    return FixedUtils.bar();
  },
  bar(): number {
    return 1;
  }
};
```

## Exact and inexact object types

Exact object types are the default.

Inexact objects (denoted with the `...`) allow extra properties to be passed in:

```js flow-check
function method(obj: {foo: string, ...}) { /* ... */ }

method({foo: "test", bar: 42}); // Works!
```

> **Note:** This is because of ["width subtyping"](../lang/depth-subtyping.md#toc-width-subtyping).

But exact object types do not:

```js flow-check
function method(obj: {foo: string}) { /* ... */ }

method({foo: "test", bar: 42}); // Error!
```

[Intersections](./intersections.md) of exact object types are usually impossible types: a value would have to be exactly `A` *and* exactly `B` simultaneously, which is [uninhabitable](./intersections.md#toc-impossible-intersection-types) as soon as `A` and `B` differ at all. The operation that combines exact object types is [object type spread](#object-type-spread), which mirrors the runtime semantics of value-level spread directly — own properties only, later keys overwrite earlier ones, and exactness propagates:

```js flow-check
type FooT = {foo: string};
type BarT = {bar: number};

type FooBarT = {...FooT, ...BarT};
const fooBar: FooBarT = {foo: '123', bar: 12}; // Works!

type FooBarFailT = FooT & BarT;
const fooBarFail: FooBarFailT = {foo: '123', bar: 12}; // Error!
```

## Object type spread

Just like you can spread object values, you can also spread object types:

```js flow-check
type ObjA = {
  a: number,
  b: string,
};

const x: ObjA = {a: 1, b: "hi"};

type ObjB = {
  ...ObjA,
  c: boolean,
};

const y: ObjB = {a: 1, b: 'hi', c: true}; // Works!
const z: ObjB = {...x, c: true}; // Works!
```

Object type spread uses the declared properties of an inexact object type. Unknown
properties allowed by the inexact type do not participate in the spread:

```js flow-check
type Inexact = {
  a: number,
  b: string,
  ...
};

type ObjB = {
  c: boolean,
  ...Inexact,
};

const x: ObjB = {a:1, b: 'hi', c: true};
```

The same applies to the unknown keys of an [indexer](#toc-objects-as-maps):
properties spread after it keep their declared types. An object with an indexer
still can't be spread after other properties, because its keys may overwrite them:

```js flow-check
type Dict = {
  [string]: number,
};

type ObjB = { // Error!
  c: boolean,
  ...Dict,
};

const x: ObjB = {a: 1, b: 2, c: true};
```

Unlike value spread, which only copies own properties and therefore rejects
Flow interface values, object type spread uses the declared shape of an
[interface](./interfaces.md), including inherited properties:

```js flow-check
interface Iface {
  a: number;
  b: string;
}

type ObjB = {
  c: boolean,
  ...Iface,
};

const x: ObjB = {a: 1, b: 'hi', c: true};
```

## Objects as maps {#toc-objects-as-maps}

JavaScript includes a [`Map`](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Map) class,
but it is still very common to use objects as maps as well. In this use case, an object
will likely have properties added to it and retrieved throughout its lifecycle.
Furthermore, the property keys may not even be known statically, so writing out
a type annotation would not be possible.

For objects like these, Flow provides a special kind of property, called an
"indexer property." An indexer property allows reads and writes using any key
that matches the indexer key type.

```js flow-check
const o: {[string]: number} = {};
o["foo"] = 0;
o["bar"] = 1;
const foo: number = o["foo"];
```

An indexer can be optionally named, for documentation purposes:

```js flow-check
const obj: {[user_id: number]: string} = {};
obj[1] = "Julia";
obj[2] = "Camille";
obj[3] = "Justin";
obj[4] = "Mark";
```

When an object type has an indexer property, property accesses are assumed to
have the annotated type, even if the object does not have a value in that slot
at runtime. It is the programmer's responsibility to ensure the access is safe,
as with arrays.

```js flow-check
const obj: {[number]: string} = {};
obj[42].length; // No type error, but will throw at runtime
```

Indexer properties can be mixed with named properties:

```js flow-check
const obj: {
  size: number,
  [id: number]: string
} = {
  size: 0
};

function add(id: number, name: string) {
  obj[id] = name;
  obj.size++;
}
```

You can mark an indexer property as read-only (or write-only) using [variance](../lang/variance.md) annotations:

```js flow-check
type ReadOnly = {readonly [string]: number};
type WriteOnly = {writeonly [string]: number};
```

## Keys, values, and indexed access

You can extract the keys of an object type using the [`keyof`](./utilities.md#toc-keys) utility type:

```js flow-check
type Obj = {
  foo: string,
  bar: number,
};

type T = keyof Obj;

function acceptsKeys(k: T) { /* ... */ }

acceptsKeys('foo'); // Works!
acceptsKeys('bar'); // Works!
acceptsKeys('hi'); // Error!
```

You can extract the values of an object type using the [`Values`](./utilities.md#toc-values) utility type:

```js flow-check
type Obj = {
  foo: string,
  bar: number,
};

type T = Values<Obj>;

function acceptsValues(v: T) { /* ... */ }

acceptsValues(2); // Works!
acceptsValues('hi'); // Works!
acceptsValues(true); // Error!
```

You can get the type of an object type's specific property using [indexed access types](./indexed-access.md):

```js flow-check
type Obj = {
  foo: string,
  bar: number,
};

type T = Obj['foo'];

function acceptsStr(x: T) { /* ... */ }

acceptsStr('hi'); // Works!
acceptsStr(1); // Error!
```

## Arbitrary objects

If you want to accept an arbitrary object safely, there are a couple of patterns you could use.

An empty inexact object `{...}` accepts any object:

```js flow-check
function func(obj: {...}) {
  // ...
}

func({}); // Works!
func({a: 1, b: "foo"}); // Works!
```

It's often the right choice for a [generic](./generics.md) bounded to accept any object:

```js flow-check
function func<T extends {...}>(obj: T) {
  // ...
}

func({}); // Works!
func({a: 1, b: "foo"}); // Works!
```

However, you can't access any properties off of `{...}`.

You can also try using a [dictionary](#toc-objects-as-maps) with [`unknown`](./unknown.md) values, which would allow you to access any property (with a resulting `unknown` type):

```js flow-check
function func(obj: {readonly [string]: unknown}) {
  const x: unknown = obj['bar'];
}

func({}); // Works!
func({a: 1, b: "foo"}); // Works!
```

The type `Object` refers to the global `Object` class. It does not allow arbitrary
property access. Use an object type with declared properties or an indexer when
you need to access those properties.

## Common Issues

### Computed property access (`invalid-computed-prop`) {#toc-invalid-computed-prop}

Flow raises an `invalid-computed-prop` error when you use a computed key to access an object that doesn't have an [indexer type](#toc-objects-as-maps). Flow needs to know the exact set of keys at the type level, so arbitrary computed access on regular object types is not allowed:

```js flow-check
const obj = {a: 1, b: 2};

const key: string = "a";
obj[key]; // Error! Can't use string to index into an object with known keys.
```

To fix this, you have a few options:

**Use `keyof typeof` to narrow the key type** when you know the key is valid:

```js flow-check
const obj = {a: 1, b: 2};

function getVal(key: keyof typeof obj): number {
  return obj[key]; // Works!
}
```

**Use an indexer type** when the object is meant to be used as a dictionary:

```js flow-check
const obj: {[string]: number} = {a: 1, b: 2};

const key: string = "a";
obj[key]; // Works!
```

### Exponential type spread {#toc-exponential-type-spread}

When you spread multiple conditional expressions into an object, Flow computes all possible combinations, which can grow exponentially. This causes an error when it exceeds Flow's limit:

```js flow-check
declare const cond1: boolean;
declare const cond2: boolean;
declare const cond3: boolean;

// Each conditional spread doubles the number of possible types
const obj = {
  ...cond1 ? {a: 1} : {},
  ...cond2 ? {b: 2} : {}, // Error: too many combinations to reason about
  ...cond3 ? {c: 3} : {},
  // Flow must consider 2^3 = 8 possible object types
};
```

To fix this, annotate intermediate variables with a single type that covers all possibilities using optional properties:

```js flow-check
type Flags = {
  a?: number,
  b?: number,
  c?: number,
};

declare const cond1: boolean;
declare const cond2: boolean;

const flags1: Flags = cond1 ? {a: 1} : {};
const flags2: Flags = cond2 ? {b: 2} : {};
const obj: Flags = {...flags1, ...flags2};
```

## See Also {#toc-see-also}

- [Interfaces](./interfaces.md) — structural types that can describe both class instances and objects
- [Classes](./classes.md) — nominally typed values; use interfaces to bridge between classes and object types
- [Tuples](./tuples.md) — fixed-length collections with per-element types
- [Indexed Access Types](./indexed-access.md) — accessing specific property types from object types
- [Utility Types](./utilities.md) — `Partial`, `Required`, `Readonly`, `keyof`, and other object helpers
- [Variance](../lang/variance.md) — covariant (read-only) and contravariant (write-only) properties
- [Depth Subtyping](../lang/depth-subtyping.md) — how width and depth subtyping interact with object types
- [Mapped Types](./mapped-types.md) — systematically transforming object property types
- [Modernizing Legacy Flow Syntax](../modernizing-legacy-syntax.md) — migrating legacy `{| ... |}` exact syntax, `+` / `-` property sigils, and `$Exact<T>` to modern forms
