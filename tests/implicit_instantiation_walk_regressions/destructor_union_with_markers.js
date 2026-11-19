import * as React from 'react';

// A destructor with markers inside that makes a union of objects, from a union among its operands,
// is matched member by member, as the flow-based solve matches it by speculation.

// The config of a component whose rest parameter is a union.
type Label = {label: string, id?: void} | {id: string, label?: void};
component Select<V>(onChange: (v: V) => void, ...label: Label) {
  return null;
}
Select as component(label: string, onChange: (v: string) => void); // was an error

// The rest of an object type that spreads a union.
type Props<A> = {...{r: string} | {m: number}, set: (v: A) => void, key?: string};
component Ex<A>(...props: Omit<Props<A>, 'key'>) {
  return null;
}
function f<A>(rest: Omit<Props<A>, 'key'>): React.Node {
  return <Ex {...rest} />; // was an error
}
