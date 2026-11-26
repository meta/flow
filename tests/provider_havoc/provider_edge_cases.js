//@flow

var x = 42;
declare export default class C { }
declare const returnsString: (x: unknown) => string;
if (((x: unknown): number => 42) === returnsString) { // constant-condition error
}

f = function() {
  f = 42;
};
