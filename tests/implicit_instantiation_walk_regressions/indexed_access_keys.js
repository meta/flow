type Api = {ns: {readonly a: (number) => string, readonly b: (string) => number}};
type Lookup<A extends {...}, N extends string, K extends string> = A[N][K];
type ParamOf<F> = F extends (x: infer I) => unknown ? I : empty;

declare function send<NS extends keyof Api, K extends string>(
  ns: NS,
  key: K,
  arg: ParamOf<Lookup<Api, NS, K>>,
): Lookup<Api, NS, K>;

send('ns', 'a', 1);
send('ns', 'a', 1) as empty; // error
