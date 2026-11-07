// The rest parameter of the argument receives the parameters of the expected function past its own
// and that function's rest parameter.
declare function afterSecond<A extends ReadonlyArray<unknown>>(
  f: (a: string, b: number, ...args: A) => void,
): A;

declare function tupleRest(a: string, ...rest: [number, boolean]): void;
afterSecond(tupleRest) as [boolean];
afterSecond(tupleRest) as [number]; // error
