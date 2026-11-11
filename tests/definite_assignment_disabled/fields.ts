// TypeScript files accept definite assignment assertions without the option.

class Box {
  value!: number; // ok
  #secret!: string; // ok
  ['computed']!: number; // ok
}
