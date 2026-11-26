---
title: Function/Class Components
slug: /react/function-and-class-components
description: "Legacy patterns for typing React function and class components in Flow, including props, default props, and state."
---

:::danger
Function and class component typing patterns are legacy. For new code, use [Component Syntax](./component-syntax.md) instead.
:::

Adding Flow types to your [React components](https://react.dev/learn/your-first-component) is incredibly powerful. After typing
your component, Flow will statically ensure that you are using the component in
the way it was designed to be used.

## Functional Components {#toc-functional-components}

Adding Flow types to a functional component is the same as [adding types to a standard function](../types/functions.md).
Just create an object type for the props and Flow will ensure that the props passed to the component match up with what is expected.

```js flow-check
import React from 'react';

type Props = {
  foo: number,
  bar?: string,
};

function MyComponent(props: Props) {
  props.doesNotExist; // Error! You did not define a `doesNotExist` prop.

  return <div>{props.bar}</div>;
}

<MyComponent foo={42} />
```


### Adding Default Props to Functional Components {#toc-adding-default-props-to-functional-components}

A nice pattern to add default props to functional components is to use
[destructuring with default values](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Operators/Destructuring_assignment/#default_value).
By destructuring the props in the function parameter, you can assign a value to any props that are not passed
to the component (or passed with the value `undefined`).

```js flow-check
import React from 'react';

type Props = {
  foo?: number, // foo is optional to pass in.
  bar: string, // bar is required.
};

function MyComponent({foo = 42, bar}: Props) {
  // Flow knows that foo is not null or undefined
  const baz = foo + 1;
  return <div>{bar}: {baz}</div>;
}

// And we don't need to include foo.
<MyComponent bar={"abc"} />;
```

> **Note:** React 19 no longer applies `defaultProps` at runtime for function components, and Flow has never treated it as making the prop optional. Use destructuring defaults instead:

```js flow-check
function MyComponent({foo = 42}: {foo?: number}) { /* ... */ }
```

## Class Components {#toc-class-components}

To Flowify a [class component](https://react.dev/reference/react/Component#defining-a-class-component), the type of the props can be passed as the first
argument to the `React.Component` type. This will have the same effect as adding types
to the `props` parameter of a function component.

```js flow-check
import React from 'react';

type Props = {
  foo: number,
  bar?: string,
};

class MyComponent extends React.Component<Props> {
  render(): React.Node {
    this.props.doesNotExist; // Error! You did not define a `doesNotExist` prop.

    return <div>{this.props.bar}</div>;
  }
}

<MyComponent foo={42} />;
```

Now wherever we use `this.props` in our React component Flow will treat it as
the `Props` type we defined.

> **Note:** If you don't need to use the `Props` type again you could also
> define it inline: `extends React.Component<{ foo: number, bar?: string }>`.

`React.Component<Props, State>` is a [generic type](../types/generics.md) that takes two type
arguments: props and state. The second type argument, `State`, is optional. By
default it is `undefined` so you can see in the example above we did not include
`State`. We will learn more about state in the next section...

### Adding State {#toc-adding-state}

To add a type for state to your React class component: create a new object
type, in the example below we name it `State`, and pass it as the second type
argument to `React.Component`.

```js flow-check
import React from 'react';

type Props = { /* ... */ };

type State = {
  count: number,
};

class MyComponent extends React.Component<Props, State> {
  state: State = {
    count: 0,
  };

  componentDidMount() {
    setInterval(() => {
      this.setState(prevState => ({
        count: prevState.count + 1,
      }));
    }, 1000);
  }

  render(): React.Node {
    return <div>Count: {this.state.count}</div>;
  }
}

<MyComponent />;
```

In the example above we are using a [React `setState()` updater function](https://react.dev/reference/react/Component#setstate)
but you could also pass a partial state object to `setState()`.

> **Note:** If you don't need to use the `State` type again you could also
> define it inline: `extends React.Component<{}, { count: number }>`.

### Using Default Props for Class Components {#toc-using-default-props-for-class-components}

React supports the notion of `defaultProps` which you can think of as default
function arguments. When you create an element and do not include a prop
which has a default then React will substitute that prop with its corresponding
value from `defaultProps`. Flow supports this notion as well. To type default
props add a `static defaultProps` property to your class.

```js flow-check
import React from 'react';

type Props = {
  foo: number, // foo is required.
  bar: string, // bar is required.
};

class MyComponent extends React.Component<Props> {
  static defaultProps: {foo: number} = {
    foo: 42, // ...but we have a default prop for foo.
  };
}

// So we don't need to include foo.
<MyComponent bar={"abc"} />
```

> **Note:** You don't need to make `foo` nullable in your `Props` type. Flow
> will make sure that `foo` is optional if you have a default prop for `foo`.

If you add a type annotation to `defaultProps` you can define the type as
```js flow-check
type DefaultProps = {
  foo: number,
};
```
and spread that into the `Props` type:
```js flow-check
type DefaultProps = {
  foo: number,
};

type Props = {
  ...DefaultProps,
  bar: string,
};
```
This way you avoid duplicating the properties that happen to have a default value.

### Refs in Class Components {#toc-refs-in-class-components}

To use a ref in a class component, add a property to your class and assign the
result of `React.createRef` to it.

```js flow-check
import * as React from 'react';

class MyComponent extends React.Component<{}> {
  // The `null` here is important because you may not always have the instance.
  buttonRef: {current: null | HTMLButtonElement};

  constructor() {
    super({});
    this.buttonRef = React.createRef<HTMLButtonElement>();
  }

  render(): React.Node {
    return <button ref={this.buttonRef}>Toggle</button>;
  }
}
```

One notable difference between `useRef` and `createRef` is that `createRef` does not accept
a default value. It will initialize the ref with the value `null`. This is because
DOM elements will not exist until the first render of `MyComponent` and so a `null` value
must be used.

Note that the ref value is wrapped in an object with a `current` property.

## See Also {#toc-see-also}

- [Classes](../types/classes.md) — how class types work in Flow (relevant for class components)
- [Objects](../types/objects.md) — props are typed as object types
- [Annotation Requirement](../lang/annotation-requirement.md) — when explicit type annotations are needed on component props
