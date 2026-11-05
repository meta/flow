type T1 = '1' extends string ? number : boolean;
//   ^

type T2 = 1 extends number ? string : boolean;
//   ^

type T3 = Array<string> extends ReadonlyArray<infer X> ? X : empty;
//   ^

// Test on nested extends
type T4_1 = string extends (infer X extends (infer X extends string) ? string : number) ? string : number;
//   ^
type T4_2 = string extends (infer X extends (infer Y extends string) ? string : number) ? string : number;
//   ^

// Test that we can distinguish infer type with non-infer type with the same name.
type T5 = string extends {foo: infer String, bar: String, ...} ? string : number;
//   ^

// Test that we can distinguish infer type with non-infer generic type with the same name.
type T6 = string extends {foo: infer T, bar: <T>(T) => void, ...} ? string : number;
//   ^

// Test that infer type and upper generic type can coexist.
type T7<T> = T extends infer T ? string : number;
//   ^

// Test that we can distinguish between all the confusing Ts.
type T8<T> = T extends {foo: infer T, bar: <T>(T) => void, ...} ? string : number;
//   ^

// Distribute-over-union still works for unions coming from implicit instantiation.
declare function Distributive<T>(x: T, y: T): T extends string ? string : number;
let x = Distributive(3, 'str');
//  ^

// An `infer` type of a tuple spread is bounded by `ReadonlyArray<unknown>`, and so are the other
// `infer` types of the same name. That bound is not printed, even when it is written.
type T9<T> = T extends [unknown, ...infer R extends ReadonlyArray<unknown>] ? R : empty;
//   ^
type T10<T> = T extends [infer R, ...infer R] ? R : empty;
//   ^
