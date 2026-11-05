declare function isNum(x: unknown): x is number;

declare function h<T>(f: (x: unknown) => x is T, v: T): T;

// The types of two two-sided type guards are unified.
h(isNum, 'a'); // error: `string` is not `number`

declare function k<T>(f: (x: unknown) => implies x is T, v: T): T;

// Against a one-sided type guard, a type guard is covariant.
k(isNum, 'a') as number | string; // ok
