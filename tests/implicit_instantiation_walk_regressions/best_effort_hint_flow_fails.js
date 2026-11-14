declare class Elem {
  attributes: string;
}
declare function closest<E extends Elem = Elem>(selectors: string): E | null;
declare function closestNoDefault<E extends Elem>(selectors: string): E | null;

declare const b: boolean;
declare const n: number;

// The best-effort hint from the left operand or the other branch cannot fit `E | null`,
// so it says nothing about `E`.
if (b || closest('a')) {
}
if (n || closest('a')) {
}
const c = b ? true : closest('a');
if (b || closestNoDefault('a')) {
}

declare function clean<T extends {...}>(
  obj: Readonly<{...$Exact<T>, extra?: unknown}>,
): Readonly<$Exact<T>>;
const e = b ? clean({a: 1}) : null;

// A best-effort hint that fits is still used.
declare function emptyArray<T>(): Array<T>;
declare const strs: ?Array<string>;
const f = strs || emptyArray();
f as Array<string>;
