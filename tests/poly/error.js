/**
 * @format
 * @flow
 */

declare function fn1<T>(x: (y: T) => void): (y: T) => void;
const newFn1 = fn1((x: number) => {});
newFn1('string'); // The error should point here.

declare function fn2<T>(x: T): (cb: (y: T) => void) => void;
const newFn2 = fn2(42);
newFn2((x: string) => {});

declare function fn3<T>(x: T, y: (z: T) => void): void;
fn3(42, (x: string) => {}); // The error should point to 42 and not string.

declare function fn4<T>(): [T, (x: T) => void];
const [newVal4, newFn4] = fn4<number>();
newFn4(42);
newVal4 as string; // The error should point here.
