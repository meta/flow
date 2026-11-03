// The key `P` must not capture the `P` in the spread `{...p}`.
type Pick<T, K extends keyof T> = {[P in K]: T[P]};

function f<P extends {a: number, ...}>(p: P) {
  const o = {...p};
  declare const x: Pick<typeof o, 'a'>;
  x.a as number; // ok
  x.a as string; // error
}
