// A bracket after a bare decorator starts a computed field (issue #9279).
class Repro {
  @decorators[0] // OK
  method() {}
}

// Computed methods and initialized fields keep their keys outside the decorator.
class Computed {
  @decorator [0]() {} // OK
  @decorators.member [1]() {} // OK
  @decorator(value) [2]() {} // OK
  @(decorators[0]) [3]() {} // OK
  @(decorator()) [4]() {} // OK
  @decorator [5] = value; // OK
  @decorator(value) [6] = value; // OK
}
