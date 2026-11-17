//@flow

const React = require('react');

type Props = Readonly<{
  a: number,
  ...
}>;

function a<P extends Props>() {
  declare const a: React.ComponentType<P>;
  declare const b: React.Component<{readonly a: number, ...}>;
  a as React.ComponentType<{a: number, ...}>; // nope, contravariance
  b as React.Component<P>; // nope
}

type DefaultProps = {
  b: string,
};

function connect<TProps extends {...}, SProps extends TProps>(): component(ref?: React.RefSetter<unknown>, ...SProps) {
  return class extends React.Component<TProps> {};
}

function hoc<Props extends {...}, Component extends component(...Props)>(
  WrappedComponent: Component,
): React.ComponentType<React.ComponentProps<Component>> {
  return (props: Props) => <WrappedComponent {...props} />;
}

function HOC<Config extends {...}, Instance>(
  x: component(ref: React.RefSetter<Instance>, ...Config),
): component(ref: React.RefSetter<Instance>, ...Config) {
  return x;
}

function WrapInDivWithExtraProp<Props extends {...}>(
  X: component(...Props),
): component(...{...$Exact<Props>, baz: number}) {
  const C = ({baz, ...props}: {...$Exact<Props>, baz: number}) => (
    <div>
      {baz}
      <X {...props} />
    </div>
  );
  C.defaultProps = {...X.defaultProps}; // nope
  return C;
}

function mapProps<InputProps extends {...}, SubInputProps extends InputProps, OutputProps extends {...}>(
  mapperFn: (props: InputProps) => OutputProps,
): (c: component(...OutputProps)) => component(...SubInputProps) {
  return Component => props => <Component {...mapperFn(props)} />;
}

export type PCCP = {
  commentsConfig: number,
  ...
};

export function withCommentsConfig<TProps extends PCCP>(
  Component: component(...TProps),
): component(...Omit<TProps, keyof PCCP>) {
  return function (props) {
    return <Component {...props} commentsConfig={42} />;
  };
}
