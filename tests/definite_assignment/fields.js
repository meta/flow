// `experimental.strict_property_initialization` does not check a field with a
// definite assignment assertion (`!`).

class Box {
  value!: number; // ok: no initializer, and the constructor does not assign it
  #secret!: string; // ok
  constructor() {
    this.value as number; // ok: read before the constructor assigns it
    this.#secret as string; // ok
  }
}

class NoConstructor {
  value!: number; // ok
}

declare function setUp(widget: Widget): void;

class Widget {
  value!: number; // ok: assigned where the check cannot see it
  constructor() {
    setUp(this);
  }
}

class Plain {
  value: number; // error: no initializer and not definitely assigned
}

class InitializerOrder {
  first: number = this.second; // error: read before the initializer of `second` runs
  second!: number;
}

class Derived extends Box {
  extra!: string; // ok
  constructor() {
    super();
    this.extra as string; // ok
  }
}
