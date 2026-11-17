---
title: Hook Syntax
slug: /react/hook-syntax
description: "How to use Flow's hook syntax for first-class React hook declarations with built-in Rules of React enforcement."
---

Hook Syntax gives React hooks their own syntax and types, separate from regular functions.
Flow uses this distinction to enforce the [Rules of React](https://react.dev/reference/rules).

## Basic Usage

The primary difference between writing a function and a hook is the `hook` keyword:
```js flow-check
import {useState, useEffect} from 'react';

hook useOnlineStatus(initial: boolean): boolean {
  const [isOnline, setIsOnline] = useState(initial);
  useEffect(() => {
    // ...
  }, []);
  return isOnline;
}
```

Hooks can be called just like regular functions:
```js flow-check
import * as React from 'react';

hook useOnlineStatus(): boolean {
    return true;
}

component StatusBar() {
  const isOnline = useOnlineStatus();
  return <h1>{isOnline ? '✅ Online' : '❌ Disconnected'}</h1>;
}
```

Hooks can be exported just like normal functions:

```js flow-check
export hook useNamedExportedHook(): boolean {
    return true;
}

export default hook useDefaultExportedHook(): boolean {
    return true;
}
```

## Hook Type Annotations
There are a few cases where you might wish to define a value as having the type of a
hook. Because function types and hook types aren’t compatible (more on this below!),
we also introduce a new syntax for hook type annotations, which is simply the
existing function type annotation but preceded by `hook`.

```js flow-check
declare function experiment(name: string): boolean;
declare hook useOnlineStatus(x: boolean): boolean;
declare hook useAlwaysOnlineStatus(x: boolean): boolean;

export const useGKOnlineStatus: hook (x: boolean) => boolean =
  experiment('show_online_status')
  ? useOnlineStatus
  : useAlwaysOnlineStatus
```

## Enforcing the Rules of React with Hook Syntax
With hook syntax, we can now unambiguously distinguish syntactically between hooks and
non-hooks. Flow will use this information to enforce a number of the rules of hooks and
[Rules of React](https://react.dev/reference/rules) generally.

### Preventing Unsafe Mutation
According to the [Rules of React](https://react.dev/reference/rules), refs aren’t allowed
to be read from or written to while a component is rendering, and the return value of
other hooks (especially `useState`) cannot be safely mutated directly at all. By making
Flow aware of hooks as a first-class concept, we can now detect these issues in many cases
and raise errors early, rather than depending on testing to uncover them.

```js flow-check
import {useState, useEffect, useRef} from 'react';
import * as React from 'react';

component MyComponent() { 
  const ref = useRef<?number>(null);
  const [state, setState] = useState<{ val: number }>({val: 0});

  state.val = 42; // Error: cannot mutate return value of hook

  return (
    <div>
      {ref.current /* Error: cannot read ref during rendering */}
    </div>
  );
}
```
Flow currently prevents component props from being modified within the component.
Hook syntax allows us to extend this checking to hooks, and will let us detect and
raise errors when illegal mutations occur within hook declarations.

```js flow-check
hook useIllegalMutation(values: Array<number>) {
  values[0] = 42; // Error: mutating argument to hook
  // ...
}
```

### Preventing Conditional Hook Calls
[The Rules of Hooks](https://react.dev/reference/rules#rules-of-hooks) prohibit hooks
from being called conditionally. This is covered by [React's ESLint plugin](https://www.npmjs.com/package/eslint-plugin-react-hooks),
but now Flow will check for these violations too.

```js flow-check
hook useOnlineStatus(): boolean {
    return true;
}

component StatusBar(shouldShowOnlineStatus: boolean) {
  if (shouldShowOnlineStatus) {
    const onlineStatus = useOnlineStatus(); // Error
  }

  return null;
}
```

### Preventing Conflation of Hooks and Functions
The distinction between hooks and regular functions is reflected in the Flow type system.
Because of the different properties that hooks and functions must obey, it’s a Flow error
to pass a value defined as a hook into a position that expects a function type, and
an error to pass a regular JavaScript function into a position that expects a hook.

Tracking hook-ness as part of the function type lets Flow catch violations that an AST-based linter (such as `eslint-plugin-react-hooks`) cannot reach: at the call site, there's no syntactic clue that the callee will invoke `fn` as a hook, so a syntactic check has nothing to flag. The whole-program type information Flow tracks is what makes the diagnostic possible.

```js flow-check
import {useState, useEffect} from 'react';

hook useMultiplier(x: number): number {
  const [y, setY] = useState(1);
  useEffect(() => { setY(0) })
  return x * y;
}

component Mapper(args: Array<number>) {
  const multArgs = args.map(useMultiplier); // Error [react-rule-hook-incompatible]
  
  return multArgs;
}
```

In addition, Flow enforces that callees with hook-like names inside hooks and components
are indeed hooks. We also ensure that callees inside of regular function definitions
are never hooks.

```js flow-check
hook useHook() { return null }

function regularJavascript() {
  const x = useHook(); // Error: cannot call a hook outside of a component or hook
}

component Component() { 
  const renamedHook = useHook;
  renamedHook(); // Error: cannot call a hook whose name does not begin with `use`

  return null;
}
```

## Availability {#toc-availability}

Hook Syntax is enabled by default since Flow v0.317, by the same option that enables [Component Syntax](./component-syntax.md). On older versions, add `component_syntax=true` to the `[options]` section of your `.flowconfig`.

## See Also {#toc-see-also}

- [Generics](../types/generics.md) — generic type parameters used in hook signatures
- [Functions](../types/functions.md) — hooks follow similar typing rules to functions
