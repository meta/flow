/* @flow */

function takesBotFunction (x : (y: empty) => empty) {}
function takesTopFunction (x : (y: unknown) => unknown) {}
function takesMidFunction (x : (y: empty) => unknown) {}
function takesMidFunction2 (x : (y: unknown) => empty) {}
function takesHigherOrderFn (x : (y: empty) => (z: empty) => unknown) {}
function takesHigherOrderFn2 (x : (f: (y: empty) => unknown) => unknown) {}
function takesMultiArgFn (x : (a: empty, b: empty) => unknown) {}
function takesMultiArgFn2 (x : (a: unknown, b: unknown) => unknown) {}

function fun(x: unknown) {
  if (typeof x === "function") {
    takesBotFunction(x); //error
  }
}

function fun2(x: unknown) {
  if (typeof x === "function") {
    takesTopFunction(x); // error
  }
}

function fun3(x: unknown) {
  if (typeof x === "function") {
    takesMidFunction(x); // error
  }
}

function fun4(x: unknown) {
  if (typeof x === "function") {
    takesMidFunction2(x); // error
  }
}

function fun5(x: unknown) {
  if (typeof x === "function") {
    takesHigherOrderFn(x); // error
  }
}

function fun6(x: unknown) {
  if (typeof x === "function") {
    takesHigherOrderFn2(x); // error
  }
}

function fun7(x: unknown) {
  if (typeof x === "function") {
    takesMultiArgFn(x); // error
  }
}

function fun8(x: unknown) {
  if (typeof x === "function") {
    takesMultiArgFn2(x); // error
  }
}

function fun9(x: unknown, y : unknown, z : empty) {
  if (typeof x === "function") {
    x(y); // error
    x(z);
    x(x); // error
    x(1,2,3,4,5); //error
    x(...[1,2]); //error
    x();
  }
}

declare const obj : {field : unknown, ...};
if (typeof obj.field === 'function') {
  const f = obj.field(0); // error
  const f2 = f.foo;
}

function fun10(x: unknown) {
  if (typeof x === "function") {
    x.name;
    x.length;
    x.foo; // error
  }
}
