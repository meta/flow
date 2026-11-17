function test1() {
  declare function foo(tag: 'boolean', cb: (x: boolean) => void): void;
  declare function foo(tag: 'str', cb: (x: string) => void): void;
  declare function foo(tag: 'num', cb: (x: number) => void): void;

  foo('boolean', (x) => { x as boolean; }); // okay
  foo('str', (x) => { x as number; }); // error
  foo('a', (x) => { x as number; }); // error
}

function test2() {
  type B = (tag: 'boolean', cb: (x: boolean) => void) => void;
  type S = (tag: 'str', cb: (x: string) => void) => void;
  type N = (tag: 'num', cb: (x: number) => void) => void;
  type O = (tag: 'obj', cb: (x: {}) => void) => void;

  declare const foo: B & (S & (N & O));
  foo('boolean', (x) => { x as boolean; }); // okay
  foo('str', (x) => { x as string; }); // okay
  foo('obj', (x) => { x as string; }); // error

  declare const bar: (B & S) & (N & O);
  bar('boolean', (x) => { x as boolean; }); // okay
  bar('str', (x) => { x as string; }); // okay
  bar('obj', (x) => { x as string; }); // error

  declare const bak: (B & S) & (N & B);
  bak('boolean', (x) => { x as boolean; }); // ok
  bak('boolean', (x) => { x as string; }); // error in cast (ideally would pick the right overload)
}

function test4() {
  [[1]].filter(([n], i: number) => n as number > 0); // okay
}

function test5() {
  ["a"].reduce((a, b, i) => a + b); // okay
  ["a"].reduce((a, b, i) => a + b, ''); // okay
}

function test6() {
  declare const array: ?Array<string>;
  (array || []).reduce((acc, item) => acc, {}); // okay
  (array ?? []).reduce((acc, item) => acc, {}); // okay
}

function test7() {
  declare function arrayFrom<A>(iter: ReadonlyArray<A>): Array<A>;
  declare function arrayFrom(arrayLike: interface { length: number }): Array<void>;

  declare const tags: ?ReadonlyArray<string>;
  const tagsList = arrayFrom(tags ?? []);
  tagsList.forEach(tag => tag as number); // error: string ~> number
}

function test8() {
  declare function overload(a: string, b: (x: string) => void): void;
  declare function overload(a: number, b: (x: number) => void): void;

  declare const foo: ?string;
  overload(foo || "", (s) => {}); // okay
  overload(foo || 42, (s) => {}); // error because we can't resolve overload, and on 1st arg
}

function test9() {
  declare const f1: { <T>(f: (x: { cb: T }) => void): T } & ((x: empty) => unknown);
  declare const f2: (<T>(f: (x: { cb: T }) => void) => T) & ((x: empty) => unknown);

  f1(x => 0 as any) as {};
  f2(x => 0 as any) as {};
}
