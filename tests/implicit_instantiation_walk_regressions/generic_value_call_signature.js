// An object with a generic call signature is a generic value, instantiated after the other
// arguments, as a generic function is. Instantiated first, with no evidence yet, `thunk` took the
// bounds of its type parameters and the default `Dispatch<A>` of `D`, which made `S` `unknown` and
// put `Dispatch<unknown>` in `D`, outside the bound of `Dispatch`.
type Dispatch<A extends {readonly type: unknown, ...}> = (action: A) => A;
type API<S, A extends {readonly type: unknown, ...}, D = Dispatch<A>> = {
  dispatch: D,
  getState(): S,
  ...
};
type Middleware<S, A extends {readonly type: unknown, ...}, D = Dispatch<A>> = (
  api: API<S, A, D>,
) => (next: D) => D;
declare function applyMiddleware<S, A extends {readonly type: unknown, ...}, D = Dispatch<A>>(
  ...middlewares: Array<Middleware<S, A, D>>
): [S, D];

declare const thunk: {<S, A extends {readonly type: unknown, ...}, D>(api: API<S, A, D>): (next: D) => D};

type Action = {type: 'a'};
declare function logger(api: {
  getState(): {x: number},
  ...
}): (next: (action: Action) => unknown) => (action: Action) => unknown;

const [s] = applyMiddleware(thunk, logger); // was an error
s as {x: number};
s as empty; // error

declare const untyped: any;
const [, d] = applyMiddleware<_, Action, _>(thunk, untyped);
d as (action: Action) => Action; // was an error
