//@flow

let [a]: (x: number) => number = (x) => { // ERROR
  const d = (x) => 3;
  return 3; // annot available
};

// Errors: x and y are not on the statics of the function.
const {x: b, y: c}: (x: number) => void = (x) => { // This x has an annot available
  const b = ((x) => 3)(3); // This x does not
  const c = (x) => 3; // Neither does this one
};

declare let mixed: unknown;
declare let func_tuple : [
    (x: number) => number,
    (x: string) => number,
    (x: boolean) => number
];
[mixed, ...func_tuple] = [
    (x) => 1, // Error in LTI: need an annotation
    (x) => 1,
    (x) => 1,
    (x) => 1,
];

var null_1 = null;
[null_1] = [(x) => 1]; // error needs annotation because `null_1` pattern is a provider

var k = (x: string) => 1;
var l = (x: string) => 1;
var m = (x: string) => 1;
[k, [l, m]] = [(x) => 1, [(x) => 1, (x) => 1]]; // okay

var n = (x: string) => 1;
var o = (x: string) => 1;
var null_2 = null;
[n, [o, null_2]] = [
    (x) => 1, // TODO should not need annotation
    [
        (x) => 1, // TODO should not need annotation
        (x) => 1, // error needs annotation because null_2 pattern is a provider
    ]
];
