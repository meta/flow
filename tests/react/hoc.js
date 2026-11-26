import * as React from 'react';

function myHOC(
  Component: component(ref?: React.RefSetter<unknown>, foo: number, bar: number),
): component(ref?: React.RefSetter<unknown>, foo: number) {
  return class extends React.Component<{foo: number, ...}, {bar: number, ...}> {
    state: {bar: number, ...} = {bar: 2};
    render(): React.Node {
      <Component />; // Error: `foo` is required.
      <Component foo={42} />; // Error: `bar` is required.
      <Component foo={1} bar={2} />; // OK
      return <Component foo={this.props.foo} bar={this.state.bar}/>;
    }
  }
}

class Unwrapped extends React.Component<{
  foo: number,
  bar: number,
 ...}, {
  buz: number,
 ...}> {
  state: {buz: number, ...} = {buz: 3};
  render(): React.Node {
    return this.props.foo + this.props.bar + this.state.buz;
  }
}

function UnwrappedFun(props: {foo: number, bar: number, ...}) {
  return props.foo + props.bar;
}

myHOC(class Empty extends React.Component<{foo: string, ...}, void> {}); // Error
myHOC(function Empty(props: {foo: string, ...}) { return undefined; }); // Error

const Wrapped: component(ref?: React.RefSetter<unknown>, foo: number) = myHOC(Unwrapped);
const WrappedFun = myHOC(UnwrappedFun);

<Wrapped nonsense="what" />; // Error: `foo` is required.
<Wrapped foo={1} />; // OK
<WrappedFun />; // Error: `foo` is required.
<WrappedFun foo={1}/>; // OK
