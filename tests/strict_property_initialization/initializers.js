class LaterWithInitializer {
  a: number = this.b; // error
  b: number = 1;
}

class LaterWithoutInitializer {
  a: number = this.b; // error
  b: number;
  constructor() {
    this.b = 1;
  }
}

class LaterDeclared {
  a: number = this.b; // error
  declare b: number;
}

class SameField {
  a: number = this.a; // error
}

class LaterPrivate {
  #a: number = this.#b; // error
  #b: number = 1;
  read(): number {
    return this.#a;
  }
}

class LaterStatic {
  static a: number = this.b; // error
  static b: number = 1;
}

class OptionalChain {
  a: ?number = this?.b; // error
  b: number = 1;
}

class InNestedExpression {
  a: Array<number> = [this.b]; // error
  b: number = 1;
}

class EarlierWithInitializer {
  b: number = 1;
  a: number = this.b;
}

class InsideArrow {
  a: () => number = () => this.b;
  b: number = 1;
}

class LaterMethod {
  a: number = this.m();
  m(): number {
    return 1;
  }
}

class ElementAccess {
  a: number = this['b'];
  b: number = 1;
}

class AssignLater {
  a: number = (this.b = 2); // error
  b: number = 1;
}

class CallLater {
  a: number = this.b(); // error
  b: () => number = () => 1;
}

class InnerClass {
  a: unknown = class {
    x: number = this.y; // error, for the inner class's own `y`
    y: number = 1;
  };
  y: number = 2;
}

class OptionalLaterField {
  a: ?number = this.b; // `b` is optional, so `undefined` is expected
  b?: number = 2;
}

class OptionalFieldReadsLaterField {
  a?: number = this.b; // error
  b: number = 2;
}
