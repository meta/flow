function test1() {
  declare const x: (x: unknown) => void;
  x(y => {}); // error missing annot
}

function test2() {
  declare const x: (x: ?unknown) => void;
  x(y => {}); // error missing annot
}

function test3() {
  declare const x: (x: any) => void;
  x(y => {}); // okay due to any propagation
}
