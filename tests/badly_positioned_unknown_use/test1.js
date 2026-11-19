import * as React from 'react';

declare export function foo<P>(
  Component: React.ComponentType<{...P}>,
): React.ComponentType<P>; // error: P is not known to be an object

class Comp extends React.Component<{...}, {...}> {}

function f<
  Comp extends React.ComponentType<{...}>,
>(): (c: Comp) => {
...} {
  return function() {
    return {}
  };
}

// OK: type spreads accept inexact inputs
var x = f<React.ComponentType<{...}>>()(foo(Comp));
