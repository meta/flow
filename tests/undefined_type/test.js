const u: undefined = undefined; // OK
function f(): void {}
const v: undefined = f(); // OK: void still means undefined
const wrong: undefined = 1; // ERROR

type N = never; // ERROR: the flag does not enable other TS syntax
