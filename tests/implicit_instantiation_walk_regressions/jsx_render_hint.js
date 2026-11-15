import * as React from 'react';

component Header() {
  return null;
}
component Other() {
  return null;
}
component Page(header: renders Header, footer?: renders Header) {
  return null;
}
declare component Boundary<
  TChildren extends React.Node = React.Node,
  TFallback extends React.Node | null = React.Node,
>(
  children?: TChildren,
  fallback?: (error: Error) => TFallback,
) renders (TChildren | TFallback);

// The element of a component renders what the component renders, which goes to the render type
// expected of the element: the fragment's type parameter gets `Header`.
<Page header={<></>} />; // was an error

// The render type expected by an optional prop.
<Page footer={<></>} header={<Header />} />; // was an error

// The render type of the component that returns the element.
component Section() renders Header {
  return <Boundary><Header /></Boundary>; // was an error
}

// The props still come first: `TChildren` is `Other`.
<Page header={<Boundary><Other /></Boundary>} />; // error
