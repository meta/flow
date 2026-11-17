import * as React from 'react';

declare export function compose<A, B, R>(
  f1: (a: A) => R,
  f2: (b: B) => A,
): (b: B) => R;

export function mapProps<InputProps extends {...}, OutputProps extends {...}>(
  mapperFn: (props: InputProps) => OutputProps,
): (c: component(...OutputProps)) => component(...InputProps) {
  return Component => props => <Component {...mapperFn(props)} />;
}

export function withProps<Props extends {...}, ExtraProps extends {...}>(
  extraFn: (props: Props) => $Exact<ExtraProps>,
): (c: component(...{
  ...$Exact<Props>,
  ...$Exact<ExtraProps>,
})) => component(...$Exact<Props>) {
  return Component => props => <Component {...props} {...extraFn(props)} />;
}
