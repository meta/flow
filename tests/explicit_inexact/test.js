//@flow
type T = {...};
type U = {foo: number, ...};

declare const x: U;
x as T; // Ok, by width subtyping

x as {}; // Error, inexact vs. exact
