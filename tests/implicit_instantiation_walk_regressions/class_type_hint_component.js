import * as React from 'react';

type PlainObj = {__proto__: null, ...};
type Selector<out Val, in Arg = unknown> = {
  (Arg): Val,
  ...
};

declare function container<TProps extends PlainObj, TState extends PlainObj>(
  selector: Selector<TState, TProps>,
  Renderer: React.ComponentType<TState & {...}>,
): Class<React.Component<TProps, TState>>;

declare const selector: Selector<{a: number}>;

// `Class<C<T>>` against a component type: the config goes to the props of `C`.
const C: React.ComponentType<{}> = container(selector, (state: {a: number}) => null); // ok
