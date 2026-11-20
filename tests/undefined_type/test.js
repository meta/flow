const u: undefined = undefined; // OK
function f(): void {}
const v: undefined = f(); // OK: void still means undefined
const wrong: undefined = 1; // ERROR

type N = never; // ERROR: other TS syntax stays unsupported
