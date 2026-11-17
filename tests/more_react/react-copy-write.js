//@flow
var React = require("react");

import type {ComponentType} from 'react';

export type Recipe<T> = (draft: T, state: Readonly<T>) => void;
export type Mutate<T> = (recipe: Recipe<T>) => void;

type ConsumerRender<S> = (...args: S) => globalThis.React.Node;

type ProviderProps<T> = {
  children: globalThis.React.Node,
  initialState?: T,
};

export type Provider<T> = ComponentType<ProviderProps<T>>;

type GetReturnType = <T, S>(f: (x: T) => S) => S;

type ConsumerProps<T, TSelect extends ReadonlyArray<(state: T) => unknown>> = {
  select?: TSelect,
  children?: ConsumerRender<{[K in keyof TSelect]: ReturnType<TSelect[K]>}>,
  render?: ConsumerRender<{[K in keyof TSelect]: ReturnType<TSelect[K]>}>,
};

type Selector<T, R> = (state: T) => R;

export type Store<T> = {
  readonly Provider: Provider<T>,
  readonly Consumer: {
    <TSelect extends ReadonlyArray<(state: T) => unknown> = ReadonlyArray<(state: T) => unknown>>(
      props: ConsumerProps<T, TSelect>,
    ): globalThis.React.Node,
    // Need the following to fake this as a functional component
    displayName?: ?string,
    propTypes?: any,
    contextTypes?: any,
 ...  },
  readonly mutate: Mutate<T>,
  createSelector<R>(selector: Selector<T, R>): Selector<T, R>,
 ...};

declare const store : Store<number>;
<store.Consumer></store.Consumer>
