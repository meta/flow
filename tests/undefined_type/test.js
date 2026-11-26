const u: undefined = undefined; // OK
function f(): undefined {}
const v: undefined = f(); // OK
const wrong: undefined = 1; // ERROR

type N = never; // ERROR: other TS syntax stays unsupported
