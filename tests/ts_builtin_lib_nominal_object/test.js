// TS's library declares `Object` and `Function` as interfaces. The prototype
// chain of an instance still reaches them nominally, and ends at the null
// prototype of `Object`.

class A {}
class B extends A {}

new B() as A; // OK
new A() as Object; // OK
new A() as interface {toString(): string}; // OK
(() => {}) as Object; // OK
