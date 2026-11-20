/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @flow
 */

import * as React from 'react';

component Panel(
  header: React.Node,
  children: React.Node,
  footer as footerContent?: React.Node,
) {
  return (
    <section className="panel">
      <header>{header}</header>
      <div className="panel-body">{children}</div>
      {footerContent != null && <footer>{footerContent}</footer>}
    </section>
  );
}

function repeat(element: React.MixedElement, times: number): React.MixedElement {
  const copies: Array<React.MixedElement> = [];
  for (let i = 0; i < times; i++) {
    copies.push(element);
  }
  return <>{copies}</>;
}

component App() {
  return (
    <Panel header={<h1>Title</h1>} footer={<small>fin</small>}>
      {repeat(<span>hi</span>, 3)}
    </Panel>
  );
}

export default App;
