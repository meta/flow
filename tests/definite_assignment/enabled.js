// `experimental.definite_assignment_assertions` enables definite assignment
// assertions.

let count!: number; // ok

class Box {
  value!: number; // ok
  #secret!: string; // ok
  ['computed']!: number; // ok
}
