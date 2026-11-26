import * as React from 'react';

function ref_tests() {
  declare const Component1: component(ref: React.RefSetter<string>);
  <Component1 ref={(s) => { s as string | null }} />;
  declare class Component2 extends React.Component<{}> {}
  <Component2 ref={(s) => { s as Component2 | null }} />;
}

function cannot_resolve_name_regression_tests() {
  <div>{new Array<number>(1)}</div>; // ok
  <fbs>{new Array<number>(1)}</fbs>; // ok
  <fbt>{new Array<number>(1)}</fbt>; // ok
}

function react_abstract_component_subtyping() {
  const _: React.ComponentType<{foo: string}> = (props) => {
    props.foo as string; // ok
    props as empty; // error
    return null;
  };
}

function jsx_function_children_ok() {
  declare function Child(x: {foo: number}): React.Node;
  declare function Parent(x: {children: (n: number) => React.Node}): React.Node;

  return <Parent>
    {n => <Child foo={n as number} /> /* OK */}
  </Parent>;
}

function jsx_function_children_error() {
  declare function Child(x: {foo: number}): React.Node;
  declare function Parent(x: {children: (n: number) => React.Node}): React.Node;

  return <Parent>
    {n => <Child foo={n as empty} /> /* ERROR */}
  </Parent>;
}

function jsx_children_array_decomp() {
  declare function Parent(props: {
    children: React.Node,
  }): React.Node;

  const x = <Parent>
    <span />
    {Array.from({length: 3}, (_, index) => { // OK
      return <div />;
    })}
  </Parent>;
}

function jsx_fragment_array_from() {
  // Test that Array.from inside a Fragment resolves overloads correctly.
  // Fragment has Nominal component kind. The subtyping rule for
  // FunT ~> AbstractComponent must only match Structural, not Nominal.
  // Otherwise the Fragment's Renders generic param leaks as a hint
  // and kills the correct Array.from overload.
  const _a = <>
    {Array.from({length: 3}, (_, index) => { // OK
      return <div />;
    })}
  </>;
  const _b = <>
    {Array.from({length: 5}, (_, i) => i)} {/* OK */}
  </>;
  const arr: React.Node = <>
    {Array.from({length: 3}, (_, index) => index)}
  </>; // OK
}
