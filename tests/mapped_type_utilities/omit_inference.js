function projectWithoutA<T extends {a: number, b: string}>(): Omit<T, 'a'> {
  throw 'unimplemented';
}

projectWithoutA() as {b: string}; // ERROR

function actionWithDefaults<
  TArgs extends any,
  TDefaults extends Partial<TArgs>,
  TResult,
>(
  defaults: TDefaults,
  create: (args: TArgs) => TResult,
): (args: Omit<TArgs, keyof TDefaults>) => TResult {
  throw 'unimplemented';
}

actionWithDefaults(
  {a: 1},
  ({a, b}) => b,
) as (x: {b: string}) => string; // ERROR

function selectProvidedKey<
  TArgs extends any,
  TDefaults extends Partial<TArgs>,
>(
  defaults: TDefaults,
  key: keyof TDefaults,
): keyof TDefaults {
  return key;
}

selectProvidedKey({a: 1}, 'a') as 'a';

function actionWithoutLiteralKey<
  TArgs extends any,
  TResult,
>(
  create: (args: TArgs) => TResult,
): (args: Omit<TArgs, 'a'>) => TResult {
  throw 'unimplemented';
}

actionWithoutLiteralKey(
  ({a, b}) => b,
) as (x: {b: string}) => string;

type HasB = {
  a: string,
  b: number,
};

declare function acceptsOmit<T>(value: Omit<T, 'b'>): void;

acceptsOmit<HasB>({a: 'x'});
