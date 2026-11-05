import * as React from 'react';

type Props = Readonly<{
  children: React.Node,
  fallback?: null | (string => React.Node),
  resetCount?: number,
}>;

class Boundary<TProps extends Props = Props> extends React.PureComponent<TProps> {
  static defaultProps: Readonly<{resetCount: number}> = {resetCount: 0};
}

<Boundary fallback={null}>
  <div />
</Boundary>; // ok
<Boundary resetCount={1}>
  <div />
</Boundary>; // ok
<Boundary extra={1}><div /></Boundary>; // error: `extra` is not a prop of `Boundary`
