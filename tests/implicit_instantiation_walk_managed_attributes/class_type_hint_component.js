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

// `Class<C<T>>` against a component type: the config goes to the props of `C`, as without
// `React.ReactManagedAttributes`.
const C: React.ComponentType<{}> = container(selector, (state: {a: number}) => null); // ok

// The config of the hint, whose props are read-only, does not agree with the props the selector
// takes, so it gives no evidence, and `TProps` is those props.
type Props = {a: number};
declare const propsSelector: Selector<{a: number}, Props>;
const P: React.ComponentType<Props> = container(propsSelector, (state: {a: number}) => null); // ok
