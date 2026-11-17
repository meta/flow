import React from 'react';

const any: any = null;

type State = {foo: number, bar: number};

type StateUpdater1 = {(state: State): Partial<State>, ...};
type StateUpdater2 = {(state: State): Partial<State>, foo: number, bar: number, ...};
type StateUpdater3 = {(n: number): number, foo: number, bar: number, ...};

class MyComponent extends React.Component<{prop: number, ...}, State> {
  state: State = {foo: 1, bar: 2};

  componentDidUpdate() {
    this.setState(prevState => ({ // OK: Updating partial state with a function.
      foo: prevState.foo + 1,
    }));
    this.setState((prevState, props) => ({ // OK: Updating partial state with
      foo: props.prop + 1,                 // a function.
    }));
    this.setState(prevState => { // OK: May return void.
      if (Math.random() > 0.5) {
        return;
      }
      return {foo: prevState.foo + 1};
    });
    this.setState(() => ({ // Error: `bar` should be a number.
      bar: '42',
    }));
    this.setState(prevState => {
      console.log(prevState.nope); // Error: `nope` does not exist.
    });
    this.setState((prevState, props) => {
      console.log(props.nope); // Error: `nope` does not exist.
    });
    this.setState(any as StateUpdater1); // OK: It has the right signature.
    this.setState(any as StateUpdater2); // OK: It has the right signature and
                                         // the right properties.
    this.setState(any as StateUpdater3); // Error: It has the wrong signature
                                         // even though it has the right
                                         // properties.
  }
}

(() => {}) as Partial<State>; // Error: Functions are not a valid object shape.
