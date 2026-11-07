// The default library merges Flow and TypeScript library files together. The
// classes of the Flow files keep Flow's rules.
import * as React from 'react';

class MyObject extends Object {
  static keys(o: {a: number}): Array<string> { return []; } // ERROR: narrower parameter.
}

// ERROR: the `displayName` field is invariant.
class Foo extends React.Component<{}> {
  static displayName: string = 'Foo';
}

React.Component as {displayName?: ?string | number, ...}; // ERROR: the field is invariant.
