//@flow

function foo<T>(x: T): T {
  return x;
}
foo.bar = 'foo';
type Foo = {
  <T>(x: T): T,
  bar: string,
 ...};

foo as Foo;

function f(x: unknown): empty {
  return f(x);
}

f as <T>(x: T) => T;

function g<T>(x: T): T {
  return x;
}

g as (x: empty) => unknown;
