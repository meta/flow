// A type that fits an instance member of a union does not go to the bare marker, even when that
// member gives no new evidence, as the second `Promise<R>` here: `TResult2` is `R`.
type R = {success: true, value: number} | {success: false, error: string};
declare const p: Promise<R>;
declare function f(): Promise<R>;
declare const c: boolean;
const m: Promise<R> = p.then(
  (r: R) => r,
  (e: unknown) => (c ? f() : f()),
); // ok
