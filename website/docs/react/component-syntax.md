---
title: Component Syntax
slug: /react/component-syntax
description: "How to use Flow's component syntax to declare React components with built-in type system support for props, refs, and render types."
---

[Components](https://react.dev/learn/your-first-component) are the foundation for building UIs in React. Component Syntax is the standard way to write them in Flow: a dedicated component primitive, enabled by default, with several advantages over plain function components:

- **Individual named params instead of a props object.** Removes the destructuring-and-typing duplication of `({name}: {name: string})` and the need to wrap props in `Readonly<{...}>` — component params are read-only by default.
- **No return type annotation.** Flow infers and enforces `React.Node`, and rejects components that implicitly return on any branch.
- **Optional [`renders`](./render-types.md) clause** that constrains what JSX shape the component is allowed to produce, enabling composition contracts across wrapper components and HOCs.
- **Structural rules enforced at parse/type-check time:** no `this`, no nested component definitions, and components can only be rendered as JSX — they cannot be called as plain functions. See [Rules for Components](#rules-for-components) below.
- **Better support for [React refs](https://react.dev/learn/manipulating-the-dom-with-refs)** via the dedicated ref parameter position.

:::info TypeScript comparison
`component` syntax is Flow-only. TypeScript models components as plain functions plus a props type. Flow's type-checker enforces rules that TypeScript's plain-function model leaves to convention or ESLint rules. See [Flow's component syntax](../flow-vs-typescript.md#toc-component-syntax) for the full comparison.
:::

## Basic Usage
You can declare a component with Component Syntax similar to how you'd declare a function:

```js flow-check
import * as React from 'react';

component Introduction(name: string, age: number) {
  return <h1>My name is {name} and I am {age} years old</h1>
}
```

You can use a component directly in JSX: `<Introduction age={9} name="Mr. Flow" />`.

There are a few important details to notice here:

1. the prop parameter names declared in the Introduction component are the same as the prop names passed to Introduction in JSX
2. the order of the parameters in the declaration does not need to match the order that they are provided in JSX

## Parameters

### Children

`children` is the most common prop, and with Component Syntax it is just an ordinary parameter; declare it like any other. Its type is usually [`React.Node`](./types.md#toc-react-node), which covers anything renderable (elements, strings, arrays, `null`, and more):

```js flow-check
import * as React from 'react';

component Card(title: string, children: React.Node) {
  return (
    <section>
      <h2>{title}</h2>
      {children}
    </section>
  );
}

<Card title="Welcome">
  <p>Hello!</p>
</Card>;
```

:::info TypeScript comparison
There is no need for a `React.PropsWithChildren` helper or for adding `children` to a props object type. `children` is just a parameter, and `React.Node` is the type to give it.
:::

Sometimes you want children to be more specific than "anything renderable". To restrict which elements can be passed, give `children` a [render type](./render-types.md) instead of `React.Node`. For example, a `Menu` whose children must be `MenuItem`s can require them with `renders*`:

```js flow-check
import * as React from 'react';

component MenuItem(label: string) {
  return <li>{label}</li>;
}

component Menu(children: renders* MenuItem) {
  return <ul>{children}</ul>;
}

<Menu>
  <MenuItem label="Home" />
  <MenuItem label="Profile" />
</Menu>;
```

[Render Types](./render-types.md) cover the full set of constraints: `renders` for a single required element, `renders?` for an optional one, and `renders*` for any number of children.

### String Parameters/Renaming Parameters

Components also allow you to rename parameters, which is useful when your parameter name is not a valid JavaScript identifier:

```js flow-check
import * as React from 'react';

component RenamedParameter(
  'required-renamed' as foo: number,
  'optional-renamed' as bar?: number,
  'optional-with-default-renamed' as baz?: number = 3,
) {
  foo as number; // OK
  bar as number | void; // OK
  baz as number; // OK

  return <div />;
}
```

### Rest Parameters

Sometimes you do not want to list out every prop explicitly because you do not intend to reference them individually in your component. This is common when you are writing a component that wraps another and need to pass props from your component to the inner one:

```js flow-check
import * as React from 'react';

// star.js
component Star(color: string, size: number) {
  return <div />;
}
type StarProps = React.PropsOf<Star>;

// blue_star.js
component BlueStar(...props: StarProps) {
  return <Star {...props} color="blue" />;
}
```

Rest parameters use an object type as an annotation, which means you can use existing type utilities like object spreads and Pick to annotate more complex prop patterns:

```js flow-check
import * as React from 'react';

component OtherComponent(foo: string, bar: number) {
  return <div>{foo} {bar}</div>;
}

component FancyProps(
  ...props: {
    ...React.PropsOf<OtherComponent>,
    additionalProp: string,
  }
) {
  return <OtherComponent foo={props.foo} bar={props.bar} />;
}
```

#### Disjoint Union Props

You can use a rest parameter with a [disjoint union](../types/unions.md#toc-disjoint-object-unions) type to define a component that accepts one of several prop shapes. Refine the props inside the component body using the discriminant field:

```js flow-check
import * as React from 'react';

type TextProps = {kind: 'text', text: string};
type ImageProps = {kind: 'image', src: string, alt: string};
type Props = TextProps | ImageProps;

component MediaItem(...props: Props) {
  if (props.kind === 'text') {
    return <span>{props.text}</span>;
  } else {
    return <img src={props.src} alt={props.alt} />;
  }
}
```

### Optional Parameters and Defaults

Components allow you to declare optional parameters and specify defaults:

```js flow-check
import * as React from 'react';

component OptionalAndDefaults(
  color: string = "blue",
  extraMessage?: string,
) {
  let message = `My favorite color is ${color}.`;
  if (extraMessage != null) {
    message += `\n${extraMessage}`;
  }
  return <p>{message}</p>
}

<OptionalAndDefaults /> // No error, all of the parameters are optional!
```

### Destructuring Parameters

The `as` operator also allows you to destructure your parameters:

```js flow-check
import * as React from 'react';

component Destructuring(
  config as {color, height}: Readonly<{color: number, height: number}>,
) { return <div /> }
```

Rest parameters can be destructured without using as:

```js flow-check
import * as React from 'react';

type Props = Readonly<{ color: string, height: number }>;

component DestructuredRest(...{color, height}: Props) { return <div /> }
```

### Ref Parameters

To access refs in components you just need to add a ref parameter.

```js flow-check
import * as React from 'react';

component ComponentWithARef(ref: React.RefSetter<HTMLElement>) {
  return <div ref={ref} />;
}
```

Behind the scenes, Component Syntax generates the right code for your target React version: when targeting React 18 it wraps the component in the required [`React.forwardRef`](https://react.dev/reference/react/forwardRef) call, and when targeting React 19 it passes `ref` through as an ordinary prop (React 19 no longer needs `forwardRef`). You declare the `ref` parameter the same way regardless of target. The one restriction for refs is that they must be defined as an inline parameter; refs within rest params are not supported, because Component Syntax needs to statically determine the ref from the component definition.

See [Refs](./refs.md) for more on typing refs, including the `useRef` hook, `React.RefObject`, `React.RefSetter`, callback refs, and reading `ref.current`.

## Rules for Components

Component Syntax enforces a few restrictions in components to help ensure correctness:

1. The return values must be a subtype of `React.Node`, otherwise React may crash while rendering your component.
2. All branches of a component must end in an explicit return. Even though `undefined` is a valid return value, we've seen many instances where an explicit return would have prevented bugs in production.
3. You cannot use `this` in a component.

4. You cannot define a component inside another component or a hook. This triggers a `nested-component` error because a nested component gets a new identity on every render, causing React to unmount and remount it each time. Use a regular function instead if the inner "component" doesn't use hooks, or move the component to the top level.

5. Components cannot be called as plain functions — they can only be rendered as JSX. Calling `Foo(props)` directly errors with `[react-rule-call-component]`. Use `<Foo {...props} />` instead.

So these components are invalid:

```js flow-check
import * as React from 'react';

component InvalidReturnValue() {
  return new Object(); // ERROR: Value does not match `React.Node` type
}

component ImplicitReturn(someCond: boolean) { // ERROR
  if (someCond) {
    return <h1>Hello World!</h1>;
  }
  // ERROR: No return in this branch
}

component UsesThis() {
  this.foo = 3; // ERROR: Accessing `this`
  return null;
}

component Greeting() {
  return <h1>Hello</h1>;
}
const elem = Greeting({}); // ERROR [react-rule-call-component]: use <Greeting /> instead
```

## `import typeof` with Generic Components {#toc-import-typeof-generics}

When you use [`import typeof`](../types/typeof.md) on a generic component's default export, the imported type alias exposes the generic as a required type argument. This means you cannot use the imported type without supplying the argument explicitly:

```js
// GenericComp.js
import * as React from 'react';

component MyComp<T>(data: T, render: (item: T) => React.Node) {
  return render(data);
}

export default MyComp;

// Consumer.js
import typeof MyComp from './GenericComp';

type CompType = MyComp; // Error: missing 1 type argument
```

To sidestep the eager-instantiation error at declaration time, use a namespace `import typeof` and index into it. Note that this collapses the polymorphism — the resulting type is treated as a concrete (non-polymorphic) component type, so you still cannot instantiate it with a type argument like `CompType<number>`:

```js
import typeof * as GenericCompModule from './GenericComp';

type CompType = GenericCompModule['default']; // Declaration works, but CompType is no longer polymorphic
```

## Availability {#toc-enable-component-syntax}

Component Syntax is enabled by default since Flow v0.317. On older versions, add `component_syntax=true` to the `[options]` section of your `.flowconfig`.

## See Also {#toc-see-also}

- [Generics](../types/generics.md) — using generic type parameters in component props
- [Variance](../lang/variance.md) — understanding read-only props and covariance
