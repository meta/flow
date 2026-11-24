// `Promise.resolve()` is `Promise<void>`, as in TypeScript.
declare function click(): Promise<void>;
async function f(xs: Array<number>) {
  await xs.reduce(p => p.then(() => click()), Promise.resolve()); // OK
}
Promise.resolve() as Promise<number>; // ERROR

// React's `act`, typed as in `@types/react`, returns a promise for a callback
// that returns a value: only a callback that returns no value takes the
// overload returning `void`.
import {act} from 'react';
act(() => {}) as void; // OK
act(async () => 1) as Promise<number>; // OK
act(() => 1) as void; // ERROR
