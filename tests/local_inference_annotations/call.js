//@flow


function f(x: (n: number) => number, y: any) {}

f((x) => 3, (x: number) => 3);

f((x) => {
  const a = (x) => 3; // Required annot
  return 3;
  // Error, function is not iterable. We should be asking for an annotation too.
}, ...((x) => 3));

function g(x: number) {}
g(3); // Ok


f((x) => 3, (x: number) => 3) || []; // no annot, constant-condition error

const h: ?((x: number) => number) = null;
h ?? ((x) => 3); // no annot

class Base {
  constructor(f: (x: number) => number) {}
}

new Base(x => 3); // no annot

class Extending extends Base {
  constructor() {
    super((x) => 3); // no annot
  }
}
