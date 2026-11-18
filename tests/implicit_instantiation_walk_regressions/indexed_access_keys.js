type Api = {ns: {readonly a: (x: number) => string, readonly b: (x: string) => number}};
type Lookup<A extends {...}, N extends string, K extends string> = A[N][K];
type ParamOf<F> = F extends (x: infer I) => unknown ? I : empty;

declare function send<NS extends keyof Api, K extends string>(
  ns: NS,
  key: K,
  arg: ParamOf<Lookup<Api, NS, K>>,
): Lookup<Api, NS, K>;

send('ns', 'a', 1);
send('ns', 'a', 1) as empty; // error

// A conditional type on `keyof` of a type parameter being inferred.
type Excl<T, U> = T extends U ? empty : T;
declare function pickOther<T>(o: T, k: Excl<keyof T, 'a'>): T;
pickOther({a: 1, b: 2}, 'b');
pickOther({a: 1, b: 2}, 'a'); // error
