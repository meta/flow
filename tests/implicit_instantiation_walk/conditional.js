// @flow

declare class Box<T> {
  value: T;
}

// A conditional type that holds for every type argument takes its true branch,
// so the false branch gives no evidence.
{
  declare function f<T>(x: Array<T> extends ReadonlyArray<unknown> ? T : Box<T>): T;
  declare const b: Box<string>;
  f(b).value as string; // ok
}

// The bound of a type parameter does not decide the branch, as in TypeScript:
// both branches give evidence.
{
  declare function g<T extends string>(x: T extends string ? Box<T> : Array<T>): T;
  declare const b: Box<string>;
  g(b) as string; // ok
  g(['a']); // error: the parameter is a `Box`
}

// Otherwise, the branch for the solution so far.
{
  declare function h<T, U = number>(
    value: T,
    x: T extends number ? {readonly a: U, ...} : {readonly b: U, ...},
  ): U;
  const both = {a: 'not number', b: 42};
  h(3, both) as string; // ok
  h('s', both) as number; // ok
}

// The default counts as the solution of a type parameter without evidence.
{
  declare function k<U = number, T = number>(
    x: T extends number ? {readonly a: U, ...} : {readonly b: U, ...},
  ): U;
  k({a: 'not number', b: 42}) as string; // ok
}
