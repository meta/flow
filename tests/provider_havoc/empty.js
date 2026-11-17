//@flow
// class Set<+T> {
//   add<U>(value: U): Set<T | U> {
//     return (42: any);
//   }
// }

function test() {
  let foo: {n: Set<string>} = {n: new Set()};

  let si = new Set<empty>();
  (function () {
    declare const x: ?string;
    si = si.add(x);
  })(); // error

  foo = {n: si};
}

function arr() {
  let arr: Array<empty> = [];
  arr = [1, 2, 3]; // error
}

function arr_ok() {
  let arr = [];
  arr.push(1);
  arr = [1, 2, 3]; // no error, original array tvar has number as lower
}

function fn() {
  let fn = (x: number) => 42;
  function havoc() {
    fn = y => 42; // error in old inference, no error in LTI
  }
}

function fn_completely_unannotated() {
  let fn = (x: number) => 42;
  function havoc() {
    fn = y => 42; // error in old inference, no error in LTI
  }
  fn(52);
}

function obj_this_empty() {
  let obj = {
    f() {
      let b = this.b; // Error: object-this-reference
      b = 42; // no error since `this` is any-typed above.
    },
  };
}

function reversed() {
  let rts: Array<number> = [];

  function initRts(): void {
    rts = [];
  }

  function getRts(): Array<number> {
    return rts;
  }
}

declare class Set<out T> {
  add<U>(value: U): Set<T | U>;
}

function set_replace() {
  let si = new Set<number>();

  si = new Set<string>(); // error

  si as Set<number>;
}

function set_add() {
  let si = new Set<number>();

  si = si.add(42); // error
}

function set_app() {
  let si = new Set<empty>();

  si = si.add(42); // error

  si as Set<number>;
}
