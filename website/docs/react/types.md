---
title: React Type Reference
slug: /react/types
description: "Reference for all React utility types exported by Flow, including React.Node, React.MixedElement, and more."
---

React exports a handful of utility types that may be useful to you when typing
advanced React patterns. In previous sections we have seen a few of them. The
following is a complete reference for each of these types along with some
examples for how/where to use them.

These types are all exported as named type exports from the `react` module. If
you want to access them as members on the `React` object (e.g.
[`React.Node`](#toc-react-node)) and
you are importing React as an ES module then you can import `React` either as a
namespace or as a default import:

```js flow-check
import * as React from 'react';

<div /> as React.MixedElement; // OK
```

```js flow-check
import React from 'react';

<div /> as React.MixedElement; // OK
```

If you are using CommonJS you can also require React:

```js flow-check
const React = require('react');
```

You can also use named type imports in either an ES module environment or a
CommonJS environment:

```js flow-check
import type {Node} from 'react';
```

## Content types {#toc-content-types}

Types for the content a component renders or accepts as children.

### `React.Node` {#toc-react-node}

This represents any node that can be rendered in a React application.
`React.Node` can be `undefined`, `null`, a boolean, a number, a string, a React
element, a React portal, or an iterable (including arrays) of any of those, possibly nested.

Its most common use is typing the `children` a component takes in. With
[Component Syntax](./component-syntax.md), declare `children` like any other parameter:

```js flow-check
import * as React from 'react';

component MyComponent(children: React.Node) {
  return <div>{children}</div>;
}
```

All `react-dom` JSX intrinsics have `React.Node` as their children type:
`<div>`, `<span>`, and all the rest.

`React.Node` is also a good default to annotate the return type of a function
component or a class `render` method. (Component Syntax infers this return type, so
you do not write it there.)

```js flow-check
import * as React from 'react';

function MyComponent(props: {}): React.Node {
  return null;
}

class MyClassComponent extends React.Component<{}> {
  render(): React.Node {
    return null;
  }
}
```

:::info TypeScript comparison
Use `React.Node`, not TypeScript's `ReactNode`. `React.Node` already includes
`null`, `undefined`, and `false`, so you do not need to add those yourself when a
value may be absent.
:::

### `React.MixedElement` {#toc-react-mixedelement}

The most general type of all React elements (similar to `unknown` for all values).

Unlike [`React.Node`](#toc-react-node), `React.MixedElement` is a single element only. It does not include strings, numbers, arrays, `null`, `undefined`, or `false`.

A common use case of this type is when we want to annotate an element with a type that hides the element details. For example
```js flow-check
import * as React from 'react';

const element: React.MixedElement = <div />;
```

### Choosing between `React.Node`, `renders`, and `React.MixedElement` {#toc-choosing-node-renders-element}

When you need to type a value that holds renderable React content, pick based on how much you want to constrain it:

- [`React.Node`](#toc-react-node): anything renderable (elements, strings, numbers, arrays, `null`, `undefined`, `false`). Use it for `children` and any prop or return position that accepts arbitrary content. This is the default.
- [`renders Foo`](./render-types.md): a value that renders the specific element `Foo` (or something that renders `Foo`). Use it when a prop must be a particular design-system element rather than arbitrary content.
- [`React.MixedElement`](#toc-react-mixedelement): exactly one React element, with its specific type hidden. Unlike `React.Node`, it does not include strings, numbers, arrays, `null`, `undefined`, or `false`. Use it when you need a single element but do not care which one.

### `React.ChildrenArray<T>` {#toc-react-childrenarray}

A React children array can be a single value or an array nested to any level.
It is designed to be used with the [`React.Children` API](https://react.dev/reference/react/Children).

For example if you want to get a normal JavaScript array from a
`React.ChildrenArray<T>` see the following example:

```js flow-check
import * as React from 'react';

// A children array can be a single value...
const childrenSingle: React.ChildrenArray<number> = 42;
// ...or an arbitrarily nested array.
const childrenNested: React.ChildrenArray<number> = [[1, 2], 3, [4, 5]];

// Using the `React.Children` API can flatten a single value into an array.
const array: Array<number> = React.Children.toArray(childrenSingle);
```

## Component and element types {#toc-component-element-types}

Types that describe components, element types, and element keys.

### `React.ComponentType<Props>` {#toc-react-componenttype}

The type of a component that accepts an object of props `Props`. It is equivalent to the
[Component Type](./component-types.md) `component(...Props)`:

```js flow-check
import * as React from 'react';

type Props = {foo: number};

// `React.ComponentType<Props>` is equivalent to `component(...Props)`:
declare const a: React.ComponentType<Props>;
declare const b: component(...Props);

a as component(...Props); // OK
b as React.ComponentType<Props>; // OK
```

A `React.ComponentType<Props>` value can be rendered with the props it expects:

```js flow-check
import * as React from 'react';

declare const Comp: React.ComponentType<{foo: number}>;

<Comp foo={3} />; // OK
<Comp />; // ERROR: `foo` is required
```

### `React.ElementType` {#toc-react-elementtype}

Similar to [Component Types](./component-types.md) except it also
includes JSX intrinsics (strings).

The definition for `React.ElementType` is roughly:

```js flow-check
import * as React from 'react';

type ElementType =
  | string
  | React.ComponentType<empty>;
```

It accepts both JSX intrinsics and components:

```js flow-check
import * as React from 'react';

component Button() {
  return <button />;
}

const intrinsic: React.ElementType = 'div'; // OK: a JSX intrinsic
const custom: React.ElementType = Button; // OK: a component
```

### `React.Key` {#toc-react-key}

The type of the `key` prop on React elements, a union of strings and numbers:

```js flow-check
type Key = string | number;
```

You rarely need to write `React.Key` yourself: `key` is a special prop handled by React, passed directly on elements (`<Row key={id} />`), and it appears mostly in React's own type definitions.

## Ref types {#toc-ref-types}

Types for working with refs. See [Refs](./refs.md) for the full guide.

### `React.RefObject<T>` {#toc-react-refobject}

The type of the value returned by the [`useRef`](https://react.dev/reference/react/useRef) hook. Use it to annotate anything that holds or accepts that ref object.

```js flow-check
import {useRef} from 'react';

declare class Dog {}
hook useRefDemo() {
  const ref: React.RefObject<?Dog> = useRef<?Dog>(null);
}
```

See [Refs](./refs.md#toc-refs-in-functional-components) for how it is used with `useRef`.

### `React.RefSetter<T>` {#toc-react-refsetter}

The general type of the [ref prop on React elements][], and the type you give a component's `ref` parameter (for example `component Foo(ref: React.RefSetter<T>)`). A `React.RefSetter<T>` accepts both a ref object with `T | null` stored in the `current` property and a ref function (callback ref) accepting `T`.

[ref prop on React elements]: https://react.dev/learn/manipulating-the-dom-with-refs

Give it to a component's `ref` parameter:

```js flow-check
import * as React from 'react';

component TextInput(ref: React.RefSetter<HTMLInputElement>) {
  return <input ref={ref} />;
}
```

The ref function will take one and only argument which will be the element
instance which is retrieved using
[`React.ElementRef<typeof Component>`](#toc-react-elementref) or null since
[React will pass null into a ref function when unmounting][].

[React will pass null into a ref function when unmounting]: https://react.dev/learn/manipulating-the-dom-with-refs#how-to-manage-a-list-of-refs-using-a-ref-callback

The definition for `React.RefSetter<T>` is roughly:

```js flow-check
type RefSetter<in T> =
  | { writeonly current: T | null, ... }
  | ((instance: T | null) => unknown)
  | null
  | void;
```

Here `in` and `writeonly` are [contravariance](../lang/variance.md#toc-contravariance) annotations, which are rarely seen in everyday code.

See [Accepting a ref with `React.RefSetter`](./refs.md#toc-accepting-a-ref) for how to accept a ref in a component.

### `React.RefOf<Component>` {#toc-react-refof}

When using [Component Syntax](./component-syntax.md), `React.RefOf<Component>` will give you
the type of the `current` field on the `ref` prop of the component. If there is no `ref` prop
on the component it will return `void`.

```js flow-check
import * as React from 'react';

component TextInput(ref: React.RefSetter<HTMLInputElement>) {
  return <input ref={ref} />;
}

// Forward a ref of the same instance type that `TextInput` exposes.
component LabeledInput(ref: React.RefSetter<React.RefOf<TextInput>>) {
  return <TextInput ref={ref} />;
}
```

See [Deriving ref types](./refs.md#toc-deriving-ref-types) for how this relates to refs.

### `React.ElementRef<typeof Component>` {#toc-react-elementref}

Gets the instance type for a React element. The instance will be different for
various component types:

- `component(ref: React.RefSetter<Instance>)` will return the `Instance` type.
- React class components will be the class instance. So if you had
  `class Foo extends React.Component<{}> {}` and used
  `React.ElementRef<typeof Foo>` then the type would be the instance of `Foo`.
- React function components do not have a backing instance and so
  `React.ElementRef<typeof Bar>` (when `Bar` is `function Bar() {}`) will give
  you the void type.
- JSX intrinsics like `div` will give you their DOM instance. For
  `React.ElementRef<'div'>` that would be `HTMLDivElement`. For
  `React.ElementRef<'input'>` that would be `HTMLInputElement`.

Note that `typeof Component` must be the type *of* a React component so you need to use
`typeof` as in `React.ElementRef<typeof MyComponent>`.

```js flow-check
import * as React from 'react';

component TextInput(ref: React.RefSetter<HTMLInputElement>) {
  return <input ref={ref} />;
}

// The instance type a component exposes via its ref.
declare const instance: React.ElementRef<typeof TextInput>;
instance as HTMLInputElement | null; // OK

// For a JSX intrinsic you get the DOM instance directly.
declare const div: React.ElementRef<'div'>;
div as HTMLDivElement; // OK
```

See [Deriving ref types](./refs.md#toc-deriving-ref-types) for how this relates to refs.

## Prop types {#toc-prop-types}

Types that extract the props a component accepts.

### `React.PropsOf<Component>` {#toc-react-propsof}
When `Component` is written using [Component Syntax](./component-syntax.md), `React.PropsOf<Component>`
gives you the type of an object that you must pass in to instantiate `Component` with JSX.
Importantly, the props with defaults are optional in the resulting type.

For example:
```js flow-check
import * as React from 'react';

component MyComponent(foo: number, bar: string = 'str') {
  return null;
}

// Only foo is required
({foo: 3}) as React.PropsOf<MyComponent>;
```

### `React.ComponentProps<typeof Component>` {#toc-react-componentprops}

Use `React.ComponentProps` to extract the props accepted by a component or JSX intrinsic.

Like [React.PropsOf](#toc-react-propsof), this utility gets the type of the object that you must pass in to a
component in order to instantiate it via `createElement()` or JSX. While `PropsOf` takes in an element of
a component, which is convenient when using [Component Syntax](./component-syntax.md), `ComponentProps` takes in the type of a component
instead. Use `typeof` as in `React.ComponentProps<typeof Component>`.

Props with defaults are optional in the resulting type. The result also includes
any `ref` prop the component accepts.

```js flow-check
import * as React from 'react';

component MyComponent(foo: number = 42) {
  return foo;
}

// `foo` is optional because it has a default value.
({}) as React.ComponentProps<typeof MyComponent>; // OK
```

For JSX intrinsics, pass the tag name, such as `React.ComponentProps<'div'>`.

## Deprecated types {#toc-deprecated-types}

Types kept for backward compatibility. Prefer the alternatives noted below.

### `ExactReactElement_DEPRECATED<typeof Component>` {#toc-react-element}

:::warning
This is an exact replacement of the removed `React.Element` type since 0.245.
Use [`React.MixedElement`](#toc-react-mixedelement) or [`React.Node`](#toc-react-node) instead.
To enforce design system constraints, use [render types](./render-types.md) instead.
:::

The type for the value of a JSX element, parameterized by the element's component type.

## Deleted types {#toc-deleted-types}

### `React.ElementConfig<typeof Component>` {#toc-react-elementconfig}

This type has been removed. Use [`React.ComponentProps<typeof Component>`](#toc-react-componentprops) instead.

## See Also {#toc-see-also}

- [Utility Types](../types/utilities.md) — Flow's general-purpose utility types
- [Generics](../types/generics.md) — many React types are parameterized with generics
