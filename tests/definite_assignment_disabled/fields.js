// Definite assignment assertions need `experimental.definite_assignment_assertions`.

class Box {
  value!: number; // error: not enabled
  #secret!: string; // error: not enabled
  ['computed']!: number; // error: not enabled
}
