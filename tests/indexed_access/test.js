type Arr = Array<number>;
type ArrElem = Arr[number];

42 as ArrElem; // OK
'hello world' as ArrElem; // Error

function foo(a: Arr): Arr[number] {
  if (false) return a[0];
  else return 0;
}

type Dict = { [key: string]: number };
type DictElem = Dict[string];

42 as DictElem; // OK
'hello world' as DictElem; // Error

function bar(o: Dict): Dict[string] {
  if (false) return o['buz'];
  else return 0;
}

type Obj = {
  foo: boolean,
  baz: string,
  ...
};

export type ObjFoo = Obj['foo'];
true as ObjFoo; // OK
1 as ObjFoo; // Error

'hi' as Obj['baz']; // OK
1 as Obj['baz']; // Error

function f<To extends Obj>(o: To): To['foo'] {
  return o.foo;
}
declare const o: Obj;
f(o) as boolean; // OK
f({foo: true, baz: 'hi', bort: 1}) as boolean; // OK
f(o) as string; // Error

type Nonexistant1 = Obj['bork']; // Error
1 as Nonexistant1;

type Nonexistant2 = Obj[string]; // Error
1 as Nonexistant2;

type ArrNonexistant = Arr[boolean]; // Error
1 as ArrNonexistant;

1 as void['x']; // Error
1 as null['x']; // Error

type Bar = 'bar';

type O = {
  [key: number]: boolean;
  bar: number;
}
1 as O['bar']; // OK
1 as O[Bar]; // OK
true as O[number]; // OK
'xx' as O['bar']; // Error

declare class C {
  [key: number]: boolean;
  bar: number;
}
1 as C['bar']; // OK
1 as C[Bar]; // OK
true as C[number]; // OK
'xx' as C['bar']; // Error

interface I {
  [key: number]: boolean;
  bar: number;
}
1 as I['bar']; // OK
1 as I[Bar]; // OK
true as I[number]; // OK
'xx' as I['bar']; // ERROR

interface M {
  method(): number;
}
(() => 1) as M['method']; // OK
(() => true) as M['method']; // ERROR

{
  const test = (f: M['method']) => {
    f() as number; // OK
    f() as empty; // ERROR
  }
}
