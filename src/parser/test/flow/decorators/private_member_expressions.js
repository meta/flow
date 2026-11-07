// Private dotted members resolve against the enclosing class's private names.
class Outer {
  static #decorator;
  @Outer.#decorator method() {} // OK
  @Outer.#decorator(value) called() {} // OK
  @Outer.#decorator [0]() {} // OK
  nested() {
    @Outer.#decorator class Inner {} // OK
  }
}
