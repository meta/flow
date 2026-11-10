// A class extending a value with a construct signature inherits its statics.
// Errors in them describe the value by its type.

interface Base {}
interface BaseCtor {
  new (): Base;
  create(o: {}): Base;
}
declare const BaseC: BaseCtor;

class Derived extends BaseC {
  static create(o: {a: number}): Base { // ERROR
    return new BaseC();
  }
}
