// A rest that removes the props of another type parameter (`Omit<AllProps, keyof ServiceProps>`)
// is reversed once that type parameter has evidence, as the flow-based solve reverses it once its
// tvar has lower bounds: `AllProps` has the props of `ServiceProps` too, so the component that
// takes `Props` fits.
import * as React from 'react';

declare function connect<AllProps extends {...}, ServiceProps extends {...}>(
  mapToProps: (own: Omit<AllProps, keyof ServiceProps>) => ServiceProps,
): (c: React.ComponentType<AllProps>) => React.ComponentType<Omit<AllProps, keyof ServiceProps>>;

type Props = {title: string, description: string, ...};
declare function mapToProps(own: {location?: string, ...}): Props;
declare function Banner(props: Props): React.Node;

const wrap = connect(mapToProps);
wrap(Banner); // ok
