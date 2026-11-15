import * as React from 'react';

type Base = {readonly id: string, ...};
type WithMap<T> = {readonly map: Map<string, T>, ...};

// An inexact object type of several properties meets an intersection one property at a time, as
// in subtyping: `extra` comes from `A`, and `map` from `WithMap<T>`, so `T` meets the `Map` of the
// props, invariantly.
declare function listing<T, A>(
  extra: A,
  render: (props: WithMap<T> & A) => void,
  items: ReadonlyArray<T>,
): T;

type RenderProps = {readonly extra: number, readonly map: Map<string, Base>, ...};
declare const render: (props: RenderProps) => void;
declare const items: ReadonlyArray<{readonly id: string, readonly name: string}>;
declare const bad: ReadonlyArray<{readonly id: number}>;

listing({extra: 1}, render, items) as Base; // ok: T = Base
listing({extra: 1}, render, bad); // error: `number` is not `string`

// The config of a component that is an alias of such an intersection.
type Props<T> = Readonly<{tier: T, tierMap: Map<string, T>, ...}>;
type Config<A, T> = Props<T> & A;

declare component Listing<A, T extends Base = Base>(
  additionalProps?: A,
  tierComponent: component(...Config<A, T>),
  tiers: ReadonlyArray<T>,
);

component Card(extra: number, ...props: Props<Base>) {
  return null;
}

declare const nodes: ReadonlyArray<{readonly id: string, readonly level: ?number}>;

<Listing additionalProps={{extra: 1}} tierComponent={Card} tiers={nodes} />; // ok: T = Base
