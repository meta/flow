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
declare const maybe: ?string;
function mayThrow() {
  if (maybe == null) throw 0;
}
mayThrow() as undefined; // ERROR
async function asyncMayThrow() {
  if (maybe == null) {
    return;
  }
  throw 0;
}
asyncMayThrow() as Promise<undefined>; // ERROR
const alwaysThrows = () => {
  throw 0;
};
alwaysThrows() as string; // OK

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
const f6: () => ?string = () => {
  if (maybe == null) throw 0;
}; // OK

export function exportedNoReturn() {}
export const exportedArrow = () => {};
export async function exportedAsync() {}
