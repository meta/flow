class A {}
class B {}

const a = new A() as B; // ERROR
const c = A as B; // ERROR
