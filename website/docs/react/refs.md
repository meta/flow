---
title: Refs
slug: /react/refs
description: "How to type React refs in Flow: the useRef hook and its React.RefObject return type, accepting a ref with React.RefSetter, callback refs, and reading ref.current."
---

React allows you to grab the instance of an element or component with [refs](https://react.dev/learn/manipulating-the-dom-with-refs).

## Refs with the `useRef` hook {#toc-refs-in-functional-components}

Inside a component, refs are created with the `useRef` hook:

```js flow-check
import {useRef} from 'react';
import * as React from 'react';

component MyComponent() {
  const buttonRef = useRef<null | HTMLButtonElement>(null);
  return <button ref={buttonRef}>Toggle</button>;
}
```

Note that `useRef` wraps the ref value in an object with a `current` property. This must be
reflected in the type of anything accepting the ref value.

The type `useRef` returns is [`React.RefObject<T>`](./types.md#toc-react-refobject). Use it to annotate anything that accepts the ref object, such as a helper that reads `current`:

```js flow-check
import {useRef} from 'react';
import * as React from 'react';

function focusInput(ref: React.RefObject<null | HTMLInputElement>) {
  if (ref.current != null) {
    ref.current.focus();
  }
}

component MyComponent() {
  const inputRef = useRef<null | HTMLInputElement>(null);
  return <input ref={inputRef} onClick={() => focusInput(inputRef)} />;
}
```

### Reading `ref.current` {#toc-reading-ref-current}

The `current` property starts out `null` and is only populated after the element mounts, so its type includes `null`. Refine it with a `!= null` check before using it:

```js flow-check
import {useRef} from 'react';
import * as React from 'react';

component MyComponent() {
  const inputRef = useRef<null | HTMLInputElement>(null);

  const focusInput = () => {
    inputRef.current.focus(); // ERROR: `current` may be null
    if (inputRef.current != null) {
      inputRef.current.focus(); // OK: refined to non-null
    }
  };

  return <input ref={inputRef} onClick={focusInput} />;
}
```

## Accepting a ref with `React.RefSetter` {#toc-accepting-a-ref}

To let a parent attach a ref to your component, add a `ref` parameter typed with [`React.RefSetter<T>`](./types.md#toc-react-refsetter), where `T` is the instance you expose. With [Component Syntax](./component-syntax.md) this is just another parameter:

```js flow-check
import {useRef} from 'react';
import * as React from 'react';

component FancyInput(ref: React.RefSetter<HTMLInputElement>) {
  return <input ref={ref} />;
}

component Form() {
  const inputRef = useRef<null | HTMLInputElement>(null);
  return <FancyInput ref={inputRef} />;
}
```

You write the `ref` parameter the same way regardless of the React version you target; behind the scenes Component Syntax compiles it to a [`React.forwardRef`](https://react.dev/reference/react/forwardRef) call (React 18) or a plain `ref` prop (React 19). The ref must be a direct parameter, not nested in a rest parameter. See [Ref Parameters](./component-syntax.md#ref-parameters) for the full explanation.

## Callback refs {#toc-callback-refs}

Instead of a ref object, you can pass a function as a ref. React calls it with the element instance when it mounts, and with `null` when it unmounts, so the parameter type is `T | null`:

```js flow-check
import * as React from 'react';

component MyComponent() {
  const setButtonRef = (button: HTMLButtonElement | null) => {
    if (button != null) {
      button.focus();
    }
  };

  return <button ref={setButtonRef}>Toggle</button>;
}
```

A `ref` parameter typed [`React.RefSetter<T>`](./types.md#toc-react-refsetter) accepts both a ref object and a callback ref, so a component written with the pattern above works with either.

## Deriving ref types {#toc-deriving-ref-types}

To name the type behind a ref without spelling it out, Flow provides two utilities:

- [`React.ElementRef<typeof Component>`](./types.md#toc-react-elementref) gives the instance type a component exposes (for example `HTMLInputElement` for an `'input'`, or a class instance for a class component).
- [`React.RefOf<Component>`](./types.md#toc-react-refof) gives the type of the `current` field on a component's `ref` prop, or `undefined` if it has none.

## See Also {#toc-see-also}

- [React Types](./types.md) — [`React.RefSetter`](./types.md#toc-react-refsetter), [`React.RefObject`](./types.md#toc-react-refobject), [`React.RefOf`](./types.md#toc-react-refof), [`React.ElementRef`](./types.md#toc-react-elementref), and other built-in React types
- [Ref Parameters](./component-syntax.md#ref-parameters) — how Component Syntax compiles the ref parameter
- [Function and Class Components](./function-and-class-components.md) — typing class components, including their refs
