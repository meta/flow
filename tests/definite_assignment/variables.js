// A variable with a definite assignment assertion (`!`) is assigned its
// annotation at its declaration, so reads never see it uninitialized.

let count!: number;
count as number; // ok

let plain: number;
plain as number; // error: uninitialized

var total!: number;
total as number; // ok

let both!: number, other!: string;
both as number; // ok
other as string; // ok

let afterCall!: number;
function init() {
  afterCall = 1;
}
init();
afterCall as number; // ok

let plainAfterCall: number;
function initPlain() {
  plainAfterCall = 1;
}
initPlain();
plainAfterCall as number; // error: possibly uninitialized

let resolve!: () => void;
new Promise<void>(r => {
  resolve = r;
});
resolve(); // ok

let inClosure!: number;
const read = () => inClosure as number; // ok

let assigned!: number;
assigned = 'str'; // error: string is incompatible with number

let maybe!: ?number;
maybe as number; // error: null or undefined is incompatible with number

let looped!: number;
while (Math.random() > 0.5) {
  looped = 1;
}
looped as number; // ok

beforeDeclaration as number; // error: used before its declaration
let beforeDeclaration!: number;

readVar as number; // error: uninitialized, the declaration below has not run
var readVar!: number;
