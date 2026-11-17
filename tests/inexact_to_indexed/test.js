//@flow
type X = {
  [key: string]: number,
};

declare const x: {foo: number, ...};

x as X; // Error, inexact objects are incompatible with indexed objects
