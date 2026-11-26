//@flow
import * as React from 'react';

function Foo(): React.Node { return undefined; }

component Bar() renders typeof Foo { // invalid-render, turned into any
  return <Foo />; // OK
}

(<Bar />) as renders typeof Foo; // invalid-render, so RHS becomes any

function Baz(): renders Bar {
  return <Bar />;
} // OK
component Qux() {
  return null;
}
(<Baz />) as renders Bar; // OK
(<Baz />) as renders Qux; // OK, since the super render of Bar is any due to invalid-render

function RendersBaz(): renders typeof Baz { // invalid-render
  return <Baz />;
}
(<RendersBaz />) as renders Bar; // OK
(<RendersBaz />) as renders Qux; // OK, since the super render of Baz, which is Bar, which has any super-render due to invalid-render

function LongerChain(): renders typeof RendersBaz { // invalid-render
  return <RendersBaz />;
}
(<LongerChain />) as renders Bar; // OK
(<LongerChain />) as renders Qux; // OK, since the super render of Bar is any due to invalid-render
