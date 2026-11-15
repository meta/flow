import * as React from 'react';

declare function memo<Config extends {...}, Renders extends React.Node = React.Node>(
  component: component(...Config) renders Renders,
): component(...Config) renders Renders;

// `Renders` of `memo` gets the rigid `Renders` of `wrap`, not the members of its bound.
function wrap<Config extends {...}, Renders extends React.Node = React.Node>(
  c: component(...Config) renders Renders,
): component(...Config) renders Renders {
  return memo(c); // was an error
}

type Ref<M> = {m: M};
declare function load<M, Config extends {...}, Renders extends React.Node = React.Node>(
  ref: Ref<M>,
  resolve: (m: M) => component(...Config) renders Renders,
): component(...Config) renders Renders;
const identity = <M>(m: M): M => m;

// The same through a generic value.
function deferred<Config extends {...}, Renders extends React.Node = React.Node>(
  ref: Ref<component(...Config) renders Renders>,
): component(...Config) renders Renders {
  return load(ref, identity); // was an error
}
