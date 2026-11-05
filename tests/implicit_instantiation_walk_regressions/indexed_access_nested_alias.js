type Route<TIn, TRoute> = {tool: string, load: (TRoute, ctx: number) => string};
type Entry<TParams> = Route<unknown, TParams>;

declare function makeLoad<TParams extends {...}>(
  getConfig: (TParams, {id: string}) => string,
): Entry<TParams>['load'];

// The indexed access of an alias of an application of another alias, against the hint.
function outer<TParams extends {...}>(): Entry<TParams>['load'] {
  return makeLoad((params, {id}) => id); // ok
}
