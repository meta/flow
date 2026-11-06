declare class C {
  [string]: number; // error
  static [number]: string; // error
}

declare class Labeled {
  [key: string]: number; // ok
  static [key: number]: string; // ok
}

declare class Computed {
  ['a']: number; // ok
}

interface I {
  [string]: number; // error
}

interface LabeledI {
  [key: string]: number; // ok
}
