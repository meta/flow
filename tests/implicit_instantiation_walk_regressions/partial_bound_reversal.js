// What the solution of `TDefaults` gives `TArgs` through its bound `Partial<TArgs>` says only
// which properties `TArgs` may have: the parameter of `create`, direct evidence, is `TArgs`.
function mergeWithDefaults<TArgs extends interface {}, TDefaults extends Partial<TArgs>, TResult>(
  defaults: TDefaults,
  create: (args: TArgs) => TResult,
): (Omit<TArgs, keyof TDefaults>) => TResult {
  throw 'unimplemented';
}

const merged = mergeWithDefaults(
  {a: 1}, // error: `{a: 1}` is not below `Partial<{a: number, b: string}>`
  ({a, b}: {a: number, b: string}) => b,
);
merged({b: 'ok'}) as string; // ok
