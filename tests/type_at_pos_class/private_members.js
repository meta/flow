// @flow

class A {
  // A private member is framed like any other, under the name it is written with:
  // the `#` is part of what hover reports, though the parser drops it from the
  // name it records. A reference resolves through the enclosing class body rather
  // than the receiver's type, which does not carry private members at all.
  #f: number = 1;
//^
  #m(): void {}
//^
  get #g(): string { return ''; }
//    ^
  static #sf: boolean = true;
//       ^

  use(): void {
    this.#m();
//       ^
    this.#f;
//       ^
    this.#g;
//       ^
    A.#sf;
//    ^
  }
}

class B {
  // A second class reusing the name resolves to its own declaration.
  #f: string = '';
//^
  use(): string {
    return this.#f;
//              ^
  }
}
