// @flow

// Synthesizing an element of a polymorphic component synthesizes its children
// too, so a child element's name is read while the enclosing element's hint is
// evaluated. That name is a `jsx::Identifier`, not an expression, so the walk
// over the hint's synthesizable expressions used to miss it: `Line`'s entry was
// read before its def ran, resolving to a placeholder `any` that is cached and
// then reused by every later read -- silently dropping the error below.

import * as React from 'react';

component Chart<TPoint>(format: (x: number) => string, children: React.Node) {
  return null;
}

component App() {
  const el: React.Node = <Line value={0 as unknown} />; // error

  return (
    <Chart format={value => ''}>
      <Line value={0} />
    </Chart>
  );
}

component Line(value: number) {
  return null;
}

export {App};
