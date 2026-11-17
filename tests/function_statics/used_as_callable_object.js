function add(x: number, y: number) {
  return x + y;
}

function addWithBar(x: number, y: number) {
  return x + y;
}
addWithBar.bar = "bar";


type InexactCallableObj = {
  (a: number, b: number): number,
  bar: string,
  ...
};

type ExactCallableObj = {
  (a: number, b: number): number,
  bar: string,
};

declare class C {
  (n: number, m: number): number;
  bar: string;
};

interface I {
  (n: number, m: number): number;
  bar: string;
};


add as InexactCallableObj; // error prop 'bar' missing
add as ExactCallableObj; // error inexact obj
add as C; // error object incompatible with class instance
add as I; // error prop 'bar' missing

addWithBar as InexactCallableObj; // okay
addWithBar as ExactCallableObj; // error inexact obj
addWithBar as C; // error object incompatible with class instance
addWithBar as I; // okay
