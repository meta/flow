---
title: Higher-order Components
slug: /react/hoc
description: "How to type React higher-order components in Flow using Component Types."
---

:::danger
Higher-order components are discouraged in modern React code and will not be updated for [Component Syntax](./component-syntax.md).
Consider using a hook to accomplish your task instead.
:::

A popular pattern in React is the [higher-order component pattern][], so it's
important that we can provide effective types for higher-order components in
Flow. If you don't already know what a higher-order component is then make sure
to read the [React documentation on higher-order components][] before
continuing.

[higher-order component pattern]: https://legacy.reactjs.org/docs/higher-order-components.html
[React documentation on higher-order components]: https://legacy.reactjs.org/docs/higher-order-components.html

You can make use of the [Component Types](./component-types.md) to annotate your higher order components.

### The Trivial HOC {#toc-the-trivial-hoc}

Let's start with the simplest HOC:

```js flow-check
import * as React from 'react';

function trivialHOC<Config extends {...}>(
  Component: component(...Config),
): component(...Config) {
  return Component;
}
```

This is a basic template for what your HOCs might look like. At runtime, this HOC doesn't
do anything at all. Let's take a look at some more complex examples.

### Injecting Props {#toc-injecting-props}

A common use case for higher-order components is to inject a prop.
The HOC automatically sets a prop and returns a component which no longer requires
that prop. For example, consider a navigation prop. How would one type this?

To remove a prop from the config, we can take a component that includes the
prop and return a component that does not. It's best to construct these
types using object type spread.

```js flow-check
import * as React from 'react';

type InjectedProps = {foo: number}

function injectProp<Config extends {...}>(
  Component: component(...{...Config, ...InjectedProps, ...})
): component(...Config) {
  return function WrapperComponent(
    props: Config,
  ) {
    return <Component {...props} foo={42} />;
  };
}

function MyComponent(props: {
  a: number,
  b: number,
  ...InjectedProps,
  ...
}): React.Node {
  return <div>{props.foo}</div>;
}

const MyEnhancedComponent = injectProp(MyComponent);

// We don't need to pass in `foo` even though `MyComponent` requires it:
<MyEnhancedComponent a={1} b={2} />; // OK

// We still require `a` and `b`:
<MyEnhancedComponent a={1} />; // ERROR
```

### Exporting Wrapped Components {#toc-exporting-wrapped-components}

If you try to export a wrapped component, chances are that you'll run into a missing annotation error:

```js flow-check
import * as React from 'react';

function trivialHOC<Config extends {...}>(
  Component: component(...Config),
): component(...Config) {
  return Component;
}

type Props = Readonly<{bar: number, foo?: number}>;

function MyComponent({bar, foo = 3}: Props): React.Node {
  return <div>{bar + foo}</div>;
}

export const MyEnhancedComponent = trivialHOC(MyComponent); // ERROR
```

You can add an annotation to your exported component using component types:

```js flow-check
import * as React from 'react';

function trivialHOC<Config extends {...}>(
  Component: component(...Config),
): component(...Config) {
  return Component;
}

type Props = Readonly<{bar: number, foo?: number}>;

function MyComponent({bar, foo = 3}: Props): React.Node {
  return <div>{bar + foo}</div>;
}

export const MyEnhancedComponent: component(...Props) = trivialHOC(MyComponent); // OK
```

## See Also {#toc-see-also}

- [Generics](../types/generics.md) — HOC patterns make heavy use of generic type parameters
- [Functions](../types/functions.md) — HOCs are functions that take and return components
- [Annotation Requirement](../lang/annotation-requirement.md) — HOC exports typically require type annotations
