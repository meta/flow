//@flow

const React = require('react');

function Component(props: { foo: (x: number) => number }) { return null }

<Component foo={(x) => 3} />;
<Component foo={(x) => {
  const y = (x) => 3; // error, required annot
  return 3;
}} />;

function Component2(props: { children: (x: number) => React.Node }) {
  return null
}
  <Component2>{(x) => <div />}</Component2> // no annot

