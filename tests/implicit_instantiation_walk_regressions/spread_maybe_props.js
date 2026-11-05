import * as React from 'react';

type Entity = {entityID: string};
type Props<TRender extends React.Node> = {children: TRender, name: string, ...?Entity};

component Inner<TRender extends React.Node>(...props: Props<TRender>) renders TRender {
  return props.children;
}

// A spread of a maybe type makes a union of objects: the props are walked against it.
component Outer<TR extends React.Node>(...props: Props<TR>) renders TR {
  return <Inner {...props} />; // ok
}
