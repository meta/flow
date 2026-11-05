import * as React from 'react';

type Context = Readonly<{logger: string, log: string => void}>;
type WithContext<TProps> = Readonly<{...TProps, ...Context, ...}>;

declare function withContext<
  Config extends {...},
  Ref extends React.RefSetter<unknown> = React.RefSetter<unknown>,
>(
  Component: component(ref?: Ref, ...WithContext<Config>),
): component(ref?: Ref, ...Config);

type Props = Readonly<{children?: React.Node, maxLines?: number}>;

component Body(...{children, maxLines = 9, ...rest}: WithContext<Props>) {
  return null;
}

// The config of a component with declared props, against a config: the rest props get the
// config without the declared props.
const B = withContext(Body); // ok
<B maxLines={3} />; // ok
<B maxLines="no" />; // error
