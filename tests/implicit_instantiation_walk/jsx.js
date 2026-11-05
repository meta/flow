// @flow

import * as React from 'react';

// The props of an element against the props its component takes.
{
  function Fun<T>(props: {x: T, y: T}): React.Node {
    return null;
  }
  component Comp<T>(x: T, y: T, ...rest: {z: T}) {
    return null;
  }
  <Fun x="a" y={1} />; // ok
  <Comp x="a" y={1} z={true} />; // ok
}
