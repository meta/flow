type O = {...empty, ...};
declare const o: O;
42 as O; // Error: number ~> empty
o as empty; // OK: empty ~> empty

function fn1<T>(x: {...T, ...}) {
  x as number; // Error: mixed ~> number,
               // but only one error. empty ~> number is ok.
}

function fn2<T>(fn: (x: {...T, ...}) => void) {
  fn({}); // Error: object ~> empty
}

function fn3<T>(x: T, fn: (y: {...T, foo: number, ...}) => void) {
  fn({...x, foo: 42}); // OK
}
