// Supers of the builtin `Function` and `Object` that lead back to
// `Function.prototype` and `Object.prototype`.

interface Function {
  (...args: any): any;
}

interface Foo {}
interface Object extends Foo {
  (): void;
}
