import * as React from 'react';

// The props of a function component with an optional or defaulted props parameter go into an
// optional slot.
function WithDefault(props: {value?: number} = {}): React.Node {
  return null;
}
<WithDefault
  value={
    'a' // error
  }
/>;

function Optional(props?: {items?: Array<string>}): React.Node {
  return null;
}
declare function propsOf<P extends {...}>(c: React.ComponentType<P>): P;
propsOf(Optional) as {items?: Array<string>};
