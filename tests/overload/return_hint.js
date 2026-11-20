// A return hint only chooses among the overloads that the arguments allow:
// when it rules out all of them, the arguments choose, as in TypeScript.
declare class Box<T> {
  value: T;
}
declare function resolve(): Box<void>;
declare function resolve<T>(x: T): Box<T>;
declare function call<R = number>(f: () => R): R;
const b = call(() => resolve('a')); // ok
b.value as string; // ok
b.value as number; // error

declare function pick<T>(x: T): Array<T>;
declare function pick<T>(x: T): T;
const s: string = pick('a'); // ok, the hint picks the second overload
declare const nested: typeof resolve & (<T>(x: T) => T);
const t: string = nested('a'); // ok, the hint picks the last member
