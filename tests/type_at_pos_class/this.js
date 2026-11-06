class Foo2 {
  bar(
    baz: Readonly<this>,
  ): Readonly<this> {
    baz.bar(baz);
//          ^
    return this
  }
}
