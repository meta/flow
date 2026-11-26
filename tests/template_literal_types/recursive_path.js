// Conditional types left undecided over a type parameter, whose true branch is the
// same conditional type again.
type PathValue<T, P extends string> = T extends any
  ? P extends `${infer K}.${infer R}`
    ? PathValue<T[K], R>
    : never
  : never;

type Undecided<P extends string> = Partial<
  PathValue<{[k: string]: any}, P> extends 1 ? 1 : 2,
>;

function f<P extends string>(v: PathValue<{[k: string]: any}, P>) {
  v.foo; // OK
  `${v}`; // OK
}

function g<T extends {[k: string]: any}, P extends string>(v: PathValue<T, P>) {
  v.foo; // OK
}

type Deep<T> = T extends {a: infer A} ? Deep<A> : T;

function h<T>(x: Deep<T>) {
  x.foo; // ERROR
}

type Segments<T, S extends string> = T extends string
  ? S extends `${infer H}.${infer R}`
    ? H | Segments<T, R>
    : 1
  : never;

function i<T>(x: Segments<T, 'a.b.c.d.e'>) {
  x as string; // ERROR
}

type Shift<T, U, P extends string> = P extends `${infer H}.${infer R}`
  ? Shift<U, number, R>
  : T;

function j<P extends string>(x: Shift<any, any, P>) {
  x as string; // ERROR
}
