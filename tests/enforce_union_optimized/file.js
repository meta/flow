type Ok1 = $Flow$EnforceOptimized<0 | 1>;
type Ok2 = $Flow$EnforceOptimized<"a" | "b">;
type Ok3 = $Flow$EnforceOptimized<true | false>;
type Ok4 = $Flow$EnforceOptimized<undefined | null>;
type Ok5 = $Flow$EnforceOptimized<0 | "a">;

type Ok6 = $Flow$EnforceOptimized<{f: 1} | {f: 2}>;
type Ok7 = $Flow$EnforceOptimized<{f: 1, ...} | {f: 2}>;
type Ok8 = $Flow$EnforceOptimized<{f: 1, ...} | {f: 2} | null>;

type T1 = {f: 1} | {f: 2};
type T2 = {f: 3} | {f: 4};

type Ok9 = $Flow$EnforceOptimized<T1>;
type Ok10 = $Flow$EnforceOptimized<Readonly<T1>>;
type Ok11 = $Flow$EnforceOptimized<T1 | T2>;
type Ok12 = $Flow$EnforceOptimized<Readonly<T1 | T2>>;
type Ok13 = $Flow$EnforceOptimized<T1 | T2 | number | string>; // okay, partial

type Ok14 = $Flow$EnforceOptimized<number | string | unknown>; // okay due to unknown short-circuit

// Unoptimizable cases

type Error1 = $Flow$EnforceOptimized<number | string>; // error not object candidates

type Error2 = $Flow$EnforceOptimized< // error no common keys
  | { f: 1 }
  | { h: 0 }
>;

type Error3 = $Flow$EnforceOptimized< // error unsupported form (EvalT)
  | Readonly<{ f: 1 }>
  | Readonly<{ f: 2 }>
>;

type Foo<X, Y> = { x: X, y: Y };
type Error4 = $Flow$EnforceOptimized< // error unsupported form (TypeAppT)
  | Foo<1, 2>
  | Foo<1, 3>
>;

type Error5 = $Flow$EnforceOptimized< // error no unique keys
  | { f: 1, g: "a", h: true, k: undefined, l: null, m: 1n }
  | { f: 1, g: "a", h: true, k: undefined, l: null, m: 1n }
>;

type Error6a = $Flow$EnforceOptimized<1>; // error non-union argument
type Error6c = $Flow$EnforceOptimized<any>; // error non-union argument
type Error6d = $Flow$EnforceOptimized<empty>; // error non-union argument
type Error6e = $Flow$EnforceOptimized<() => void>; // error non-union argument

type Error7a = $Flow$EnforceOptimized<>; // error no arg
type Error7b = $Flow$EnforceOptimized<1 | 2, 1 | 2>; // error too many args

type Ok15 = $Flow$EnforceOptimized<Values<{a: 1, b: 2}> | 3 | 4>; // okay
