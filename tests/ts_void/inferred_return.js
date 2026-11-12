// A function that returns no value returns `void`.
function noReturn() {}
noReturn() as void; // OK
noReturn() as undefined; // ERROR
const arrow = () => {};
arrow() as undefined; // ERROR
function bareReturn() {
  return;
}
bareReturn() as undefined; // ERROR
async function asyncNoReturn() {}
asyncNoReturn() as Promise<void>; // OK
asyncNoReturn() as Promise<undefined>; // ERROR
if (noReturn()) {
} // ERROR

// A function that returns a value on some paths returns `undefined` on the others.
function someReturns(x: boolean) {
  if (x) return 1;
}
someReturns(true) as number | undefined; // OK

// Unless the contextual return type admits `undefined`.
const f1: () => number | undefined = () => {}; // OK
const f2: () => ?string = () => {}; // OK
const f3: () => Promise<string | undefined> = async () => {}; // OK
const f4: () => Promise<string | void> = async () => {}; // OK
const f5: () => number | undefined = noReturn; // ERROR

export function exportedNoReturn() {}
export const exportedArrow = () => {};
export async function exportedAsync() {}
