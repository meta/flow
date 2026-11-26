/**
 * @noformat
 * @flow
 */

type T = {p: unknown, ...};

let a = 0;
const f = (x: unknown): unknown => x;
const o = {p: 42};

declare const React: {createElement: React$CreateElement, ...};
function MyComponent() { return null; }

(([] as empty) as T).p as empty;
(([1] as empty) as T).p as empty;
(([1, 2, 3] as empty) as T).p as empty;
(((() => {}) as empty) as T).p as empty;
(((() => 42) as empty) as T).p as empty;
(((() => ({})) as empty) as T).p as empty;
(((a = 42) as empty) as T).p as empty;
(((a >>>= 42) as empty) as T).p as empty;
((({a} = {a: 42}) as empty) as T).p as empty;
((([a] = [42]) as empty) as T).p as empty;
(((a = a = 42) as empty) as T).p as empty;
((1 + 2 as empty) as T).p as empty;
((1 + 2 + 3 as empty) as T).p as empty;
((1 - (2 + 3) as empty) as T).p as empty;
(((1 + 2) * (3 + 4) as empty) as T).p as empty;
(((1 + 2 * (3 + 4)) as empty) as T).p as empty;
((f() as empty) as T).p as empty;
((f(42) as empty) as T).p as empty;
(((class {}) as empty) as T).p as empty;
(((1 ? 2 : 3) as empty) as T).p as empty;
((((1 ? 2 : 3) ? 4 : 5) as empty) as T).p as empty;
(((function () {}) as empty) as T).p as empty;
((a as empty) as T).p as empty;
((import('./expression') as empty) as T).p as empty;
((<MyComponent /> as empty) as T).p as empty;
((<>foo</> as empty) as T).p as empty;
(('foo' as empty) as T).p as empty;
(('abcdefghijklmnopqrstuvwxyz' as empty) as T).p as empty;
((42 as empty) as T).p as empty;
((true as empty) as T).p as empty;
((false as empty) as T).p as empty;
((null as empty) as T).p as empty;
((o.p as empty) as T).p as empty;
((new f() as empty) as T).p as empty;
((new f(42) as empty) as T).p as empty;
((({}) as empty) as T).p as empty;
(((1, 2, 3) as empty) as T).p as empty;
class X extends class {
  m() {}
} {
  m() {
    ((super.m as empty) as T).p as empty;
  }
}
((f`foobar` as empty) as T).p as empty;
((`foobar` as empty) as T).p as empty;
(function (this: number) {
  ((this as empty) as T).p as empty;
}).call(42);
(((42 as string) as empty) as T).p as empty;
((-42 as empty) as T).p as empty;
((!42 as empty) as T).p as empty;
((++a as empty) as T).p as empty;
((a++ as empty) as T).p as empty;
function* g1(): Generator<number | void, void, number> {
  (((yield) as empty) as T).p as empty;
  (((yield 42) as empty) as T).p as empty;
}
function* g2(): Generator<number | void, void, number> {
  (((yield* g1()) as empty) as T).p as empty;
}
