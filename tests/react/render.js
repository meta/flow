import * as React from 'react';

/**
 * Basic render: Class component tests
 */
class A extends React.Component<{...}, void> {
  render = (): React.Node => undefined;
}
class B extends React.Component<{...}, void> {
  render = (): React.Node => null;
}

<A />;
// OK

<B />;
// OK

/**
 * Basic render: stateless functional component tests
 */
const C: React.ComponentType<{...}> = props => undefined;
const D: React.ComponentType<{...}> = props => {
  return undefined;
};
const E: React.ComponentType<{...}> = props => {
  return null;
};

<C foo="bar" />;
// OK: React$Node permits undefined

<D foo="bar" />;
// OK: React$Node permits undefined

<E foo="bar" />;
// OK

type UnfixedPropsType = {foo?: string};

/**
 * PropsType inference: Class component test
 */
class F extends React.Component<UnfixedPropsType, void> {
  render = (): React.Node => this.props.foo;
}
// OK: React$Node permits undefined

/**
 * PropsType inference: stateless functional component test
 */
const G: React.ComponentType<UnfixedPropsType> = props => {
  return props.foo;
};
// OK: React$Node permits undefined
