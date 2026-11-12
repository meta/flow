class A {
  constructor(): void {} // OK
}
class B {
  constructor(): undefined {} // OK
}
class C {
  constructor(): number { // ERROR
    return 1;
  }
}
