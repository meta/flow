//@flow

declare const c: {
    <T>(x: T, f: (y: T) => void): void,
    (): void,
}

c<number>(42, (x) => {}) // ok

declare class C<T> {
  static <X>(values: Array<X>): C<X>;
}
const x1: C<string> = C([]); // ok
type Enum = 'A' | 'B';
const x2 = C<Enum>([]); // ok
