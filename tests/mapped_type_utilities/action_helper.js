export function action<
  TArgs extends any,
  TDefaults extends Partial<TArgs>,
  TResult,
>(
  defaults: TDefaults,
  create: (args: TArgs) => TResult,
): (args: Omit<TArgs, keyof TDefaults>) => TResult {
  throw 'unimplemented';
}
