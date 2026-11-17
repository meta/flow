//@flow

declare const obj1: {f: number};
delete obj1.f; // error, f is required

declare const obj2: {f?: number};
delete obj2.f; //fine

var x: number | void = 42;
delete x;
x as number;
x as number | void;

var y: number = 42;
delete y;

type A = {writeonly a?: string};

declare const a: A;

const z = delete a.a;

declare const b: ReadonlyArray<number>;
delete b[0];

declare const c: [number, number];
delete c[1];
delete c[2];

var w: {a: number | void} = {a: 42};
w.a as void;

var w2: {a: number | void} = {a: 42};
delete w2.a;
w2.a as void;

const obj4 = Object.freeze({f: 1});
delete obj4.f; // error, just like when writing to frozen object

declare const obj5: {readonly f?: number};
delete obj5.f; // error, just like when writing to read-only object

class C {
  x: void;
  m() {}
}
declare const obj6: C;
delete obj6.x;
delete obj6.m; // warn, m is not own (delete only has effect
// on own-properties)

delete 1;
delete C;
delete (() => 42);

declare const f: Array<string>;
delete f[0];

declare const index: {[key: string]: number, a: boolean};
delete index.a;
delete index['a'];
delete index.b;
delete index['foo'];
