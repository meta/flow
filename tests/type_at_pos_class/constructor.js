// @flow

class Foo {
  constructor(x: string) {
//    ^

  }
}

// A constructor is framed as its class's own declaration, both at its site
// and at a `new` callee naming it.
const foo = new Foo("hi");
//              ^
//           ^

class Bar {
  constructor(x: number) {}
}

const ns = { Bar };

// A member callee names the constructor under its property name.
const bar = new ns.Bar(42);
//                 ^

const Anon = class {
  // An anonymous class has no name to frame its constructor under, so the
  // keyword is left unframed.
  constructor(x: string) {}
//^
};

class Box<T> {
  constructor(x: T) {}
}

// Constructing a polymorphic class prints the type arguments it was
// instantiated with.
const box = new Box(1);
//              ^
//          ^

const ns2 = { Box };

const box2 = new ns2.Box<string>("a");
//                   ^
