declare class C<T> {
  constructor(x: string, y: T): void;
  constructor(x: number, y: Array<T>): void;
  get(): T;
}

// The second constructor: the first one does not take a number.
new C(1, [true]).get() as boolean; // ok
