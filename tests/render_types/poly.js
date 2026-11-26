//@flow

import * as React from 'react';

component Fragment<T extends React.Node>(children: T) renders T {
  return children;
}

const el = <Fragment>{null}</Fragment>;
el as renders null; // type checks, but invalid-render
el as renders undefined; // error

component Foo() {
  return null;
}
component Bar() {
  return null;
}

component FooContainer(children: renders Foo) {
  return null;
}

const transitive = (
  <FooContainer>
    <Fragment>
      <Foo />
    </Fragment>
  </FooContainer>
); // ok
const transitiveBad = (
  <FooContainer>
    <Fragment>
      <Bar />
    </Fragment>
  </FooContainer>
); // error

component Baz() renders Foo {
  return <Foo />;
}

const doubleTransitive = (
  <FooContainer>
    <Fragment>
      <Baz />
    </Fragment>
  </FooContainer>
); // ok

component Menu(
  children: renders* MenuItem,
) {
  return null;
}
component MenuItem() {
  return null;
}

const menu = // OK!
  (
    <Menu>
      <Fragment>
        <MenuItem />
        <MenuItem />
        <MenuItem />
      </Fragment>
    </Menu>
  );
