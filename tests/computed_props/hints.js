declare const x: string;
declare const y: string;
declare const z: string;

declare const v1: 1;
declare const v2: 2;
declare const v3: 3;

const obj1: {[key: string]: 1|2} = {
  [x]: v1,
  [y]: v2,
  [z]: v3, // error 3 ~> 1|2
}

const obj2: {[key: string]: empty} = {
  [x]: v1, // error 1 ~> empty
  [y]: v2, // error 2 ~> 1
  [z]: v3, // error 3 ~> 1
}

const obj3: empty = { // error obj ~> empty
  [x]: v1,
  [y]: v2,
  [z]: v3,
}

const obj4: unknown = { // okay
  [x]: v1,
  [y]: v2, // error 2 ~> 1
}

const obj5: any = { // okay
  [x]: 1,
  [y]: 2,
}

const obj6 = { // no hint
  [x]: v1,
  [y]: v2, // error 2 ~> 1
}

const obj7: () => {} = { // error obj ~> funt
  [x]: v1, // values okay
  [y]: v2, // values okay
}

function partial_hints() {
  type T = Readonly<{
    a?: number,
    b?: number,
    ...
  }>;

  declare const key: 'a'|'b';
  declare function updater(x: Partial<T>): void;
  updater({[key]: 1}); // okay
}

function regression_todo() {
  type T =
    | {[key: string]: boolean, ...}
    | {[key: string]: number, ...}

  declare const x: string;
  declare const y: string;
  declare const b: boolean;

  declare function foo(o: T): void;

  foo({ // errors due to hint being boolean|number which won't match any indexer
    [x]: b,
    [y]: b,
  });
}
