type Obj = {
  method(string): void, // error
  named(x: string): void, // ok
  (number): void, // error: call property
  prop: (boolean) => void, // error
};

declare class C {
  method(string): void; // error
  static staticMethod(number): void; // error
  named(x: string): void; // ok
}

interface I {
  method(string): void; // error
  named(x: string): void; // ok
}
