// @flow

class A {
  // TODO this is not the ideal type
  m() { }
//^
}

class B extends A {
  m(): number { return 1; }
//^
  constructor() {
    super();
  }
}
