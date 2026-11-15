import * as React from 'react';

type Meta = Readonly<{event?: ?string, payload?: ?string}>;
type WithMeta<T extends {...}> = Readonly<{...$Exact<T>, ...Meta}>;

declare function hoc<C extends {...}>(
  Component: React.ComponentType<WithMeta<C>>,
): React.ComponentType<C>;

// The config of `Button` is an application of `WithMeta` too, but the configs meet as the
// object types they evaluate to, not argument by argument: `C` is the hint's `Props`, and
// `WithMeta<Props>` is `Props` again.
type Props = WithMeta<{label: string}>;
declare component Button(...props: Props);
hoc(Button) as React.ComponentType<Props>; // ok

declare component Label(label: string);
type LabelProps = WithMeta<React.PropsOf<Label>>;
declare component LabelButton(...props: LabelProps);
hoc(LabelButton) as React.ComponentType<LabelProps>; // ok

// Here the configs meet as `{node: this}` and `{node: Node<State>}`: `S` is `State` through the
// `this` of `Node<S>`, not through the first argument of `RendererProps`, which its object type
// does not use.
type State = {key: string, ...};
type RendererProps<S extends State, N extends Node<S>> = Readonly<{node: N, ...}>;
type Renderer<S extends State, N extends Node<S>> = component(...RendererProps<S, N>);

class Node<S extends State> {
  constructor(state: S, renderer: Renderer<S, this>) {}
}

component NodeRenderer(...props: RendererProps<State, Node<State>>) {
  return null;
}
new Node({key: 'a'}, NodeRenderer); // ok

type Holder<T> = Readonly<{value: T}>;
class Box<S extends {...}> {
  constructor(state: S, holder: Holder<this>) {}
}
declare const holder: Holder<Box<{a: number, ...}>>;
new Box({a: 1, b: 2}, holder); // ok
