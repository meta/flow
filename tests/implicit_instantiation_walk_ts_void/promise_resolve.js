// `Promise.resolve()` is `Promise<void>`, as in TypeScript.
declare function click(): Promise<void>;
async function f(xs: Array<number>) {
  await xs.reduce(p => p.then(() => click()), Promise.resolve()); // OK
}
Promise.resolve() as Promise<number>; // ERROR
