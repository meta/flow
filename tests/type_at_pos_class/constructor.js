// @flow

class Foo {
  constructor(x: string) {

  }
}

const foo = new Foo("hi");

class Bar {
  constructor(x: number) {}
}

const ns = { Bar };

const bar = new ns.Bar(42);

const Anon = class {
  constructor(x: string) {}
};

class Box<T> {
  constructor(x: T) {}
}

const box = new Box(1);

const ns2 = { Box };

const box2 = new ns2.Box<string>("a");
