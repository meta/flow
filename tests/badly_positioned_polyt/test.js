import * as React from 'react';

declare opaque type T;
type Props = {t: T};
declare const props: Props;

type D<Context extends {readonly context: unknown, ...}, Props> = Omit<
  Props, // error: Props is not known to be an object
  'context'
>;
type X<Context extends {readonly context: unknown, ...}, Props> = {
  x: D<Context, Props>,
};
class Foo<
  Props extends {},
  Context extends {readonly context: unknown, ...},
> extends React.Component<
  X<Context, Props>
> {}
//Error: cannot create Foo
<Foo x={props.t} />;

type Y<Context extends {readonly context: unknown, ...}, Props> = {
  y: Omit<Props, 'context'>; // error: Props is not known to be an object
};
class Bar<
  Props extends {readonly context: unknown, ...},
  Context extends {},
> extends React.Component<
  Y<Context, Props> // error: Context is not known to have a `context` property
> {}
//Error: cannot create Bar
<Bar y={props.t} />;
