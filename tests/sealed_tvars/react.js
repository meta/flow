//@flow
import * as React from 'react';

const RC = <T />;

function T(): undefined {}

declare function Wrap(_: {children: unknown}): React.Node;

<Wrap>
  {/* ok */ true ? <div></div> : <div></div>}
  <div></div>
</Wrap>;

function Dialog() {
  return 1;
}

Dialog.c = ''; // OK
Dialog.Prop = function (): void {
  return Dialog.c as empty;
};
